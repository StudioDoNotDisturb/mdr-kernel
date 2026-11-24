//! Bounded local history search over handoffs, STATE and the archived decision logs.
//! A rebuildable SQLite FTS5 cache for lexical matches.
//! The cache rebuilds itself when any source file is added, edited, renamed or removed, and a
//! failed rebuild leaves the previous complete index in place.

use crate::args::{absolute, Args};
use regex::Regex;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const STOP: &str = "a an the is are was were be been being do does did has have had how what why where when which who may must should can could would we i you it its this that these those of for to in on at by with and or as from any many much same never only asked call made";
const LOGS: [&str; 2] = ["DECISIONS.md", "DECISIONS-ARCHIVE.md"];
const QUERY_LIMIT: usize = 4096;

fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn ordinary(meta: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // Reject reparse points, including directory junctions.
        if meta.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    !meta.file_type().is_symlink()
}

struct Source {
    path: PathBuf,
    size: u64,
    modified: u128,
}

pub struct Row {
    pub path: String,
    pub line: i64,
    pub id: String,
    pub excerpt: String,
    pub kind: &'static str,
}

fn stat(path: &Path) -> Option<Source> {
    let meta = std::fs::symlink_metadata(path)
        .ok()
        .filter(|m| ordinary(m) && m.is_file())?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    Some(Source {
        path: path.to_path_buf(),
        size: meta.len(),
        modified,
    })
}

fn collect(folder: &Path, files: &mut Vec<Source>) {
    if !std::fs::symlink_metadata(folder).is_ok_and(|m| ordinary(&m) && m.is_dir()) {
        return;
    }
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().to_string();
        if kind.is_dir() {
            collect(&entry.path(), files);
        } else if kind.is_file() && name.ends_with(".md") && !name.starts_with("INDEX") {
            files.extend(stat(&entry.path()));
        }
    }
}

fn sources(root: &Path) -> Vec<Source> {
    let mut files = Vec::new();
    collect(&root.join("handoffs"), &mut files);
    for name in ["STATE.md", "DECISIONS.md", "DECISIONS-ARCHIVE.md"] {
        files.extend(stat(&root.join(name)));
    }
    files.sort_by_key(|f| f.path.to_string_lossy().to_lowercase());
    files
}

/// Paragraphs: runs of text separated by a blank (whitespace-only) line. Returns (byte offset, text).
fn paragraphs(text: &str) -> Vec<(usize, &str)> {
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(skip) = text[at..].find(|c: char| !c.is_whitespace()) {
        let start = at + skip;
        let mut end = text.len();
        let mut search = start;
        while let Some(offset) = text[search..].find('\n') {
            let newline = search + offset;
            let run_end = text[newline..]
                .find(|c: char| !c.is_whitespace())
                .map_or(text.len(), |o| newline + o);
            if text[newline + 1..run_end].contains('\n') {
                end = newline;
                break;
            }
            search = run_end.max(newline + 1);
        }
        found.push((start, &text[start..end]));
        if end == text.len() {
            break;
        }
        at = end;
    }
    found
}

fn line_of(text: &str, offset: usize) -> i64 {
    text[..offset].matches('\n').count() as i64 + 1
}

fn without_footer(text: &str) -> &str {
    let mut fence = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_end();
        let content = trimmed.trim_start_matches(' ');
        if trimmed.len() - content.len() <= 3 {
            if let Some((marker, width)) = fence {
                let run = content.bytes().take_while(|&b| b == marker).count();
                if run >= width && content[run..].trim().is_empty() {
                    fence = None;
                }
            } else if let Some(marker @ (b'`' | b'~')) = content.bytes().next() {
                let run = content.bytes().take_while(|&b| b == marker).count();
                if run >= 3 && (marker == b'~' || !content[run..].contains('`')) {
                    fence = Some((marker, run));
                }
            } else if trimmed == "## SYSTEM NODE LINKS" {
                let next = text[offset + line.len()..].trim_start();
                if next.starts_with("Directory Path:") {
                    return &text[..offset];
                }
            }
        }
        offset += line.len();
    }
    text
}

type Record = (String, i64, String, String, String);

