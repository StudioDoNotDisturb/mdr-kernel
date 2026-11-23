//! Git hook entry points. `.githooks/*` exec these when `bin/mdr-kernel` is installed; the shell
//! gates stay as the fallback (`MDR_ENGINE=legacy`, or no binary) until the release checkpoint.
//! Gates not yet ported (doors, write approval, lock ownership, decisions, staged suites) still run
//! their PowerShell scripts, in the same order and with the same messages as the shell hook.
//!
//!   mdr-kernel hook pre-commit
//!   mdr-kernel hook prepare-commit-msg <message-file>
//!   mdr-kernel hook commit-msg <message-file>
//!   mdr-kernel install            copy this binary to <repo>/bin, where the hooks find it

use crate::fingerprint::{compiler_input, digest};
use crate::git::{common_top, git, lossy, names, pwsh, pwsh_to_stderr, top};
use regex::Regex;
use std::path::Path;
use std::sync::OnceLock;

const DOORS: [&str; 5] = [
    "protocol/CONTRACT.md",
    "AGENTS.md",
    "CLAUDE.md",
    "GEMINI.md",
    "skills/mdr-kernel/SKILL.md",
];
const REBUILD: &str = "cargo run --locked --release --manifest-path native/Cargo.toml -- install";

/// Exit code of a git query whose answer is its status (`diff --quiet`, `cat-file -e`).
fn git_ok(dir: &Path, args: &[&str]) -> Result<bool, String> {
    std::process::Command::new("git")
        .current_dir(dir)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .map_err(|e| format!("required utility 'git' is unavailable ({e})"))
}

/// The lock check or heartbeat, in process: the same code `bin/state-lock.ps1` hands off to.
fn lock_port(root: &Path, argv: &[&str]) -> Result<crate::state_lock::Report, String> {
    let mut all: Vec<String> = argv.iter().map(|a| a.to_string()).collect();
    all.extend(["-Root".to_string(), root.to_string_lossy().into_owned()]);
    let args = crate::args::Args::parse(
        &all,
        &crate::state_lock::VALUES,
        &crate::state_lock::SWITCHES,
    )?;
    crate::state_lock::run(root, &args)
}

fn staged_fingerprint(root: &Path) -> Result<String, String> {
    let mut files = Vec::new();
    for entry in names(root, &["ls-files", "-s", "-z", "--", "native"])? {
        let (meta, path) = entry
            .split_once('\t')
            .ok_or("unreadable git ls-files output")?;
        if compiler_input(path) {
            let blob = meta
                .split(' ')
                .nth(1)
                .ok_or("unreadable git ls-files output")?;
            files.push((path.to_string(), git(root, &["cat-file", "blob", blob])?));
        }
    }
    if files.is_empty() {
        return Err("no native/ sources are in the index".into());
    }
    Ok(digest(&files))
}

/// A binary built from other sources than the staged ones must not guard this commit.
pub fn fresh(root: &Path) -> Result<(), String> {
    if staged_fingerprint(root)? == env!("MDR_SOURCE_FINGERPRINT") {
        return Ok(());
    }
    Err(format!(
        "bin/mdr-kernel was built from different native/ sources than the staged ones.\n  Rebuild and reinstall it: {REBUILD}\n  (or set MDR_ENGINE=legacy to use the shell and PowerShell gates)"
    ))
}

fn block(lines: &[String]) -> i32 {
    eprintln!();
    for line in lines {
        eprintln!("{line}");
    }
    eprintln!();
    1
}

fn indent(paths: &[String]) -> Vec<String> {
    paths.iter().map(|p| format!("    {p}")).collect()
}

