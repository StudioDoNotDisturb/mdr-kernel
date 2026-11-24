//! Write approval for new agent-authored reference notes and skills. A port of
//! `bin/write-approval.ps1`: propose into ignored local staging, approve into the repo with an
//! audit JSONL entry, list or show proposals, and the pre-commit's staged-approval check.
//!
//!   mdr-kernel write-approval -Propose -Target <path> -Source <file> -Reason <why> [-DryRun]
//!   mdr-kernel write-approval -Approve -ProposalId <id> -Reason <why> [-Force] [-DryRun]
//!   mdr-kernel write-approval -List | -Show -ProposalId <id> | -ValidateStagedApprovals

use crate::args::Args;
use crate::git::{git, lossy, names, top};
use crate::recall::to_json;
use regex::Regex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const WORKFLOW: &str = "write-approval-v1";
const AUDIT: &str = "inbox/write-approval-audit.jsonl";
const PROPOSALS: &str = ".write-proposals";

fn sha_file(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

fn text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// PowerShell's `-match` and `-ne` ignore case, so the gated-target rules do too.
fn approved_target(rel: &str) -> bool {
    let lower = rel.to_lowercase();
    let reference = Regex::new(r"(?i)^reference/[^/]+\.md$").unwrap();
    let skill = Regex::new(r"(?i)^skills/[^/]+/SKILL\.md$").unwrap();
    (reference.is_match(rel) && lower != "reference/index.md")
        || (skill.is_match(rel) && lower != "skills/mdr-kernel/skill.md")
}

/// A path under the memory root, as a forward-slash relative path; anything outside refuses.
fn repo_path(root: &Path, path: &str) -> Result<String, String> {
    let base = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        root.join(path)
    };
    let full = crate::args::absolute(&base).to_string_lossy().into_owned();
    let root_full = crate::args::absolute(root).to_string_lossy().into_owned();
    let (f, r) = (full.to_lowercase(), root_full.to_lowercase());
    let under = f == r || f.starts_with(&format!("{r}\\")) || f.starts_with(&format!("{r}/"));
    if !under {
        return Err(format!("Path is outside repo: {path}"));
    }
    Ok(full[root_full.len()..]
        .trim_start_matches(['\\', '/'])
        .replace('\\', "/"))
}

/// An explicit session identity; git config brain.model is display-only.
fn model() -> Result<String, String> {
    let explicit = std::env::var("BRAIN_MODEL")
        .unwrap_or_default()
        .trim()
        .to_string();
    if explicit.is_empty() || explicit == "unknown" {
        return Err("write-approval needs an explicit session identity: set $env:BRAIN_MODEL before -Propose or -Approve (git config brain.model is display-only and is not used).".into());
    }
    Ok(explicit)
}

fn slug(text: &str) -> String {
    let lower = text.to_lowercase();
    let mut slug = Regex::new(r"[^a-z0-9]+")
        .unwrap()
        .replace_all(&lower, "-")
        .into_owned();
    slug = Regex::new(r"^-|-$")
        .unwrap()
        .replace_all(&slug, "")
        .into_owned();
    if slug.chars().count() > 48 {
        slug = slug
            .chars()
            .take(48)
            .collect::<String>()
            .trim_end_matches('-')
            .to_string();
    }
    if slug.is_empty() {
        "proposal".into()
    } else {
        slug
    }
}

fn proposal_dir(root: &Path, id: &str) -> Result<PathBuf, String> {
    if !Regex::new(r"(?i)^[0-9TZ-]+-[a-z0-9-]+$")
        .unwrap()
        .is_match(id)
    {
        return Err(format!("Invalid proposal id: {id}"));
    }
    let dir = root.join(PROPOSALS).join(id);
    if !dir.exists() {
        return Err(format!("Proposal not found: {id}"));
    }
    Ok(dir)
}

