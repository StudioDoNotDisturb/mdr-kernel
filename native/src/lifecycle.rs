//! The working-tree document lifecycle. Archive eligibility and moves share the same scan.

use crate::args::{absolute, Args};
use crate::git::{common_top, git, top};
use crate::hook::lock_port;
use regex::Regex;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{ErrorKind, Write};
use std::ops::Range;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

const ARCHIVE: &str = "archive";
const DONE: &[&str] = &["Ingested", "Superseded", "Rejected"];
const ADOPT: &[&str] = &["Open", "Adopted?", "Ingested", "Superseded", "Rejected"];
const OTHER_HISTORICAL: &[&str] = &["Draft", "Record", "Archived"];
const RECORD_AGE: f64 = 30.0 * 86_400.0;

fn title_state(text: &str) -> Option<(&str, Range<usize>)> {
    static TITLE: OnceLock<Regex> = OnceLock::new();
    let header_end = text.split_inclusive('\n').take(6).map(str::len).sum();
    let pattern = TITLE.get_or_init(|| {
        Regex::new(r#"(?m)^(?:\u{feff})?Title:[ \t]*["']?\[([^\]\r\n]+)\]"#).unwrap()
    });
    let mut titles = pattern.captures_iter(&text[..header_end]);
    let state = titles.next()?.get(1)?;
    titles
        .next()
        .is_none()
        .then(|| (state.as_str(), state.range()))
}

fn has_state(states: &[&str], state: &str) -> bool {
    states.iter().any(|s| state.eq_ignore_ascii_case(s))
}

fn is_main(text: &str) -> bool {
    !title_state(text)
        .is_some_and(|(state, _)| has_state(ADOPT, state) || has_state(OTHER_HISTORICAL, state))
}

fn old_enough(touched: i64, now: f64) -> bool {
    now - touched as f64 > RECORD_AGE
}

fn archived_path(path: &str) -> bool {
    path.split('/')
        .next()
        .is_some_and(|p| p.eq_ignore_ascii_case(ARCHIVE))
}

fn is_markdown(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("md"))
}

fn is_index(path: &str) -> bool {
    basename(path).eq_ignore_ascii_case("index.md")
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn checked(path: &Path) -> Result<Option<Metadata>, String> {
    let mut current = PathBuf::new();
    let mut parts = path.components().peekable();
    while let Some(part) = parts.next() {
        current.push(part);
        if !matches!(part, Component::Normal(_)) {
            continue;
        }
        let meta = match fs::symlink_metadata(&current) {
            Ok(meta) => meta,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("{}: {e}", current.display())),
        };
        #[cfg(windows)]
        let reparse = {
            use std::os::windows::fs::MetadataExt;
            meta.file_attributes() & 0x400 != 0
        };
        #[cfg(not(windows))]
        let reparse = false;
        if reparse || meta.file_type().is_symlink() {
            return Err(format!("link or junction in path: {}", current.display()));
        }
        if parts.peek().is_none() {
            return Ok(Some(meta));
        }
        if !meta.is_dir() {
            return Err(format!("not a directory: {}", current.display()));
        }
    }
    Ok(None)
}

fn read(path: &Path) -> Result<String, String> {
    read_if_present(path)?.ok_or_else(|| format!("not a readable regular file: {}", path.display()))
}

fn read_if_present(path: &Path) -> Result<Option<String>, String> {
    match checked(path)? {
        None => Ok(None),
        Some(meta) if meta.is_file() => fs::read_to_string(path)
            .map(Some)
            .map_err(|e| format!("{}: {e}", path.display())),
        Some(_) => Err(format!("not a readable regular file: {}", path.display())),
    }
}

fn memory_root(args: &Args) -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let start = args.value("Root").map_or(cwd, |p| absolute(Path::new(p)));
    checked(&start)?;
    let root = top(&start)?;
    if !checked(&root)?.is_some_and(|m| m.is_dir()) {
        return Err("Git root is not a readable directory".into());
    }
    Ok(root)
}

