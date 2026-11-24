//! Session start and stop. Ports of `mdr-kernel start-session` (doctor, pull, STATE readback, one
//! STARTUP_STATUS line) and `mdr-kernel session-stop` (a read-only Stop-hook nudge). The doctor
//! runs in process.
//!
//!   mdr-kernel start-session [-Model <id>] [-FullDoctor]
//!   mdr-kernel session-stop

use crate::args::Args;
use crate::git::{common_top, git, lossy, top};
use std::path::{Path, PathBuf};

const RESPONSE: &str = "RESPONSE_CONTRACT=Report Right now, blockers, and top next action from STATE.md; if work was assigned, use STATE.md plus reference/INDEX.md to pick the smallest relevant file set before reading broadly; the generated node map checks link health.";
const SHARED: &str = "SHARED_MEMORY_CONTRACT=Anything another model needs must be written into the repo; never rely on private model memory.";
const RECALL: &str = "RECALL_CONTRACT=Before acting on assigned work, recall must run: a verified prompt hook (mdr-kernel hook-recall) or mdr-kernel prepare-work -Objective '<operator objective>'; if gates match, read the listed sources and state how the traps change the next action.";

fn lines(root: &Path, args: &[&str]) -> Vec<String> {
    git(root, args)
        .map(lossy)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn start(root: &Path, model: Option<&str>, full_doctor: bool) -> i32 {
    let env_model = std::env::var("BRAIN_MODEL").unwrap_or_default();
    let model = model
        .map(str::to_string)
        .unwrap_or_else(|| env_model.clone());
    let model_id = if model.trim().is_empty() {
        "UNSET".to_string()
    } else {
        model.trim().to_string()
    };
    let mut status = "READY";
    let mut notes = Vec::new();
    println!("mdr-kernel canonical session start");
    println!("ROOT={}", root.display());
    println!("MODEL_ID={model_id}");
    // git config brain.model is display-only: lock, close and commit refuse it as identity.
    let fallback = lines(root, &["config", "brain.model"])
        .into_iter()
        .next()
        .unwrap_or_default();
    if model_id != "UNSET" && env_model.is_empty() {
        if !fallback.is_empty() && model_id != fallback {
            println!("NOTE: session started as '{model_id}' but this clone's display-only fallback identity is '{fallback}'.");
        }
        println!("IDENTITY: locks/commits without an explicit -Model or BRAIN_MODEL are refused. Seal this session:");
        println!("        $env:BRAIN_MODEL = '{model_id}'");
    }
    if !lines(root, &["rev-parse", "--is-inside-work-tree"])
        .iter()
        .any(|l| l == "true")
    {
        println!("\n== startup result ==");
        println!("STARTUP_STATUS=BLOCKED");
        println!("NOTE=This folder is not a git working tree. Use git clone (preferred) or git init before bootstrap/start-session.");
        println!("{RESPONSE}\n{SHARED}\n{RECALL}");
        return 1;
    }
    println!("---\nMDR KERNEL: Induction record opened.\nExternal context quarantined. STATE.md will be loaded.\n---");
    let stale = crate::hook::fresh(root).err();
    if let Some(e) = &stale {
        status = "BLOCKED";
        notes.push(e.replace('\n', " "));
    }
    if model_id == "UNSET" {
        notes.push("Model identity is unset. Use -Model <id> now and set BRAIN_MODEL or an explicit Brain-Model trailer before lock/commit work.".to_string());
    }

    println!("\n== 1. brain-doctor ==");
    let (doctor_exit, doctor) = crate::doctor::run(root, &crate::doctor::Options::default());
    // Compact by default: [ ok ] lines are counted, not printed; the lock line always prints.
    if full_doctor || std::env::var("BRAIN_STARTUP_FULL_DOCTOR").as_deref() == Ok("1") {
        doctor.lines().for_each(|l| println!("{l}"));
    } else {
        let mut hidden = 0;
        for line in doctor.lines() {
            let lower = line.to_lowercase();
            if lower.starts_with("[ ok ]") && !lower.contains("state.lock") {
                hidden += 1;
            } else if !line.trim().is_empty() {
                println!("{line}");
            }
        }
        println!("({hidden} ok checks passed, not shown; full output: mdr-kernel doctor)");
    }
    if doctor_exit != 0 {
        status = "BLOCKED";
        notes.push(format!(
            "brain-doctor exited {doctor_exit}; resolve the reported hazard before working."
        ));
    }

    println!("\n== 2. git pull --rebase ==");
    // Tracked changes only: stray untracked files cannot make a pull --rebase unsafe.
    if !lines(root, &["status", "--porcelain", "--untracked-files=no"]).is_empty() {
        status = "BLOCKED";
        notes.push("Tracked changes are uncommitted; pull was not attempted. Reconcile or commit existing work first.".into());
        println!("SKIPPED: tracked changes are uncommitted.");
    } else if doctor_exit != 0 {
        println!("SKIPPED: brain-doctor reported a hard failure.");
    } else {
        let branch = lines(root, &["branch", "--show-current"])
            .into_iter()
            .next()
            .unwrap_or_default();
        let upstream = lines(
            root,
            &[
                "for-each-ref",
                "--format=%(upstream:short)",
                &format!("refs/heads/{branch}"),
            ],
        );
        if upstream.is_empty() {
            println!("SKIPPED: no upstream configured (new repo).");
        } else {
            match std::process::Command::new("git")
                .current_dir(root)
                .args(["pull", "--rebase"])
                .output()
            {
                Ok(o) => {
                    print!("{}{}", lossy(o.stdout), lossy(o.stderr));
                    if !o.status.success() {
                        status = "BLOCKED";
                        notes.push(format!("git pull --rebase exited {}; do not write from stale or conflicted state.", o.status.code().unwrap_or(1)));
                    }
                }
                Err(e) => {
                    status = "BLOCKED";
                    notes.push(format!(
                        "git pull --rebase could not run ({e}); do not write from stale state."
                    ));
                }
            }
            // A pull can bring new native/ sources; say so before the next lock or commit refuses.
            if let (None, Err(e)) = (&stale, crate::hook::fresh(root)) {
                notes.push(format!(
                    "the pull changed native/ sources: {}",
                    e.replace('\n', " ")
                ));
            }
        }
    }

    println!("\n== 3. STATE.md ==");
    match std::fs::read(root.join("STATE.md")) {
        Ok(bytes) => println!("{}", lossy(bytes)),
        Err(_) => {
            status = "BLOCKED";
            notes.push("STATE.md is missing - first run: mdr-kernel setup-private -Model '<your model id>' (example: -Model 'claude-opus-5'); repairing a clone that was already set up: mdr-kernel bootstrap-clone.".into());
            println!("MISSING: STATE.md");
        }
    }

    println!("\n== startup result ==");
    println!("STARTUP_STATUS={status}");
    notes.iter().for_each(|n| println!("NOTE={n}"));
    println!("{RESPONSE}\n{SHARED}\n{RECALL}");
    i32::from(status != "READY")
}

/// Read-only Stop-hook nudge: JSON with a systemMessage, or `{}` when there is nothing to say.
pub fn stop(root: &Path) -> String {
    let mut messages = Vec::new();
    if !lines(root, &["status", "--porcelain"]).is_empty() {
        messages.push("mdr-kernel has uncommitted changes — close out with: mdr-kernel close-session -Topic <handoff-slug> -Model <your-model-id> (verifies handoff/STATE, commits with your trailer, pushes, releases the lock).".to_string());
    }
    let lock = common_top(root)
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| root.to_path_buf())
        .join("STATE.lock");
    if lock.exists() {
        let raw = std::fs::read(&lock).map(lossy).unwrap_or_default();
        let holder = serde_json::from_str::<serde_json::Value>(&raw)
            .ok()
            .and_then(|v| v.get("model").and_then(|m| m.as_str()).map(str::to_string))
            .unwrap_or_else(|| raw.split_whitespace().next().unwrap_or("").to_string());
        messages.push(format!("STATE.lock is still held by '{holder}' — fine mid-session; close-session releases it after a successful push; a lock you hold outside close: mdr-kernel state-lock -Release -Model <your-model-id> -Token <your acquired token>."));
    }
    if messages.is_empty() {
        return "{}".into();
    }
    serde_json::json!({ "systemMessage": messages.join(" ") }).to_string()
}

fn root_of(args: &Args) -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    args.value("Root")
        .map_or_else(|| top(&cwd), |r| Ok(PathBuf::from(r)))
}

pub fn cli(command: &str, argv: &[String]) -> i32 {
    let result = (|| -> Result<i32, String> {
        let args = Args::parse(argv, &["Model", "Root"], &["FullDoctor"])?;
        if command == "session-stop" {
            println!(
                "{}",
                root_of(&args).map_or_else(|_| "{}".to_string(), |root| stop(&root))
            );
            return Ok(0);
        }
        let root = root_of(&args)?;
        Ok(start(&root, args.value("Model"), args.switch("FullDoctor")))
    })();
    result.unwrap_or_else(|e| {
        eprintln!("X mdr-kernel {command}: {e}");
        1
    })
}