/// `None` when the file is not UTF-8: it is skipped and reported instead of failing the search.
fn records(path: &Path, relative: &str) -> Result<Option<Vec<Record>>, String> {
    static HEADING: OnceLock<Regex> = OnceLock::new();
    static TITLE: OnceLock<Regex> = OnceLock::new();
    let relative = relative.to_string();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let bytes = std::fs::read(path).map_err(|e| format!("{relative}: {e}"))?;
    let Ok(decoded) =
        std::str::from_utf8(bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes))
    else {
        return Ok(None);
    };
    let normalized = decoded.replace("\r\n", "\n").replace('\r', "\n");
    let text = without_footer(&normalized);
    let mut out = Vec::new();
    if LOGS.contains(&name.as_str()) {
        let heading = HEADING.get_or_init(|| Regex::new(r"(?m)^## (D\d+)\b[^\n]*").unwrap());
        let found: Vec<regex::Captures> = heading.captures_iter(text).collect();
        for (i, m) in found.iter().enumerate() {
            let whole = m.get(0).unwrap();
            let end = found
                .get(i + 1)
                .map_or(text.len(), |n| n.get(0).unwrap().start());
            let section = &text[whole.start()..end];
            let first_line = line_of(text, whole.start());
            for (at, part) in paragraphs(section) {
                out.push((
                    relative.clone(),
                    first_line + line_of(section, at) - 1,
                    m[1].to_string(),
                    whole.as_str().to_string(),
                    part.to_string(),
                ));
            }
        }
    } else {
        let title = TITLE
            .get_or_init(|| Regex::new(r"(?m)^Title: (.*)").unwrap())
            .captures(text)
            .map(|c| c[1].to_string())
            .unwrap_or_default();
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        for (at, part) in paragraphs(text) {
            out.push((
                relative.clone(),
                line_of(text, at),
                String::new(),
                format!("{stem} {title}"),
                part.to_string(),
            ));
        }
    }
    Ok(Some(out))
}

fn query_words(query: &str) -> Vec<String> {
    static WORD: OnceLock<Regex> = OnceLock::new();
    let stop: Vec<&str> = STOP.split(' ').collect();
    let mut words: Vec<String> = Vec::new();
    for m in WORD
        .get_or_init(|| Regex::new(r"\w+").unwrap())
        .find_iter(&query.to_lowercase())
    {
        let w = m.as_str();
        if w.chars().count() > 1 && !stop.contains(&w) && !words.iter().any(|x| x == w) {
            words.push(w.to_string());
        }
    }
    words
}

fn default_cache(root: &Path) -> Result<PathBuf, String> {
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
    };
    let base = base
        .filter(|p| p.is_absolute())
        .ok_or("Cannot locate a local user cache directory.")?;
    let dir = base.join("mdr-kernel").join("history");
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&dir).map_err(|e| e.to_string())?;
    let meta = std::fs::symlink_metadata(&dir).map_err(|e| e.to_string())?;
    if !ordinary(&meta) || !meta.is_dir() {
        return Err("History cache must be an ordinary directory.".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o077 != 0 {
            return Err("History cache directory must be private (mode 0700).".into());
        }
    }
    Ok(dir.join(format!("{}.sqlite3", &hash(&root.to_string_lossy())[..32])))
}

pub struct Found {
    pub rows: Vec<Row>,
    /// Memory files left out of the index because they are not UTF-8.
    pub skipped: Vec<String>,
}

enum Fail {
    Cache(rusqlite::Error),
    Other(String),
}

impl From<rusqlite::Error> for Fail {
    fn from(e: rusqlite::Error) -> Self {
        Fail::Cache(e)
    }
}

impl From<String> for Fail {
    fn from(e: String) -> Self {
        Fail::Other(e)
    }
}

/// Corrupt, not a database, or a plain SQLITE_ERROR such as a foreign or outdated schema.
/// Busy, locked and I/O errors are not damage and never discard the cache.
fn damaged(e: &rusqlite::Error) -> bool {
    use rusqlite::ErrorCode::{DatabaseCorrupt, NotADatabase, Unknown};
    matches!(
        e.sqlite_error_code(),
        Some(NotADatabase | DatabaseCorrupt | Unknown)
    )
}

