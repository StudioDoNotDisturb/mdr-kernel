//! First run and repair. Ports of `bin/setup-private.ps1` (verify and pin the destination, then
//! bootstrap and push the first checkpoint) and `bin/bootstrap-clone.ps1` (arm hooks, scaffold
//! the grow-your-own files under STATE.lock, optionally commit them, run the doctor). Decision
//! rulings and node-map generation still run their PowerShell scripts until checkpoint 6.
//!
//!   mdr-kernel setup-private -Model <id> [-LocalOnly | -ConfirmPrivateRemote]
//!   mdr-kernel bootstrap-clone -Model <id> [-CommitScaffold]

use crate::args::Args;
use crate::git::{lossy, names, pwsh_command, text, top};
use crate::hook::lock_port;
use crate::remote_privacy::{self, Live};
use regex::Regex;
use std::path::{Path, PathBuf};
use std::process::Command;

const STATE: &str = "Title: STATE - the current state of play
Type: Live State / Coordination File
Context: Live coordination snapshot; when status conflicts, this file wins over older docs.

# STATE - the current state of play

> Read this first, keep it current. If anything else disagrees with this file, this file wins -
> then go fix the other doc. Keep it scannable in ~30s and within the doctor budget (90 lines/10 KB).

## Right now

Fresh mdr-kernel clone. Replace this with your real current state.

## Active work

_Nothing yet._

## Blockers

_None._

## Next actions

- Replace this scaffold with your real state. Adopting existing work? Follow
  docs/adoption-guide.md: inventory first, then file each item deliberately.

---

## SYSTEM NODE LINKS

Directory Path: ./
Next Sequential File: DECISIONS.md
Related Workflows: DECISIONS.md, handoffs/INDEX.md
AI Execution Prompts: protocol/CONTRACT.md";

const DECISIONS: &str = "Title: Decisions
Type: Decision Log / Coordination File
Context: Append-only decision log; newer entries supersede older ones and explain why.

# Decisions

Append-only. Newest at the top. Don't edit old entries - if a decision changes, add a new one
with `bin/add-decision.ps1` and an explicit supersession field. Each entry answers: what, why,
what it rules out. A proposal does not bind; an operator ruling includes the operator's words.

---

_No decisions yet._

---

## SYSTEM NODE LINKS

Directory Path: ./
Next Sequential File: STATE.md
Related Workflows: STATE.md, handoffs/INDEX.md
AI Execution Prompts: protocol/CONTRACT.md";

const HANDOFFS: &str = "Title: Handoffs index
Type: Handoffs Index / Session Registry
Context: Registry of session handoffs; use the newest relevant handoff for detail after STATE.md.

# Handoffs index

One line per session handoff, newest first.

| Date | File | What it covers | Status |
|---|---|---|---|

---

## SYSTEM NODE LINKS

Directory Path: handoffs/
Next Sequential File: STATE.md
Related Workflows: STATE.md, DECISIONS.md
AI Execution Prompts: protocol/CONTRACT.md";

const REFERENCE: &str = "Title: Reference index
Type: Reference Index / Knowledge Registry
Context: Registry of durable reference notes; the shared facts other sessions rely on.

# Reference index

One row per reference note. [[wiki-links]] elsewhere resolve to reference/<slug>.md.

| Note | File | What it covers |
|---|---|---|

---

## SYSTEM NODE LINKS

Directory Path: reference/
Next Sequential File: STATE.md
Related Workflows: STATE.md
AI Execution Prompts: protocol/CONTRACT.md";

const SPECS: &str = "Title: Specs index
Type: Specs Index / Authority Registry
Context: Registry of specs with current status and update dates.

# Specs index

One file per topic. Superseded versions go in archive/.

| Spec | File | Status | Updated |
|---|---|---|---|
| Node map | node-map.md | active | 2026-07-06 |

---

## SYSTEM NODE LINKS

Directory Path: specs/
Next Sequential File: STATE.md
Related Workflows: STATE.md
AI Execution Prompts: protocol/CONTRACT.md";

const INBOX: &str = "Title: inbox
Type: Inbox / Staging Area
Context: Background/advisory output lands here as atomic notes until one session folds it into STATE.