fn secret_hits(root: &Path) -> Result<Vec<String>, String> {
    static SECRET: OnceLock<Regex> = OnceLock::new();
    let secret = SECRET.get_or_init(|| {
        Regex::new(concat!(
            r"(?i)sk-ant-[A-Za-z0-9_-]{20,}|sk-(proj|svcacct)-[A-Za-z0-9_-]{20,}|sk-[A-Za-z0-9]{20,}",
            r"|ghp_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{40,}|AKIA[0-9A-Z]{16}|AIza[0-9A-Za-z_-]{30,}",
            r"|xox[baprs]-[A-Za-z0-9-]{10,}|-----BEGIN [A-Z ]*PRIVATE KEY-----",
            r#"|[[:alnum:]_-]*(api[_-]?key|secret|passwd|password|access[_-]?token)[[:alnum:]_-]*[[:space:]]*[:=][[:space:]]*["'][^"']{8,}["']"#
        ))
        .unwrap()
    });
    let diff = lossy(git(
        root,
        &[
            "diff",
            "--cached",
            "--no-color",
            "--diff-filter=ACM",
            "-U0",
            "--",
            ".",
            ":(exclude).githooks/*",
        ],
    )?);
    Ok(diff
        .lines()
        .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
        .enumerate()
        .filter(|(_, l)| secret.is_match(l))
        .map(|(n, l)| format!("    {}:{l}", n + 1))
        .collect())
}

/// Staged paths from `--name-status -z`, both sides of a rename or copy.
fn staged_paths(root: &Path) -> Result<Vec<String>, String> {
    let tokens = names(
        root,
        &[
            "diff",
            "--cached",
            "--name-status",
            "-z",
            "-M",
            "--diff-filter=ACDMRT",
        ],
    )?;
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let pair = tokens[i].starts_with('R') || tokens[i].starts_with('C');
        let take = if pair { 2 } else { 1 };
        out.extend(tokens.iter().skip(i + 1).take(take).cloned());
        i += 1 + take;
    }
    Ok(out)
}

fn coordination_paths(root: &Path) -> Result<Vec<String>, String> {
    let policy = root.join("protocol/coordination-files.txt");
    let working = std::fs::read(&policy)
        .map_err(|_| "protocol/coordination-files.txt is missing.".to_string())?;
    let mut text = lossy(working);
    for view in [
        ":protocol/coordination-files.txt",
        "HEAD:protocol/coordination-files.txt",
    ] {
        if let Ok(bytes) = git(root, &["show", view]) {
            text.push('\n');
            text.push_str(&lossy(bytes));
        }
    }
    Ok(text
        .split('\n')
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .map(str::to_string)
        .collect())
}

struct Lock {
    exists: bool,
    holder: String,
    token: String,
}

fn read_lock(path: &Path) -> Lock {
    static MODEL: OnceLock<Regex> = OnceLock::new();
    static TOKEN: OnceLock<Regex> = OnceLock::new();
    let Ok(bytes) = std::fs::read(path) else {
        return Lock {
            exists: false,
            holder: String::new(),
            token: String::new(),
        };
    };
    let text = lossy(bytes);
    let field = |cell: &'static OnceLock<Regex>, name: &str| {
        let re = cell.get_or_init(|| {
            Regex::new(&format!(r#""{name}"[[:space:]]*:[[:space:]]*"([^"]*)""#)).unwrap()
        });
        text.lines()
            .find_map(|l| re.captures(l).map(|c| c[1].to_string()))
            .unwrap_or_default()
    };
    let mut holder = field(&MODEL, "model");
    let token = field(&TOKEN, "token");
    if holder.is_empty() {
        holder = text
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().next())
            .unwrap_or("")
            .to_string();
    }
    Lock {
        exists: true,
        holder,
        token,
    }
}

