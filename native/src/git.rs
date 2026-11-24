//! Git process helpers shared by the commit guards.

use std::path::{Path, PathBuf};
use std::process::Command;

pub fn lossy(bytes: Vec<u8>) -> String {
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Run git in `dir` (inheriting the hook's GIT_* environment) and return stdout.
pub fn git(dir: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .map_err(|e| format!("required utility 'git' is unavailable ({e})"))?;
    if !out.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(out.stdout)
}

/// Trimmed stdout of a git query; empty when git fails.
pub fn text(dir: &Path, args: &[&str]) -> String {
    git(dir, args)
        .map(lossy)
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// NUL-separated path listing (`-z`), empty entries dropped.
pub fn names(dir: &Path, args: &[&str]) -> Result<Vec<String>, String> {
    Ok(lossy(git(dir, args)?)
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect())
}

pub fn top(dir: &Path) -> Result<PathBuf, String> {
    Ok(PathBuf::from(
        lossy(git(dir, &["rev-parse", "--show-toplevel"])?).trim(),
    ))
}

/// The folder that owns the shared .git: the canonical clone for every linked worktree.
pub fn common_top(dir: &Path) -> Option<PathBuf> {
    let common = git(
        dir,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .or_else(|_| git(dir, &["rev-parse", "--git-common-dir"]))
    .ok()?;
    let common = dir.join(lossy(common).trim());
    common.parent().map(Path::to_path_buf)
}