# inbox

Drop atomic notes here instead of editing STATE.md directly. One session folds them in later.
Delete a note once it is folded. This README is ignored by the fold check.

---

## SYSTEM NODE LINKS

Directory Path: inbox/
Next Sequential File: STATE.md
Related Workflows: STATE.md
AI Execution Prompts: protocol/CONTRACT.md";

/// Everything bootstrap may stage; anything else in the index is someone else's work.
const SCAFFOLD: [&str; 16] = [
    "STATE.md",
    "DECISIONS.md",
    "handoffs/INDEX.md",
    "reference/INDEX.md",
    "specs/INDEX.md",
    "inbox/README.md",
    "protocol/recall-gates.json",
    "CLAUDE.md",
    "AGENTS.md",
    "GEMINI.md",
    "protocol/advisory.md",
    "protocol/decision-authority.json",
    "protocol/rulings.json",
    "maps/node-map.json",
    "maps/NODE-MAP.md",
    "maps/OBSIDIAN-NODE-LINKS.md",
];

fn ok(m: &str) {
    println!("[ ok ] {m}");
}

fn warn(m: &str) {
    println!("[warn] {m}");
}

fn step(m: &str) {
    println!("\n== {m} ==");
}

fn indent(text: &str) {
    text.lines().for_each(|l| println!("  {l}"));
}

fn git_status(root: &Path, args: &[&str]) -> i32 {
    Command::new("git")
        .current_dir(root)
        .args(args)
        .status()
        .map_or(1, |s| s.code().unwrap_or(1))
}

/// A PowerShell child whose stdout is indented under the step (stderr passes through).
fn pwsh_indented(root: &Path, script: &str) -> i32 {
    match pwsh_command(root, script, &[])
        .stderr(std::process::Stdio::inherit())
        .output()
    {
        Ok(o) => {
            indent(&lossy(o.stdout));
            o.status.code().unwrap_or(1)
        }
        Err(e) => {
            eprintln!("required utility 'pwsh' is unavailable ({e})");
            1
        }
    }
}

fn new_if_missing(root: &Path, rel: &str, content: &str) -> Result<(), String> {
    let path = root.join(rel);
    if path.exists() {
        ok(&format!("exists: {rel}"));
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{rel}: {e}"))?;
    }
    std::fs::write(&path, content).map_err(|e| format!("{rel}: {e}"))?;
    ok(&format!("created: {rel}"));
    Ok(())
}

fn node_map(root: &Path, required: bool) -> Result<(), String> {
    if !root.join("bin/update-node-map.ps1").exists() {
        if required {
            return Err("bin/update-node-map.ps1 missing - cannot generate node map".into());
        }
        warn("bin/update-node-map.ps1 missing - node map not generated");
        return Ok(());
    }
    match pwsh_indented(root, "update-node-map.ps1") {
        0 => ok("node map generated (maps/NODE-MAP.md + maps/node-map.json)"),
        code if required => {
            return Err(format!("node map generation failed with exit code {code}"))
        }
        code => warn(&format!("node map generation failed with exit code {code}")),
    }
    Ok(())
}