fn relative(root: &Path, input: &str) -> Result<String, String> {
    let input = input.replace('\\', "/");
    let path = Path::new(&input);
    let rel = if path.is_absolute() {
        path.strip_prefix(root)
            .ok()
            .and_then(Path::to_str)
            .map(|p| p.replace('\\', "/"))
            .ok_or("archive path must be inside the repository")?
    } else {
        input
    };
    let rel = rel.strip_prefix("./").unwrap_or(&rel);
    if rel.split('/').any(|p| {
        p.is_empty()
            || p == "."
            || p == ".."
            || p.eq_ignore_ascii_case(".git")
            || p.ends_with(['.', ' '])
            || p.chars().any(|c| c.is_control() || ":*?<>\"|".contains(c))
    }) {
        return Err("archive path must name one exact file inside the repository".into());
    }
    if !is_markdown(rel) || is_index(rel) || archived_path(rel) {
        return Err(
            "archive path must name a Markdown document outside archive/ and INDEX.md".into(),
        );
    }
    Ok(rel.into())
}

fn paths(root: &Path) -> Result<Vec<String>, String> {
    let names = String::from_utf8(git(
        root,
        &[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
    )?)
    .map_err(|e| format!("unreadable Git paths: {e}"))?;
    let mut paths: Vec<_> = names
        .split_terminator('\0')
        .filter(|p| !archived_path(p))
        .map(str::to_owned)
        .collect();
    paths.sort_unstable();
    paths.dedup();
    Ok(paths)
}

fn old_record(root: &Path, path: &str) -> Result<bool, String> {
    let literal = format!(":(literal){path}");
    if !git(
        root,
        &[
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "--",
            &literal,
        ],
    )?
    .is_empty()
    {
        return Ok(false);
    }
    let date = git(root, &["log", "-1", "--format=%ct", "--", &literal])?;
    let date = std::str::from_utf8(&date)
        .map_err(|e| e.to_string())?
        .trim();
    if date.is_empty() {
        return Ok(false);
    }
    let touched = date
        .parse::<i64>()
        .map_err(|e| format!("invalid Git timestamp for {path}: {e}"))?;
    Ok(old_enough(touched, crate::state_lock::now_seconds()))
}

fn url_path(path: &str) -> String {
    let mut encoded = String::new();
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
            encoded.push(char::from(b));
        } else {
            use std::fmt::Write as _;
            let _ = write!(encoded, "%{b:02X}");
        }
    }
    encoded
}

fn cites(text: &str, path: &str) -> bool {
    let text = text.to_lowercase().replace('\\', "/");
    let name = basename(path).to_lowercase();
    let encoded = url_path(&name).to_ascii_lowercase();
    let stem = name.strip_suffix(".md").unwrap_or(&name);
    let path_char = |c: char| c.is_alphanumeric() || matches!(c, '.' | '_' | '-');
    [&name, &encoded].into_iter().any(|word| {
        text.match_indices(word).any(|(start, found)| {
            let tail = &text[start + found.len()..];
            let tail = if tail.starts_with('.') {
                tail.trim_start_matches('.')
            } else {
                tail
            };
            !text[..start].chars().next_back().is_some_and(path_char)
                && !tail.chars().next().is_some_and(path_char)
        })
    }) || text.split("[[").skip(1).any(|link| {
        link.split_once("]]").is_some_and(|(target, _)| {
            basename(target.split(['|', '#']).next().unwrap_or("")).trim() == stem
        })
    })
}

struct Document {
    path: String,
    text: String,
}