fn metadata(dir: &Path) -> Result<Value, String> {
    let path = dir.join("metadata.json");
    let raw = fs::read(&path).map_err(|_| format!("Missing metadata.json in {}", dir.display()))?;
    let raw = raw.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&raw);
    serde_json::from_slice(raw).map_err(|e| format!("Invalid JSON in {}: {e}", path.display()))
}

/// `yyyyMMddTHHmmssZ` in UTC.
fn stamp() -> String {
    let seconds = crate::state_lock::now_seconds() as i64;
    let (y, m, d) = crate::state_lock::civil_from_days(seconds.div_euclid(86_400));
    let s = seconds.rem_euclid(86_400);
    format!(
        "{y:04}{m:02}{d:02}T{:02}{:02}{:02}Z",
        s / 3600,
        s % 3600 / 60,
        s % 60
    )
}

fn audit(root: &Path, entry: &Value, dry: bool, out: &mut String) -> Result<(), String> {
    let line = entry.to_string();
    if dry {
        out.push_str(&format!("DRY_RUN audit: {line}\n"));
        return Ok(());
    }
    let path = root.join(AUDIT);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    use std::io::Write;
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| f.write_all(format!("{line}\n").as_bytes()))
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// The pre-commit gate: every newly staged gated file needs a staged approve entry whose
/// local proposal payload still hashes to what was approved. `Err` is the message to print.
pub fn validate_staged(root: &Path) -> Result<(), String> {
    let mut targets: Vec<String> = names(
        root,
        &[
            "diff",
            "--cached",
            "--name-only",
            "-z",
            "--diff-filter=A",
            "--",
            "reference/*.md",
            "skills/*/SKILL.md",
        ],
    )
    .unwrap_or_default()
    .into_iter()
    .map(|p| p.replace('\\', "/").trim().to_string())
    .filter(|p| !p.is_empty() && approved_target(p))
    .collect();
    targets.sort_by_key(|t| t.to_lowercase());
    targets.dedup_by_key(|t| t.to_lowercase());
    if targets.is_empty() {
        return Ok(());
    }
    let audit_text = git(root, &["show", "--no-textconv", &format!(":{AUDIT}")])
        .map(lossy)
        .unwrap_or_default();
    if audit_text.trim().is_empty() {
        return Err(format!("new reference/skill files need write approval. Missing staged inbox/write-approval-audit.jsonl entry for: {}", targets.join(", ")));
    }
    // A later approve entry for the same target replaces an earlier one; keys ignore case.
    let mut approvals: HashMap<String, Value> = HashMap::new();
    for line in audit_text.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if !text(&entry["action"]).eq_ignore_ascii_case("approve")
            || !text(&entry["workflow"]).eq_ignore_ascii_case(WORKFLOW)
            || text(&entry["target"]).is_empty()
        {
            continue;
        }
        approvals.insert(text(&entry["target"]).to_lowercase(), entry);
    }
    let mut errors = Vec::new();
    for target in &targets {
        let Some(entry) = approvals.get(&target.to_lowercase()) else {
            errors.push(format!("{target}: missing staged approve entry"));
            continue;
        };
        let id = text(&entry["proposal_id"]);
        if id.is_empty() {
            errors.push(format!("{target}: approve entry missing proposal_id"));
            continue;
        }
        let dir = root.join(PROPOSALS).join(&id);
        let content = dir.join("content.md");
        if !dir.join("metadata.json").exists() || !content.exists() {
            errors.push(format!("{target}: local proposal payload missing for {id}"));
            continue;
        }
        let Ok(meta) = metadata(&dir) else {
            errors.push(format!("{target}: unreadable metadata for {id}"));
            continue;
        };
        let hash = sha_file(&content)?;
        let expected = text(&meta["content_sha256"]).to_lowercase();
        let audited = text(&entry["content_sha256"]).to_lowercase();
        if !text(&meta["proposal_id"]).eq_ignore_ascii_case(&id) {
            errors.push(format!("{target}: metadata proposal_id mismatch for {id}"));
        }
        if !text(&meta["target"]).eq_ignore_ascii_case(target) {
            errors.push(format!("{target}: metadata target mismatch for {id}"));
        }
        if expected.is_empty() || hash != expected {
            errors.push(format!("{target}: proposal content hash mismatch for {id}"));
        }
        if audited.is_empty() || hash != audited {
            errors.push(format!("{target}: audit content hash mismatch for {id}"));
        }
    }
    if errors.is_empty() {
        return Ok(());
    }
    Err(format!(
        "new reference/skill files need valid write approval.\n  {}",
        errors.join("\n  ")
    ))
}

