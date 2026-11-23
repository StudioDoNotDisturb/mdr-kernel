//! Session close and private sync. Ports of `bin/close-session.ps1` (close set, handoff and INDEX
//! checks, lock, commit, push, release) and `bin/sync-private.ps1` (live privacy check, fetch and
//! rebase, push with one retry). Node-map generation still runs its PowerShell script
//! until its own checkpoint.
//!
//!   mdr-kernel close-session [-Topic <slug> | -NoHandoff] [-Model <id>] [-Token <token>]
//!       [-Message <subject>] [-SessionDate yyyy-MM-dd] [-Also a,b] [-LeastCode <why>]
//!       [-LeastFile <why>] [-OfflineCheckpoint] [-DryRun]
//!   mdr-kernel sync-private [-PreserveWorkingTree]

use crate::args::Args;
use crate::git::{common_top, git, lossy, names, pwsh_command, text as out, top};
use crate::hook::lock_port;
use crate::remote_privacy::{self, Live};
use regex::Regex;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const VALUES: [&str; 9] = [
    "Topic",
    "Model",
    "Token",
    "Message",
    "SessionDate",
    "Also",
    "LeastCode",
    "LeastFile",
    "Root",
];
const SWITCHES: [&str; 3] = ["OfflineCheckpoint", "DryRun", "NoHandoff"];
/// The handoff's terminal block, as `_templates/handoff-template.md` writes it.
const LINKS: &str = r"## SYSTEM NODE LINKS\r?\n\r?\nDirectory Path: [^\r\n]+\r?\nNext Sequential File: [^\r\n]+\r?\nRelated Workflows: [^\r\n]+\r?\nAI Execution Prompts: [^\r\n]+";

fn ok(m: &str) {
    println!("[ ok ] {m}");
}

fn warn(m: &str) {
    println!("[warn] {m}");
}

/// A git command whose output goes to the console, as `| Out-Host` does; returns its exit code.
fn shown(root: &Path, args: &[&str]) -> i32 {
    Command::new("git")
        .current_dir(root)
        .args(args)
        .status()
        .map_or(1, |s| s.code().unwrap_or(1))
}

fn quiet(root: &Path, args: &[&str]) -> i32 {
    Command::new("git")
        .current_dir(root)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_or(1, |s| s.code().unwrap_or(1))
}

fn count(root: &Path, args: &[&str]) -> i64 {
    out(root, args).parse().unwrap_or(0)
}

fn set_pending_sync(root: &Path) {
    let _ = git(root, &["config", "--local", "brain.pendingSync", "true"]);
}

// ---------- dates: UTC today, host-local today, UTC +/- 1 ----------

fn utc_date(offset_days: i64) -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let (y, m, d) = crate::state_lock::civil_from_days(seconds.div_euclid(86_400) + offset_days);
    format!("{y:04}-{m:02}-{d:02}")
}

fn local_date() -> String {
    #[cfg(windows)]
    {
        #[repr(C)]
        struct SystemTime {
            year: u16,
            month: u16,
            weekday: u16,
            day: u16,
            rest: [u16; 4],
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetLocalTime(time: *mut SystemTime);
        }
        let mut t = SystemTime {
            year: 0,
            month: 0,
            weekday: 0,
            day: 0,
            rest: [0; 4],
        };
        // SAFETY: GetLocalTime fills the caller-owned SYSTEMTIME and keeps nothing.
        unsafe { GetLocalTime(&mut t) };
        let _ = (t.weekday, t.rest);
        format!("{:04}-{:02}-{:02}", t.year, t.month, t.day)
    }
    #[cfg(not(windows))]
    {
        Command::new("date")
            .arg("+%Y-%m-%d")
            .output()
            .ok()
            .map(|o| lossy(o.stdout).trim().to_string())
            .filter(|d| d.len() == 10)
            .unwrap_or_else(|| utc_date(0))
    }
}

// ---------- the close set ----------

/// PowerShell compares these paths without regard to case (`-contains`, `-like`).
fn in_close_set(path: &str, coord: &[String], handoff: &[String], also: &[String]) -> bool {
    let n = path.replace('\\', "/").to_lowercase();
    if coord.iter().any(|c| c.to_lowercase() == n)
        || n.starts_with("maps/")
        || n == "inbox/write-approval-audit.jsonl"
        || handoff.iter().any(|h| h.to_lowercase() == n)
    {
        return true;
    }
    also.iter().any(|a| also_match(&n, a))
}