fn documents(root: &Path) -> Result<Vec<Document>, String> {
    paths(root)?
        .into_iter()
        .filter(|p| {
            is_markdown(p)
                || p.starts_with("protocol/") && (p.ends_with(".json") || p.ends_with(".txt"))
        })
        .filter_map(|path| {
            read_if_present(&root.join(&path))
                .transpose()
                .map(|text| text.map(|text| Document { path, text }))
        })
        .collect()
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

fn registry(path: &str) -> String {
    let dir = parent(path);
    if dir.is_empty() {
        "INDEX.md".into()
    } else {
        format!("{dir}/INDEX.md")
    }
}

struct IndexEdit {
    row: Range<usize>,
    updated: String,
}

fn index_edit(text: &str, path: &str) -> Result<Option<IndexEdit>, String> {
    let redirect = format!(
        "{}{ARCHIVE}/{path}",
        "../".repeat(path.matches('/').count())
    );
    let mut columns = None;
    let mut found = None;
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        let row = start..start + line.len();
        start = row.end;
        let line = line
            .trim_start_matches('\u{feff}')
            .trim_end_matches(['\r', '\n']);
        let Some(body) = line
            .trim()
            .strip_prefix('|')
            .and_then(|s| s.strip_suffix('|'))
        else {
            continue;
        };
        let mut cells: Vec<_> = body.split('|').collect();
        if let Some(file) = cells
            .iter()
            .position(|c| c.trim().eq_ignore_ascii_case("File"))
        {
            columns = Some((
                file,
                cells
                    .iter()
                    .position(|c| c.trim().eq_ignore_ascii_case("Status"))
                    .filter(|i| *i != file),
            ));
            continue;
        }
        let Some((file, status)) = columns else {
            continue;
        };
        let Some(cell) = cells.get(file).copied() else {
            continue;
        };
        let value = cell.trim();
        let link = value.starts_with('[');
        let offset = if link {
            let Some((label, _)) = value.split_once("](") else {
                continue;
            };
            if !value.ends_with(')') {
                continue;
            }
            label.len() + 2
        } else {
            usize::from(value.starts_with('`') && value.ends_with('`') && value.len() > 1)
        };
        let target = &value[offset..value.len() - usize::from(offset > 0)];
        let normalized = target.replace('\\', "/");
        let normalized = normalized.strip_prefix("./").unwrap_or(&normalized);
        let name = basename(path);
        if ![name, path, &url_path(name), &url_path(path)]
            .iter()
            .any(|p| normalized.eq_ignore_ascii_case(p))
        {
            continue;
        }
        if found.is_some() {
            return Err(format!("multiple INDEX rows name {path}"));
        }
        if cells
            .iter()
            .enumerate()
            .any(|(i, cell)| i != file && cites(cell, path))
        {
            return Err(format!("{path} has another citation in its INDEX row"));
        }
        let redirect = if link {
            url_path(&redirect)
        } else {
            redirect.clone()
        };
        let offset = cell.len() - cell.trim_start().len() + offset;
        let mut updated_file = cell.to_string();
        updated_file.replace_range(offset..offset + target.len(), &redirect);
        cells[file] = &updated_file;
        let updated_status = status
            .and_then(|i| cells.get(i))
            .map(|cell| replace_cell(cell, "Archived"));
        if let (Some(status), Some(updated)) = (status, &updated_status) {
            cells[status] = updated;
        }
        let original = &text[row.clone()];
        let left = original.find('|').unwrap();
        let right = original.rfind('|').unwrap();
        found = Some(IndexEdit {
            row,
            updated: format!(
                "{}{}{}",
                &original[..=left],
                cells.join("|"),
                &original[right..]
            ),
        });
    }
    Ok(found)
}

fn replace_cell(cell: &str, value: &str) -> String {
    let left = cell.len() - cell.trim_start().len();
    let right = cell.trim_end().len().max(left);
    format!("{}{value}{}", &cell[..left], &cell[right..])
}

fn registry_edit<'a>(
    docs: &'a [Document],
    path: &str,
) -> Result<Option<(&'a Document, IndexEdit)>, String> {
    let registry = registry(path);
    let mut registries = docs
        .iter()
        .filter(|d| d.path.eq_ignore_ascii_case(&registry));
    if let Some(doc) = registries.next() {
        if registries.next().is_some() {
            return Err(format!("ambiguous INDEX files for {path}"));
        }
        if let Some(edit) = index_edit(&doc.text, path)? {
            return Ok(Some((doc, edit)));
        }
    }
    Ok(None)
}