fn run(root: &Path, args: &Args) -> Result<String, String> {
    let modes = [
        "Propose",
        "Approve",
        "List",
        "Show",
        "ValidateStagedApprovals",
    ];
    let chosen: Vec<&str> = modes.iter().copied().filter(|m| args.switch(m)).collect();
    if chosen.len() > 1 {
        return Err(
            "Choose exactly one of -Propose, -Approve, -List, -Show, or -ValidateStagedApprovals."
                .into(),
        );
    }
    let mode = chosen.first().copied().unwrap_or("List");
    let value = |k: &str| args.value(k).filter(|v| !v.is_empty());
    let dry = args.switch("DryRun");
    let mut out = String::new();
    match mode {
        "Propose" => {
            let (Some(target), Some(source), Some(reason)) =
                (value("Target"), value("Source"), value("Reason"))
            else {
                return Err("-Propose requires -Target, -Source, and -Reason.".into());
            };
            let actor = model()?; // refuse before any proposal directory is written
            let target_rel = repo_path(root, target)?;
            if !approved_target(&target_rel) {
                return Err(format!(
                    "Target is not gated by this workflow: {target_rel}"
                ));
            }
            let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
            let source_full = crate::args::absolute(&cwd.join(source));
            if !source_full.exists() {
                return Err(format!("Source not found: {source}"));
            }
            let stem = Path::new(&target_rel)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let id = format!("{}-{}", stamp(), slug(&stem));
            let hash = sha_file(&source_full)?;
            let meta = json!({
                "workflow": WORKFLOW,
                "proposal_id": id,
                "target": target_rel,
                "source": source_full.to_string_lossy(),
                "content_sha256": hash,
                "reason": reason,
                "proposed_by": actor,
                "proposed_at": crate::state_lock::iso(crate::state_lock::now_seconds()),
            });
            if dry {
                return Ok(format!("DRY_RUN propose: {id} -> {target_rel}\n"));
            }
            let dir = root.join(PROPOSALS).join(&id);
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            fs::copy(&source_full, dir.join("content.md")).map_err(|e| e.to_string())?;
            fs::write(dir.join("metadata.json"), to_json(&meta) + "\n")
                .map_err(|e| e.to_string())?;
            out.push_str(&format!("PROPOSAL_ID={id}\nTARGET={target_rel}\nNEXT=pwsh -NoProfile -File bin/write-approval.ps1 -Approve -ProposalId {id} -Reason '<review outcome>'\n"));
        }
        "Approve" => {
            let (Some(id), Some(reason)) = (value("ProposalId"), value("Reason")) else {
                return Err("-Approve requires -ProposalId and -Reason.".into());
            };
            let actor = model()?; // refuse before the target or the audit log is touched
            let dir = proposal_dir(root, id)?;
            let meta = metadata(&dir)?;
            let target_rel = text(&meta["target"]);
            if !approved_target(&target_rel) {
                return Err(format!(
                    "Target is not gated by this workflow: {target_rel}"
                ));
            }
            let content = dir.join("content.md");
            if !content.exists() {
                return Err(format!("Missing proposal content: {}", content.display()));
            }
            let target_full = root.join(&target_rel);
            if target_full.exists() && !args.switch("Force") {
                return Err(format!(
                    "Target exists; use -Force only after review: {target_rel}"
                ));
            }
            let hash = sha_file(&content)?;
            let pinned = text(&meta["content_sha256"]);
            if !pinned.is_empty() && hash != pinned.to_lowercase() {
                return Err(format!("Proposal content hash changed for {id}"));
            }
            let entry = json!({
                "workflow": WORKFLOW,
                "action": "approve",
                "proposal_id": id,
                "target": target_rel,
                "approved_by": actor,
                "approved_at": crate::state_lock::iso(crate::state_lock::now_seconds()),
                "reason": reason,
                "content_sha256": hash,
            });
            if dry {
                out.push_str(&format!("DRY_RUN approve: {id} -> {target_rel}\n"));
                audit(root, &entry, true, &mut out)?;
                return Ok(out);
            }
            if let Some(parent) = target_full.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::copy(&content, &target_full).map_err(|e| e.to_string())?;
            audit(root, &entry, false, &mut out)?;
            out.push_str(&format!("APPROVED={target_rel}\nAUDIT={AUDIT}\n"));
        }
        "Show" => {
            let Some(id) = value("ProposalId") else {
                return Err("-Show requires -ProposalId.".into());
            };
            let dir = proposal_dir(root, id)?;
            out.push_str(&(to_json(&metadata(&dir)?) + "\n"));
            out.push_str("\n--- content.md ---\n");
            let body = fs::read(dir.join("content.md")).map_err(|e| e.to_string())?;
            for line in crate::health::read_lines(&body) {
                out.push_str(&line);
                out.push('\n');
            }
        }
        "ValidateStagedApprovals" => {
            return validate_staged(root).map(|()| String::new());
        }
        _ => {
            let base = root.join(PROPOSALS);
            let mut items: Vec<PathBuf> = fs::read_dir(&base)
                .map(|e| {
                    e.flatten()
                        .map(|e| e.path())
                        .filter(|p| p.is_dir())
                        .collect()
                })
                .unwrap_or_default();
            items.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
            if items.is_empty() {
                return Ok("No pending proposals.\n".into());
            }
            for item in items {
                let name = item
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                match metadata(&item) {
                    Ok(meta) => out.push_str(&format!(
                        "{} -> {} :: {}\n",
                        text(&meta["proposal_id"]),
                        text(&meta["target"]),
                        text(&meta["reason"])
                    )),
                    Err(_) => out.push_str(&format!("{name} -> unreadable metadata\n")),
                }
            }
        }
    }
    Ok(out)
}

