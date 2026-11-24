//! Commit-message trailers: the least-code and least-file ladders are answered in the commit.
//!
//! A commit that adds a new `bin/` file or more than LINE_LIMIT code lines carries `Least-code:`;
//! one that adds a file or folder carries `Least-file:`. One dated handoff and `inbox/` notes are
//! exempt from Least-file, so an ordinary session close needs neither.

use crate::git::{git, lossy, names};
use regex::Regex;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::OnceLock;

pub const LINE_LIMIT: u64 = 60;
const CODE_EXT: [&str; 11] = [
    "rs", "ps1", "psm1", "py", "sh", "js", "mjs", "cjs", "ts", "tsx", "jsx",
];

fn pattern(cell: &'static OnceLock<Regex>, source: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(source).unwrap())
}

fn least_code() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    pattern(&CELL, r"(?mi)^Least-code:[ \t]*(\S.{9,})$")
}

fn least_file() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    pattern(&CELL, r"(?mi)^Least-file:[ \t]*(\S.{9,})$")
}

fn dated_handoff() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    pattern(&CELL, r"^handoffs/\d{4}-\d{2}-\d{2}-[^/]+\.md$")
}

pub fn is_code(path: &str) -> bool {
    path.rsplit_once('.')
        .is_some_and(|(_, ext)| CODE_EXT.contains(&ext))
        || (path.starts_with(".githooks/") && !path.rsplit('/').next().unwrap_or("").contains('.'))
}

/// (added lines, path) rows from `--numstat -z -M`. A rename record is
/// `A<TAB>D<TAB><NUL>old<NUL>new<NUL>`; binary files report `-` and add nothing.
pub fn parse_numstat(numstat: &str) -> Vec<(u64, String)> {
    let tokens: Vec<&str> = numstat.split('\0').collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let parts: Vec<&str> = tokens[i].trim_start_matches('\n').split('\t').collect();
        i += 1;
        if parts.len() != 3 {
            continue;
        }
        let mut path = parts[2];
        if path.is_empty() {
            path = tokens.get(i + 1).copied().unwrap_or("");
            i += 2;
        }
        if let (Ok(added), false) = (parts[0].parse::<u64>(), path.is_empty()) {
            out.push((added, path.to_string()));
        }
    }
    out
}

fn first(items: &[String], n: usize) -> String {
    items.iter().take(n).cloned().collect::<Vec<_>>().join(", ")
}

/// The refusal text, or "" when the commit may proceed.
pub fn check_code(text: &str, lines: u64, new_bin: &[String]) -> String {
    if (lines <= LINE_LIMIT && new_bin.is_empty()) || least_code().is_match(text) {
        return String::new();
    }
    let mut why = Vec::new();
    if !new_bin.is_empty() {
        why.push(format!("adds new bin/ file(s): {}", first(new_bin, 5)));
    }
    if lines > LINE_LIMIT {
        why.push(format!(
            "adds {lines} code lines (limit {LINE_LIMIT} without a trailer)"
        ));
    }
    format!(
        "X mdr-kernel commit-msg: this commit {}.\n  Walk the least-code ladder (protocol/CONTRACT.md): needed at all? already in the repo (grep)?\n  stdlib or platform? an installed dependency? one line? Then say which rung held, as a trailer:\n    Least-code: <what you reused or why nothing smaller works, at least 10 characters>\n  mdr-kernel close-session takes it as -LeastCode '<text>'.",
        why.join(" and ")
    )
}

/// Every parent folder of an added path that HEAD does not have.
pub fn new_folders(added: &[String], head_dirs: &BTreeSet<String>) -> Vec<String> {
    let mut out = BTreeSet::new();
    for path in added {
        let parts: Vec<&str> = path.split('/').collect();
        for depth in 1..parts.len() {
            out.insert(parts[..depth].join("/"));
        }
    }
    out.into_iter().filter(|d| !head_dirs.contains(d)).collect()
}