fn citer<'a>(
    docs: &'a [Document],
    path: &str,
    index: Option<&(&Document, IndexEdit)>,
) -> Option<&'a str> {
    docs.iter()
        .find(|doc| {
            if doc.path == path
                || !is_index(&doc.path)
                    && !doc.path.eq_ignore_ascii_case("STATE.md")
                    && !is_main(&doc.text)
            {
                return false;
            }
            if let Some((registry, edit)) = index.filter(|(registry, _)| registry.path == doc.path)
            {
                let remainder = format!(
                    "{}{}",
                    &registry.text[..edit.row.start],
                    &registry.text[edit.row.end..]
                );
                return cites(&remainder, path);
            }
            cites(&doc.text, path)
        })
        .map(|doc| doc.path.as_str())
}

fn list(root: &Path) -> Result<(), String> {
    let docs = documents(root)?;
    let mut rows = Vec::new();
    for doc in docs
        .iter()
        .filter(|d| is_markdown(&d.path) && !is_index(&d.path))
    {
        let Some((state, _)) = title_state(&doc.text) else {
            continue;
        };
        let stale = if state.eq_ignore_ascii_case("Record") && old_record(root, &doc.path)? {
            let index = registry_edit(&docs, &doc.path)?;
            citer(&docs, &doc.path, index.as_ref()).is_none()
        } else {
            has_state(DONE, state)
        };
        if stale {
            rows.push(format!("{} [{state}]", doc.path));
        }
    }
    println!("STALE_COUNT={}", rows.len());
    for row in rows {
        println!("{row}");
    }
    Ok(())
}

/// A private, newly created staging file. Cleanup only removes files created by this invocation.
struct Staged {
    path: Option<PathBuf>,
}

impl Staged {
    fn create(path: PathBuf) -> Result<(Self, File), String> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        Ok((Self { path: Some(path) }, file))
    }

    fn prepare(path: &Path, label: &str, text: &str) -> Result<Self, String> {
        let permissions = checked(path)?
            .ok_or("file disappeared while preparing archive")?
            .permissions();
        if permissions.readonly() {
            return Err(format!("read-only document: {}", path.display()));
        }
        let suffix = format!("lifecycle-{}-{label}.tmp", std::process::id());
        let temp = path.with_extension(suffix);
        let (staged, mut file) = Self::create(temp.clone())?;
        file.write_all(text.as_bytes())
            .and_then(|()| file.sync_all())
            .and_then(|()| fs::set_permissions(&temp, permissions))
            .map_err(|e| format!("{}: {e}", temp.display()))?;
        Ok(staged)
    }

    fn install(&mut self, target: &Path) -> Result<(), String> {
        fs::rename(self.path.as_ref().unwrap(), target)
            .map_err(|e| format!("{}: {e}", target.display()))?;
        self.path = None;
        Ok(())
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        if let Some(path) = &self.path {
            let _ = fs::remove_file(path);
        }
    }
}

fn require_owner(root: &Path, args: &Args) -> Result<(), String> {
    let common = common_top(root).unwrap_or_else(|| root.to_path_buf());
    let lock = common.join("STATE.lock");
    if !checked(&lock)?.is_some_and(|m| m.is_file()) {
        return Err("your own STATE.lock is required".into());
    }
    let mut check = vec!["-CheckOwner"];
    for option in ["-Model", "-Token"] {
        if let Some(value) = args.value(option) {
            check.extend([option, value]);
        }
    }
    let report = lock_port(root, &check)?;
    if report.code != 0 {
        return Err(format!(
            "your own STATE.lock is required: {}",
            report.out.trim()
        ));
    }
    Ok(())
}

fn move_file(source: &Path, dest: &Path) -> Result<(), String> {
    if !checked(source)?.is_some_and(|m| m.is_file()) {
        return Err(format!("not a readable regular file: {}", source.display()));
    }
    checked(dest)?;
    // Reserve the name before renaming. An existing document cannot be overwritten.
    let (mut reservation, file) = Staged::create(dest.to_path_buf())?;
    drop(file);
    fs::rename(source, dest)
        .map_err(|e| format!("moving {} to {}: {e}", source.display(), dest.display()))?;
    reservation.path = None;
    Ok(())
}