/// Bootstrap. `Err` is the script's `throw`: the message, exit 1.
pub fn bootstrap(root: &Path, model: Option<&str>, commit: bool) -> Result<i32, String> {
    println!("mdr-kernel bootstrap - {}", root.display());
    if text(root, &["rev-parse", "--is-inside-work-tree"]).is_empty() {
        return Err("This folder is not a git working tree. ZIP/no-git copies are unsupported for bootstrap; use git clone (preferred) or git init first.".into());
    }
    // Explicit identity and sealed ownership before the first write; shared config never counts.
    let current = model
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .map(str::to_string)
        .or_else(|| {
            std::env::var("BRAIN_MODEL")
                .ok()
                .map(|m| m.trim().to_string())
                .filter(|m| !m.is_empty())
        })
        .ok_or("Bootstrap requires explicit -Model or process BRAIN_MODEL before writing; git config brain.model is display-only.")?;
    std::env::set_var("BRAIN_MODEL", &current);
    let pre_staged = names(root, &["diff", "--cached", "--name-only", "-z"]).unwrap_or_default();
    if !pre_staged.is_empty() {
        return Err(format!(
            "Bootstrap refuses a nonempty index; commit or unstage your own work first: {}",
            pre_staged.join(", ")
        ));
    }
    let original_token = std::env::var("BRAIN_LOCK_TOKEN").ok();
    let owned_before = original_token.as_deref().is_some_and(|t| !t.is_empty())
        && lock_port(root, &["-CheckOwner", "-Model", &current]).is_ok_and(|r| r.code == 0);
    let acquired = lock_port(
        root,
        &[
            "-Acquire",
            "-Model",
            &current,
            "-Objective",
            "initialize or repair mdr-kernel memory scaffold",
        ],
    )?;
    print!("{}", acquired.out);
    if acquired.code != 0 {
        return Err("could not acquire STATE.lock before bootstrap writes".into());
    }
    let token = Regex::new(r"(?m)^session token: ([a-f0-9]+)$").unwrap();
    let tokens: Vec<String> = token
        .captures_iter(&acquired.out)
        .map(|c| c[1].to_string())
        .collect();
    // Acquire prints no token when it sealed the caller's BRAIN_LOCK_TOKEN into a free lock.
    if tokens.len() == 1 {
        std::env::set_var("BRAIN_LOCK_TOKEN", &tokens[0]);
    } else if !owned_before && std::env::var("BRAIN_LOCK_TOKEN").map_or(true, |t| t.is_empty()) {
        return Err(
            "Acquire did not return a session token; no bootstrap writes performed.".into(),
        );
    }
    let proof = lock_port(root, &["-CheckOwner", "-Model", &current])?;
    print!("{}", proof.out);
    if proof.code != 0 {
        return Err("Bootstrap ownership proof failed; no bootstrap writes performed.".into());
    }
    let result = bootstrap_steps(root, model.unwrap_or(""), &current, commit);
    // A caller that already owned this lock keeps it; release only what this run acquired.
    if !owned_before {
        match lock_port(root, &["-Release", "-Model", &current]) {
            Ok(r) => {
                indent(&r.out);
                if r.code != 0 {
                    println!("WARNING: Bootstrap lock release failed; inspect ownership before recovery.");
                }
            }
            Err(e) => {
                println!("  {e}");
                println!(
                    "WARNING: Bootstrap lock release failed; inspect ownership before recovery."
                );
            }
        }
    }
    match original_token {
        Some(t) => std::env::set_var("BRAIN_LOCK_TOKEN", t),
        None => std::env::remove_var("BRAIN_LOCK_TOKEN"),
    }
    result
}

