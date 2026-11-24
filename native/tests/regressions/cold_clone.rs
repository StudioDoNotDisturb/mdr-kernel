use super::*;

#[test]
fn cold_clone_bootstraps_and_starts_clean() {
    let sandbox = Sandbox::new();
    let root = sandbox.clone_template("cold");
    git(&root, &["config", "core.hooksPath", ".githooks"]);
    let bootstrap = run(
        &root,
        &["bootstrap-clone", "-Model", "demo", "-CommitScaffold"],
    );
    assert_eq!(bootstrap.0, 0, "{}", bootstrap.1);

    let startup = run(&root, &["start-session", "-Model", "demo"]);
    assert_eq!(startup.0, 0, "{}", startup.1);
    assert_eq!(
        startup
            .1
            .lines()
            .filter(|line| line.starts_with("STARTUP_STATUS="))
            .collect::<Vec<_>>(),
        ["STARTUP_STATUS=READY"],
        "{}",
        startup.1
    );
    // First-run advisories cover optional host recall, the bootstrap lock, and the shell identity.
    for line in bootstrap.1.lines().chain(startup.1.lines()) {
        assert!(!line.starts_with("NOTE="), "{line}");
        if let Some(warning) = line.strip_prefix("[warn] ") {
            assert!(
                warning.starts_with("prompt recall not configured;")
                    || warning == "Brain-Model identity unset - set $BRAIN_MODEL for this process before lock/commit."
                    || (warning.starts_with("STATE.lock held (heartbeat 0m ago): demo on ")
                        && warning.ends_with("(objective: initialize or repair mdr-kernel memory scaffold) - coordination files are single-writer; do not edit them unless this is your lock")),
                "unexpected cold-clone warning: {warning}"
            );
        }
    }
    assert!(git(&root, &["status", "--porcelain", "--untracked-files=all"]).is_empty());
    assert!(git(&root, &["remote"]).is_empty());
    assert!(!root.join("STATE.lock").exists());

    put(&root, "handoffs/ci-search.md", "fixture quartz");
    let search = run(&root, &["find", "--query", "quartz", "--json"]);
    assert_eq!(search.0, 0, "{}", search.1);
    let hits: serde_json::Value = serde_json::from_str(&search.1).unwrap();
    assert_eq!(hits[0]["path"], "handoffs/ci-search.md");
    fs::remove_file(root.join("handoffs/ci-search.md")).unwrap();
    assert!(git(&root, &["status", "--porcelain", "--untracked-files=all"]).is_empty());
}

#[test]
fn initialized_memory_can_seed_a_fresh_regression_fixture() {
    let sandbox = Sandbox::new();
    let adopted = sandbox.clone_template("adopted");
    let bootstrap = run(
        &adopted,
        &["bootstrap-clone", "-Model", "demo", "-CommitScaffold"],
    );
    assert_eq!(bootstrap.0, 0, "{}", bootstrap.1);
    put(&adopted, "inbox/private-note.md", "fixture adopter data");
    put(&adopted, ".codex/hooks.json", "fixture host config");
    git(
        &adopted,
        &["add", "--", "inbox/private-note.md", ".codex/hooks.json"],
    );
    git(
        &adopted,
        &[
            "-c",
            "core.hooksPath=.no-hooks",
            "commit",
            "-qm",
            "Add adopter data",
        ],
    );

    let fresh = sandbox.clone_template_from(&adopted, "fresh");
    assert!(adopted.join("STATE.md").exists());
    assert!(adopted.join("inbox/private-note.md").exists());
    for path in [
        "STATE.md",
        "DECISIONS.md",
        "inbox/private-note.md",
        ".codex/hooks.json",
    ] {
        assert!(!fresh.join(path).exists(), "{path}");
    }
    assert!(fs::read_to_string(fresh.join("AGENTS.md"))
        .unwrap()
        .contains("phase-zero"));
    git(&fresh, &["config", "core.hooksPath", ".githooks"]);
    let bootstrap = run(
        &fresh,
        &["bootstrap-clone", "-Model", "demo", "-CommitScaffold"],
    );
    assert_eq!(bootstrap.0, 0, "{}", bootstrap.1);
    assert_eq!(
        git(&fresh, &["log", "-1", "--format=%s"]).trim(),
        "Initialize mdr-kernel memory scaffold"
    );
    assert!(git(&fresh, &["status", "--porcelain"]).is_empty());
}