/// The cache only holds derived excerpts, so a damaged one is removed and rebuilt from the sources.
fn discard(cache: &Path) -> Result<(), String> {
    for suffix in ["-journal", "-wal", "-shm"] {
        let mut sidecar = cache.as_os_str().to_os_string();
        sidecar.push(suffix);
        let _ = std::fs::remove_file(sidecar);
    }
    std::fs::remove_file(cache).map_err(|e| {
        format!(
            "history cache {} is damaged and could not be removed ({e}); delete it to rebuild",
            cache.display()
        )
    })
}

pub(crate) fn search(
    query: &str,
    limit: usize,
    root: &Path,
    cache: Option<&Path>,
) -> Result<Found, String> {
    let root = absolute(root);
    let words = query_words(query);
    if words.is_empty() {
        return Ok(Found {
            rows: Vec::new(),
            skipped: Vec::new(),
        });
    }
    let cache = match cache {
        Some(path) => absolute(path),
        None => default_cache(&root)?,
    };
    if let Ok(meta) = std::fs::symlink_metadata(&cache) {
        if !ordinary(&meta) || !meta.is_file() {
            return Err("Cache must be an ordinary file.".into());
        }
    }
    if let Some(parent) = cache.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let attempt = || lookup(query, &words, limit, &root, &cache);
    let result = match attempt() {
        Err(Fail::Cache(e)) if damaged(&e) => {
            discard(&cache)?;
            attempt()
        }
        other => other,
    };
    result.map_err(|fail| match fail {
        Fail::Cache(e) => format!("history cache {}: {e}", cache.display()),
        Fail::Other(message) => message,
    })
}

