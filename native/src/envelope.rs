//! Envelope length check: the Markdown retrieval envelope is a label, not a summary.
//! Port of `bin/envelope-check.ps1` (same limits, sentence rule and messages). Title is a name of
//! at most 100 characters; Context is ONE sentence of at most 300 characters. A ratchet:
//! pre-commit blocks a staged `.md` over the limits, and `count` reports the untouched rest.
//!
//!   mdr-kernel envelope check <file>...   exit 1 if any file is over
//!   mdr-kernel envelope staged            check added/modified files in the git index
//!   mdr-kernel envelope count [--list]    count tracked .md over the limits (advisory)

use crate::git::{git, lossy, names, top};
use regex::Regex;
use std::path::Path;
use std::sync::OnceLock;

const TITLE_MAX: usize = 100;
const CONTEXT_MAX: usize = 300;
const USAGE: &str = "Usage: mdr-kernel envelope check <file>... | staged | count [--list]";

/// .NET string length (UTF-16 code units), so limits match the PowerShell checker exactly.
fn length(value: &str) -> usize {
    value.encode_utf16().count()
}

/// Split after . ! ? before a capital or digit, but not after e.g. / No. / U.S. / initials.
pub fn sentences(value: &str) -> usize {
    static ABBREVIATION: OnceLock<Regex> = OnceLock::new();
    let abbreviation = ABBREVIATION.get_or_init(|| {
        Regex::new(r"(?:\b(?:e\.g|i\.e|etc|vs|No|Nos|St|Dr|Mr|Mrs|Ms|Jr|Inc|approx|cf|ca|p|pp)|\b[A-Z])\.$").unwrap()
    });
    let value = value.trim();
    let chars: Vec<(usize, char)> = value.char_indices().collect();
    let mut parts = Vec::new();
    let (mut start, mut i) = (0, 0);
    while i < chars.len() {
        if matches!(chars[i].1, '.' | '!' | '?') {
            let mut j = i + 1;
            while j < chars.len() && chars[j].1.is_whitespace() {
                j += 1;
            }
            let mut k = j;
            if k < chars.len() && matches!(chars[k].1, '"' | '\'' | '(' | '[' | '`') {
                k += 1;
            }
            if j > i + 1
                && chars
                    .get(k)
                    .is_some_and(|(_, c)| c.is_ascii_uppercase() || c.is_ascii_digit())
            {
                parts.push(&value[start..chars[i + 1].0]);
                start = chars[j].0;
                i = j;
                continue;
            }
        }
        i += 1;
    }
    parts.push(&value[start..]);
    let (mut count, mut carry) = (0, String::new());
    for part in parts {
        carry = if carry.is_empty() {
            part.to_string()
        } else {
            format!("{carry} {part}")
        };
        if !abbreviation.is_match(&carry) {
            count += 1;
            carry.clear();
        }
    }
    count + usize::from(!carry.is_empty())
}

pub fn problems(text: &str) -> Vec<String> {
    static FIELD: OnceLock<Regex> = OnceLock::new();
    let field = FIELD.get_or_init(|| Regex::new(r"^(Title|Context):\s+(.+?)\s*$").unwrap());
    let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let lines: Vec<String> = if let Some(rest) = text.strip_prefix("---\n") {
        match text[4..].find("\n---") {
            Some(end) => rest[..end].split('\n').map(str::to_string).collect(),
            _ => Vec::new(),
        }
    } else {
        let mut lines: Vec<String> = text
            .split('\n')
            .filter(|l| !l.trim().is_empty())
            .take(3)
            .map(str::to_string)
            .collect();
        // A Context wrapped onto following lines is still one value: join the physical lines up
        // to the next blank line or heading, so wrapping cannot hide a paragraph from the limit.
        let physical: Vec<&str> = text.split('\n').collect();
        if let Some(i) = physical.iter().position(|l| l.starts_with("Context:")) {
            let mut j = i + 1;
            while j < physical.len()
                && !physical[j].trim().is_empty()
                && !physical[j].starts_with('#')
            {
                j += 1;
            }
            let joined = physical[i..j]
                .iter()
                .map(|l| l.trim())
                .collect::<Vec<_>>()
                .join(" ");
            for line in lines.iter_mut().filter(|l| l.starts_with("Context:")) {
                *line = joined.clone();
            }
        }
        lines
    };
    let (mut title, mut context) = (None::<String>, None::<String>);
    for line in &lines {
        let Some(m) = field.captures(line) else {
            continue;
        };
        let slot = if &m[1] == "Title" {
            &mut title
        } else {
            &mut context
        };
        if slot.is_none() {
            let value = &m[2];
            let quoted = value.chars().count() >= 2
                && value.starts_with(['"', '\''])
                && value.chars().next() == value.chars().last();
            *slot = Some(if quoted {
                value[1..value.len() - 1].to_string()
            } else {
                value.to_string()
            });
        }
    }
    let mut found = Vec::new();
    if let Some(title) = title.filter(|t| length(t) > TITLE_MAX) {
        found.push(format!(
            "Title is {} chars (limit {TITLE_MAX}): make it a name; move the summary into the body",
            length(&title)
        ));
    }
    if let Some(context) = context {
        let n = sentences(&context);
        if length(&context) > CONTEXT_MAX || n > 1 {
            found.push(format!(
                "Context is {} chars / {n} sentences (limit {CONTEXT_MAX} chars, 1 sentence): say when an agent should use this file; move provenance, approval quotes, evidence and caveats into the body",
                length(&context)
            ));
        }
    }
    found
}

