//! The public doctor. A port of `bin/brain-doctor.ps1`: advisory warnings for drift and
//! non-compliance only for real hazards (a committed secret, broken startup wiring, a private
//! memory that lost its origin, a failed authority or recall check). The hook-recall probe,
//! decision authority and node-map parity still run their PowerShell scripts
//! until their own checkpoints.
//!
//!   mdr-kernel doctor [-HardOnly] [-StaleDays 14] [-StateMaxLines 90] [-StateMaxKB 10]

use crate::args::Args;
use crate::git::{common_top, git, lossy, names, pwsh_command, text, top};
use crate::health::read_lines;
use crate::remote_privacy::{self, Live};
use regex::Regex;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;

pub struct Options {
    pub hard_only: bool,
    pub stale_days: i64,
    pub state_max_lines: i64,
    pub state_max_kb: i64,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            hard_only: false,
            stale_days: 14,
            state_max_lines: 90,
            state_max_kb: 10,
        }
    }
}

/// The tracked-file secret scan: the fast set plus generic credential assignments. PowerShell's
/// `-match` ignores case, so the whole pattern does.
const SECRETS: &str = r#"(?i)sk-ant-[A-Za-z0-9_-]{20,}|sk-(?:proj|svcacct)-[A-Za-z0-9_-]{20,}|sk-[A-Za-z0-9]{20,}|ghp_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{40,}|AKIA[0-9A-Z]{16}|AIza[0-9A-Za-z_-]{30,}|xox[baprs]-[A-Za-z0-9-]{10,}|-----BEGIN [A-Z ]*PRIVATE KEY-----|[\w-]*(api[_-]?key|secret|passwd|password|access[_-]?token)\b\s*[:=]\s*["'][^"']{8,}["']"#;
const SECRET_SKIP: [&str; 3] = [
    ".githooks/pre-commit",
    "bin/brain-doctor.ps1",
    "native/src/doctor.rs",
];
/// Files bootstrap scaffolds: absent on a pristine clone, so their absence points at bootstrap.
const SCAFFOLDED: [&str; 14] = [
    ".brain-root",
    "STATE.md",
    "DECISIONS.md",
    "handoffs/INDEX.md",
    "specs/INDEX.md",
    "reference/INDEX.md",
    "inbox/README.md",
    "protocol/recall-gates.json",
    "protocol/advisory.md",
    "CLAUDE.md",
    "AGENTS.md",
    "GEMINI.md",
    "protocol/decision-authority.json",
    "protocol/rulings.json",
];

struct Report {
    out: String,
    fail: u32,
}

impl Report {
    fn ok(&mut self, m: &str) {
        let _ = writeln!(self.out, "[ ok ] {m}");
    }
    fn warn(&mut self, m: &str) {
        let _ = writeln!(self.out, "[warn] {m}");
    }
    fn fail(&mut self, m: &str) {
        let _ = writeln!(self.out, "[non-compliance] {m}");
        self.fail += 1;
    }
    fn finish(mut self) -> (i32, String) {
        self.out.push('\n');
        if self.fail > 0 {
            let _ = writeln!(self.out, "doctor: {} non-compliance event(s).", self.fail);
            self.out.push_str("[ALERT]: MDR floor non-compliance detected. Correction is required before continuing.\n");
            return (1, self.out);
        }
        self.out.push_str("doctor: ok.\n");
        (0, self.out)
    }
}

/// `Norm` in the script: forward slashes, no trailing slash, lower case.
fn norm(s: &str) -> String {
    s.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

fn read_text(path: &Path) -> Option<String> {
    std::fs::read(path).ok().map(|b| {
        let b = b
            .strip_prefix(b"\xEF\xBB\xBF")
            .map(<[u8]>::to_vec)
            .unwrap_or(b);
        lossy(b)
    })
}

/// Combined stdout and stderr of a PowerShell child, with its exit code (`2>&1`).
fn pwsh_all(root: &Path, script: &str, args: &[&str], input: Option<&str>) -> (i32, Vec<String>) {
    let mut command = pwsh_command(root, script, args);
    command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = command.spawn().and_then(|mut child| {
        if let (Some(text), Some(mut stdin)) = (input, child.stdin.take()) {
            use std::io::Write;
            let _ = stdin.write_all(format!("{text}\n").as_bytes());
        }
        child.wait_with_output()
    });
    match output {
        Ok(o) => {
            let mut lines = read_lines(&o.stdout);
            lines.extend(read_lines(&o.stderr));
            (o.status.code().unwrap_or(1), lines)
        }
        Err(e) => (
            1,
            vec![format!("required utility 'pwsh' is unavailable ({e})")],
        ),
    }
}

/// Files `Get-ChildItem` lists without `-Force`: hidden entries are skipped.
fn visible(entry: &std::fs::DirEntry) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        entry
            .metadata()
            .is_ok_and(|m| m.file_attributes() & 0x2 == 0)
    }
    #[cfg(not(windows))]
    {
        !entry.file_name().to_string_lossy().starts_with('.')
    }
}