fn serialize_archive(root: &Path) -> Result<File, String> {
    // STATE.lock identifies a session. Serialize commands from that same session too.
    let git_dir = git(root, &["rev-parse", "--absolute-git-dir"])?;
    let git_dir = std::str::from_utf8(&git_dir)
        .map_err(|e| e.to_string())?
        .trim_end_matches(['\r', '\n']);
    let guard = Path::new(git_dir).join("mdr-lifecycle.lock");
    if checked(&guard)?.is_some_and(|m| !m.is_file()) {
        return Err("invalid lifecycle lock file".into());
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&guard)
        .map_err(|e| format!("{}: {e}", guard.display()))?;
    file.try_lock()
        .map_err(|e| format!("could not acquire archive operation lock: {e}"))?;
    Ok(file)
}

fn archive(root: &Path, args: &Args) -> Result<(), String> {
    let (input, expired) = match (args.value("Adopt"), args.value("Expired")) {
        (Some(path), None) => (path, false),
        (None, Some(path)) => (path, true),
        _ => return Err("choose exactly one of -Adopt <path> and -Expired <path>".into()),
    };
    let path = relative(root, input)?;
    require_owner(root, args)?;
    let _guard = serialize_archive(root)?;

    let source = root.join(&path);
    let original = read(&source)?;
    let (state, range) = title_state(&original).ok_or("missing or ambiguous Title state")?;
    if expired {
        if !state.eq_ignore_ascii_case("Record") || !old_record(root, &path)? {
            return Err("-Expired requires an unchanged [Record] older than 30 days".into());
        }
    } else if !has_state(ADOPT, state) {
        return Err(format!("Title state [{state}] is ineligible for -Adopt"));
    }
    let docs = documents(root)?;
    if !docs
        .iter()
        .any(|doc| doc.path == path && doc.text == original)
    {
        return Err(
            "archive source must be a Git-visible document with exact path spelling".into(),
        );
    }
    let index = registry_edit(&docs, &path)?;
    if parent(&path).eq_ignore_ascii_case("handoffs") && index.is_none() {
        return Err(format!("missing INDEX row for {path}"));
    }
    if let Some(citing) = citer(&docs, &path, index.as_ref()) {
        return Err(format!("{path} is still cited by main file {citing}"));
    }
    let dest_rel = format!("{ARCHIVE}/{path}");
    let dest = root.join(&dest_rel);
    if checked(&dest)?.is_some() {
        return Err(format!("archive target already exists: {dest_rel}"));
    }

    let mut archived = original.clone();
    archived.replace_range(range, "Archived");
    let mut stamp = Staged::prepare(&source, "stamp", &archived)?;
    let mut index = index
        .map(|(doc, edit)| {
            let index_path = root.join(&doc.path);
            let mut updated = doc.text.clone();
            updated.replace_range(edit.row, &edit.updated);
            let before = Staged::prepare(&index_path, "before", &doc.text)?;
            let after = Staged::prepare(&index_path, "after", &updated)?;
            Ok::<_, String>((doc, index_path, before, after))
        })
        .transpose()?;

    require_owner(root, args)?;
    if read(&source)? != original {
        return Err(format!("{path} changed while preparing archive"));
    }
    if let Some((doc, index_path, _, _)) = &index {
        if read(index_path)? != doc.text {
            return Err(format!("{} changed while preparing archive", doc.path));
        }
    }
    let parent = dest.parent().ok_or("archive destination has no parent")?;
    fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    move_file(&source, &dest)?;
    let mut index_changed = false;
    let finish = (|| {
        if let Some((_, index_path, _, after)) = &mut index {
            after.install(index_path)?;
            index_changed = true;
        }
        stamp.install(&dest)
    })();
    if let Err(error) = finish {
        let mut errors = vec![error];
        if index_changed {
            if let Some((_, index_path, before, _)) = &mut index {
                if let Err(e) = before.install(index_path) {
                    errors.push(format!("could not restore index: {e}"));
                }
            }
        }
        if let Err(e) = move_file(&dest, &source) {
            errors.push(format!("original remains at {dest_rel}: {e}"));
        }
        return Err(errors.join("\n"));
    }
    println!("ARCHIVED={path} -> {dest_rel}");
    Ok(())
}