/// One `-Also` spec: the path itself or anything under it (`n` is already normalized).
fn also_match(n: &str, spec: &str) -> bool {
    let a = spec.replace('\\', "/").trim_end_matches('/').to_lowercase();
    n == a || n.starts_with(&format!("{a}/"))
}

/// `-Also a,b` arrives from `pwsh -File` as one string, so split on commas and drop quotes.
fn split_also(raw: Option<&str>) -> Vec<String> {
    raw.unwrap_or("")
        .split(',')
        .map(|a| {
            a.trim()
                .trim_start_matches(['\'', '"'])
                .trim_end_matches(['\'', '"'])
                .trim()
                .to_string()
        })
        .filter(|a| !a.is_empty())
        .collect()
}

/// Paths from `git status --porcelain -z` records: a rename or copy keeps its new path.
fn parse_status(raw: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut records = raw.split('\0').filter(|r| !r.is_empty());
    while let Some(record) = records.next() {
        if record.len() < 4 {
            continue;
        }
        if record[..2].contains(['R', 'C']) {
            records.next();
        }
        paths.push(record[3..].to_string());
    }
    paths
}

fn pending(root: &Path) -> Vec<String> {
    parse_status(&lossy(
        git(
            root,
            &["status", "--porcelain", "-z", "--untracked-files=all"],
        )
        .unwrap_or_default(),
    ))
}

fn staged(root: &Path) -> Vec<String> {
    names(root, &["diff", "--cached", "--name-only", "-z"]).unwrap_or_default()
}

/// A typo'd `-Also` silently stages nothing, so name every spec that matches no pending change.
fn warn_unmatched(also: &[String], pending: &[String]) {
    for a in also {
        // Only the spec itself counts: a pending node map must not make a typo look matched.
        if !pending
            .iter()
            .any(|p| also_match(&p.replace('\\', "/").to_lowercase(), a))
        {
            warn(&format!("-Also '{a}' matched no pending change - check the path (already committed, or a typo?)"));
        }
    }
}

fn inbox_notes(root: &Path) -> usize {
    fn walk(dir: &Path, found: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            // Get-ChildItem without -Force skips hidden entries.
            let name = entry.file_name().to_string_lossy().into_owned();
            #[cfg(windows)]
            let hidden = {
                use std::os::windows::fs::MetadataExt;
                entry
                    .metadata()
                    .is_ok_and(|m| m.file_attributes() & 0x2 != 0)
            };
            #[cfg(not(windows))]
            let hidden = name.starts_with('.');
            if hidden {
                continue;
            }
            if path.is_dir() {
                walk(&path, found);
            } else if !name.eq_ignore_ascii_case("README.md")
                && !name.eq_ignore_ascii_case("write-approval-audit.jsonl")
            {
                found.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(&root.join("inbox"), &mut files);
    if files.is_empty() {
        return 0;
    }
    let relative: Vec<String> = files
        .iter()
        .filter_map(|f| f.strip_prefix(root).ok())
        .map(|f| f.to_string_lossy().replace('\\', "/"))
        .collect();
    let ignored = Command::new("git")
        .current_dir(root)
        .args(["check-ignore", "--no-index", "--stdin", "-z"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            let mut stdin = child.stdin.take().expect("piped stdin");
            stdin.write_all((relative.join("\0") + "\0").as_bytes())?;
            drop(stdin);
            child.wait_with_output()
        })
        .map(|o| {
            lossy(o.stdout)
                .split('\0')
                .filter(|p| !p.is_empty())
                .count()
        })
        .unwrap_or(0);
    relative.len().saturating_sub(ignored)
}

// ---------- handoff ----------

/// Heals the one mechanical defect: a correct terminal links block missing only `---` before it.
fn repair_separator(text: &str) -> Option<String> {
    let trimmed = text.trim_end();
    let has_separator = Regex::new(r"(?i)---\r?\n\r?\n## SYSTEM NODE LINKS").unwrap();
    let terminal = Regex::new(&format!(r"(?im)^{LINKS}\s*\z")).unwrap();
    if has_separator.is_match(trimmed) || !terminal.is_match(trimmed) {
        return None;
    }
    let block = Regex::new(r"(?s)\r?\n\s*(## SYSTEM NODE LINKS\b)").unwrap();
    Some(block.replacen(text, 1, "\n\n---\n\n${1}").into_owned())
}

fn envelope_ok(text: &str) -> bool {
    let lines: Vec<&str> = text
        .split('\n')
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| !l.trim().is_empty())
        .collect();
    let head = |i: usize, key: &str| {
        lines.get(i).is_some_and(|l| {
            Regex::new(&format!(r"(?i)^{key}:\s+.+"))
                .unwrap()
                .is_match(l)
        })
    };
    head(0, "Title")
        && head(1, "Type")
        && head(2, "Context")
        && Regex::new(&format!(r"(?i)---\r?\n\r?\n{LINKS}\z"))
            .unwrap()
            .is_match(text.trim_end())
}

// ---------- close-session ----------

struct Close {
    fail: u32,
    lock: PathBuf,
    auto_acquired: bool,
    released: bool,
    keep_lock: bool,
}

impl Close {
    fn fail(&mut self, m: &str) {
        println!("[FAIL] {m}");
        self.fail += 1;
    }
}

fn print_lock(report: &crate::state_lock::Report) {
    print!("{}", report.out);
}

pub fn close(root: &Path, args: &Args) -> i32 {
    let param_model = args.value("Model").unwrap_or("").trim().to_string();
    let env_model = std::env::var("BRAIN_MODEL").unwrap_or_default();
    let (model, source) = if !param_model.is_empty() {
        (param_model, "param")
    } else if !env_model.is_empty() {
        (env_model.trim().to_string(), "env")
    } else {
        match out(root, &["config", "brain.model"]) {
            m if m.is_empty() => ("unknown".to_string(), "unknown"),
            m => (m, "config"),
        }
    };
    if !matches!(source, "param" | "env") {
        println!("[FAIL] close-out identity must be explicit (resolved from '{source}': '{model}') — pass -Model '<model-id>' or set $env:BRAIN_MODEL for this session; git config brain.model can name another surface");
        return 1;
    }
    // Children (git commit's pre-commit gate) and the in-process lock read these from the
    // environment, as they read them from the PowerShell process.
    if source == "param" {
        std::env::set_var("BRAIN_MODEL", &model);
    }
    if let Some(token) = args.value("Token").map(str::trim).filter(|t| !t.is_empty()) {
        std::env::set_var("BRAIN_LOCK_TOKEN", token);
    }
    let lock = common_top(root)
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| root.to_path_buf())
        .join("STATE.lock");
    let mut c = Close {
        fail: 0,
        lock,
        auto_acquired: false,
        released: false,
        keep_lock: false,
    };
    let code = close_steps(root, args, &model, &mut c);
    if c.auto_acquired && !c.released && !c.keep_lock && c.lock.exists() {
        let _ = lock_port(root, &["-Release"]);
    }
    code
}

