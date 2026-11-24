//! Front-door sync copies the canonical BRAIN-CONTRACT block from
//! `protocol/CONTRACT.md` into CLAUDE.md, AGENTS.md and GEMINI.md, and BRAIN-ADVISORY into
//! `protocol/advisory.md`. `-Check` reports drift; `-Check -Staged` judges the index blobs, which
//! is what a commit snapshots.
//!
//!   mdr-kernel sync-doors [-Check [-Staged]]

use crate::args::Args;
use crate::git::{git, lossy, text, top};
use regex::Regex;
use std::path::{Path, PathBuf};

const BLOCKS: [(&str, &[&str], &str); 2] = [
    (
        "BRAIN-CONTRACT",
        &["CLAUDE.md", "AGENTS.md", "GEMINI.md"],
        "Front door",
    ),
    ("BRAIN-ADVISORY", &["protocol/advisory.md"], "Advisory door"),
];

pub struct Outcome {
    pub code: i32,
    pub out: String,
    pub err: String,
}

fn outcome(code: i32, out: impl Into<String>, err: impl Into<String>) -> Outcome {
    Outcome {
        code,
        out: out.into(),
        err: err.into(),
    }
}

/// The content to judge (`None` = missing): the working tree with CRLF folded to LF, or the
/// staged blob with every CR dropped, as the script reads each.
fn read_source(root: &Path, prefix: &str, rel: &str, staged: bool) -> Option<String> {
    if staged {
        return git(root, &["show", &format!(":{prefix}{rel}")])
            .ok()
            .map(|b| lossy(b).replace('\r', ""));
    }
    let bytes = std::fs::read(root.join(rel)).ok()?;
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    Some(String::from_utf8_lossy(bytes).replace("\r\n", "\n"))
}

fn write_lf(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, text.replace("\r\n", "\n"))
}

/// The minimal wrapper written when a target does not exist yet (or lost its markers).
fn door_shell(title: &str, block: &str, target: &str) -> String {
    if target == "protocol/advisory.md" {
        return format!(
            "Title: Advisory door
Type: Generated Advisory Instructions / Front Door
Context: Generated advisory instructions for chat surfaces that cannot read the repo or run git.

# {title}

_This file is generated from protocol/CONTRACT.md by mdr-kernel sync-doors. Edit the
contract, then run sync - do not edit this copy._

{block}

---

## SYSTEM NODE LINKS

Directory Path: protocol/
Next Sequential File: CONTRACT.md
Related Workflows: CONTRACT.md, ../native/src/doors.rs
AI Execution Prompts: CONTRACT.md"
        );
    }
    let door = format!("{target} - mdr-kernel front door");
    format!(
        "Title: {door}
Type: Generated Agent Front Door
Context: Generated CLI-agent instructions that mirror the canonical mdr-kernel contract.

# {door}

_This file is generated from protocol/CONTRACT.md by mdr-kernel sync-doors. Edit the
contract, then run sync - do not edit this copy._

{block}

---

## SYSTEM NODE LINKS

Directory Path: ./
Next Sequential File: protocol/CONTRACT.md
Related Workflows: protocol/CONTRACT.md, native/src/doors.rs
AI Execution Prompts: protocol/CONTRACT.md"
    )
}