fn pre_commit(root: &Path) -> Result<i32, String> {
    fresh(root)?;
    let repo = root.to_string_lossy().replace('\\', "/");
    // 1) canonical root: a warning, except that coordination files from a mirror are blocked below.
    let norm = |s: &str| {
        s.replace('\\', "/")
            .to_lowercase()
            .trim_end_matches('/')
            .to_string()
    };
    let mut noncanon = false;
    let mut want_raw = String::new();
    if let Ok(marker) = std::fs::read(root.join(".brain-root")) {
        want_raw = lossy(marker).replace(['\r', '\n'], "");
        if norm(&want_raw) != norm(&repo) {
            let canon = common_top(root).map(|p| p.to_string_lossy().replace('\\', "/"));
            if canon.is_some_and(|c| norm(&c) == norm(&want_raw)) {
                eprintln!("note: committing from a linked worktree of the canonical repo ({repo}) — treated as canonical.");
            } else {
                noncanon = true;
                eprintln!("WARN: committing from a non-canonical clone.");
                eprintln!("      here:      {repo}");
                eprintln!("      canonical: {want_raw}   (this may be a mirror — prefer writing in the canonical root)");
            }
        }
    }
    // 2) secret scan
    let hits = secret_hits(root)?;
    if !hits.is_empty() {
        let mut lines =
            vec!["X mdr-kernel pre-commit: possible secret in staged changes:".to_string()];
        lines.extend(hits);
        lines.extend([
            String::new(),
            "  Move the value into a secret store outside the repo and reference it by name."
                .into(),
            "  Genuine false positive? Re-run: git commit --no-verify".into(),
        ]);
        return Ok(block(&lines));
    }
    if !names(
        root,
        &["diff", "--cached", "--name-only", "-z", "--", ".brain-root"],
    )?
    .is_empty()
    {
        return Ok(block(&[
            "X mdr-kernel pre-commit: .brain-root is local clone metadata and must never be committed.".into(),
            "  Remove it from the index: git restore --staged .brain-root".into(),
        ]));
    }
    // 3) doors in sync with the contract, judged from the index
    let changed = names(
        root,
        &[
            "diff",
            "--cached",
            "--name-only",
            "-z",
            "--diff-filter=ACDMRT",
        ],
    )?;
    if changed.iter().any(|f| DOORS.contains(&f.as_str()))
        && !pwsh(root, "sync-doors.ps1", &["-Check", "-Staged"])?
            .status
            .success()
    {
        return Ok(block(&[
            "X mdr-kernel pre-commit: the STAGED contract/door set is internally drifted.".into(),
            "  Run: pwsh -NoProfile -File bin/sync-doors.ps1   then re-stage the contract AND every door together.".into(),
        ]));
    }
    // 4) write approval for new live reference/skill files
    let approval = pwsh(root, "write-approval.ps1", &["-ValidateStagedApprovals"])?;
    if !approval.status.success() {
        let said = format!("{}{}", lossy(approval.stdout), lossy(approval.stderr));
        return Ok(block(&[
            format!("X mdr-kernel pre-commit: {}", said.trim_end()),
            "  Use: pwsh -NoProfile -File bin/write-approval.ps1 -Propose ... then -Approve ...".into(),
            "  The approve entry must be staged in inbox/write-approval-audit.jsonl with the new file.".into(),
        ]));
    }
    // 5) coordination files require STATE.lock, explicit identity and the session token
    let policy = coordination_paths(root)?;
    let staged_coord: Vec<String> = staged_paths(root)?
        .into_iter()
        .filter(|p| policy.contains(p))
        .collect();
    let lock_path = common_top(root)
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| root.to_path_buf())
        .join("STATE.lock");
    let lock = read_lock(&lock_path);
    let model = std::env::var("BRAIN_MODEL").unwrap_or_default();
    let session = std::env::var("BRAIN_LOCK_TOKEN").unwrap_or_default();
    let token_ok = !lock.token.is_empty() && !session.is_empty() && lock.token == session;
    if !staged_coord.is_empty() {
        if noncanon {
            let mut lines = vec![
                "X mdr-kernel pre-commit: coordination files staged from a NON-CANONICAL clone."
                    .to_string(),
                format!("  here:      {repo}"),
                format!("  canonical: {want_raw}"),
                "  Re-do this work in the canonical root (or move the changes there). Staged:"
                    .into(),
            ];
            lines.extend(indent(&staged_coord));
            return Ok(block(&lines));
        }
        if !lock.exists {
            let mut lines = vec![
                "X mdr-kernel pre-commit: coordination files are staged but STATE.lock is not held.".to_string(),
                "  Acquire it first: pwsh -NoProfile -File bin/state-lock.ps1 -Acquire -Model '<model-id>'".into(),
                "  Staged coordination files:".into(),
            ];
            lines.extend(indent(&staged_coord));
            return Ok(block(&lines));
        }
        if model.is_empty() {
            return Ok(block(&[
                format!("X mdr-kernel pre-commit: STATE.lock is held by '{}', but explicit BRAIN_MODEL is unset.", lock.holder),
                "  Set BRAIN_MODEL for this shell/session; shared git config cannot prove session identity.".into(),
                "  Env vars do NOT persist between separate tool shells - set it inline with the commit:".into(),
                "    PowerShell:  $env:BRAIN_MODEL='<model-id>'; git commit ...".into(),
                "    bash:        BRAIN_MODEL=<model-id> git commit ...".into(),
            ]));
        }
        if lock.holder != model {
            return Ok(block(&[
                format!("X mdr-kernel pre-commit: coordination files are staged but STATE.lock is held by '{}', not '{model}'.", lock.holder),
                "  Do not commit STATE/index/contract files until the holder releases the lock.".into(),
            ]));
        }
        if !token_ok {
            return Ok(block(&[
                "X mdr-kernel pre-commit: coordination files are staged but BRAIN_LOCK_TOKEN does not match STATE.lock.".into(),
                "  Use the token printed by bin/state-lock.ps1 -Acquire, or wait for the holder to release the lock.".into(),
            ]));
        }
        let owner = lock_port(root, &["-CheckOwner", "-Model", &model])?;
        eprint!("{}", owner.out);
        if owner.code != 0 {
            eprintln!("X mdr-kernel pre-commit: STATE.lock ownership is unproven; carry your own acquire token.");
            return Ok(1);
        }
    }
    // Decision authority uses index bytes and HEAD history.
    if git_ok(root, &["cat-file", "-e", ":DECISIONS.md"])?
        || git_ok(root, &["cat-file", "-e", "HEAD:DECISIONS.md"])?
    {
        if !pwsh_to_stderr(root, "check-decision-history.ps1", &["-FromIndex"])? {
            eprintln!("X mdr-kernel pre-commit: staged decision history/authority is invalid.");
            return Ok(1);
        }
        if !pwsh_to_stderr(root, "build-rulings.ps1", &["-Check", "-FromIndex"])? {
            eprintln!(
                "X mdr-kernel pre-commit: compile rulings and stage the output with its sources."
            );
            return Ok(1);
        }
    }
    // Staged suites run from index bytes; the runner itself runs from the working tree.
    if !git_ok(root, &["diff", "--quiet", "--", "bin/test-staged.ps1"])? {
        eprintln!("X mdr-kernel pre-commit: bin/test-staged.ps1 differs from its staged copy; stage or restore it first.");
        return Ok(1);
    }
    if !pwsh_to_stderr(root, "test-staged.ps1", &[])? {
        eprintln!("X mdr-kernel pre-commit: staged regression checks failed.");
        return Ok(1);
    }
    // 5b) Markdown envelope length, from the staged blobs
    let envelope = crate::envelope::staged(root)?;
    if !envelope.is_empty() {
        for line in &envelope {
            eprintln!("{line}");
        }
        return Ok(block(&[
            "X mdr-kernel pre-commit: shorten the staged Markdown envelope(s) above.".into(),
            "  Move provenance, approvals, evidence and caveats into the body (reference/markdown-system-node-standard.md).".into(),
        ]));
    }
    // 6) commit-driven heartbeat for the verified holder; never blocks
    if lock.exists && !model.is_empty() && lock.holder == model && token_ok {
        let _ = lock_port(root, &["-Heartbeat"]);
    }
    Ok(0)
}

