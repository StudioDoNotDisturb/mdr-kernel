use super::*;

#[test]
fn pre_commit_checks_staged_gate_data_and_native_wiring() {
    let sandbox = Sandbox::new();
    let root = sandbox.clone_template("gates");
    let gate = "protocol/recall-gates.json";
    let out = run(
        &root,
        &["bootstrap-clone", "-Model", "demo", "-CommitScaffold"],
    );
    assert_eq!(out.0, 0, "{}", out.1);
    let valid = fs::read(root.join(gate)).unwrap();
    put(&root, gate, "{}");
    git(&root, &["add", "--", gate]);
    put(&root, gate, valid);

    let acquire = run(
        &root,
        &[
            "state-lock",
            "-Acquire",
            "-Model",
            "demo",
            "-Objective",
            "exercise staged gates",
        ],
    );
    assert_eq!(acquire.0, 0, "{}", acquire.1);
    let token = acquire
        .1
        .lines()
        .find_map(|line| line.strip_prefix("session token: "))
        .unwrap();
    let hook = |root| {
        result(
            command(ENGINE, root, &["hook", "pre-commit"])
                .env("BRAIN_MODEL", "demo")
                .env("BRAIN_LOCK_TOKEN", token)
                .output()
                .unwrap(),
        )
    };
    authority_ok(
        &hook(&root),
        "invalid staged gate despite valid worktree",
        Some(1),
        &["staged regression checks failed", gate],
    );
    git(&root, &["restore", "--staged", "--", gate]);
    assert_eq!(hook(&root).0, 0);
}

#[test]
fn doctor_and_optional_diagnostic_keep_their_status() {
    let sandbox = Sandbox::new();
    let root = sandbox.clone_template("diagnostic");
    let out = run(&root, &["waffle-party", "-HardOnly"]);
    authority_ok(
        &out,
        "doctor succeeds",
        Some(0),
        &["Waffle Party protocol authorized"],
    );
    let coverage = run(&root, &["secret-audit", "-CheckScannerCoverage"]);
    authority_ok(
        &coverage,
        "native scanner coverage",
        Some(0),
        &["all cover"],
    );
    put(&root, ".githooks/pre-commit", "#!/bin/sh\nexit 0\n");
    let out = run(&root, &["waffle-party", "-HardOnly"]);
    authority_ok(
        &out,
        "doctor failure propagates",
        Some(1),
        &["does not dispatch to the native engine", "protocol denied"],
    );
}

#[test]
fn git_hook_refuses_when_no_binary_is_installed() {
    let sandbox = Sandbox::new();
    let root = sandbox.clone_template("missing-engine");
    let binary = if cfg!(windows) {
        "bin/mdr-kernel.exe"
    } else {
        "bin/mdr-kernel"
    };
    fs::remove_file(root.join(binary)).unwrap();
    // Git invokes its bundled shell on Windows. Stub the one Git query after clearing PATH.
    let out = result(command("git", &root, &[
        "-c",
        "alias.missing-engine=!git () { pwd; }; PATH=; export PATH; . ./.githooks/pre-commit",
        "missing-engine",
    ]).output().unwrap());
    authority_ok(
        &out,
        "native hook fails closed without engine",
        Some(1),
        &["install the native binary"],
    );
}

#[test]
#[ignore = "requires live GitHub visibility access"]
fn live_public_repo_is_refused() {
    let sandbox = Sandbox::new();
    let root = sandbox.clone_template("live-privacy");
    let out = run(
        &root,
        &[
            "remote-privacy",
            "-RemoteUrl",
            "https://github.com/octocat/Hello-World.git",
        ],
    );
    authority_ok(
        &out,
        "live public visibility refusal",
        Some(1),
        &[
            "REMOTE_PRIVACY_STATUS=NOT_PRIVATE",
            "GitHub reports visibility PUBLIC",
        ],
    );
}