fn close_steps(root: &Path, args: &Args, model: &str, c: &mut Close) -> i32 {
    let topic = args.value("Topic").unwrap_or("");
    let dry = args.switch("DryRun");
    let offline = args.switch("OfflineCheckpoint");
    let no_handoff = args.switch("NoHandoff");
    println!(
        "mdr-kernel close-session — {} (model: {model})\n",
        root.display()
    );

    // A remote-backed memory has one boring canonical path: its approved PRIVATE origin/main.
    let push_url = out(root, &["remote", "get-url", "--push", "origin"]);
    let has_remote = !push_url.is_empty();
    let mode = out(root, &["config", "--local", "--get", "brain.mode"]);
    let approved = out(
        root,
        &["config", "--local", "--get", "brain.approvedRemote"],
    );
    let local_only = mode.eq_ignore_ascii_case("local-only");
    // A memory approved for a private remote never silently degrades to local-only.
    if !has_remote && (mode.eq_ignore_ascii_case("private-remote") || !approved.is_empty()) {
        if !dry {
            set_pending_sync(root);
        }
        c.fail("this memory is in private-remote mode but origin is missing — close refused, durable pending-sync recorded. Restore the remote (git remote add origin <approved-url>) or rerun bin/setup-private.ps1 (-LocalOnly to deliberately accept local-only), then rerun close");
    }
    let branch = out(root, &["branch", "--show-current"]);
    if !branch.eq_ignore_ascii_case("main") {
        c.fail(&format!(
            "memory closes only from main; current branch is '{branch}'"
        ));
    }
    let mut offline_ready = false;
    if offline && local_only {
        c.fail("this memory is already local-only; close normally without -OfflineCheckpoint");
    } else if offline && !has_remote {
        c.fail("-OfflineCheckpoint requires an already-approved private remote; no origin is configured");
    } else if offline && (!mode.eq_ignore_ascii_case("private-remote") || approved.is_empty()) {
        c.fail("-OfflineCheckpoint requires a destination approved earlier by setup-private while online");
    } else if offline {
        // Case-sensitive: canonical pins must not re-conflate case-distinct paths.
        if approved != remote_privacy::identity(&push_url) {
            c.fail("-OfflineCheckpoint refused because origin changed since approval; reconnect and rerun bin/setup-private.ps1 before closing");
        } else {
            offline_ready = true;
        }
    }
    if has_remote {
        if offline {
            if offline_ready {
                ok("offline checkpoint: existing private destination approval found; no live verification or push will be attempted");
            }
        } else {
            let v = remote_privacy::run(root, None, false, true, false, &Live);
            print!("{}", remote_privacy::report(&v));
            if v.code != 0 {
                c.fail("private push destination is not verified and approved — run bin/setup-private.ps1 before closing");
            } else {
                ok("push destination: approved private origin/main");
            }
        }
    }

    // 0) Close set and foreign-staging refusal BEFORE any side effect.
    let utc_today = utc_date(0);
    let local_today = local_date();
    let dates: Vec<String> = match args.value("SessionDate") {
        Some(d) => vec![d.to_string()],
        None => {
            let mut d = Vec::new();
            for day in [
                utc_today.clone(),
                local_today.clone(),
                utc_date(-1),
                utc_date(1),
            ] {
                if !d.contains(&day) {
                    d.push(day);
                }
            }
            d
        }
    };
    let also = split_also(args.value("Also"));
    let Ok(policy) = std::fs::read(root.join("protocol/coordination-files.txt")) else {
        c.fail("coordination policy is missing; restore protocol/coordination-files.txt");
        return 1;
    };
    let coord: Vec<String> = lossy(policy)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect();
    let candidates: Vec<String> = if !topic.is_empty() && !no_handoff {
        dates
            .iter()
            .map(|d| format!("handoffs/{d}-{topic}.md"))
            .collect()
    } else {
        Vec::new()
    };
    let foreign: Vec<String> = staged(root)
        .into_iter()
        .filter(|p| !in_close_set(p, &coord, &candidates, &also))
        .collect();
    if !foreign.is_empty() {
        c.fail("staged paths outside this close set belong to another workstream; nothing unstaged or committed");
        foreign.iter().for_each(|p| println!("    {p}"));
        return 1;
    }

    // 1) Prove ownership of the shared STATE.lock before any handoff repair or staging. Only the
    // token from OUR atomic acquire counts; another session's token in STATE.lock never does.
    if c.fail > 0 {
        return 1;
    }
    if !c.lock.exists() {
        if dry {
            warn("STATE.lock free — a real close would acquire it");
        } else {
            let objective = format!("session close: {topic}");
            let acquired = lock_port(
                root,
                &["-Acquire", "-Model", model, "-Objective", &objective],
            );
            let acquired = match acquired {
                Ok(r) => r,
                Err(e) => crate::state_lock::Report {
                    code: 1,
                    out: format!("{e}\n"),
                },
            };
            print_lock(&acquired);
            if acquired.code != 0 {
                c.fail("could not acquire STATE.lock; wait for the holder");
                return 1;
            }
            c.auto_acquired = true;
            let token = Regex::new(r"(?m)^session token:\s*(\S+)").unwrap();
            if let Some(t) = token.captures(&acquired.out) {
                std::env::set_var("BRAIN_LOCK_TOKEN", &t[1]);
            }
        }
    }
    if c.lock.exists() {
        let owner = lock_port(root, &["-CheckOwner", "-Model", model]);
        let owned = match owner {
            Ok(r) => {
                print_lock(&r);
                r.code == 0
            }
            Err(e) => {
                println!("{e}");
                false
            }
        };
        if !owned {
            c.fail("STATE.lock ownership unproven — carry YOUR acquired token with -Token, or wait for the holder");
            return 1;
        }
        ok(&format!(
            "STATE.lock owned by this {model} session (token verified)"
        ));
    }

    // 2) Session handoff + INDEX row. Accept the first existing match among UTC today, local today
    // and UTC +/- 1 and say which was used; -SessionDate overrides, and a rename never does.
    let mut today = dates[0].clone();
    let mut handoff: Vec<String> = Vec::new();
    if no_handoff {
        warn("NoHandoff: skipping the handoff check (explicit opt-out)");
    } else if topic.is_empty() {
        c.fail("missing -Topic (or pass -NoHandoff): expected handoffs/<today>-<topic>.md");
    } else {
        if let Some(found) = dates
            .iter()
            .find(|d| root.join(format!("handoffs/{d}-{topic}.md")).exists())
        {
            today = found.clone();
            if args.value("SessionDate").is_none() && today != utc_today {
                let flavor = if today == local_today {
                    "host-local today"
                } else {
                    "UTC±1 day"
                };
                ok(&format!(
                    "handoff date {today} accepted ({flavor}; UTC today is {utc_today})"
                ));
            }
        }
        let rel = format!("handoffs/{today}-{topic}.md");
        let path = root.join(&rel);
        if path.exists() {
            ok(&format!("handoff present: {rel}"));
            let mut text = std::fs::read(&path).map(lossy).unwrap_or_default();
            if let Some(stripped) = text.strip_prefix('\u{feff}') {
                text = stripped.to_string();
            }
            if let Some(repaired) = repair_separator(&text) {
                text = repaired;
                if dry {
                    warn("DryRun: would insert missing handoff separator");
                } else if std::fs::write(&path, &text).is_ok() {
                    ok("handoff envelope auto-repaired: inserted the missing `---` separator before SYSTEM NODE LINKS");
                }
            }
            if envelope_ok(&text) {
                ok("handoff Markdown retrieval envelope present");
            } else {
                c.fail("handoff missing Markdown retrieval envelope — start from _templates/handoff-template.md");
            }
        } else {
            c.fail(&format!("handoff missing: {rel} — write it before closing (accepted dates for -Topic '{topic}': {}; a handoff dated outside that window needs -SessionDate <yyyy-MM-dd>, never a rename)", dates.join(", ")));
        }
        let needle = format!("{today}-{topic}.md").to_lowercase();
        let index = std::fs::read(root.join("handoffs/INDEX.md"))
            .map(lossy)
            .unwrap_or_default();
        if index.to_lowercase().contains(&needle) {
            ok("handoffs/INDEX.md row present");
            // INDEX promises one line per handoff; paragraph rows tax every future read.
            if let Some(row) = index.lines().find(|l| l.to_lowercase().contains(&needle)) {
                let len = row.encode_utf16().count();
                if len > 600 {
                    warn(&format!("INDEX row is {len} chars — keep it to a sentence or two; the detail belongs in the handoff itself"));
                }
            }
        } else {
            c.fail(&format!(
                "handoffs/INDEX.md has no row for {today}-{topic}.md"
            ));
        }
        handoff.push(rel);
    }

    // 3) STATE.md freshness: pending changes now, or already committed today.
    let state_last = out(root, &["log", "-1", "--format=%cs", "--", "STATE.md"]);
    if !out(root, &["status", "--porcelain", "--", "STATE.md"]).is_empty() {
        ok("STATE.md has pending updates (will be committed)");
    } else if state_last == today {
        ok("STATE.md already committed today");
    } else {
        warn(&format!(
            "STATE.md untouched today (last commit {state_last}) — is it still accurate?"
        ));
    }

    // 3b) STATE budget, with the targeting breakdown when over.
    let budget = crate::health::state_budget(&root.join("STATE.md"), 90, 10, 5, false);
    let budget_ok = budget.code == 0;
    let mut budget_lines = budget.out.lines();
    let first = budget_lines.next().unwrap_or("");
    if budget_ok {
        ok(&format!("STATE budget: {first}"));
    } else {
        warn(&format!("STATE budget: {first}"));
        budget_lines.for_each(|l| println!("{l}"));
    }

    // 4) inbox should be folded (or consciously left).
    match inbox_notes(root) {
        0 => ok("inbox/ empty"),
        n => warn(&format!(
            "inbox/ has {n} unfolded note(s) — fold them or say in the handoff why they wait"
        )),
    }

    // 5) Doctor: only its hard failures block a close.
    let hard = crate::doctor::Options {
        hard_only: true,
        ..Default::default()
    };
    if crate::doctor::run(root, &hard).0 == 0 {
        ok("brain-doctor: no hard failures");
    } else {
        c.fail("brain-doctor reports a hard failure — run it and fix before closing");
    }

    // Close-set staging: coordination files, this handoff, generated maps and -Also paths.
    // Stray downloads and another session's work stay out.
    let mut pending_paths = pending(root);
    let in_set = |p: &String| in_close_set(p, &coord, &handoff, &also);
    let foreign: Vec<String> = staged(root).into_iter().filter(|p| !in_set(p)).collect();
    if !foreign.is_empty() {
        c.fail("staged paths outside this close set belong to another workstream; nothing unstaged or committed");
        foreign.iter().for_each(|p| println!("    {p}"));
        return 1;
    }
    if dry {
        println!("\n-- DryRun: would stage (close set) --");
        warn_unmatched(&also, &pending_paths);
        let (stage, leftover): (Vec<&String>, Vec<&String>) =
            pending_paths.iter().partition(|p| in_set(p));
        if stage.is_empty() {
            println!("(close set clean)");
        }
        stage.iter().for_each(|p| println!("{p}"));
        if !leftover.is_empty() {
            println!("-- would leave in tree ({}) --", leftover.len());
            leftover.iter().for_each(|p| println!("{p}"));
        }
        println!("\nclose-session DryRun: {} failing check(s).", c.fail);
        return i32::from(c.fail > 0);
    }
    if c.fail > 0 {
        println!(
            "\nclose-session: {} failing check(s) — nothing committed.",
            c.fail
        );
        return 1;
    }

    // 6) Stage + commit (trailer explicit; prepare-commit-msg backstops, never double-stamps).
    let add = |c: &mut Close, path: &str| -> bool {
        if quiet(root, &["add", "-A", "--", path]) == 0 {
            return true;
        }
        // A pre-staged deletion matches neither tree nor index pathspec; it is satisfied only
        // when the deletion is already in the index.
        let path = path.replace('\\', "/");
        if names(
            root,
            &["diff", "--cached", "--name-only", "-z", "--", &path],
        )
        .unwrap_or_default()
        .is_empty()
        {
            c.fail(&format!("could not stage '{path}' - stage everything yourself (git add) or nothing, then rerun close"));
            return false;
        }
        true
    };
    if pending_paths.iter().any(in_set) {
        // Retrieval truth is defined over tracked Markdown: pre-stage only this close set's
        // Markdown, then generate the maps.
        let markdown: Vec<String> = pending_paths
            .iter()
            .filter(|p| in_set(p) && p.to_lowercase().ends_with(".md"))
            .cloned()
            .collect();
        for path in &markdown {
            if !add(c, path) {
                return 1;
            }
        }
        if root.join("bin/update-node-map.ps1").exists() {
            let mapped = pwsh_command(root, "update-node-map.ps1", &[])
                .status()
                .is_ok_and(|s| s.success());
            if !mapped {
                c.fail("node-map regeneration failed after staging — see output above");
                return 1;
            }
            pending_paths = pending(root);
        }
        warn_unmatched(&also, &pending_paths);
        let (stage, leftover): (Vec<&String>, Vec<&String>) =
            pending_paths.iter().partition(|p| in_set(p));
        for path in &stage {
            if !add(c, path) {
                return 1;
            }
        }
        if !leftover.is_empty() {
            warn(&format!("left in tree, NOT committed ({} path(s)); pass -Also only for paths this session owns:", leftover.len()));
            leftover.iter().take(15).for_each(|p| println!("    {p}"));
            // A staged INDEX row must never point at a file this close leaves behind.
            let index_file = Regex::new(r"(?i)(^|/)INDEX\.md$").unwrap();
            for index in staged(root).iter().filter(|p| index_file.is_match(p)) {
                let added: Vec<String> =
                    lossy(git(root, &["diff", "--cached", "-U0", "--", index]).unwrap_or_default())
                        .lines()
                        .filter(|l| l.starts_with('+'))
                        .map(str::to_lowercase)
                        .collect();
                for lo in &leftover {
                    let name = lo.replace('\\', "/");
                    let name = name.rsplit('/').next().unwrap_or("").to_lowercase();
                    if !name.is_empty() && added.iter().any(|l| l.contains(&name)) {
                        c.fail(&format!("{index} gains a row referencing left-behind '{lo}' — commit them together: rerun with -Also '{lo}' (a new reference/skill file also needs its bin/write-approval.ps1 audit entry, which rides automatically)"));
                        return 1;
                    }
                }
            }
        }
        if !staged(root).is_empty() {
            println!("-- staging --");
            print!(
                "{}",
                lossy(git(root, &["status", "--short"]).unwrap_or_default())
            );
            let subject = match (args.value("Message"), topic) {
                (Some(m), _) if !m.is_empty() => m.to_string(),
                (_, t) if !t.is_empty() => format!("Session close: {t}"),
                _ => format!("Session close: {today}"),
            };
            let mut trailers = Vec::new();
            for (key, name) in [("LeastCode", "Least-code"), ("LeastFile", "Least-file")] {
                if let Some(v) = args.value(key).filter(|v| !v.is_empty()) {
                    trailers.push(format!("{name}: {v}"));
                }
            }
            trailers.push(format!("Brain-Model: {model}"));
            let message = format!("{subject}\n\n{}", trailers.join("\n"));
            if shown(root, &["commit", "-m", &message]) != 0 {
                c.fail("git commit failed (pre-commit gate?) — see output above");
                return 1;
            }
            ok("committed");
        } else {
            ok("close set already clean — nothing to commit");
        }
    } else {
        warn_unmatched(&also, &[]);
        ok("close set already clean — nothing to commit");
        if !pending_paths.is_empty() {
            warn(&format!(
                "left {} unrelated path(s) untouched",
                pending_paths.len()
            ));
        }
    }

    // 7) Push when a remote exists. OfflineCheckpoint commits locally, records loud pending-sync
    // state and releases the lock without a push.
    if offline {
        set_pending_sync(root);
        warn("OFFLINE CHECKPOINT: committed locally and did not push. Recovery: pwsh -NoProfile -File bin/sync-private.ps1");
    } else if !has_remote {
        if local_only {
            ok("local-only memory: committed locally (no push by design)");
        } else {
            warn("no git remote configured — committed locally, skipping push. Add a remote and push when ready.");
        }
    } else {
        if sync(root, true) != 0 {
            c.keep_lock = true;
            set_pending_sync(root);
            c.fail("private sync failed — STATE.lock kept. Resolve safely, run bin/sync-private.ps1, then release the lock");
            return 1;
        }
        ok("pushed");
    }
    match lock_port(root, &["-Release"]) {
        Ok(r) if r.code == 0 => print_lock(&r),
        other => {
            match other {
                Ok(r) => print_lock(&r),
                Err(e) => println!("{e}"),
            }
            c.fail("could not release this session lock");
            return 1;
        }
    }
    c.released = true;

    // Only uncommitted TRACKED changes are a problem: stray untracked paths stay by design.
    if out(root, &["status", "--porcelain", "--untracked-files=no"]).is_empty() {
        ok("working tree clean (tracked files)");
    } else {
        warn("tracked changes remain uncommitted after close — inspect git status");
    }
    println!("\nclose-session: done.");
    // The one sanctioned close ceremony (docs/brand-manual.md); an offline checkpoint waits.
    if !offline {
        println!("Waffle Party protocol authorized. Your continued dedication to the Work has been recorded.");
    }
    0
}