fn lookup(
    query: &str,
    words: &[String],
    limit: usize,
    root: &Path,
    cache: &Path,
) -> Result<Found, Fail> {
    let files = sources(root);
    let listing: Vec<String> = files
        .iter()
        .map(|f| format!("{}|{}|{}", f.path.to_string_lossy(), f.size, f.modified))
        .collect();
    let signature = hash(&format!("mdr-find-v2\n{}", listing.join("\n")));
    let mut db = Connection::open(cache)?;
    db.busy_timeout(std::time::Duration::from_secs(20))?;
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS meta (signature TEXT);
         CREATE TABLE IF NOT EXISTS skipped (path TEXT);
         CREATE VIRTUAL TABLE IF NOT EXISTS docs USING fts5(path UNINDEXED, line UNINDEXED, id UNINDEXED, title, body, tokenize='porter unicode61');",
    )?;
    let current: Option<String> = match db.query_row("SELECT signature FROM meta", [], |r| r.get(0))
    {
        Err(e) if damaged(&e) => return Err(e.into()),
        other => other.ok(),
    };
    if current.as_deref() != Some(signature.as_str()) {
        // Read everything first, then swap in one transaction: a failure keeps the previous index.
        let mut all = Vec::new();
        let mut skipped = Vec::new();
        for file in &files {
            let relative = file
                .path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            match records(&file.path, &relative)? {
                Some(found) => all.extend(found),
                None => skipped.push(relative),
            }
        }
        let tx = db.transaction()?;
        tx.execute("DELETE FROM docs", [])?;
        tx.execute("DELETE FROM skipped", [])?;
        {
            let mut insert = tx.prepare("INSERT INTO docs VALUES (?, ?, ?, ?, ?)")?;
            for (path, line, id, title, body) in &all {
                insert.execute(params![path, line, id, title, body])?;
            }
            let mut skip = tx.prepare("INSERT INTO skipped VALUES (?)")?;
            for path in &skipped {
                skip.execute(params![path])?;
            }
        }
        tx.execute("DELETE FROM meta", [])?;
        tx.execute("INSERT INTO meta VALUES (?)", params![signature])?;
        tx.commit()?;
    }
    let skipped = db
        .prepare("SELECT path FROM skipped ORDER BY path")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    let quoted: Vec<String> = words.iter().map(|w| format!("\"{w}\"")).collect();
    let phrases: Vec<String> = words
        .windows(2)
        .map(|p| format!("\"{} {}\"", p[0], p[1]))
        .collect();
    let expression = quoted
        .iter()
        .chain(phrases.iter())
        .cloned()
        .collect::<Vec<_>>()
        .join(" OR ");
    let mut coverage: HashMap<i64, usize> = HashMap::new();
    for word in &quoted {
        let mut statement = db.prepare("SELECT rowid FROM docs WHERE docs MATCH ?")?;
        let ids = statement.query_map(params![word], |r| r.get::<_, i64>(0))?;
        for id in ids {
            *coverage.entry(id?).or_default() += 1;
        }
    }
    let mut statement = db
        .prepare("SELECT rowid, path, line, id, snippet(docs,4,'','', ' ... ',42), bm25(docs,0,0,0,5,1) FROM docs WHERE docs MATCH ?")?;
    let mut rows: Vec<(i64, String, i64, String, String, f64)> = statement
        .query_map(params![expression], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })?
        .collect::<Result<_, _>>()?;
    // A bare decision id lists that ruling first, from its heading.
    let exact = query.trim().to_uppercase();
    let key = |row: &(i64, String, i64, String, String, f64)| {
        let is_exact = row.3 == exact;
        (
            !is_exact,
            if is_exact { row.2 } else { 0 },
            std::cmp::Reverse(coverage.get(&row.0).copied().unwrap_or_default()),
        )
    };
    rows.sort_by(|a, b| {
        let (ka, kb) = (key(a), key(b));
        ka.0.cmp(&kb.0)
            .then(ka.1.cmp(&kb.1))
            .then(ka.2.cmp(&kb.2))
            .then(a.5.total_cmp(&b.5))
            .then(a.1.cmp(&b.1))
            .then(a.2.cmp(&b.2))
    });
    let mut seen = std::collections::HashSet::new();
    let mut unique = Vec::new();
    for (_, path, line, id, snippet, _) in rows {
        if seen.insert((path.clone(), id.clone())) {
            let kind = if id.is_empty() {
                "dated context"
            } else {
                "decision evidence"
            };
            unique.push(Row {
                path,
                line,
                id,
                excerpt: snippet.chars().take(320).collect(),
                kind,
            });
            if unique.len() == limit {
                break;
            }
        }
    }
    Ok(Found {
        rows: unique,
        skipped,
    })
}

fn ascii_json(value: &Value) -> String {
    let mut out = String::new();
    for c in value.to_string().chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            let mut units = [0u16; 2];
            for unit in c.encode_utf16(&mut units) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out
}

/// Render evidence pointers for a terminal.
pub fn render(rows: &[Row]) -> Vec<String> {
    let mut lines = vec![
        "BRAIN_FIND: lexical matches. Check STATE and the source rules before acting.".to_string(),
    ];
    for row in rows {
        let id = if row.id.is_empty() {
            String::new()
        } else {
            format!(" {}", row.id)
        };
        lines.push(format!(
            "{}:{}{} [{}] {}",
            row.path,
            row.line,
            id,
            row.kind,
            row.excerpt.split_whitespace().collect::<Vec<_>>().join(" ")
        ));
    }
    if rows.is_empty() {
        lines.push("No lexical matches. Try a distinctive project name or ruling id.".into());
    }
    lines
}