fn files_under(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten().filter(visible) {
        let path = entry.path();
        if path.is_dir() {
            files_under(&path, found);
        } else {
            found.push(path);
        }
    }
}

fn now_seconds() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

fn check_index(r: &mut Report, root: &Path, dir: &str, index: &str) {
    let dir_path = root.join(dir);
    if !dir_path.exists() {
        return; // an optional folder the memory does not use
    }
    let Some(listed) = read_text(&root.join(index)) else {
        r.warn(&format!("{index} missing"));
        return;
    };
    let listed_lower = listed.to_lowercase();
    let mut files: Vec<String> = std::fs::read_dir(&dir_path)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| visible(e) && e.path().is_file())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.to_lowercase().ends_with(".md") && n != "INDEX.md")
                .collect()
        })
        .unwrap_or_default();
    files.sort_by_key(|n| n.to_lowercase());
    let missing: Vec<&String> = files
        .iter()
        .filter(|n| !listed_lower.contains(&n.to_lowercase()))
        .collect();
    if missing.is_empty() {
        r.ok(&format!("{index} lists every file in {dir}/"));
    } else {
        let names: Vec<&str> = missing.iter().map(|s| s.as_str()).collect();
        r.warn(&format!("{index} missing rows for: {}", names.join(", ")));
    }
    let row = Regex::new(r"^\s*\|").unwrap();
    let edges = Regex::new(r"^\||\|$").unwrap();
    let md = Regex::new(r"(?i)^[\w.-]+\.md$").unwrap();
    let mut stale = Vec::new();
    for line in read_lines(listed.as_bytes()) {
        if !row.is_match(&line) {
            continue;
        }
        let cells: Vec<String> = edges
            .replace_all(line.trim(), "")
            .split('|')
            .map(str::to_string)
            .collect();
        if cells.len() < 2 {
            continue;
        }
        let name = cells[1].trim();
        if !md.is_match(name) || name.eq_ignore_ascii_case("INDEX.md") {
            continue;
        }
        let here = format!("{dir}/{name}");
        let archived = format!("{dir}/archive/{name}");
        if !root.join(&here).exists() && !root.join(&archived).exists() {
            stale.push(name.to_string());
            continue;
        }
        // A target only on disk breaks the pushed repo for fresh clones.
        if names(root, &["ls-files", "-z", "--", &here, &archived])
            .unwrap_or_default()
            .is_empty()
        {
            stale.push(format!("{name} (untracked)"));
        }
    }
    if stale.is_empty() {
        r.ok(&format!("{index} rows all resolve to tracked files"));
    } else {
        r.warn(&format!(
            "{index} has row(s) not resolving to tracked files: {}",
            stale.join(", ")
        ));
    }
}