fn bootstrap_steps(root: &Path, model: &str, current: &str, commit: bool) -> Result<i32, String> {
    // 1) Arm hooks: git does not version core.hooksPath, so a fresh clone's hooks are inactive.
    step("arm git hooks");
    if text(root, &["config", "--get", "core.hooksPath"]).eq_ignore_ascii_case(".githooks") {
        ok("hooks already armed (core.hooksPath=.githooks)");
    } else if git_status(root, &["config", "core.hooksPath", ".githooks"]) == 0 {
        ok("armed hooks (set core.hooksPath=.githooks)");
    } else {
        warn("could not set core.hooksPath - run manually: git config core.hooksPath .githooks");
    }

    // 1b) A remote-backed clone must go through setup-private before it grows memory.
    step("remote privacy check");
    if text(root, &["remote", "get-url", "origin"]).is_empty() {
        ok("no origin remote - local-only clone (wire a PRIVATE repo you own when ready)");
    } else {
        let v = remote_privacy::run(root, None, false, true, false, &Live);
        print!("{}", remote_privacy::report(&v));
        if v.code != 0 {
            return Err("remote privacy is not verified and approved - run the adopter entry point: pwsh -NoProfile -File bin/setup-private.ps1 -Model '<your model id>' (example: -Model 'claude-opus-5')".into());
        }
        ok("private remote verified and approved");
    }

    // 2) The grow-your-own files the contract tells you to create: minimal, valid starters.
    step("scaffold memory files");
    let top_dir = text(root, &["rev-parse", "--show-toplevel"]);
    if !top_dir.is_empty() {
        new_if_missing(root, ".brain-root", &format!("{top_dir}\n"))?;
    }
    for (rel, content) in [
        ("STATE.md", STATE),
        ("DECISIONS.md", DECISIONS),
        ("handoffs/INDEX.md", HANDOFFS),
        ("reference/INDEX.md", REFERENCE),
        ("specs/INDEX.md", SPECS),
        ("inbox/README.md", INBOX),
    ] {
        new_if_missing(root, rel, content)?;
    }
    let recall = root.join("protocol/recall-gates.json");
    let example = root.join("protocol/recall-gates.example.json");
    if recall.exists() {
        ok("exists: protocol/recall-gates.json");
    } else if example.exists() {
        std::fs::copy(&example, &recall).map_err(|e| format!("protocol/recall-gates.json: {e}"))?;
        ok("created: protocol/recall-gates.json (from example)");
    } else {
        warn("protocol/recall-gates.example.json missing - cannot seed recall gates");
    }
    // An empty new log has no legacy to classify. Never infer authority for an existing log.
    let authority = root.join("protocol/decision-authority.json");
    if !authority.exists() {
        let log = std::fs::read(root.join("DECISIONS.md"))
            .map(lossy)
            .map_err(|e| format!("DECISIONS.md: {e}"))?;
        if Regex::new(r"(?im)^## D\d+\b").unwrap().is_match(&log) {
            warn("Existing decision history needs explicit migration: see docs/decision-authority.md.");
        } else {
            new_if_missing(
                root,
                "protocol/decision-authority.json",
                r#"{"schema_version":1,"authority_from":1}"#,
            )?;
        }
    }
    if authority.exists() {
        let built = pwsh_command(root, "build-rulings.ps1", &[])
            .status()
            .is_ok_and(|s| s.success());
        if !built {
            return Err("Could not compile decision rulings.".into());
        }
    }

    // 2b) The model front doors from the contract.
    step("generate front doors");
    if root.join("bin/sync-doors.ps1").exists() {
        let doors = crate::doors::sync(root, false, false);
        indent(&doors.out);
        eprint!("{}", doors.err);
        ok("front doors generated (CLAUDE.md / AGENTS.md / GEMINI.md / protocol/advisory.md)");
    } else {
        warn("bin/sync-doors.ps1 missing - cannot generate front doors");
    }
    if !commit {
        step("generate node map");
        node_map(root, false)?;
    }

    // 3) Identity is already explicit and lock-proven.
    step("model identity");
    ok(&format!("using process BRAIN_MODEL = {current}"));

    // 4) Optional scaffold checkpoint under STATE.lock, so start-session begins from a clean tree.
    if commit {
        step("commit scaffold");
        if text(root, &["status", "--porcelain"]).is_empty() {
            ok("nothing to commit; scaffold already checkpointed");
        } else {
            if text(root, &["config", "user.name"]).is_empty() {
                git_status(root, &["config", "user.name", "mdr-kernel bootstrap"]);
                ok("set local git user.name for bootstrap commit");
            }
            if text(root, &["config", "user.email"]).is_empty() {
                git_status(root, &["config", "user.email", "bootstrap@example.invalid"]);
                ok("set local git user.email for bootstrap commit");
            }
            // Only the scaffold bootstrap owns; .brain-root stays local clone metadata.
            let mut add = vec!["add", "--"];
            add.extend_from_slice(&SCAFFOLD[..11]);
            if git_status(root, &add) != 0 {
                return Err("git add failed for scaffold commit".into());
            }
            if git_status(root, &["add", "--", SCAFFOLD[11], SCAFFOLD[12]]) != 0 {
                return Err("git add failed for authority scaffold".into());
            }
            step("generate node map");
            node_map(root, true)?;
            if git_status(
                root,
                &["add", "--", SCAFFOLD[13], SCAFFOLD[14], SCAFFOLD[15]],
            ) != 0
            {
                return Err("git add failed for generated node maps".into());
            }
            let staged =
                names(root, &["diff", "--cached", "--name-only", "-z"]).unwrap_or_default();
            let foreign: Vec<&String> = staged
                .iter()
                .filter(|p| !SCAFFOLD.iter().any(|s| s.eq_ignore_ascii_case(p)))
                .collect();
            if !foreign.is_empty() {
                let list: Vec<&str> = foreign.iter().map(|s| s.as_str()).collect();
                return Err(format!(
                    "Foreign staged paths appeared during bootstrap; leaving index intact: {}",
                    list.join(", ")
                ));
            }
            // Someone else's untracked work dirties the tree without giving bootstrap a commit.
            if staged.is_empty() {
                ok("nothing to commit; scaffold already checkpointed (unrelated paths left untouched)");
            } else {
                let message = "Initialize mdr-kernel memory scaffold\n\nLeast-file: the kernel requires these scaffold files to start a memory";
                if git_status(root, &["commit", "-m", message]) != 0 {
                    return Err("git commit failed for scaffold commit".into());
                }
                ok("committed scaffold checkpoint");
            }
        }
    } else {
        warn("bootstrap created local scaffold files; commit them before start-session, or rerun with -CommitScaffold");
    }

    // 5) Health check.
    step("brain-doctor");
    let (doctor_exit, report) = crate::doctor::run(root, &crate::doctor::Options::default());
    print!("{report}");
    println!("\nbootstrap complete. Next:");
    if !commit {
        println!("  # checkpoint scaffold first, or run bootstrap again with -CommitScaffold");
    }
    println!("  pwsh -NoProfile -File bin/start-session.ps1 -Model '{model}'");
    Ok(doctor_exit)
}