fn read_message(path: &str) -> Result<String, String> {
    std::fs::read(path)
        .map(lossy)
        .map_err(|e| format!("cannot read {path}: {e}"))
}

/// Attribute commits only to an explicit session identity or an explicit message trailer.
fn prepare_commit_msg(path: &str) -> Result<i32, String> {
    let message = read_message(path)?;
    let model = std::env::var("BRAIN_MODEL")
        .unwrap_or_default()
        .trim()
        .to_string();
    let trailers: Vec<&str> = message
        .lines()
        .filter(|l| {
            l.len() >= 12 && l.is_char_boundary(12) && l[..12].eq_ignore_ascii_case("Brain-Model:")
        })
        .collect();
    let refuse = |why: &str| {
        eprintln!("X mdr-kernel attribution: {why}");
        Ok(1)
    };
    match trailers.as_slice() {
        [_, _, ..] => refuse("keep exactly one Brain-Model trailer."),
        [line] => {
            let trailer = line[12..].trim();
            if trailer.is_empty() || trailer == "unknown" {
                refuse("Brain-Model must name the actual committing model.")
            } else if !model.is_empty() && model != trailer {
                refuse("Brain-Model disagrees with explicit BRAIN_MODEL; fix the identity.")
            } else {
                Ok(0)
            }
        }
        [] if model.is_empty() || model == "unknown" => {
            refuse("set BRAIN_MODEL or include an explicit Brain-Model trailer.")
        }
        [] => {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(path)
                .map_err(|e| e.to_string())?;
            write!(file, "\nBrain-Model: {model}\n").map_err(|e| e.to_string())?;
            Ok(0)
        }
    }
}

