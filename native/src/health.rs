//! Memory health reports. Ports of `bin/state-budget.ps1` (the STATE.md size verdict and its
//! breakdown) and `bin/secret-audit.ps1` (the wide on-demand secret scan and the scanner-coverage
//! check). The doctor reuses the line reader and the budget numbers.
//!
//!   mdr-kernel state-budget [-Path <file>] [-MaxLines 90] [-MaxKB 10] [-Top 5] [-Detail]
//!   mdr-kernel secret-audit [-CheckScannerCoverage]

use crate::args::Args;
use crate::git::{names, top};
use regex::Regex;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Lines as PowerShell's `Get-Content` reads them: a UTF-8 BOM dropped, `\r\n`, `\n` and a lone
/// `\r` all end a line, and a final terminator adds no empty line.
pub fn read_lines(bytes: &[u8]) -> Vec<String> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let text = String::from_utf8_lossy(bytes);
    let mut lines = Vec::new();
    let mut rest = text.as_ref();
    while !rest.is_empty() {
        match rest.find(['\r', '\n']) {
            Some(i) => {
                lines.push(rest[..i].to_string());
                let skip = if rest[i..].starts_with("\r\n") { 2 } else { 1 };
                rest = &rest[i + skip..];
            }
            None => {
                lines.push(rest.to_string());
                break;
            }
        }
    }
    lines
}

// ---------- state-budget ----------

pub struct Budget {
    pub code: i32,
    pub out: String,
}

/// The verdict line first (close-session relays it verbatim), then the breakdown when over budget
/// or asked for. Exit code: 0 within budget, 1 over, 2 missing.
pub fn state_budget(path: &Path, max_lines: i64, max_kb: i64, top: usize, detail: bool) -> Budget {
    let mut out = String::new();
    let Ok(raw) = std::fs::read(path) else {
        return Budget {
            code: 2,
            out: format!("[FAIL] not found: {}\n", path.display()),
        };
    };
    let max_bytes = max_kb * 1024;
    let bytes = raw.len() as i64;
    let lines = read_lines(&raw);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let nl_bytes = if raw.windows(2).any(|w| w == b"\r\n") {
        2
    } else {
        1
    };
    let over_bytes = bytes - max_bytes;
    let over_lines = lines.len() as i64 - max_lines;
    let over = over_bytes > 0 || over_lines > 0;
    let count = lines.len();
    if over {
        let mut parts = Vec::new();
        if over_bytes > 0 {
            parts.push(format!("{over_bytes} bytes"));
        }
        if over_lines > 0 {
            parts.push(format!("{over_lines} lines"));
        }
        let _ = writeln!(out, "OVER: {name} is {bytes} bytes / {count} lines (budget {max_bytes} / {max_lines}) - trim {}", parts.join(" + "));
    } else {
        let _ = writeln!(out, "OK: {name} is {bytes} bytes / {count} lines (budget {max_bytes} / {max_lines}) - headroom {} bytes / {} lines", -over_bytes, -over_lines);
    }
    if over || detail {
        // Per-line UTF-8 cost (content + newline), so the breakdown points at real file bytes.
        let cost: Vec<usize> = lines.iter().map(|l| l.len() + nl_bytes).collect();
        out.push_str("\nSections (largest first):\n");
        let heading = Regex::new(r"(?i)^##\s+(.+)$").unwrap();
        // (name, first line, bytes, lines)
        let mut sections: Vec<(String, usize, usize, usize)> = Vec::new();
        let mut cur = ("(head)".to_string(), 1, 0, 0);
        for (i, line) in lines.iter().enumerate() {
            if let Some(c) = heading.captures(line) {
                if cur.3 > 0 {
                    sections.push(cur);
                }
                cur = (format!("## {}", &c[1]), i + 1, 0, 0);
            }
            cur.2 += cost[i];
            cur.3 += 1;
        }
        if cur.3 > 0 {
            sections.push(cur);
        }
        sections.sort_by_key(|s| std::cmp::Reverse(s.2));
        for (name, start, size, n) in &sections {
            let _ = writeln!(
                out,
                "  {size:>6} B  {n:>3} L  L{start}-L{}  {name}",
                start + n - 1
            );
        }
        let _ = writeln!(out, "\nFattest lines (top {top}):");
        let mut order: Vec<usize> = (0..lines.len()).collect();
        // Stable: equal-size lines stay in file order (Sort-Object leaves ties unordered).
        order.sort_by(|a, b| cost[*b].cmp(&cost[*a]));
        for i in order.into_iter().take(top) {
            let line = &lines[i];
            let preview = if line.chars().count() > 72 {
                format!("{}...", line.chars().take(72).collect::<String>())
            } else {
                line.clone()
            };
            let _ = writeln!(out, "  L{:<4} {:>6} B  {preview}", i + 1, cost[i]);
        }
        if over {
            out.push_str("\nTrim in ONE pass: compress the fattest rows above / prune pointers a handoff already\n");
            out.push_str(
                "records, then re-run this script once - the verdict line is byte-exact.\n",
            );
        }
    }
    Budget {
        code: i32::from(over),
        out,
    }
}