/// The refusal text when files or folders are added without a Least-file trailer, else "".
pub fn check_files(text: &str, added: &[String], head_dirs: &BTreeSet<String>) -> String {
    let handoffs: Vec<&String> = added
        .iter()
        .filter(|p| dated_handoff().is_match(p))
        .collect();
    let files: Vec<String> = added
        .iter()
        .filter(|p| !p.starts_with("inbox/") && !(handoffs.len() == 1 && *p == handoffs[0]))
        .cloned()
        .collect();
    if files.is_empty() || least_file().is_match(text) {
        return String::new();
    }
    let mut why = format!("adds file(s): {}", first(&files, 5));
    if files.len() > 5 {
        why.push_str(&format!(" (+{} more)", files.len() - 5));
    }
    let folders = new_folders(&files, head_dirs);
    if !folders.is_empty() {
        why.push_str(&format!(" and folder(s): {}", first(&folders, 5)));
    }
    format!(
        "X mdr-kernel commit-msg: this commit {why}.\n  Walk the least-file ladder (protocol/CONTRACT.md): does anyone need it after this session? can an\n  existing file hold it? is it bulk, generated or temporary? does it belong in another repo?\n  A new folder needs three files landing now, or a tool that requires the path. Then say why:\n    Least-file: <why no existing file could hold it, at least 10 characters>\n  mdr-kernel close-session takes it as -LeastFile '<text>'."
    )
}

/// Every refusal for the staged commit and this message; empty means the commit may proceed.
pub fn refusals(dir: &Path, message: &str) -> Result<Vec<String>, String> {
    let numstat = lossy(git(dir, &["diff", "--cached", "--numstat", "-z", "-M"])?);
    let lines = parse_numstat(&numstat)
        .iter()
        .filter(|(_, path)| is_code(path))
        .map(|(n, _)| n)
        .sum();
    let added_any = names(
        dir,
        &["diff", "--cached", "--name-only", "--diff-filter=A", "-z"],
    )?;
    let new_bin: Vec<String> = added_any
        .into_iter()
        .filter(|p| p.starts_with("bin/"))
        .collect();
    // -M: a pure rename is a move, not a new file.
    let added = names(
        dir,
        &[
            "diff",
            "--cached",
            "--name-only",
            "--diff-filter=A",
            "-M",
            "-z",
        ],
    )?;
    // An unborn branch has no HEAD tree: every folder is new.
    let head_dirs: BTreeSet<String> =
        names(dir, &["ls-tree", "-r", "-d", "--name-only", "-z", "HEAD"])
            .unwrap_or_default()
            .into_iter()
            .collect();
    Ok([
        check_code(message, lines, &new_bin),
        check_files(message, &added, &head_dirs),
    ]
    .into_iter()
    .filter(|r| !r.is_empty())
    .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numstat_counts_added_code_only() {
        let lines = |stat: &str| -> u64 {
            parse_numstat(stat)
                .iter()
                .filter(|(_, p)| is_code(p))
                .map(|(n, _)| n)
                .sum()
        };
        assert_eq!(
            lines(
                "3\t1\tbin/a.sh\x0010\t0\tREADME.md\x00-\t-\tlogo.png\x007\t0\tnative/src/x.rs\x00"
            ),
            10
        );
        assert_eq!(
            lines("0\t0\t\0old.sh\0_archive/old.sh\x005\t2\t\0a.sh\0b.sh\0"),
            5
        );
        assert!(is_code(".githooks/commit-msg") && !is_code(".githooks/README.md"));
    }

    #[test]
    fn trailers_hold_at_their_edges() {
        assert!(check_code("subject", 60, &[]).is_empty());
        assert!(check_code("subject", 61, &[]).contains("adds 61 code lines"));
        assert!(check_code("subject", 0, &["bin/new.sh".into()]).contains("new bin/ file"));
        assert!(!check_code("subject\n\nLeast-code: ok", 500, &[]).is_empty());
        assert!(check_code(
            "subject\n\nleast-code: reused the existing parser",
            500,
            &[]
        )
        .is_empty());
        let dirs = BTreeSet::from(["handoffs".to_string(), "docs".to_string()]);
        let handoff = "handoffs/2026-11-23-close.md".to_string();
        assert!(check_files("s", &[handoff.clone(), "inbox/note.md".into()], &dirs).is_empty());
        let two = [handoff, "handoffs/2026-11-23-other.md".into()];
        assert!(check_files("s", &two, &dirs).contains("adds file(s)"));
        let refusal = check_files("s", &["docs/new/a.md".into()], &dirs);
        assert!(refusal.contains("folder(s): docs/new"), "{refusal}");
        assert!(check_files(
            "s\n\nLeast-file: no existing doc covers this",
            &["docs/a.md".into()],
            &dirs
        )
        .is_empty());
        assert_eq!(
            new_folders(
                &["a/b/c.md".into(), "a/d.md".into()],
                &BTreeSet::from(["a".into()])
            ),
            vec!["a/b"]
        );
    }
}