/// `mdr-kernel find --query <text> [--limit 1..10] [--json]`
pub fn cli(argv: &[String]) -> i32 {
    let run = || -> Result<String, String> {
        let args = Args::parse(argv, &["query", "limit", "root"], &["json", "help", "h"])?;
        if args.switch("help") || args.switch("h") {
            return Ok(crate::USAGE.to_string());
        }
        let query = args
            .value("query")
            .ok_or("the following arguments are required: --query")?
            .to_string();
        if query.chars().count() > QUERY_LIMIT {
            return Err("query exceeds 4096 characters".into());
        }
        let limit = match args.value("limit").map(str::parse::<usize>) {
            None => 10,
            Some(Ok(n)) if (1..=10).contains(&n) => n,
            _ => return Err("--limit must be 1 to 10".into()),
        };
        let root = args.root()?;
        let Found { rows, skipped } = search(&query, limit, &root, None)?;
        for path in skipped {
            eprintln!("mdr-kernel find: warning: skipped {path}: not valid UTF-8; re-save it as UTF-8 to make it searchable");
        }
        if args.switch("json") {
            let list: Vec<Value> = rows.iter().map(|r| json!({"path": r.path, "line": r.line, "id": r.id, "excerpt": r.excerpt, "kind": r.kind})).collect();
            return Ok(ascii_json(&Value::Array(list)));
        }
        Ok(render(&rows).join("\n"))
    };
    match run() {
        Ok(text) => {
            use std::io::Write;
            let _ = std::io::stdout().write_all(format!("{text}\n").as_bytes());
            0
        }
        Err(message) => {
            eprintln!("mdr-kernel find: error: {message}");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "mdr-find-test-{name}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(dir.join("handoffs/archive")).unwrap();
        dir
    }

    #[test]
    fn paragraphs_split_on_blank_lines_only() {
        let text = "one\ntwo\n\n  \nthree\n";
        let found: Vec<&str> = paragraphs(text).into_iter().map(|(_, p)| p).collect();
        assert_eq!(found, vec!["one\ntwo", "three\n"]);
        assert_eq!(line_of(text, paragraphs(text)[1].0), 5);
    }

    #[test]
    fn cache_lifecycle_and_query_boundaries() {
        let root = fixture("lifecycle");
        std::fs::write(root.join("STATE.md"), "Live state").unwrap();
        std::fs::write(root.join("DECISIONS.md"), "## D2 - newer\n\nAn inline `## SYSTEM NODE LINKS` mention.\n\n## D1 - older\n\nquartz collision evidence\n").unwrap();
        let cache = root.join("cache.sqlite3");
        let find = |q: &str| search(q, 3, &root, Some(&cache)).map(|f| f.rows);
        assert_eq!(find("quartz").unwrap()[0].id, "D1");
        assert!(find("\" OR * : ()").unwrap().is_empty());
        assert!(find("the and of").unwrap().is_empty());
        assert!(find("unfindablexyzzy").unwrap().is_empty());
        let note = root.join("handoffs/archive/cold.md");
        std::fs::write(&note, "Title: archive\n\nzircon needle\n").unwrap();
        assert!(find("zircon").unwrap()[0].path.ends_with("cold.md"));
        // Edits, renames and removals must invalidate without an explicit rebuild.
        std::fs::write(&note, "Title: archive\n\ntopaz replacement\n").unwrap();
        assert!(find("zircon").unwrap().is_empty());
        assert_eq!(find("topaz").unwrap()[0].line, 3);
        let renamed = note.with_file_name("renamed.md");
        std::fs::rename(&note, &renamed).unwrap();
        assert!(find("topaz").unwrap()[0].path.ends_with("renamed.md"));
        std::fs::remove_file(&renamed).unwrap();
        assert!(find("topaz").unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn non_utf8_handoffs_are_skipped_and_named() {
        let root = fixture("encoding");
        std::fs::write(root.join("handoffs/good.md"), "garnet evidence\n").unwrap();
        // A cp1252 byte (0xE9) and a UTF-16LE BOM file: neither may disable search.
        std::fs::write(root.join("handoffs/cp1252.md"), b"garnet caf\xE9\n").unwrap();
        std::fs::write(root.join("handoffs/utf16.md"), b"\xFF\xFEg\0a\0r\0").unwrap();
        let cache = root.join("cache.sqlite3");
        for _ in 0..2 {
            // The second run reads the warm cache and must still name the skipped files.
            let found = search("garnet", 3, &root, Some(&cache)).unwrap();
            assert_eq!(found.rows.len(), 1);
            assert_eq!(found.rows[0].path, "handoffs/good.md");
            assert_eq!(found.skipped, ["handoffs/cp1252.md", "handoffs/utf16.md"]);
        }
        std::fs::write(root.join("handoffs/cp1252.md"), "garnet fixed\n").unwrap();
        let found = search("garnet", 3, &root, Some(&cache)).unwrap();
        assert_eq!(found.rows.len(), 2);
        assert_eq!(found.skipped, ["handoffs/utf16.md"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn corrupt_cache_rebuilds_itself() {
        let root = fixture("corrupt");
        std::fs::write(root.join("STATE.md"), "opal state\n").unwrap();
        let cache = root.join("cache.sqlite3");
        assert_eq!(
            search("opal", 3, &root, Some(&cache)).unwrap().rows.len(),
            1
        );
        // Garbage over a warm cache, and a bare header: both rebuild in the same call.
        for junk in [vec![b'x'; 8192], b"SQLite format 3\0".to_vec()] {
            std::fs::write(&cache, junk).unwrap();
            let found = search("opal", 3, &root, Some(&cache)).unwrap();
            assert_eq!(found.rows[0].path, "STATE.md");
        }
        // A valid SQLite file with a foreign schema is rebuilt too.
        std::fs::remove_file(&cache).unwrap();
        Connection::open(&cache)
            .unwrap()
            .execute_batch("CREATE TABLE meta (wrong INT); CREATE TABLE docs (wrong INT);")
            .unwrap();
        let found = search("opal", 3, &root, Some(&cache)).unwrap();
        assert_eq!(found.rows[0].path, "STATE.md");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn failed_rebuild_keeps_the_previous_index() {
        let root = fixture("rollback");
        std::fs::write(root.join("STATE.md"), "amber state\n").unwrap();
        let cache = root.join("cache.sqlite3");
        let find = |q: &str| search(q, 3, &root, Some(&cache)).map(|f| f.rows);
        assert_eq!(find("amber").unwrap().len(), 1);
        let blocked = root.join("handoffs/blocked.md");
        std::fs::write(&blocked, "jade note\n").unwrap();
        // Make the new source unreadable so the rebuild fails part way through reading.
        #[cfg(windows)]
        let lock = {
            use std::os::windows::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(&blocked)
                .unwrap()
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0)).unwrap();
        }
        // Root ignores file modes; there is then no failure to prove.
        if std::fs::read(&blocked).is_err() {
            let error = find("amber").err().expect("rebuild must fail");
            assert!(error.contains("handoffs/blocked.md"), "{error}");
            let kept: i64 = Connection::open(&cache)
                .unwrap()
                .query_row(
                    "SELECT count(*) FROM docs WHERE docs MATCH 'amber'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(kept, 1, "the failed rebuild must not touch the old index");
        }
        #[cfg(windows)]
        drop(lock);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        assert_eq!(find("jade").unwrap()[0].path, "handoffs/blocked.md");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn indexes_only_the_selected_memory_sources() {
        let root = fixture("scope");
        std::fs::write(root.join("handoffs/INDEX.md"), "hiddenquartz").unwrap();
        std::fs::write(root.join("README.md"), "hiddenquartz").unwrap();
        std::fs::write(
            root.join("handoffs/review.md"),
            "visiblequartz\n\n## SYSTEM NODE LINKS\n\nDirectory Path: hiddenquartz",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(root.join("README.md"), root.join("handoffs/external.md")).unwrap();
            symlink(root.parent().unwrap(), root.join("handoffs/external")).unwrap();
        }
        let cache = root.join("cache.sqlite3");
        assert!(search("hiddenquartz", 3, &root, Some(&cache))
            .unwrap()
            .rows
            .is_empty());
        assert_eq!(
            search("visiblequartz", 3, &root, Some(&cache))
                .unwrap()
                .rows[0]
                .path,
            "handoffs/review.md"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn exact_decision_id_ranks_its_ruling_first() {
        let root = fixture("exact");
        std::fs::write(
            root.join("DECISIONS.md"),
            "## D7 - the ruling\n\nWhat was decided.\n",
        )
        .unwrap();
        std::fs::write(
            root.join("handoffs/cites.md"),
            "Title: D7 D7\n\nD7 D7 D7 cited again: D7.\n",
        )
        .unwrap();
        assert_eq!(
            search("D7", 3, &root, Some(&root.join("cache.sqlite3")))
                .unwrap()
                .rows[0]
                .id,
            "D7"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