// ---------- secret-audit ----------

/// Scanners that must keep both modern key shapes and generic credential assignments.
const COVERAGE: [(&str, &str, [&str; 2]); 5] = [
    (
        "pre-commit fast staged scan",
        ".githooks/pre-commit",
        [
            r"(?m)^\s*patterns=.*sk-\(proj\|svcacct\)-\[A-Za-z0-9_-\]\{20,\}",
            r"(?m)^\s*patterns=.*api\[_-\]\?key\|secret\|passwd\|password\|access\[_-\]\?token",
        ],
    ),
    (
        "brain-doctor tracked-file scan",
        "bin/brain-doctor.ps1",
        [
            r"(?s)\$patterns\s*=\s*@\(.*sk-\(\?:proj\|svcacct\)-\[A-Za-z0-9_-\]\{20,\}",
            r"(?s)\$patterns\s*=\s*@\(.*api\[_-\]\?key\|secret\|passwd\|password\|access\[_-\]\?token",
        ],
    ),
    (
        "secret-audit deep scan",
        "bin/secret-audit.ps1",
        [
            r"(?m)^\s*'fast-scan set'\s*=\s*'.*sk-\(\?:proj\|svcacct\)-\[A-Za-z0-9_-\]\{20,\}",
            r"(?m)^\s*'credential assignment'\s*=\s*'.*api\[_-\]\?key\|secret\|passwd\|password\|access\[_-\]\?token",
        ],
    ),
    (
        "native pre-commit staged scan",
        "native/src/hook.rs",
        [
            r"sk-\(proj\|svcacct\)-\[A-Za-z0-9_-\]\{20,\}",
            r"\(api\[_-\]\?key\|secret\|passwd\|password\|access\[_-\]\?token\)",
        ],
    ),
    (
        "native secret-audit deep scan",
        "native/src/health.rs",
        [
            r#""fast-scan set",\s*r"[^"]*sk-\(\?:proj\|svcacct\)-\[A-Za-z0-9_-\]\{20,\}"#,
            r#""credential assignment",\s*r#?"[^\n]*\(api\[_-\]\?key\|secret\|passwd\|password\|access\[_-\]\?token\)"#,
        ],
    ),
];

/// Wider net than the fast scan. PowerShell's `-match` ignores case, so every pattern does too.
const PATTERNS: [(&str, &str); 7] = [
    (
        "fast-scan set",
        r"sk-ant-[A-Za-z0-9_-]{20,}|sk-(?:proj|svcacct)-[A-Za-z0-9_-]{20,}|sk-[A-Za-z0-9]{20,}|ghp_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{40,}|AKIA[0-9A-Z]{16}|AIza[0-9A-Za-z_-]{30,}|xox[baprs]-[A-Za-z0-9-]{10,}|-----BEGIN [A-Z ]*PRIVATE KEY-----",
    ),
    (
        "provider tokens",
        r"npm_[A-Za-z0-9]{30,}|pypi-[A-Za-z0-9_-]{30,}|hf_[A-Za-z0-9]{30,}|dop_v1_[a-f0-9]{20,}|sk_live_[A-Za-z0-9]{20,}|rk_live_[A-Za-z0-9]{20,}|SG\.[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}|glpat-[A-Za-z0-9_-]{20,}",
    ),
    (
        "slack webhook",
        r"hooks\.slack\.com/services/T[A-Za-z0-9_/]{10,}",
    ),
    (
        "JWT",
        r"eyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}",
    ),
    (
        "bearer header",
        r"Authorization:\s*Bearer\s+[A-Za-z0-9_\-.=]{20,}",
    ),
    (
        "URL with password",
        r#"[a-z][a-z0-9+.-]*://[^/\s:@"]+:[^@\s/"]{3,}@"#,
    ),
    (
        "credential assignment",
        r#"[\w-]*(api[_-]?key|secret|passwd|password|access[_-]?token)\b\s*[:=]\s*["'][^"']{8,}["']"#,
    ),
];