/// One `X <name>: <problem>` line per problem in the staged `.md` blobs, plus the summary line.
pub fn staged(root: &Path) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for name in names(
        root,
        &[
            "diff",
            "--cached",
            "--name-only",
            "--diff-filter=ACMR",
            "-z",
        ],
    )? {
        if !name.to_lowercase().ends_with(".md") {
            continue;
        }
        let blob = lossy(git(root, &["show", &format!(":{name}")])?);
        out.extend(
            problems(&blob)
                .into_iter()
                .map(|p| format!("X {name}: {p}")),
        );
    }
    if !out.is_empty() {
        out.push("X envelope length: fix the staged header(s) above (reference/markdown-system-node-standard.md)".into());
    }
    Ok(out)
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read(path)
        .map(lossy)
        .map_err(|e| format!("{}: {e}", path.display()))
}

pub fn cli(argv: &[String]) -> i32 {
    let cwd = std::env::current_dir().unwrap_or_default();
    let result = match argv.split_first() {
        Some((mode, [])) if mode == "staged" => staged(&cwd).map(|lines| {
            lines.iter().for_each(|l| println!("{l}"));
            i32::from(!lines.is_empty())
        }),
        Some((mode, rest)) if mode == "count" && rest.iter().all(|a| a == "--list") => top(&cwd)
            .and_then(|root| {
                let tracked = names(&root, &["ls-files", "-z", "*.md"])?;
                let mut over = Vec::new();
                for file in &tracked {
                    let full = root.join(file);
                    if full.is_file() && !problems(&read(&full)?).is_empty() {
                        over.push(file);
                    }
                }
                println!(
                    "ENVELOPE_LENGTH over={} tracked={}",
                    over.len(),
                    tracked.len()
                );
                if !rest.is_empty() {
                    over.iter().for_each(|f| println!("{f}"));
                }
                Ok(0)
            }),
        Some((mode, files)) if mode == "check" && !files.is_empty() => (|| {
            let mut bad = false;
            for file in files {
                for problem in problems(&read(Path::new(file))?) {
                    println!("X {file}: {problem}");
                    bad = true;
                }
            }
            Ok(i32::from(bad))
        })(),
        _ => {
            eprintln!("{USAGE}");
            return 2;
        }
    };
    result.unwrap_or_else(|e| {
        eprintln!("X mdr-kernel envelope: {e}");
        2
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(title: &str, context: &str) -> String {
        format!("Title: {title}\nType: Test\nContext: {context}\n\n# T\n\nbody\n")
    }

    const GOOD: &str = "Use when checking the envelope checker against its own fixtures.";

    #[test]
    fn limits_hold_at_their_edges() {
        assert!(problems(&doc("A short name", GOOD)).is_empty());
        assert!(problems(&doc(&"x".repeat(100), GOOD)).is_empty());
        assert_eq!(problems(&doc(&"x".repeat(101), GOOD)).len(), 1);
        assert!(problems(&doc("T", &format!("Use {}.", "x".repeat(295)))).is_empty());
        assert_eq!(
            problems(&doc("T", &format!("Use {}.", "x".repeat(296)))).len(),
            1
        );
        // .NET counts UTF-16 units: an astral character is two.
        assert_eq!(problems(&doc(&"\u{1F600}".repeat(51), GOOD)).len(), 1);
    }

    #[test]
    fn sentences_skip_abbreviations_initials_and_decimals() {
        assert_eq!(sentences("Use when testing. Written 2026-09-25."), 2);
        assert_eq!(
            sentences(
                "Use when comparing e.g. No. 0412 vs. the U.S. plan; see p. 4 of Dr. Smith's note."
            ),
            1
        );
        assert_eq!(sentences("Version 2.5 of the file, at 3.14 scale."), 1);
        assert_eq!(sentences("One. \"Two\" (three)."), 2);
    }

    #[test]
    fn wrapping_front_matter_and_body() {
        assert!(problems("# No envelope\n\nbody\n").is_empty());
        let wrapped = "Title: T\nType: Test\nContext: Use when testing a Context that\nwraps onto a second line. And a third sentence.\n\n# T\n";
        assert_eq!(problems(wrapped).len(), 1);
        assert!(problems(
            "Title: T\nType: Test\nContext: Use when testing.\n\nBody text. More text.\n"
        )
        .is_empty());
        let yaml = format!(
            "---\nTitle: \"{}\"\nType: Skill\nContext: One. Two.\nname: x\n---\n\n# S\n",
            "y".repeat(120)
        );
        assert_eq!(problems(&yaml).len(), 2);
        assert!(
            problems("---\nTitle: 'Short'\nType: Skill\nContext: 'Use when testing.'\n---\n")
                .is_empty()
        );
        assert!(problems(&format!("\u{feff}{}", doc("T", GOOD).replace('\n', "\r\n"))).is_empty());
    }
}