/// `Write-Host` then `exit 1`, as the script's `Fail` does.
fn fail(m: &str) -> i32 {
    println!("[FAIL] {m}");
    1
}

/// `String.PadRight`.
fn pad(s: &str, width: usize) -> String {
    format!("{s:<width$}")
}

pub fn setup(root: &Path, model: &str, local_only: bool, confirm: bool) -> i32 {
    println!("mdr-kernel private setup - {}\n", root.display());
    if model.trim().is_empty() {
        return fail("missing -Model: the attribution name stamped on this clone's commits and locks. An agent passes its own model id; a human setting up by hand picks any short name (no angle brackets), for example: pwsh -NoProfile -File bin/setup-private.ps1 -Model 'alex'");
    }
    if text(root, &["rev-parse", "--is-inside-work-tree"]).is_empty() {
        return fail("run this inside the repository you created from the mdr-kernel template");
    }
    let branch = text(root, &["branch", "--show-current"]);
    if !branch.eq_ignore_ascii_case("main") {
        return fail(&format!(
            "canonical memory branch must be main; current branch is '{branch}'"
        ));
    }
    let origin = text(root, &["remote", "get-url", "--push", "origin"]);
    if local_only {
        if !origin.is_empty() {
            if git_status(root, &["remote", "remove", "origin"]) != 0 {
                return fail("could not remove origin for local-only mode");
            }
            ok("removed origin; this memory is local-only");
        }
        let v = remote_privacy::run(root, None, true, false, false, &Live);
        print!("{}", remote_privacy::report(&v));
        if v.code != 0 {
            return v.code;
        }
    } else {
        if origin.is_empty() {
            return fail("origin is missing; clone the PRIVATE repository you created from the template, or use -LocalOnly");
        }
        let v = remote_privacy::run(root, None, true, false, confirm, &Live);
        print!("{}", remote_privacy::report(&v));
        if v.code != 0 {
            return v.code;
        }
        ok("private push destination verified and pinned");
    }
    // A true first setup has no scaffold yet; reruns and second-machine clones skip the badge.
    let first = !root.join("STATE.md").exists();
    let code = bootstrap(root, Some(model), true).unwrap_or_else(|e| {
        eprintln!("{e}");
        1
    });
    if code != 0 {
        return code;
    }
    if !local_only {
        if crate::close::sync(root, false) != 0 {
            return fail(
                "initial private checkpoint did not sync; fix repository access, then rerun setup",
            );
        }
        ok("initial memory checkpoint pushed to approved private origin/main");
    }
    if first {
        // Printed once per memory, on the setup run that created it. ASCII-only on purpose.
        let (width, photo) = (40, 29);
        let mut name = model.to_uppercase();
        let max = photo - 12;
        if name.chars().count() > max {
            name = name.chars().take(max - 1).collect::<String>() + "~";
        }
        let rows = [
            pad(" MDR KERNEL", photo) + "+--------+ ",
            pad(" ----------", photo) + "| PHOTO  | ",
            pad(&format!(" REFINER: {name}"), photo) + "|REDACTED| ",
            pad(" FLOOR: PRIVATE", photo) + "+--------+ ",
            " CLEARANCE: 1".to_string(),
            " ||:||:|||::||:|||:|||||:||:||||:||| ".to_string(),
        ];
        let border = format!("+{}+", "-".repeat(width));
        println!("\n{border}");
        for row in &rows {
            println!("|{}|", pad(row, width));
        }
        println!("{border}\n");
        println!("  MACRODATA REFINEMENT KERNEL READY.");
        println!("  PLEASE ENJOY EACH SESSION EQUALLY.");
    }
    let mode = text(root, &["config", "--local", "--get", "brain.mode"]);
    let approved = text(
        root,
        &["config", "--local", "--get", "brain.approvedRemote"],
    );
    println!("\nREADY");
    println!("Mode: {mode}");
    println!("Canonical branch: main");
    if approved.is_empty() {
        println!("Approved destination: local-only (no remote push)");
    } else {
        println!("Approved destination: {approved}");
    }
    println!("\nStart a session:");
    println!("  pwsh -NoProfile -File bin/start-session.ps1 -Model '{model}'");
    if local_only {
        println!("Close a session (this commits locally):");
        println!(
            "  pwsh -NoProfile -File bin/close-session.ps1 -Topic '<short-topic>' -Model '{model}'"
        );
    } else {
        println!("Close a session (this commits and pushes your private main):");
        println!(
            "  pwsh -NoProfile -File bin/close-session.ps1 -Topic '<short-topic>' -Model '{model}'"
        );
        println!("If you must close while offline (private-remote mode only):");
        println!("  pwsh -NoProfile -File bin/close-session.ps1 -Topic '<short-topic>' -Model '{model}' -OfflineCheckpoint");
        println!("When connectivity returns, use the one recovery command shown by startup:");
        println!("  pwsh -NoProfile -File bin/sync-private.ps1");
    }
    0
}