/// Files whose job is to describe secret shapes.
const SKIP: [&str; 4] = [
    ".githooks/pre-commit",
    "bin/brain-doctor.ps1",
    "bin/secret-audit.ps1",
    "native/src/health.rs",
];

pub fn scanner_coverage(root: &Path) -> (i32, String) {
    let mut missing = Vec::new();
    for (name, path, required) in COVERAGE {
        let Ok(text) = std::fs::read(root.join(path)) else {
            missing.push(format!("{name} ({path}): file missing"));
            continue;
        };
        let text = String::from_utf8_lossy(&text);
        for pattern in required {
            if !Regex::new(&format!("(?i){pattern}"))
                .unwrap()
                .is_match(&text)
            {
                missing.push(format!(
                    "{name} ({path}): missing required secret pattern coverage"
                ));
            }
        }
    }
    if missing.is_empty() {
        return (0, "[ ok ] pre-commit, brain-doctor, and secret-audit all cover both modern key shapes and generic credential assignments.\n".into());
    }
    let mut out: String = missing.iter().map(|m| format!("[missing] {m}\n")).collect();
    out.push_str("\nscanner-coverage: failed. Add the modern OpenAI project/service-account key regex to every scanner before relying on a clean audit.\n");
    (1, out)
}

pub fn secret_audit(root: &Path) -> Result<(i32, String), String> {
    let patterns: Vec<(&str, Regex)> = PATTERNS
        .iter()
        .map(|(k, p)| (*k, Regex::new(&format!("(?i){p}")).unwrap()))
        .collect();
    let file_name = Regex::new(
        r"(?i)(^|/)(\.env(\..+)?|id_rsa.*|.*\.(pem|pfx|p12|key)|.*credentials.*|.*secret.*)$",
    )
    .unwrap();
    let name_allow = Regex::new(r"(?i)(secret-audit|secrets?\.md$)").unwrap();
    let allow = Regex::new("(?i)secret-audit:allow").unwrap();
    let mut out = String::new();
    let mut hits = 0;
    for f in names(root, &["ls-files", "-z"])? {
        if SKIP.iter().any(|s| s.eq_ignore_ascii_case(&f)) {
            continue;
        }
        let Ok(bytes) = std::fs::read(root.join(&f)) else {
            continue;
        };
        if file_name.is_match(&f) && !name_allow.is_match(&f) {
            let _ = writeln!(out, "[name] {f} - secret-shaped filename is tracked; keep secret VALUES out of the repo. Store them in an untracked local path and reference them by filename.");
            hits += 1;
        }
        for (n, line) in read_lines(&bytes).iter().enumerate() {
            if allow.is_match(line) {
                continue;
            }
            if let Some((kind, _)) = patterns.iter().find(|(_, p)| p.is_match(line)) {
                let shown = if line.encode_utf16().count() > 120 {
                    format!("{}…", line.chars().take(120).collect::<String>())
                } else {
                    line.clone()
                };
                let _ = writeln!(out, "[{kind}] {f}:{}: {}", n + 1, shown.trim());
                hits += 1;
            }
        }
    }
    out.push('\n');
    if hits > 0 {
        let _ = writeln!(out, "secret-audit: {hits} finding(s). Review each: real secret -> move it to an untracked location and purge it from git history; false positive -> append `secret-audit:allow` to the line.");
        return Ok((1, out));
    }
    out.push_str("secret-audit: clean.\n");
    Ok((0, out))
}

