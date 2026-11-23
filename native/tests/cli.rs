use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let name = format!(
            "mdr-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(name);
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn memory(&self, name: &str) -> PathBuf {
        let root = self.0.join(name);
        fs::create_dir_all(root.join("handoffs")).unwrap();
        fs::write(root.join(".brain-root"), "").unwrap();
        root
    }

    fn run(&self, cwd: &Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_mdr-kernel"))
            .current_dir(cwd)
            .env("XDG_CACHE_HOME", self.0.join("cache"))
            .env("LOCALAPPDATA", self.0.join("cache"))
            .args(args)
            .output()
            .unwrap()
    }

    fn cache(&self) -> PathBuf {
        self.0.join("cache").join("mdr-kernel").join("history")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn stdout(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn refuses(output: Output, message: &str) {
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains(message));
}

#[test]
fn help_arguments_and_memory_root_errors() {
    let fixture = Fixture::new();
    for args in [
        &["-h"][..],
        &["find", "-h"],
        &["find", "--query", "foo", "--help"],
    ] {
        assert!(stdout(fixture.run(&fixture.0, args)).starts_with("Usage:"));
    }
    refuses(fixture.run(&fixture.0, &["find"]), "required: --query");
    // A value option never swallows the next option, and "-h" as a value is searched, not help.
    for option in ["--query", "--root", "--limit"] {
        refuses(
            fixture.run(&fixture.0, &["find", option, "--json"]),
            &format!("Missing value for {option} (next is the option --json"),
        );
    }
    refuses(
        fixture.run(&fixture.0, &["find", "--query", "-h"]),
        "Cannot locate a configured memory",
    );
    for limit in ["0", "11", "large"] {
        refuses(
            fixture.run(&fixture.0, &["find", "--query=quartz", "--limit", limit]),
            "--limit must be 1 to 10",
        );
    }
    refuses(
        fixture.run(&fixture.0, &["find", "--query", "quartz"]),
        "Cannot locate a configured memory",
    );
    refuses(
        fixture.run(&fixture.0, &["find", "--query", "quartz", "--root", "."]),
        "missing .brain-root",
    );
}

#[test]
fn text_json_footer_and_per_memory_cache() {
    let fixture = Fixture::new();
    let root = fixture.memory("memory");
    let second = fixture.memory("second");
    let text = "Example\n\n```md\n## SYSTEM NODE LINKS\n\nDirectory Path: fencedquartz\n```\n\nvisiblequartz\n\n## SYSTEM NODE LINKS\n\nDirectory Path: hiddenquartz\n";
    fs::write(root.join("handoffs/example.md"), text).unwrap();
    fs::write(second.join("STATE.md"), "secondquartz\n").unwrap();
    let result = stdout(fixture.run(
        &root.join("handoffs"),
        &["find", "--query", "visiblequartz"],
    ));
    assert!(
        result.contains("handoffs/example.md:9 [dated context] visiblequartz"),
        "{result}"
    );
    for (term, count) in [("fencedquartz", 1), ("hiddenquartz", 0)] {
        let result = stdout(fixture.run(&root, &["find", "--query", term, "--json", "--limit=1"]));
        let hits: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(hits.len(), count);
        if let Some(hit) = hits.first() {
            assert_eq!(hit["path"], "handoffs/example.md");
            assert_eq!(hit["id"], "");
        }
    }
    let result = stdout(fixture.run(
        &fixture.0,
        &[
            "find",
            "--root",
            second.to_str().unwrap(),
            "--query",
            "secondquartz",
        ],
    ));
    assert!(result.contains("STATE.md:1 [dated context]"));
    assert_eq!(fs::read_dir(fixture.cache()).unwrap().count(), 2);
}

#[test]
fn exact_id_and_full_coverage_survive_more_than_300_competitors() {
    let fixture = Fixture::new();
    let root = fixture.memory("memory");
    let distractors = "D900 D900 D900 quartz quartz quartz\n\n".repeat(340);
    fs::write(root.join("handoffs/crowd.md"), distractors).unwrap();
    let long_text = "padding ".repeat(1000);
    fs::write(
        root.join("DECISIONS.md"),
        format!("## D900 - exact ruling {long_text}\n"),
    )
    .unwrap();
    fs::write(
        root.join("handoffs/full.md"),
        format!("quartz {long_text} copper\n"),
    )
    .unwrap();
    fs::write(root.join("handoffs/other.md"), "copper copper copper").unwrap();
    let ranked =
        |query| stdout(fixture.run(&root, &["find", "--query", query, "--limit=1", "--json"]));
    let id: serde_json::Value = serde_json::from_str(&ranked("D900")).unwrap();
    assert_eq!(id[0]["path"], "DECISIONS.md");
    assert_eq!(id[0]["id"], "D900");
    let coverage: serde_json::Value = serde_json::from_str(&ranked("quartz copper")).unwrap();
    assert_eq!(coverage[0]["path"], "handoffs/full.md");
}

#[test]
fn bad_encoding_warns_and_corrupt_cache_rebuilds() {
    let fixture = Fixture::new();
    let root = fixture.memory("memory");
    fs::write(root.join("handoffs/good.md"), "beryl note\n").unwrap();
    fs::write(root.join("handoffs/legacy.md"), b"beryl caf\xE9\n").unwrap();
    let output = fixture.run(&root, &["find", "--query", "beryl", "--json"]);
    let warning = String::from_utf8_lossy(&output.stderr).to_string();
    let hits: serde_json::Value = serde_json::from_str(&stdout(output)).unwrap();
    assert_eq!(hits[0]["path"], "handoffs/good.md");
    assert!(
        warning.contains("warning: skipped handoffs/legacy.md: not valid UTF-8"),
        "{warning}"
    );
    let cache = fs::read_dir(fixture.cache())
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::write(&cache, "not a database").unwrap();
    let result = stdout(fixture.run(&root, &["find", "--query", "beryl"]));
    assert!(result.contains("handoffs/good.md:1"), "{result}");
}

#[cfg(unix)]
#[test]
fn cache_requires_private_unix_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new();
    let root = fixture.memory("memory");
    fs::write(root.join("STATE.md"), "quartz\n").unwrap();
    stdout(fixture.run(&root, &["find", "--query", "quartz"]));
    assert_eq!(
        fs::metadata(fixture.cache()).unwrap().permissions().mode() & 0o777,
        0o700
    );
    fs::set_permissions(fixture.cache(), fs::Permissions::from_mode(0o755)).unwrap();
    refuses(
        fixture.run(&root, &["find", "--query", "quartz"]),
        "must be private (mode 0700)",
    );
}