pub fn run(root: &Path, o: &Options) -> (i32, String) {
    let mut r = Report {
        out: String::new(),
        fail: 0,
    };
    let shown = root.display();
    if o.hard_only {
        let _ = writeln!(
            r.out,
            "mdr-kernel doctor - {shown} (HardOnly: hazard gates only)\n"
        );
    } else {
        let _ = writeln!(r.out, "mdr-kernel doctor - {shown}\n");
    }
    if text(root, &["rev-parse", "--is-inside-work-tree"]).is_empty() {
        r.fail("this folder is not a git working tree - use git clone (preferred) or git init before running bootstrap/start-session");
        return r.finish();
    }
    let exists = |rel: &str| root.join(rel).exists();

    // 1) Canonical root (warning only). A linked worktree of the canonical repo shares its .git.
    let top_dir = text(root, &["rev-parse", "--show-toplevel"]);
    match read_text(&root.join(".brain-root")) {
        Some(marker) => {
            let want = norm(marker.trim());
            if want == norm(&top_dir) {
                r.ok(&format!("canonical root ({top_dir})"));
            } else if common_top(root).is_some_and(|c| norm(&c.to_string_lossy()) == want) {
                r.ok("canonical root - linked worktree");
            } else {
                r.warn(&format!("not in canonical root - here: {top_dir} ; canonical: {}. You may be in a mirror clone; avoid writing.", marker.trim()));
            }
        }
        None => r.warn(".brain-root marker missing - run bin/bootstrap-clone.ps1"),
    }

    // 2) Working tree: only uncommitted changes to tracked files block a clean report.
    if text(root, &["status", "--porcelain", "--untracked-files=no"]).is_empty() {
        r.ok("working tree clean (tracked files)");
    } else {
        r.warn("uncommitted changes to tracked files present (commit STATE/DECISIONS/handoff before stopping)");
    }

    // 3) STATE staleness.
    match text(root, &["log", "-1", "--format=%ct", "--", "STATE.md"]).parse::<i64>() {
        Ok(ct) => {
            let age = (now_seconds() as i64 - ct).div_euclid(86_400);
            if age <= o.stale_days {
                r.ok(&format!("STATE.md updated {age}d ago"));
            } else {
                r.warn(&format!(
                    "STATE.md not touched in {age}d (> {}) - is it still current?",
                    o.stale_days
                ));
            }
        }
        Err(_) => {
            r.warn("could not read STATE.md git history (new repo? commit your first STATE.md)")
        }
    }

    // 3b) STATE budget, byte-exact so it agrees with state-budget.
    let state_path = root.join("STATE.md");
    let state_raw = std::fs::read(&state_path).ok();
    match &state_raw {
        Some(raw) => {
            let lines = read_lines(raw).len() as i64;
            let bytes = raw.len() as i64;
            let (max_lines, max_bytes) = (o.state_max_lines, o.state_max_kb * 1024);
            if lines <= max_lines && bytes <= max_bytes {
                r.ok(&format!("STATE.md within budget ({lines}/{max_lines} lines, {bytes}/{max_bytes} bytes; headroom {} bytes / {} lines)", max_bytes - bytes, max_lines - lines));
            } else {
                r.warn(&format!("STATE.md over budget ({lines}/{max_lines} lines, {bytes}/{max_bytes} bytes) - run bin/state-budget.ps1 for the per-section/fattest-line breakdown; drop completed items into handoffs"));
            }
        }
        None => r.warn("STATE.md missing - first run: bin/setup-private.ps1 -Model '<your model id>'; repairing an already-set-up clone: bin/bootstrap-clone.ps1"),
    }

    // 4) Hooks armed for this clone: git does not version core.hooksPath.
    let hooks_path = text(root, &["config", "--get", "core.hooksPath"]);
    if hooks_path.eq_ignore_ascii_case(".githooks") {
        r.ok("git hooks armed (core.hooksPath=.githooks)");
    } else if !hooks_path.is_empty() {
        r.warn(&format!(
            "core.hooksPath is '{hooks_path}', expected .githooks - run bin/bootstrap-clone.ps1"
        ));
    } else {
        r.warn("core.hooksPath not set - run: git config core.hooksPath .githooks (or bin/bootstrap-clone.ps1)");
    }

    // 4a) Remote privacy: live visibility and the approved destination pin.
    if !exists("bin/remote-privacy.ps1") {
        r.warn("remote privacy verifier missing - restore bin/remote-privacy.ps1");
    } else {
        let v = remote_privacy::run(root, None, false, true, false, &Live);
        match v.status {
            "PRIVATE" | "LOCAL_ONLY" => r.ok(&format!("remote privacy: {}", v.message)),
            // A private-remote memory without origin silently stops backing up: a hazard.
            "MISSING_ORIGIN" => r.fail(&format!("remote privacy: {}", v.message)),
            status => r.warn(&format!(
                "remote privacy NOT READY ({status}): {}",
                v.message
            )),
        }
    }

    // 4aa) A deliberately offline close must be impossible to forget.
    let pending = text(root, &["config", "--local", "--get", "brain.pendingSync"]);
    let ahead = git(
        root,
        &[
            "show-ref",
            "--verify",
            "--quiet",
            "refs/remotes/origin/main",
        ],
    )
    .ok()
    .and_then(|_| {
        text(root, &["rev-list", "--count", "origin/main..HEAD"])
            .parse::<u64>()
            .ok()
    });
    if pending.eq_ignore_ascii_case("true") || ahead.is_some_and(|a| a > 0) {
        let count = ahead.map_or("local checkpoint pending".to_string(), |a| {
            format!("{a} local commit(s) ahead of origin/main")
        });
        r.warn(&format!(
            "PENDING PRIVATE SYNC: {count}. Run: pwsh -NoProfile -File bin/sync-private.ps1"
        ));
    } else if ahead.is_some() {
        r.ok("private sync: no local commits pending");
    }

    // 4b) Identity: the process value is authoritative; git config is a display fallback.
    let id_env = std::env::var("BRAIN_MODEL").unwrap_or_default();
    let id_cfg = text(root, &["config", "brain.model"]);
    if !id_env.is_empty() {
        r.ok(&format!(
            "Brain-Model identity = {id_env} (from $BRAIN_MODEL)"
        ));
    } else if !id_cfg.is_empty() {
        r.warn(&format!("Brain-Model fallback = {id_cfg} (git config brain.model): display only; locks and commits require an explicit session identity. Pass -Model or set $BRAIN_MODEL."));
    } else {
        r.warn(
            "Brain-Model identity unset - set $BRAIN_MODEL for this process before lock/commit.",
        );
    }

    // 5) Doors in sync with the contract (advisory; pre-commit still blocks a drifted door).
    if !o.hard_only {
        if exists("bin/sync-doors.ps1") {
            let doors = crate::doors::sync(root, true, false);
            let (code, lines) = (doors.code, read_lines((doors.out + &doors.err).as_bytes()));
            if code == 0 {
                r.ok("front doors in sync with protocol/CONTRACT.md");
            } else {
                let missing: Vec<&str> = [
                    "CLAUDE.md",
                    "AGENTS.md",
                    "GEMINI.md",
                    "protocol/advisory.md",
                ]
                .into_iter()
                .filter(|d| !exists(d))
                .collect();
                if !missing.is_empty() {
                    r.warn(&format!("front doors not generated yet ({}) - first run: bin/setup-private.ps1; repair: bin/sync-doors.ps1", missing.join(", ")));
                } else if !exists("STATE.md") {
                    r.warn("phase-zero front doors present (setup has not run) - run bin/setup-private.ps1 -Model '<your model id>' to generate the full contract doors");
                } else {
                    r.warn("front doors drifted from CONTRACT - run: pwsh -NoProfile -File bin/sync-doors.ps1");
                }
            }
            for line in lines
                .iter()
                .filter(|l| l.to_lowercase().starts_with("warn:"))
            {
                r.warn(line);
            }
        } else {
            r.warn("bin/sync-doors.ps1 missing");
        }
    }

    // 5b) Canonical startup wiring: the core scripts are a hard block.
    let contract = read_text(&root.join("protocol/CONTRACT.md"));
    let mut problems = Vec::new();
    if !exists("bin/start-session.ps1") {
        problems.push("bin/start-session.ps1 missing");
    }
    if !exists("bin/session-stop.ps1") {
        problems.push("bin/session-stop.ps1 missing");
    }
    match &contract {
        Some(c) if !c.to_lowercase().contains("bin/start-session.ps1") => {
            problems.push("CONTRACT does not route startup through bin/start-session.ps1")
        }
        None => problems.push("protocol/CONTRACT.md missing"),
        _ => {}
    }
    if problems.is_empty() {
        r.ok("canonical startup wiring present (start-session + session-stop + contract)");
    } else {
        r.fail(&format!(
            "canonical startup wiring broken: {}",
            problems.join("; ")
        ));
    }
    for hook in ["pre-commit", "prepare-commit-msg", "pre-push"] {
        let path = root.join(".githooks").join(hook);
        if !path.is_file() {
            r.fail(&format!("required Git hook missing: {hook}"));
            continue;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).map_or(0, |m| m.permissions().mode());
            if mode & 0o111 == 0 {
                r.fail(&format!(
                    "Git hook is not executable: {hook} (chmod +x .githooks/{hook})"
                ));
            }
        }
    }
    for helper in [
        "brain-recall",
        "hook-recall",
        "prepare-work",
        "check-decision-history",
        "build-rulings",
        "test-staged",
    ] {
        if !exists(&format!("bin/{helper}.ps1")) {
            r.fail(&format!("required safeguard missing: bin/{helper}.ps1"));
        }
    }
    if exists("bin/brain-recall.ps1") && exists("protocol/recall-gates.json") {
        let gates = crate::recall::check_gates(&root.join("protocol/recall-gates.json"));
        let (code, lines) = (gates.code, read_lines((gates.out + &gates.err).as_bytes()));
        if code != 0 {
            r.fail(&format!("recall schema invalid: {}", lines.join(" ")));
        } else {
            r.ok("recall schema valid");
        }
    }
    // build-rulings -Check runs the decision-history check first and fails on either.
    if exists("DECISIONS.md") && exists("bin/build-rulings.ps1") {
        let (code, lines) = pwsh_all(root, "build-rulings.ps1", &["-Check"], None);
        let ansi = Regex::new(r"\x1b\[[0-9;]*m").unwrap();
        let result = ansi.replace_all(&lines.join(" "), "").into_owned();
        if code != 0 {
            r.fail(&format!("decision authority: {result}"));
        } else {
            r.ok(&format!("decision authority: {result}"));
        }
    }

    if !o.hard_only {
        advisory_wiring(&mut r, root, contract.as_deref());
    }

    // 7) Secret scan over tracked files (a hazard).
    let secret = Regex::new(SECRETS).unwrap();
    let allow = Regex::new("(?i)secret-audit:allow").unwrap();
    let mut hits = Vec::new();
    for f in names(root, &["ls-files", "-z"]).unwrap_or_default() {
        if SECRET_SKIP.iter().any(|s| s.eq_ignore_ascii_case(&f)) {
            continue;
        }
        let Ok(bytes) = std::fs::read(root.join(&f)) else {
            continue;
        };
        for (n, line) in read_lines(&bytes).iter().enumerate() {
            if !allow.is_match(line) && secret.is_match(line) {
                hits.push(format!("{f}:{}", n + 1));
            }
        }
    }
    if hits.is_empty() {
        r.ok("no secret-shaped strings in tracked files");
    } else {
        r.fail(&format!("possible secret(s) in: {}", hits.join(", ")));
    }

    if !o.hard_only {
        advisory_memory(&mut r, root, &top_dir, state_raw.as_deref());
    }
    r.finish()
}