// ---------- sync-private ----------

fn update_from_origin(root: &Path, preserve: bool) -> Result<(), String> {
    let listed = Command::new("git")
        .current_dir(root)
        .args([
            "ls-remote",
            "--exit-code",
            "--heads",
            "origin",
            "refs/heads/main",
        ])
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("required utility 'git' is unavailable ({e})"))?;
    if listed.status.code() == Some(2) {
        let ahead = count(root, &["rev-list", "--count", "HEAD"]);
        println!("SYNC_STATUS=ahead:{ahead} behind:0 remote-main:missing");
        return Ok(());
    }
    if !listed.status.success() || lossy(listed.stdout).trim().is_empty() {
        return Err("could not inspect private origin/main; no push was attempted".into());
    }
    if shown(root, &["fetch", "origin", "main"]) != 0 {
        return Err("fetch failed; no push was attempted".into());
    }
    let ahead = count(root, &["rev-list", "--count", "origin/main..HEAD"]);
    let behind = count(root, &["rev-list", "--count", "HEAD..origin/main"]);
    println!("SYNC_STATUS=ahead:{ahead} behind:{behind}");
    if behind > 0 {
        let rebase: &[&str] = if preserve {
            &["rebase", "--autostash", "origin/main"]
        } else {
            &["rebase", "origin/main"]
        };
        if shown(root, rebase) != 0 {
            quiet(root, &["rebase", "--abort"]);
            return Err("rebase conflicted and was aborted; STATE.lock was not changed. Resolve with an experienced Git user before retrying".into());
        }
        if preserve && !out(root, &["diff", "--name-only", "--diff-filter=U"]).is_empty() {
            return Err("rebase completed, but restoring unrelated tracked changes conflicted; no push was attempted and your changes remain in the autostash. Run git status and ask an experienced Git user before retrying".into());
        }
    }
    Ok(())
}