pub fn sync(root: &Path, check: bool, staged: bool) -> Outcome {
    if staged && !check {
        return outcome(
            2,
            "",
            "-Staged validates the commit snapshot and requires -Check\n",
        );
    }
    // show-prefix keeps index lookups right when the kernel lives in a subfolder of a larger repo.
    let prefix = if staged {
        text(root, &["rev-parse", "--show-prefix"])
    } else {
        String::new()
    };
    let Some(contract) = read_source(root, &prefix, "protocol/CONTRACT.md", staged) else {
        let path = root.join("protocol").join("CONTRACT.md");
        return outcome(2, "", format!("CONTRACT not found: {}\n", path.display()));
    };
    let mut drift = Vec::new();
    let mut changed = Vec::new();
    for (name, targets, title) in BLOCKS {
        let block = Regex::new(&format!("(?s)<!-- BEGIN {name}.*?<!-- END {name} -->")).unwrap();
        let Some(canonical) = block.find(&contract).map(|m| m.as_str().to_string()) else {
            return outcome(
                2,
                "",
                format!("No {name} markers in protocol/CONTRACT.md\n"),
            );
        };
        for target in targets {
            let path = root.join(target);
            let written = |text: &str| write_lf(&path, text).map_err(|e| format!("{target}: {e}"));
            let Some(current) = read_source(root, &prefix, target, staged) else {
                if check {
                    drift.push(format!("{target} (missing)"));
                } else if let Err(e) = written(&door_shell(title, &canonical, target)) {
                    return outcome(1, "", e + "\n");
                } else {
                    changed.push(format!("{target} (created)"));
                }
                continue;
            };
            let Some(found) = block.find(&current) else {
                if check {
                    drift.push(format!("{target} (no {name} markers)"));
                } else if let Err(e) = written(&door_shell(title, &canonical, target)) {
                    return outcome(1, "", e + "\n");
                } else {
                    changed.push(format!("{target} (recreated)"));
                }
                continue;
            };
            if found.as_str() != canonical {
                if check {
                    drift.push(target.to_string());
                } else {
                    let updated = format!(
                        "{}{canonical}{}",
                        &current[..found.start()],
                        &current[found.end()..]
                    );
                    if let Err(e) = written(&updated) {
                        return outcome(1, "", e + "\n");
                    }
                    changed.push(target.to_string());
                }
            }
        }
    }
    if check {
        if !drift.is_empty() {
            return outcome(
                1,
                format!(
                    "TARGETS OUT OF SYNC: {}\nFix: mdr-kernel sync-doors\n",
                    drift.join(", ")
                ),
                "",
            );
        }
        return outcome(
            0,
            "doors + advisory in sync with protocol/CONTRACT.md\n",
            "",
        );
    }
    if changed.is_empty() {
        outcome(0, "All targets already in sync.\n", "")
    } else {
        outcome(0, format!("Updated: {}\n", changed.join(", ")), "")
    }
}

pub fn cli(argv: &[String]) -> i32 {
    let result = (|| -> Result<i32, String> {
        let args = Args::parse(argv, &["Root"], &["Check", "Staged"])?;
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        let root = args
            .value("Root")
            .map_or_else(|| top(&cwd), |r| Ok(PathBuf::from(r)))
            .map(|r| crate::args::absolute(&r))?;
        crate::hook::fresh_in_repo(&root)?;
        let o = sync(&root, args.switch("Check"), args.switch("Staged"));
        print!("{}", o.out);
        eprint!("{}", o.err);
        Ok(o.code)
    })();
    result.unwrap_or_else(|e| {
        eprintln!("X mdr-kernel sync-doors: {e}");
        2
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_creates_repairs_and_checks() {
        let root = std::env::temp_dir().join(format!("mdr-doors-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("protocol")).unwrap();
        let contract = "x\r\n<!-- BEGIN BRAIN-CONTRACT -->\r\nrules v1\r\n<!-- END BRAIN-CONTRACT -->\r\n<!-- BEGIN BRAIN-ADVISORY -->\nadvice\n<!-- END BRAIN-ADVISORY -->\n";
        std::fs::write(root.join("protocol/CONTRACT.md"), contract).unwrap();
        assert_eq!(sync(&root, true, false).code, 1);
        let made = sync(&root, false, false);
        assert_eq!(made.out, "Updated: CLAUDE.md (created), AGENTS.md (created), GEMINI.md (created), protocol/advisory.md (created)\n");
        let claude = std::fs::read_to_string(root.join("CLAUDE.md")).unwrap();
        assert!(claude.starts_with("Title: CLAUDE.md - mdr-kernel front door\n"));
        assert!(
            claude.contains("<!-- BEGIN BRAIN-CONTRACT -->\nrules v1\n<!-- END BRAIN-CONTRACT -->")
        );
        assert!(claude.ends_with("AI Execution Prompts: protocol/CONTRACT.md"));
        assert_eq!(sync(&root, true, false).code, 0);
        std::fs::write(
            root.join("AGENTS.md"),
            "head\n<!-- BEGIN BRAIN-CONTRACT -->old<!-- END BRAIN-CONTRACT -->\ntail\n",
        )
        .unwrap();
        let check = sync(&root, true, false);
        assert_eq!(
            check.out,
            "TARGETS OUT OF SYNC: AGENTS.md\nFix: mdr-kernel sync-doors\n"
        );
        assert_eq!(sync(&root, false, false).out, "Updated: AGENTS.md\n");
        assert_eq!(
            std::fs::read_to_string(root.join("AGENTS.md")).unwrap(),
            "head\n<!-- BEGIN BRAIN-CONTRACT -->\nrules v1\n<!-- END BRAIN-CONTRACT -->\ntail\n"
        );
        assert_eq!(sync(&root, false, true).code, 2);
        let _ = std::fs::remove_dir_all(&root);
    }
}
