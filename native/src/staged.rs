//! Check affected regression suites against an isolated snapshot of the Git index.

use crate::git::{git, names};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const GATES: [&str; 2] = [
    "protocol/recall-gates.json",
    "protocol/recall-gates.example.json",
];

fn affects_suites(path: &str) -> bool {
    crate::fingerprint::compiler_input(path)
        || path.starts_with("native/tests/")
        || matches!(
            path,
            ".githooks/pre-commit" | ".githooks/prepare-commit-msg"
        )
        || path == GATES[1]
}

struct Snapshot(PathBuf);

impl Snapshot {
    fn create() -> std::io::Result<Self> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("mdr-staged-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&root)?;
        Ok(Self(root))
    }
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn isolated(root: &Path, program: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.current_dir(root).stdin(Stdio::null());
    for (key, _) in std::env::vars_os() {
        let name = key.to_string_lossy().to_ascii_uppercase();
        if [
            "GIT_",
            "BRAIN_",
            "MDR_FORCE_",
            "MDR_TEST_",
            "CARGO_TARGET_DIR",
            "CARGO_BUILD_TARGET_DIR",
        ]
        .iter()
        .any(|prefix| name.starts_with(prefix))
        {
            cmd.env_remove(key);
        }
    }
    cmd.env("GIT_CONFIG_NOSYSTEM", "1").env(
        "GIT_CONFIG_GLOBAL",
        if cfg!(windows) { "NUL" } else { "/dev/null" },
    );
    cmd
}

fn snapshot_git(root: &Path, args: &[&str]) -> Result<(), String> {
    let output = isolated(root, "git")
        .args(args)
        .output()
        .map_err(|e| format!("staged snapshot: git {}: {e}", args.join(" ")))?;
    if !output.status.success() {
        return Err(format!(
            "staged snapshot: git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

pub fn check_gates(root: &Path, staged: &[String]) -> Result<(), String> {
    if staged.iter().any(|path| GATES.contains(&path.as_str())) {
        let present = names(root, &["ls-files", "-z", "--", GATES[0], GATES[1]])?;
        for path in present {
            let bytes = git(root, &["show", &format!(":{path}")])?;
            crate::recall::parse_gates(&bytes)
                .map_err(|reason| format!("staged regression checks failed: {path}: {reason}"))?;
        }
    }
    Ok(())
}

fn stop_suite(child: &mut std::process::Child) {
    let id = child.id().to_string();
    #[cfg(windows)]
    let mut stop = {
        let mut command = Command::new("taskkill");
        command.args(["/T", "/F", "/PID", &id]);
        command
    };
    #[cfg(unix)]
    let mut stop = {
        let mut command = Command::new("kill");
        command.args(["-KILL", "--", &format!("-{id}")]);
        command
    };
    let _ = stop.stdout(Stdio::null()).stderr(Stdio::null()).status();
    let _ = child.kill();
    let _ = child.wait();
}

pub fn run_suites(root: &Path, staged: &[String]) -> Result<(), String> {
    if !staged.iter().any(|path| affects_suites(path)) {
        return Ok(());
    }

    let snapshot = Snapshot::create().map_err(|e| format!("create staged snapshot: {e}"))?;
    let dir = &snapshot.0;
    let tree = git(root, &["write-tree"])?;
    let tree = std::str::from_utf8(&tree).map_err(|e| format!("staged tree ID: {e}"))?;
    let objects = git(
        root,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "objects",
        ],
    )?;
    snapshot_git(dir, &["init", "-q", "-b", "main"])?;
    // Share objects read-only. The candidate has its own index, refs, worktree and config.
    std::fs::write(dir.join(".git/objects/info/alternates"), objects)
        .map_err(|e| format!("staged object access: {e}"))?;
    snapshot_git(dir, &["read-tree", tree.trim()])?;
    snapshot_git(dir, &["checkout-index", "--all"])?;
    snapshot_git(
        dir,
        &[
            "-c",
            "core.hooksPath=.no-hooks",
            "-c",
            "user.name=Staged Regression",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "Staged candidate",
        ],
    )?;

    let mut command = isolated(dir, "cargo");
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut cargo = command
        .env("CARGO_TARGET_DIR", dir.join("native/target"))
        .args([
            "test",
            "--locked",
            "--offline",
            "--manifest-path",
            "native/Cargo.toml",
        ])
        .stdout(Stdio::from(std::io::stderr()))
        .spawn()
        .map_err(|e| {
            format!("staged regression checks require Cargo for enforcement changes: {e}")
        })?;
    let start = Instant::now();
    loop {
        match cargo.try_wait() {
            Ok(Some(status)) if status.success() => {
                eprintln!("PASS staged Cargo regression tests (index snapshot)");
                return Ok(());
            }
            Ok(Some(status)) => {
                return Err(format!("staged Cargo regression tests failed: {status}"))
            }
            Ok(None) if start.elapsed() < Duration::from_secs(240) => {
                std::thread::sleep(Duration::from_millis(100));
            }
            result => {
                stop_suite(&mut cargo);
                return Err(match result {
                    Err(e) => format!("staged Cargo regression checks: {e}"),
                    _ => "staged Cargo regression checks timed out".into(),
                });
            }
        }
    }
}