/// Recall hook configuration, the node map, envelope lengths and the folder indexes.
fn advisory_wiring(r: &mut Report, root: &Path, contract: Option<&str>) {
    let mut adapter = false;
    for config in [".claude/settings.json", ".codex/hooks.json"] {
        let Some(raw) = read_text(&root.join(config)) else {
            continue;
        };
        match serde_json::from_str::<serde_json::Value>(&raw) {
            Ok(cfg) => {
                let hook = &cfg["hooks"]["UserPromptSubmit"];
                let configured = match hook {
                    serde_json::Value::Null | serde_json::Value::Bool(false) => false,
                    serde_json::Value::Array(a) => !a.is_empty(),
                    serde_json::Value::String(s) => !s.is_empty(),
                    _ => true,
                };
                if configured && raw.to_lowercase().contains("hook-recall.ps1") {
                    adapter = true;
                    r.ok(&format!("prompt recall CONFIGURED in {config}"));
                }
            }
            Err(_) => r.warn(&format!("invalid host hook config: {config}")),
        }
    }
    if !adapter {
        r.warn("prompt recall not configured; run bin/prepare-work.ps1 for each objective. See docs/recall-hooks.md.");
    } else {
        // Configured (above), adapter tested (this probe), fired (never observable here).
        let (code, lines) = pwsh_all(
            root,
            "hook-recall.ps1",
            &[],
            Some(r#"{"prompt":"mdr doctor adapter probe"}"#),
        );
        let probe = lines.join(" ");
        if code == 0 && !probe.to_uppercase().contains("RECALL_HOOK_STATUS=DEGRADED") {
            r.ok("prompt recall adapter TESTED: bin/hook-recall.ps1 answered a probe prompt");
        } else {
            r.warn(&format!(
                "prompt recall adapter DEGRADED on a probe prompt: {probe}"
            ));
        }
        r.warn("prompt recall FIRING UNVERIFIED: configuration and the adapter probe do not prove your host runs the hook; see docs/recall-hooks.md");
    }

    // 5d) The generated node map checks link health.
    let mut problems = Vec::new();
    if !root.join("bin/update-node-map.ps1").exists() {
        problems.push("bin/update-node-map.ps1 missing");
    }
    if !root.join("maps/node-roots.json").exists() {
        problems.push("maps/node-roots.json missing");
    }
    if let Some(c) = contract {
        let c = c.to_lowercase();
        if !c.contains("maps/node-map.md") && !c.contains("maps/node-map.json") {
            problems.push("CONTRACT does not describe the generated link-health map");
        }
    }
    let json = root.join("maps/node-map.json");
    if !problems.is_empty() {
        r.warn(&format!(
            "node map subsystem incomplete: {}",
            problems.join("; ")
        ));
    } else if !json.exists() || !root.join("maps/NODE-MAP.md").exists() {
        r.warn("node map missing - run: pwsh -NoProfile -File bin/update-node-map.ps1");
    } else {
        match read_text(&json).and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) {
            Some(map) => {
                let files = map["files"].as_array().cloned().unwrap_or_default();
                let with_issues = files
                    .iter()
                    .filter(|f| f["issues"].as_array().is_some_and(|i| !i.is_empty()))
                    .count();
                let (code, _) = pwsh_all(root, "update-node-map.ps1", &["-Check"], None);
                if code != 0 {
                    r.warn("node map differs from tracked Markdown or generated bridge output - run: pwsh -NoProfile -File bin/update-node-map.ps1");
                } else {
                    r.ok(&format!("node map exact parity ({} tracked files; {with_issues} file(s) with active issues)", files.len()));
                }
            }
            None => r.warn(
                "node map unreadable - regenerate with: pwsh -NoProfile -File bin/update-node-map.ps1",
            ),
        }
    }

    // 5e) Envelope length: pre-commit ratchets touched files; this counts the rest.
    let armed = root.join("bin/envelope-check.ps1").exists()
        && read_text(&root.join(".githooks/pre-commit")).is_some_and(|t| {
            t.to_lowercase()
                .contains(r#"bin/envelope-check.ps1" -staged"#)
        });
    if !armed {
        r.warn("envelope length ratchet UNARMED: .githooks/pre-commit does not run bin/envelope-check.ps1 -Staged");
    } else {
        match crate::envelope::count(root) {
            Ok((over, tracked)) => {
                let line = format!("ENVELOPE_LENGTH over={} tracked={tracked}", over.len());
                if over.is_empty() {
                    r.ok(&format!(
                        "{line} (Title <= 100, Context <= 300 chars / 1 sentence)"
                    ));
                } else {
                    r.warn(&format!("{line} - fix with: pwsh -NoProfile -File bin/envelope-check.ps1 -Count -List (pre-commit enforces on touch)"));
                }
            }
            Err(_) => r.warn("envelope length check failed to run (bin/envelope-check.ps1)"),
        }
    }

    // 6) Indexes match folders: every .md has a row, and every row names a real tracked file.
    check_index(r, root, "specs", "specs/INDEX.md");
    check_index(r, root, "handoffs", "handoffs/INDEX.md");
    check_index(r, root, "reference", "reference/INDEX.md");
}

/// Links, reference index, inbox, coordination list, lock, worktrees and the rules with teeth.
fn advisory_memory(r: &mut Report, root: &Path, top_dir: &str, state: Option<&[u8]>) {
    // 8) Wiki-links resolve to reference/ files. Kebab-case slugs only.
    const IGNORE: [&str; 8] = [
        "links",
        "link",
        "wikilinks",
        "wiki-link",
        "wiki-links",
        "slug",
        "name",
        "their-name",
    ];
    let wiki = Regex::new(r"\[\[([a-z0-9][a-z0-9-]*)\]\]").unwrap();
    let mut seen = std::collections::HashSet::new();
    let mut unresolved = Vec::new();
    for f in names(root, &["ls-files", "-z", "*.md"]).unwrap_or_default() {
        let Some(body) = read_text(&root.join(&f)) else {
            continue;
        };
        for c in wiki.captures_iter(&body) {
            let slug = c[1].to_string();
            if IGNORE.contains(&slug.as_str()) || !seen.insert(slug.clone()) {
                continue;
            }
            if !root.join(format!("reference/{slug}.md")).exists() {
                unresolved.push(slug);
            }
        }
    }
    if unresolved.is_empty() {
        r.ok("all [[wiki-links]] resolve to reference/ files");
    } else {
        r.warn(&format!(
            "unresolved [[links]] (no reference/<slug>.md): {}",
            unresolved.join(", ")
        ));
    }

    // 9) reference/ index present (optional folder).
    if root.join("reference").exists() {
        if root.join("reference/INDEX.md").exists() {
            r.ok("reference/ + INDEX present");
        } else {
            r.warn("reference/INDEX.md missing");
        }
    }

    // 10) inbox/ notes waiting to be folded into STATE (optional folder).
    let inbox = root.join("inbox");
    if inbox.exists() {
        let mut files = Vec::new();
        files_under(&inbox, &mut files);
        let notes = files
            .iter()
            .filter(|f| {
                let n = f.file_name().map(|n| n.to_string_lossy().to_lowercase());
                !matches!(
                    n.as_deref(),
                    Some("readme.md" | "write-approval-audit.jsonl")
                )
            })
            .count();
        if notes == 0 {
            r.ok("inbox/ empty");
        } else {
            r.warn(&format!(
                "inbox/: {notes} note(s) waiting - fold into STATE and clear"
            ));
        }
    }

    // 11) The single-writer list names real files (catches renames that silently unprotect).
    match read_text(&root.join("protocol/coordination-files.txt")) {
        Some(list) => {
            let entries: Vec<String> = read_lines(list.as_bytes())
                .iter()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .collect();
            let missing: Vec<&String> = entries
                .iter()
                .filter(|e| *e != "protocol/decision-ratifications.jsonl" && !root.join(e).exists())
                .collect();
            let real: Vec<&str> = missing
                .iter()
                .filter(|m| !SCAFFOLDED.iter().any(|s| s.eq_ignore_ascii_case(m)))
                .map(|m| m.as_str())
                .collect();
            if missing.is_empty() {
                r.ok(&format!(
                    "coordination-files.txt: all {} protected paths exist",
                    entries.len()
                ));
            } else if real.is_empty() {
                r.warn(&format!("coordination-files.txt lists {} not-yet-scaffolded file(s) - run bin/bootstrap-clone.ps1", missing.len()));
            } else {
                r.warn(&format!(
                    "coordination-files.txt names missing file(s): {} - fix the list or restore the file",
                    real.join(", ")
                ));
            }
        }
        None => r.warn("protocol/coordination-files.txt missing - pre-commit lock gate falls back to a minimal built-in list"),
    }

    // 12) STATE.lock: the ONE shared lock, holder and heartbeat age.
    let lock = common_top(root)
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| root.to_path_buf())
        .join("STATE.lock");
    if lock.exists() {
        let raw = read_text(&lock).unwrap_or_default().trim().to_string();
        match serde_json::from_str::<serde_json::Value>(&raw)
            .ok()
            .filter(|v| v.is_object())
        {
            Some(l) => {
                let field = |k: &str| l[k].as_str().unwrap_or("").to_string();
                let hb = Some(field("heartbeat_at"))
                    .filter(|h| !h.is_empty())
                    .unwrap_or_else(|| field("acquired_at"));
                // [math]::Round rounds half to even.
                let age = crate::state_lock::parse_iso(&hb)
                    .map(|t| ((now_seconds() - t) / 60.0).round_ties_even() as i64);
                let objective = Some(field("objective"))
                    .filter(|o| !o.is_empty())
                    .unwrap_or_else(|| "MISSING".into());
                let who = format!(
                    "{} on {} (objective: {objective})",
                    field("model"),
                    field("host")
                );
                let shown = age.map(|a| a.to_string()).unwrap_or_default();
                if age.is_some_and(|a| a > 120) {
                    r.warn(&format!("STATE.lock heartbeat {shown}m old - stale, not proof of abandonment: {who}. Verify your own token or obtain independent recovery evidence."));
                } else {
                    r.warn(&format!("STATE.lock held (heartbeat {shown}m ago): {who} - coordination files are single-writer; do not edit them unless this is your lock"));
                }
            }
            None => r.warn(&format!(
                "STATE.lock held (unstructured): {raw} - single-writer coordination applies"
            )),
        }
    } else {
        r.ok("STATE.lock free");
    }

    // 12b) Linked worktrees: dead harness sessions linger as full repo copies.
    let main = norm(top_dir);
    for line in text(root, &["worktree", "list", "--porcelain"]).lines() {
        let Some(path) = line.strip_prefix("worktree ") else {
            continue;
        };
        if norm(path) == main {
            continue;
        }
        let idle = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .map(|d| (d.as_secs_f64() / 3600.0).round_ties_even() as i64);
        if idle.is_some_and(|h| h <= 48) {
            r.ok(&format!(
                "linked worktree present (recent - likely a live session): {path}"
            ));
        } else {
            let hours = idle.map(|h| h.to_string()).unwrap_or_default();
            r.warn(&format!("linked worktree idle; verify ownership before cleanup: {path} (idle {hours}h) - verify clean, then: git worktree remove '{path}'"));
        }
    }

    // 12c) Rules with teeth: prose rules about coordination files drift unless a check holds them.
    let state = state.map(|s| lossy(s.to_vec())).unwrap_or_default();
    if !state.is_empty() {
        let missing: Vec<&str> = ["## Right now", "## Active work", "## Next actions"]
            .into_iter()
            .filter(|s| {
                !Regex::new(&format!("(?im)^{}", regex::escape(s)))
                    .unwrap()
                    .is_match(&state)
            })
            .collect();
        if missing.is_empty() {
            r.ok("STATE.md carries its contract sections (Right now / Active work / Next actions)");
        } else {
            r.warn(&format!(
                "STATE.md missing section header(s): {} - the 30-second scan shape relies on them",
                missing.join(", ")
            ));
        }
    }
    if let Some(decisions) = read_text(&root.join("DECISIONS.md")) {
        let numbers: Vec<u64> = Regex::new(r"(?m)^## D(\d+)\b")
            .unwrap()
            .captures_iter(&decisions)
            .filter_map(|c| c[1].parse().ok())
            .collect();
        if numbers.len() >= 2 {
            let max = numbers.iter().max().copied().unwrap_or(0);
            if numbers[0] == max {
                r.ok(&format!(
                    "DECISIONS.md newest-on-top holds (top = D{})",
                    numbers[0]
                ));
            } else {
                r.warn(&format!("DECISIONS.md newest-on-top violated: top entry is D{} but newest is D{max} - new entries go at the TOP", numbers[0]));
            }
        }
    }
    let row_date = Regex::new(r"^\| (\d{4}-\d{2}-\d{2}) \|").unwrap();
    let dates: Vec<String> = read_text(&root.join("handoffs/INDEX.md"))
        .map(|t| {
            read_lines(t.as_bytes())
                .iter()
                .filter_map(|l| row_date.captures(l).map(|c| c[1].to_string()))
                .collect()
        })
        .unwrap_or_default();
    if dates.len() >= 2 {
        let max = dates.iter().max().cloned().unwrap_or_default();
        if dates[0] == max {
            r.ok(&format!(
                "handoffs/INDEX.md newest-first holds (top row {})",
                dates[0]
            ));
        } else {
            r.warn(&format!("handoffs/INDEX.md newest-first violated: top row is {} but newest is {max} - new rows go at the TOP of the table", dates[0]));
        }
    }
}

