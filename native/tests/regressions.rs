#[path = "regressions/envelope_check.rs"]
mod envelope_check;
#[path = "regressions/skill_structure.rs"]
mod skill_structure;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

const ENGINE: &str = env!("CARGO_BIN_EXE_mdr-kernel");

fn template() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}

struct Sandbox(PathBuf);

impl Sandbox {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "mdr-regression-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        Self(dir)
    }

    fn clone_template(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        git(
            &self.0,
            &[
                "clone",
                "-q",
                "--no-hardlinks",
                "--",
                template().to_str().unwrap(),
                path.to_str().unwrap(),
            ],
        );
        git(&path, &["remote", "remove", "origin"]);
        git(&path, &["branch", "-M", "main"]);
        assert!(git(&path, &["remote"]).is_empty());
        configure(&path);
        let binary = if cfg!(windows) {
            "bin/mdr-kernel.exe"
        } else {
            "bin/mdr-kernel"
        };
        fs::copy(ENGINE, path.join(binary)).unwrap();
        path
    }

    fn repo(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::create_dir(&path).unwrap();
        git(&path, &["init", "-q", "-b", "main"]);
        configure(&path);
        path
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn command(program: &str, cwd: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(program);
    cmd.current_dir(cwd).args(args);
    for (key, _) in std::env::vars_os() {
        let name = key.to_string_lossy();
        if ["GIT_", "BRAIN_", "MDR_FORCE_", "MDR_TEST_", "MDR_ENGINE"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            cmd.env_remove(key);
        }
    }
    cmd.envs([
        (
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        ),
        ("GIT_CONFIG_NOSYSTEM", "1"),
        ("GIT_TERMINAL_PROMPT", "0"),
    ]);
    cmd
}

fn configure(root: &Path) {
    git(root, &["config", "user.name", "Regression Fixture"]);
    git(root, &["config", "user.email", "fixture@example.invalid"]);
    git(root, &["config", "core.autocrlf", "false"]);
    git(root, &["config", "commit.gpgsign", "false"]);
}

fn git(root: &Path, args: &[&str]) -> String {
    let out = command("git", root, args).output().unwrap();
    let (code, text) = result(out);
    assert_eq!(code, 0, "git {args:?}: {text}");
    text
}

fn run(root: &Path, args: &[&str]) -> (i32, String) {
    result(
        command(ENGINE, root, args)
            .stdin(Stdio::null())
            .output()
            .unwrap(),
    )
}

fn result(out: Output) -> (i32, String) {
    (
        out.status.code().unwrap(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

fn put(root: &Path, path: &str, bytes: impl AsRef<[u8]>) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn check(name: &str, condition: bool, detail: impl std::fmt::Debug) {
    assert!(condition, "{name}: {detail:?}");
}
#[path = "regressions/recall.rs"]
mod recall;

fn stage_compiler_inputs(root: &Path) {
    let paths = git(template(), &["ls-files", "--", "native"]);
    for path in paths.lines().filter(|path| {
        path.starts_with("native/src/")
            || matches!(
                *path,
                "native/Cargo.toml" | "native/Cargo.lock" | "native/build.rs"
            )
    }) {
        put(root, path, fs::read(template().join(path)).unwrap());
    }
    git(root, &["add", "--", "native"]);
}

fn authority_ok(out: &(i32, String), name: &str, expected: Option<i32>, needles: &[&str]) {
    check(
        name,
        expected.map_or(out.0 != 0, |code| out.0 == code)
            && needles.iter().all(|needle| out.1.contains(needle)),
        out,
    );
}

#[path = "regressions/decision_authority.rs"]
mod decision_authority;
#[path = "regressions/node_map_retrieval.rs"]
mod node_map_retrieval;

#[path = "regressions/session_safety.rs"]
mod session_safety;

#[path = "regressions/private_onboarding.rs"]
mod private_onboarding;

#[path = "regressions/cold_clone.rs"]
mod cold_clone;

#[path = "regressions/native_release.rs"]
mod native_release;