pub fn cli(argv: &[String]) -> i32 {
    // Every refusal is one [FAIL] line on stdout, as the script's trap renders it.
    let result = (|| -> Result<String, String> {
        let args = Args::parse(
            argv,
            &["Target", "Source", "ProposalId", "Reason", "Root"],
            &[
                "Propose",
                "Approve",
                "List",
                "Show",
                "ValidateStagedApprovals",
                "Force",
                "DryRun",
            ],
        )?;
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        let root = args
            .value("Root")
            .map_or_else(|| top(&cwd), |r| Ok(PathBuf::from(r)))
            .map(|r| crate::args::absolute(&r))?;
        crate::hook::fresh_in_repo(&root)?;
        if args.switch("ValidateStagedApprovals") {
            // The staged check prints its message bare, without the [FAIL] prefix.
            return match validate_staged(&root) {
                Ok(()) => Ok(String::new()),
                Err(message) => {
                    println!("{message}");
                    std::process::exit(1);
                }
            };
        }
        run(&root, &args)
    })();
    match result {
        Ok(out) => {
            print!("{out}");
            0
        }
        Err(e) => {
            println!("[FAIL] {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gated_targets_and_slugs() {
        assert!(approved_target("reference/note.md"));
        assert!(approved_target("Reference/Note.MD"));
        assert!(!approved_target("reference/INDEX.md"));
        assert!(!approved_target("reference/sub/note.md"));
        assert!(approved_target("skills/tool/SKILL.md"));
        assert!(!approved_target("skills/mdr-kernel/SKILL.md"));
        assert_eq!(slug("My Note v2!"), "my-note-v2");
        assert_eq!(slug("---"), "proposal");
        assert_eq!(slug(&"ab-".repeat(30)).len(), 47);
    }
}