pub fn cli(argv: &[String]) -> i32 {
    let result = (|| -> Result<i32, String> {
        let args = Args::parse(
            argv,
            &[
                "StaleDays",
                "StateMaxLines",
                "StateMaxKB",
                "NodeMapStaleDays",
                "Root",
            ],
            &["HardOnly"],
        )?;
        let number = |name: &str, default: i64| -> Result<i64, String> {
            args.value(name).map_or(Ok(default), |v| {
                v.trim()
                    .parse()
                    .map_err(|_| format!("-{name} needs a whole number, not '{v}'"))
            })
        };
        let options = Options {
            hard_only: args.switch("HardOnly"),
            stale_days: number("StaleDays", 14)?,
            state_max_lines: number("StateMaxLines", 90)?,
            state_max_kb: number("StateMaxKB", 10)?,
        };
        number("NodeMapStaleDays", 7)?; // kept for CLI compatibility; exact parity supersedes age
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        let root = args
            .value("Root")
            .map_or_else(|| top(&cwd), |r| Ok(PathBuf::from(r)))
            .map(|r| crate::args::absolute(&r))?;
        crate::hook::fresh_in_repo(&root)?;
        let (code, out) = run(&root, &options);
        print!("{out}");
        Ok(code)
    })();
    result.unwrap_or_else(|e| {
        eprintln!("X mdr-kernel doctor: {e}");
        1
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_scan_matches_like_powershell() {
        let secret = Regex::new(SECRETS).unwrap();
        assert!(secret.is_match(&format!("key AKIA{}", "Q".repeat(16))));
        assert!(secret.is_match(&format!("akia{}", "q".repeat(16))));
        // Built at run time, so this source file never looks like a committed credential.
        let assignment = |value: &str| format!("api_{}ey = \"{value}\"", "k");
        assert!(secret.is_match(&assignment("abcdefgh12")));
        assert!(!secret.is_match(&assignment("short")));
        assert!(!secret.is_match("secret-audit:allow is a marker"));
    }

    #[test]
    fn norm_matches_the_script() {
        assert_eq!(norm(r"C:\Users\X\repo\"), "c:/users/x/repo");
    }
}