fn sync_steps(root: &Path, preserve: bool) -> Result<(), String> {
    println!("mdr-kernel private sync - {}\n", root.display());
    if out(root, &["config", "--local", "--get", "brain.mode"]).eq_ignore_ascii_case("local-only") {
        return Err("this memory is local-only; there is no private remote to sync".into());
    }
    if out(root, &["remote", "get-url", "--push", "origin"]).is_empty() {
        return Err("origin is missing; rerun bin/setup-private.ps1 after cloning or adding your PRIVATE repository".into());
    }
    if !out(root, &["branch", "--show-current"]).eq_ignore_ascii_case("main") {
        return Err("private memory sync runs only from main".into());
    }
    if !preserve && !out(root, &["status", "--porcelain", "--untracked-files=no"]).is_empty() {
        return Err("tracked changes are not committed; close the session before syncing".into());
    }
    let v = remote_privacy::run(root, None, false, true, false, &Live);
    print!("{}", remote_privacy::report(&v));
    if v.code != 0 {
        return Err("origin is not live-verified as the approved private destination; run bin/setup-private.ps1".into());
    }
    update_from_origin(root, preserve)?;
    if shown(root, &["push", "origin", "main"]) != 0 {
        println!("[warn] push failed; rechecking privacy and origin/main, then retrying once");
        update_from_origin(root, preserve)?;
        if shown(root, &["push", "origin", "main"]) != 0 {
            return Err(
                "push retry failed; pending-sync state remains and no further attempt was made"
                    .into(),
            );
        }
    }
    quiet(
        root,
        &["config", "--local", "--unset-all", "brain.pendingSync"],
    );
    ok("private origin/main is synchronized");
    println!("PENDING_SYNC=false");
    Ok(())
}