pub fn cli(cmd: &str, argv: &[String]) -> i32 {
    let result = if cmd == "find-stale" {
        match argv.split_first() {
            Some((sub, args)) if sub == "list" => {
                Args::parse(args, &["Root"], &[]).and_then(|args| list(&memory_root(&args)?))
            }
            _ => Err("usage: mdr-kernel find-stale list [--root <repo>]".into()),
        }
    } else {
        Args::parse(argv, &["Root", "Model", "Token", "Adopt", "Expired"], &[])
            .and_then(|args| archive(&memory_root(&args)?, &args))
    };
    if let Err(error) = result {
        eprintln!("{cmd}: {error}");
        return 2;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_state_is_read_from_the_envelope_and_returns_the_exact_edit_span() {
        let text =
            "\u{feff}Title: '[Record] Old note'\r\nType: Record\r\n\r\nTitle example: [Draft]\r\n";
        let (state, span) = title_state(text).unwrap();
        assert_eq!(state, "Record");
        let mut archived = text.to_owned();
        archived.replace_range(span, "Archived");
        assert_eq!(archived, text.replacen("Record]", "Archived]", 1));
        assert!(title_state("Title: ordinary\n\n[Record] in the body").is_none());
        assert!(title_state("Title: [Draft] One\nTitle: [Open] Two").is_none());
        assert!(title_state("\n\n\n\n\n\nTitle: [Record] Outside the envelope").is_none());
    }

    #[test]
    fn record_age_is_strictly_more_than_thirty_days() {
        assert!(!old_enough(0, RECORD_AGE - 1.0));
        assert!(!old_enough(0, RECORD_AGE));
        assert!(old_enough(0, RECORD_AGE + 1.0));
        assert!(!old_enough(1, 0.0));
    }

    #[test]
    fn index_edit_preserves_other_rows_labels_and_line_endings() {
        let index = "\u{feff}| Date | File | Status |\r\n|---|---|---|\r\n| Jan | [same.md](same.md) | Done |\r\n| Feb | same-other.md | Active |\r\n";
        let edit = index_edit(index, "reference/nested/same.md")
            .unwrap()
            .unwrap();
        let mut updated = index.to_owned();
        updated.replace_range(edit.row, &edit.updated);
        assert_eq!(
            updated,
            index.replace(
                "(same.md) | Done",
                "(../../archive/reference/nested/same.md) | Archived"
            )
        );
        let input = "| File | Notes |\n| `with space.md` | a note |";
        assert_eq!(
            index_edit(input, "with space.md").unwrap().unwrap().updated,
            "| `archive/with space.md` | a note |"
        );
        let encoded = "| File |\n| [label](with%20space.md) |\n";
        assert_eq!(
            index_edit(encoded, "with space.md")
                .unwrap()
                .unwrap()
                .updated,
            "| [label](archive/with%20space.md) |\n"
        );
        assert!(
            index_edit("| File |\n| elsewhere/same.md |\n", "reference/same.md")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn citations_include_relative_paths_wiki_aliases_and_encoded_names() {
        for text in [
            "[link](../handoffs/old%20note.md#heading)",
            "`handoffs\\old note.md`",
            "[[old note|alias]]",
            "[[handoffs/old note#Heading]]",
            "File: OLD NOTE.MD",
        ] {
            assert!(cites(text, "handoffs/old note.md"), "{text}");
        }
        for text in [
            "older note.md",
            "new-old note.md",
            "old note.md2",
            "[[other-note]]",
        ] {
            assert!(!cites(text, "handoffs/old note.md"), "{text}");
        }
    }
}