fn commit_msg(root: &Path, path: &str) -> Result<i32, String> {
    let git_dir = lossy(git(root, &["rev-parse", "--absolute-git-dir"])?);
    if Path::new(git_dir.trim()).join("MERGE_HEAD").exists() {
        return Ok(0); // a merge message is git's, not a session's
    }
    let message = read_message(path)?.replace("\r\n", "\n");
    let found = crate::least_code::refusals(root, &message)?;
    for refusal in &found {
        eprintln!("{refusal}");
    }
    Ok(i32::from(!found.is_empty()))
}

fn install(root: &Path) -> Result<i32, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dest = root
        .join("bin")
        .join(format!("mdr-kernel{}", std::env::consts::EXE_SUFFIX));
    let temp = root.join("bin").join(".mdr-kernel.installing");
    std::fs::copy(&exe, &temp)
        .map_err(|e| format!("cannot copy {} -> {}: {e}", exe.display(), temp.display()))?;
    std::fs::rename(&temp, &dest).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        format!(
            "cannot replace {} (is a commit running?): {e}",
            dest.display()
        )
    })?;
    println!(
        "installed {} (sources {})",
        dest.display(),
        &env!("MDR_SOURCE_FINGERPRINT")[..12]
    );
    if let Err(e) = fresh(root) {
        println!("warning: {e}");
    }
    Ok(0)
}

pub fn cli(argv: &[String]) -> i32 {
    let run = || -> Result<i32, String> {
        let root = top(&std::env::current_dir().map_err(|e| e.to_string())?)?;
        match argv.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
            ["install"] => install(&root),
            ["hook", "pre-commit"] => pre_commit(&root),
            ["hook", "prepare-commit-msg", file, ..] => {
                fresh(&root)?;
                prepare_commit_msg(file)
            }
            ["hook", "commit-msg", file] => {
                fresh(&root)?;
                commit_msg(&root, file)
            }
            ["hook", "pre-push", _remote, url] => {
                fresh(&root)?;
                let live = crate::remote_privacy::Live;
                let verdict = crate::remote_privacy::run(&root, Some(url), false, true, false, &live);
                print!("{}", crate::remote_privacy::report(&verdict));
                Ok(verdict.code)
            }
            _ => Err("usage: mdr-kernel hook pre-commit | hook prepare-commit-msg <file> | hook commit-msg <file> | hook pre-push <remote> <url> | install".into()),
        }
    };
    run().unwrap_or_else(|e| {
        eprintln!("X mdr-kernel: {e}");
        1
    })
}