/// Re-verifies privacy live, rebases when behind, and pushes explicit origin/main, retrying once.
pub fn sync(root: &Path, preserve: bool) -> i32 {
    match sync_steps(root, preserve) {
        Ok(()) => 0,
        Err(m) => {
            println!("[FAIL] {m}");
            1
        }
    }
}

pub fn cli(command: &str, argv: &[String]) -> i32 {
    let result = (|| -> Result<i32, String> {
        let (values, switches): (&[&str], &[&str]) = if command == "sync-private" {
            (&["Root"], &["PreserveWorkingTree"])
        } else {
            (&VALUES, &SWITCHES)
        };
        let args = Args::parse(argv, values, switches)?;
        if let Some(d) = args.value("SessionDate") {
            if !Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap().is_match(d) {
                return Err(format!(
                    "-SessionDate '{d}' does not match the pattern yyyy-MM-dd"
                ));
            }
        }
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        let root = args
            .value("Root")
            .map_or_else(|| top(&cwd), |r| Ok(PathBuf::from(r)))?;
        crate::hook::fresh(&root)?;
        std::env::set_current_dir(&root).map_err(|e| e.to_string())?;
        Ok(if command == "sync-private" {
            sync(&root, args.switch("PreserveWorkingTree"))
        } else {
            close(&root, &args)
        })
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
    fn close_set_matches_like_powershell() {
        let coord = vec!["STATE.md".to_string()];
        let handoff = vec!["handoffs/2026-11-23-x.md".to_string()];
        let also = split_also(Some(" 'notes-a.md', \"dir/\" ,,"));
        assert_eq!(also, ["notes-a.md", "dir/"]);
        for p in [
            "state.md",
            "maps/node-map.json",
            "inbox/write-approval-audit.jsonl",
            "Handoffs/2026-11-23-X.md",
            "notes-a.md",
            "dir/sub/file.md",
            "dir",
        ] {
            assert!(in_close_set(p, &coord, &handoff, &also), "{p}");
        }
        assert!(!also_match("maps/node-map.json", "ghost-note.md"));
        for p in ["notes-a.md.bak", "directory/x", "inbox/note.md"] {
            assert!(!in_close_set(p, &coord, &handoff, &also), "{p}");
        }
    }

    #[test]
    fn status_records_keep_new_paths() {
        let raw = " M STATE.md\0R  new.md\0old.md\0?? dir/a b.md\0";
        assert_eq!(parse_status(raw), ["STATE.md", "new.md", "dir/a b.md"]);
    }

    #[test]
    fn handoff_separator_repair_and_envelope() {
        let links = "## SYSTEM NODE LINKS\n\nDirectory Path: handoffs/\nNext Sequential File: STATE.md\nRelated Workflows: STATE.md\nAI Execution Prompts: none\n";
        let head = "Title: [Record] x\nType: Handoff\nContext: Use when testing.\n\nBody.\n";
        let good = format!("{head}\n---\n\n{links}");
        assert!(repair_separator(&good).is_none());
        assert!(envelope_ok(&good));
        let missing = format!("{head}\n{links}");
        assert!(!envelope_ok(&missing));
        let repaired = repair_separator(&missing).unwrap();
        assert!(envelope_ok(&repaired), "{repaired}");
        let malformed = format!("{head}\n{}", links.replace("Directory Path", "Dir"));
        assert!(repair_separator(&malformed).is_none());
        assert!(!envelope_ok(&format!("Type: x\n{good}")));
    }

    #[test]
    fn dates_are_iso_days() {
        let day = Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap();
        for d in [utc_date(0), utc_date(-1), local_date()] {
            assert!(day.is_match(&d), "{d}");
        }
        assert_ne!(utc_date(1), utc_date(0));
    }
}