fn root_of(args: &Args) -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    // Native separators, as the script's Join-Path prints them.
    args.value("Root")
        .map_or_else(|| top(&cwd), |r| Ok(PathBuf::from(r)))
        .map(|r| crate::args::absolute(&r))
}

fn number(args: &Args, name: &str, default: i64) -> Result<i64, String> {
    args.value(name).map_or(Ok(default), |v| {
        v.trim()
            .parse()
            .map_err(|_| format!("-{name} needs a whole number, not '{v}'"))
    })
}

pub fn cli(command: &str, argv: &[String]) -> i32 {
    let result = (|| -> Result<i32, String> {
        if command == "secret-audit" {
            let args = Args::parse(argv, &["Root"], &["CheckScannerCoverage"])?;
            let root = root_of(&args)?;
            let (code, out) = if args.switch("CheckScannerCoverage") {
                scanner_coverage(&root)
            } else {
                secret_audit(&root)?
            };
            print!("{out}");
            return Ok(code);
        }
        let args = Args::parse(
            argv,
            &["Path", "MaxLines", "MaxKB", "Top", "Root"],
            &["Detail"],
        )?;
        let path = match args.value("Path").filter(|p| !p.is_empty()) {
            Some(p) => PathBuf::from(p),
            None => root_of(&args)?.join("STATE.md"),
        };
        let top = number(&args, "Top", 5)?.max(0) as usize;
        let report = state_budget(
            &path,
            number(&args, "MaxLines", 90)?,
            number(&args, "MaxKB", 10)?,
            top,
            args.switch("Detail"),
        );
        print!("{}", report.out);
        Ok(report.code)
    })();
    result.unwrap_or_else(|e| {
        eprintln!("X mdr-kernel {command}: {e}");
        1
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_read_like_get_content() {
        assert_eq!(
            read_lines(b"\xEF\xBB\xBFa\r\nb\nc\rd\n"),
            ["a", "b", "c", "d"]
        );
        assert_eq!(read_lines(b"a\n\nb"), ["a", "", "b"]);
        assert!(read_lines(b"").is_empty());
    }

    #[test]
    fn budget_verdicts_and_breakdown() {
        let dir = std::env::temp_dir().join(format!("mdr-budget-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("STATE.md");
        std::fs::write(&path, "# S\n\n## A\nshort\n## B\na much longer line here\n").unwrap();
        let ok = state_budget(&path, 90, 10, 5, false);
        assert_eq!(ok.code, 0);
        assert_eq!(
            ok.out,
            "OK: STATE.md is 45 bytes / 6 lines (budget 10240 / 90) - headroom 10195 bytes / 84 lines\n"
        );
        let over = state_budget(&path, 4, 10, 1, false);
        assert_eq!(over.code, 1);
        assert!(
            over.out.starts_with(
                "OVER: STATE.md is 45 bytes / 6 lines (budget 10240 / 4) - trim 2 lines\n"
            ),
            "{}",
            over.out
        );
        assert!(
            over.out.contains("      29 B    2 L  L5-L6  ## B\n"),
            "{}",
            over.out
        );
        assert!(
            over.out
                .contains("  L6        24 B  a much longer line here\n"),
            "{}",
            over.out
        );
        assert_eq!(state_budget(&dir.join("none.md"), 90, 10, 5, false).code, 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn audit_patterns_ignore_case_and_cover_scanners() {
        let fast = Regex::new(&format!("(?i){}", PATTERNS[0].1)).unwrap();
        assert!(fast.is_match(&format!("akia{}", "a".repeat(16))));
        let source = include_str!("health.rs");
        for pattern in COVERAGE[4].2 {
            assert!(
                Regex::new(&format!("(?i){pattern}"))
                    .unwrap()
                    .is_match(source),
                "{pattern}"
            );
        }
        let hook = include_str!("hook.rs");
        for pattern in COVERAGE[3].2 {
            assert!(Regex::new(pattern).unwrap().is_match(hook), "{pattern}");
        }
    }
}