pub fn cli(command: &str, argv: &[String]) -> i32 {
    let result = (|| -> Result<i32, String> {
        let (values, switches): (&[&str], &[&str]) = if command == "setup-private" {
            (&["Model", "Root"], &["LocalOnly", "ConfirmPrivateRemote"])
        } else {
            (&["Model", "Root"], &["CommitScaffold"])
        };
        let args = Args::parse(argv, values, switches)?;
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        let root = args
            .value("Root")
            .map_or_else(|| top(&cwd), |r| Ok(PathBuf::from(r)))
            .map(|r| crate::args::absolute(&r))?;
        crate::hook::fresh(&root)?;
        std::env::set_current_dir(&root).map_err(|e| e.to_string())?;
        if command == "setup-private" {
            if args.switch("LocalOnly") && args.switch("ConfirmPrivateRemote") {
                return Err("-LocalOnly and -ConfirmPrivateRemote cannot be combined".into());
            }
            return Ok(setup(
                &root,
                args.value("Model").unwrap_or(""),
                args.switch("LocalOnly"),
                args.switch("ConfirmPrivateRemote"),
            ));
        }
        bootstrap(&root, args.value("Model"), args.switch("CommitScaffold"))
    })();
    result.unwrap_or_else(|e| {
        eprintln!("{e}");
        1
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaffolds_carry_their_envelope() {
        for text in [STATE, DECISIONS, HANDOFFS, REFERENCE, SPECS, INBOX] {
            assert!(text.starts_with("Title: "), "{text}");
            assert!(text.ends_with("AI Execution Prompts: protocol/CONTRACT.md"));
            assert!(!text.contains('\r'));
        }
        assert_eq!(pad("ab", 4), "ab  ");
    }
}
