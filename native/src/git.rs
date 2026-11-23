//! Process helpers shared by the commit guards: git reads and the PowerShell gates that later
//! checkpoints port.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

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

fn pwsh_command(root: &Path, script: &str, args: &[&str]) -> Command {
    let mut command = Command::new("pwsh");
    command
        .current_dir(root)
        .arg("-NoProfile")
        .arg("-File")
        .arg(root.join("bin").join(script))
        .args(args);
    command
}

/// A PowerShell gate whose output is captured.
pub fn pwsh(root: &Path, script: &str, args: &[&str]) -> Result<Output, String> {
    pwsh_command(root, script, args)
        .output()
        .map_err(|e| format!("required utility 'pwsh' is unavailable ({e})"))
}

/// A PowerShell gate whose stdout and stderr both go to this process's stderr, as `>&2` does.
pub fn pwsh_to_stderr(root: &Path, script: &str, args: &[&str]) -> Result<bool, String> {
    let status = pwsh_command(root, script, args)
        .stdout(Stdio::from(std::io::stderr()))
        .status()
        .map_err(|e| format!("required utility 'pwsh' is unavailable ({e})"))?;
    Ok(status.success())
}
