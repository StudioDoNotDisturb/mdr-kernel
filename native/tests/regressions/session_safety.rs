use super::*;

const MODEL: &str = "test-model";

fn env_run(root: &Path, program: &str, args: &[&str], env: &[(&str, &str)]) -> (i32, String) {
    result(
        command(program, root, args)
            .envs(env.iter().copied())
            .stdin(Stdio::null())
            .output()
            .unwrap(),
    )
}

fn lock(root: &Path, args: &[&str]) -> (i32, String) {
    run(root, &[&["state-lock"], args].concat())
}

fn acquire(root: &Path) -> String {
    let out = lock(
        root,
        &[
            "-Acquire",
            "-Model",
            MODEL,
            "-Objective",
            "isolated lifecycle test",
        ],
    );
    let token = out
        .1
        .lines()
        .find_map(|line| line.strip_prefix("session token:"))
        .map(str::trim);
    check(
        "acquire prints a fresh session proof",
        out.0 == 0 && token.is_some_and(|value| !value.is_empty()),
        &out,
    );
    token.unwrap().into()
}

#[test]
fn lock_proof_and_recovery() {
    let sandbox = Sandbox::new();
    let root = sandbox.clone_template("lock");
    git(&root, &["config", "brain.model", MODEL]);
    let token = acquire(&root);
    let path = root.join("STATE.lock");
    let sealed = fs::read(&path).unwrap();
    let actions = ["-Acquire", "-Heartbeat", "-Release", "-CheckOwner"];
    for action in actions {
        for (name, args) in [
            (
                "refuses config identity without changing lock",
                vec![action, "-Token", &token],
            ),
            (
                "refuses another same-model session",
                vec![action, "-Model", MODEL, "-Token", "wrong-token"],
            ),
        ] {
            let out = lock(&root, &args);
            check(
                &format!("{action} {name}"),
                out.0 == 1 && fs::read(&path).unwrap() == sealed,
                out,
            );
        }
    }
    let owner = lock(&root, &["-CheckOwner", "-Model", MODEL, "-Token", &token]);
    authority_ok(
        &owner,
        "CheckOwner verifies explicit identity and our acquired token",
        Some(0),
        &["LOCK_OWNER=VERIFIED"],
    );
    let original: serde_json::Value = serde_json::from_slice(&sealed).unwrap();
    let mut tokenless = original.clone();
    tokenless.as_object_mut().unwrap().remove("token");
    let tokenless = serde_json::to_vec(&tokenless).unwrap();
    fs::write(&path, &tokenless).unwrap();
    for action in actions {
        let out = lock(&root, &[action, "-Model", MODEL, "-Token", &token]);
        check(
            &format!("{action} refuses tokenless legacy ownership"),
            out.0 == 1 && fs::read(&path).unwrap() == tokenless,
            out,
        );
    }
    let mut foreign = original.clone();
    foreign["host"] = "unrelated-fixture-host".into();
    fs::write(&path, serde_json::to_vec(&foreign).unwrap()).unwrap();
    authority_ok(
        &lock(&root, &["-CheckOwner", "-Model", MODEL, "-Token", &token]),
        "matching token does not override a different recorded host",
        Some(1),
        &[],
    );
    fs::write(&path, &sealed).unwrap();
    let forced = ["state-lock", "-Release", "-Force", "-Model", MODEL];
    let out = env_run(&root, ENGINE, &forced, &[("MDR_FORCE_LIVE_LOCK", "1")]);
    check(
        "live recovery valve alone cannot prove abandonment",
        out.0 == 1 && path.exists(),
        out,
    );
    let evidence = [
        "-AbandonmentReason",
        "Fixture has independent test recovery evidence",
    ];
    let out = run(&root, &[&forced[..], &evidence].concat());
    check(
        "recovery reason alone cannot take a fresh-heartbeat lock without the live valve",
        out.0 == 1 && out.1.contains("presumed ALIVE") && fs::read(&path).unwrap() == sealed,
        out,
    );
    let mut stale = original;
    stale["heartbeat_at"] = "2000-01-01T00:00:00Z".into();
    let stale = serde_json::to_vec(&stale).unwrap();
    fs::write(&path, &stale).unwrap();
    let out = lock(
        &root,
        &[
            "-Acquire",
            "-Model",
            MODEL,
            "-Token",
            "another-session-token",
        ],
    );
    check(
        "old-but-live same-model holder is not taken over by acquire",
        out.0 == 1
            && out.1.contains("abandonment is NOT PROVEN")
            && fs::read(&path).unwrap() == stale,
        out,
    );
    let out = run(&root, &forced);
    check(
        "stale heartbeat alone cannot authorize forced release",
        out.0 == 1
            && out.1.contains("AbandonmentReason must record")
            && fs::read(&path).unwrap() == stale,
        out,
    );
    let out = run(&root, &[&forced[..], &evidence].concat());
    check(
        "explicit independent recovery evidence permits stale recovery",
        out.0 == 0 && !path.exists(),
        out,
    );
    fs::write(&path, "").unwrap();
    authority_ok(
        &lock(&root, &["-CheckOwner", "-Model", MODEL, "-Token", &token]),
        "empty lock never proves ownership",
        Some(1),
        &[],
    );
    let out = run(&root, &[&forced[..], &evidence].concat());
    check(
        "unreadable lock can be recovered only by explicit evidence",
        out.0 == 0 && !path.exists(),
        out,
    );
}

#[test]
fn hook_attribution() {
    let sandbox = Sandbox::new();
    let root = sandbox.clone_template("attribution");
    git(&root, &["config", "brain.model", MODEL]);
    let path = sandbox.0.join("message.txt");
    let args = ["hook", "prepare-commit-msg", path.to_str().unwrap()];
    let initial = "Fixture commit\n";
    fs::write(&path, initial).unwrap();
    let out = run(&root, &args);
    check(
        "config-only attribution refuses without modifying the message",
        out.0 == 1 && fs::read_to_string(&path).unwrap() == initial,
        out,
    );
    let model = [("BRAIN_MODEL", MODEL)];
    assert_eq!(env_run(&root, ENGINE, &args, &model).0, 0);
    let out = env_run(&root, ENGINE, &args, &model);
    let trailer_count = || {
        fs::read_to_string(&path)
            .unwrap()
            .lines()
            .filter(|line| *line == "Brain-Model: test-model")
            .count()
    };
    check(
        "explicit session identity adds exactly one trailer",
        out.0 == 0 && trailer_count() == 1,
        out,
    );
    fs::write(&path, "Fixture commit\n\nBrain-Model: another-model\n").unwrap();
    authority_ok(
        &env_run(&root, ENGINE, &args, &model),
        "explicit identity and trailer disagreement refuses",
        Some(1),
        &[],
    );
    authority_ok(
        &run(&root, &args),
        "explicit message trailer alone remains supported",
        Some(0),
        &[],
    );
    for (kind, trailer) in [
        ("blank", "Brain-Model: \n"),
        ("unknown", "Brain-Model: unknown\n"),
        ("duplicate", "Brain-Model: one\nBrain-Model: two\n"),
    ] {
        fs::write(&path, format!("{initial}\n{trailer}")).unwrap();
        authority_ok(
            &run(&root, &args),
            &format!("blank, unknown or duplicate attribution refuses ({kind})"),
            Some(1),
            &[],
        );
    }
    fs::write(&path, initial).unwrap();
    let out = env_run(&root, ENGINE, &args, &[("BRAIN_MODEL", "  test-model  ")]);
    check(
        "BRAIN_MODEL whitespace is trimmed in the trailer",
        out.0 == 0 && trailer_count() == 1,
        out,
    );
    fs::write(&path, initial).unwrap();
    let out = env_run(&root, ENGINE, &args, &[("BRAIN_MODEL", "unknown")]);
    check(
        "BRAIN_MODEL=unknown is refused without modifying the message",
        out.0 == 1 && fs::read_to_string(&path).unwrap() == initial,
        out,
    );
}

#[test]
fn bootstrap_commit_and_close_boundaries() {
    let sandbox = Sandbox::new();
    let root = sandbox.clone_template("close");
    let lock_path = root.join("STATE.lock");
    let status = || git(&root, &["status", "--porcelain", "--untracked-files=all"]);
    let indexed = || git(&root, &["write-tree"]);
    let head = || git(&root, &["rev-parse", "HEAD"]);
    let tracked = |path| !git(&root, &["ls-files", "--", path]).is_empty();
    let restore = |path| {
        git(&root, &["restore", "--staged", "--", path]);
    };
    let commit_paths = || git(&root, &["show", "--pretty=format:", "--name-only", "HEAD"]);
    git(&root, &["config", "brain.model", MODEL]);
    let status_before = status();
    let out = run(&root, &["bootstrap-clone", "-CommitScaffold"]);
    check(
        "bootstrap refuses config identity before creating scaffold or lock",
        out.0 == 1 && status_before == status() && !lock_path.exists(),
        out,
    );
    put(&root, "bootstrap-foreign.txt", "unrelated");
    git(&root, &["add", "bootstrap-foreign.txt"]);
    let before = (indexed(), head());
    let bootstrap = ["bootstrap-clone", "-Model", MODEL, "-CommitScaffold"];
    let out = run(&root, &bootstrap);
    check(
        "bootstrap preserves and refuses a preexisting foreign index",
        out.0 == 1 && before == (indexed(), head()) && !lock_path.exists(),
        out,
    );
    restore("bootstrap-foreign.txt");
    fs::remove_file(root.join("bootstrap-foreign.txt")).unwrap();
    put(&root, "setup-foreign.txt", "unrelated");
    let out = run(&root, &["setup-private", "-LocalOnly", "-Model", MODEL]);
    assert_eq!(out.0, 0, "{out:?}");
    let allowed = [
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
    let paths = commit_paths();
    check(
        "scaffold commit holds only scaffold outputs",
        !paths.trim().is_empty()
            && paths
                .lines()
                .filter(|line| !line.is_empty())
                .all(|line| allowed.contains(&line)),
        paths,
    );
    check(
        "scaffold commit leaves a foreign untracked file untracked",
        root.join("setup-foreign.txt").exists() && !tracked("setup-foreign.txt"),
        status(),
    );
    let token = acquire(&root);
    let out = env_run(&root, ENGINE, &bootstrap, &[("BRAIN_LOCK_TOKEN", &token)]);
    let owner = lock(&root, &["-CheckOwner", "-Model", MODEL, "-Token", &token]);
    check(
        "bootstrap keeps a lock its caller already owned",
        out.0 == 0 && owner.0 == 0,
        (out, owner),
    );
    assert_eq!(
        lock(&root, &["-Release", "-Model", MODEL, "-Token", &token]).0,
        0
    );
    let out = env_run(
        &root,
        ENGINE,
        &bootstrap,
        &[("BRAIN_LOCK_TOKEN", "leftover-token-from-a-finished-session")],
    );
    check(
        "bootstrap with a leftover token acquires and releases only its own lock",
        out.0 == 0 && !lock_path.exists(),
        out,
    );
    fs::remove_file(root.join("setup-foreign.txt")).unwrap();
    check(
        "close fixture has no remote destination",
        git(&root, &["remote"]).is_empty(),
        root.display(),
    );
    let fixture_head = head();
    let token = acquire(&root);
    let body = fs::read(&lock_path).unwrap();
    let mut state = fs::read_to_string(root.join("STATE.md")).unwrap();
    state.push_str("\nCoordination hook fixture.\n");
    fs::write(root.join("STATE.md"), state).unwrap();
    git(&root, &["add", "STATE.md"]);
    let hook = |env: &[(&str, &str)]| env_run(&root, ENGINE, &["hook", "pre-commit"], env);
    let model = [("BRAIN_MODEL", MODEL)];
    let proof = [("BRAIN_MODEL", MODEL), ("BRAIN_LOCK_TOKEN", token.as_str())];
    authority_ok(
        &hook(&[("BRAIN_LOCK_TOKEN", &token)]),
        "coordination hook refuses config-only identity even with correct token",
        Some(1),
        &["explicit BRAIN_MODEL"],
    );
    authority_ok(
        &hook(&model),
        "coordination hook refuses missing session proof",
        Some(1),
        &["BRAIN_LOCK_TOKEN"],
    );
    let mut lock_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    lock_json.as_object_mut().unwrap().remove("token");
    fs::write(&lock_path, serde_json::to_vec(&lock_json).unwrap()).unwrap();
    authority_ok(
        &hook(&proof),
        "coordination hook refuses a tokenless lock record",
        Some(1),
        &["BRAIN_LOCK_TOKEN"],
    );
    fs::write(&lock_path, &body).unwrap();
    authority_ok(
        &hook(&proof),
        "coordination hook accepts only verified session ownership",
        Some(0),
        &["LOCK_OWNER=VERIFIED"],
    );
    authority_ok(
        &hook(&[("BRAIN_MODEL", "other-model"), ("BRAIN_LOCK_TOKEN", &token)]),
        "coordination hook refuses the right token under a different model",
        Some(1),
        &["held by 'test-model', not 'other-model'"],
    );
    assert_eq!(
        lock(&root, &["-Release", "-Model", MODEL, "-Token", &token]).0,
        0
    );
    let commit = |message: &str, env: &[(&str, &str)]| {
        env_run(&root, "git", &["commit", "-qm", message], env)
    };
    let out = commit("Coordination commit without a lock", &model);
    check(
        "real commit of a coordination file without the lock is refused",
        out.0 != 0 && out.1.contains("STATE.lock is not held") && fixture_head == head(),
        out,
    );
    git(
        &root,
        &["restore", "--staged", "--worktree", "--", "STATE.md"],
    );
    put(&root, "ordinary.txt", "Ordinary non-coordination work.");
    git(&root, &["add", "ordinary.txt"]);
    let out = commit(
        "Ordinary commit without a lock\n\nLeast-file: fixture file for the ordinary-commit test",
        &model,
    );
    check(
        "real ordinary commit needs no lock and gets one trailer",
        out.0 == 0
            && git(&root, &["log", "-1", "--format=%B"])
                .lines()
                .filter(|line| *line == "Brain-Model: test-model")
                .count()
                == 1,
        out,
    );
    git(&root, &["rm", "-q", "ordinary.txt"]);
    authority_ok(
        &commit("Commit with no identity at all", &[]),
        "real commit with neither BRAIN_MODEL nor trailer is refused",
        None,
        &["set BRAIN_MODEL or include"],
    );
    authority_ok(
        &commit("Remove ordinary fixture\n\nBrain-Model: test-model", &[]),
        "real commit with only an explicit message trailer is accepted",
        Some(0),
        &[],
    );
    put(
        &root,
        "untrailed.txt",
        "A new file with no Least-file trailer.",
    );
    git(&root, &["add", "untrailed.txt"]);
    authority_ok(
        &commit(
            "Add a file without a trailer\n\nBrain-Model: test-model",
            &[],
        ),
        "engine: a new file without a Least-file trailer is refused",
        None,
        &["Least-file:"],
    );
    restore("untrailed.txt");
    fs::remove_file(root.join("untrailed.txt")).unwrap();
    let main = root.join("native/src/main.rs");
    let mut source = fs::read_to_string(&main).unwrap();
    source.push_str("\n// drift\n");
    fs::write(main, source).unwrap();
    git(&root, &["add", "native/src/main.rs"]);
    authority_ok(
        &hook(&[]),
        "engine: a binary built from other sources refuses to guard the commit",
        Some(1),
        &["different native/ sources"],
    );
    git(
        &root,
        &[
            "restore",
            "--staged",
            "--worktree",
            "--",
            "native/src/main.rs",
        ],
    );

    let close = |args: &[&str]| run(&root, &[&["close-session", "-Model", MODEL], args].concat());
    let fixture_head = head();
    put(&root, "foreign.txt", "Another session owns this change.");
    git(&root, &["add", "foreign.txt"]);
    let before = indexed();
    let out = close(&["-NoHandoff"]);
    check(
        "close refuses foreign staging without unstaging or committing it",
        out.0 == 1
            && out.1.contains("outside this close set")
            && indexed() == before
            && head() == fixture_head
            && !lock_path.exists()
            && !out.1.contains("acquired STATE.lock"),
        out,
    );
    restore("foreign.txt");
    let out = close(&["-NoHandoff"]);
    check(
        "foreign-only dirty tree creates no artificial map commit",
        out.0 == 0 && head() == fixture_head && root.join("foreign.txt").exists(),
        out,
    );
    check(
        "close leaves a foreign untracked file unstaged and untracked",
        !tracked("foreign.txt") && git(&root, &["diff", "--cached", "--name-only"]).is_empty(),
        status(),
    );
    put(
        &root,
        "inbox/foreign-note.md",
        "Another session owns this inbox note.",
    );
    put(&root, "owned one.txt", "First owned change.");
    put(&root, "owned two.txt", "Second owned change.");
    let out = close(&[
        "-NoHandoff",
        "-Also",
        "\"owned one.txt\", \"owned two.txt\"",
        "-LeastFile",
        "owned fixture files for the Also test",
    ]);
    let paths = commit_paths();
    check(
        "quoted comma-separated Also paths both reach the checkpoint",
        out.0 == 0
            && paths.lines().any(|line| line == "owned one.txt")
            && paths.lines().any(|line| line == "owned two.txt"),
        out,
    );
    check(
        "unowned inbox and unrelated work remain outside close",
        !paths.contains("foreign") && root.join("inbox/foreign-note.md").exists(),
        paths,
    );
    fs::remove_file(root.join("foreign.txt")).unwrap();
    fs::remove_file(root.join("inbox/foreign-note.md")).unwrap();
    let token = acquire(&root);
    let lock_time: serde_json::Value =
        serde_json::from_slice(&fs::read(&lock_path).unwrap()).unwrap();
    let date = &lock_time["acquired_at"].as_str().unwrap()[..10];
    assert_eq!(
        lock(&root, &["-Release", "-Model", MODEL, "-Token", &token]).0,
        0
    );
    let relative = format!("handoffs/{date}-dry-run.md");
    let handoff = "Title: Dry run fixture\nType: Handoff\nContext: Tests ownership before handoff repair.\n\n## SYSTEM NODE LINKS\n\nDirectory Path: handoffs/\nNext Sequential File: none\nRelated Workflows: none\nAI Execution Prompts: none\n";
    put(&root, &relative, handoff);
    let index_path = root.join("handoffs/INDEX.md");
    let mut index = fs::read_to_string(&index_path).unwrap();
    index.push_str(&format!("\n{date}-dry-run.md\n"));
    fs::write(index_path, index).unwrap();
    let untouched = || fs::read_to_string(root.join(&relative)).unwrap() == handoff;
    let out = close(&["-Topic", "dry-run", "-DryRun"]);
    check(
        "DryRun never repairs a handoff or leaves a lock",
        out.0 == 0 && untouched() && !lock_path.exists(),
        out,
    );
    let other = acquire(&root);
    let out = close(&["-Topic", "dry-run"]);
    check(
        "unproven owner cannot repair handoff before close refuses",
        out.0 == 1 && out.1.contains("LOCK_OWNER=UNPROVEN") && untouched(),
        out,
    );
    authority_ok(
        &lock(&root, &["-Release", "-Model", MODEL, "-Token", &other]),
        "test holder releases its own lock by acquired proof",
        Some(0),
        &[],
    );
    put(&root, "staged-foreign.txt", "Another session staged this.");
    git(&root, &["add", "staged-foreign.txt"]);
    let out = close(&["-Topic", "dry-run"]);
    check(
        "foreign staging refuses before handoff repair or lock acquire",
        out.0 == 1
            && out.1.contains("outside this close set")
            && untouched()
            && !lock_path.exists(),
        out,
    );
    restore("staged-foreign.txt");
    let before = head();
    let out = close(&["-Topic", "dry-run"]);
    let paths = commit_paths();
    check(
        "handoff close commits exactly the close set and releases the lock",
        out.0 == 0
            && head() != before
            && paths.contains(&relative)
            && paths.contains("handoffs/INDEX.md")
            && paths.lines().filter(|line| !line.is_empty()).all(|line| {
                line == relative || line == "handoffs/INDEX.md" || line.starts_with("maps/")
            })
            && !lock_path.exists(),
        (out, paths),
    );
    check(
        "handoff close leaves foreign untracked work in place",
        root.join("staged-foreign.txt").exists() && !tracked("staged-foreign.txt"),
        status(),
    );
}

#[test]
fn hook_fails_closed_without_git_or_policy() {
    let sandbox = Sandbox::new();
    let root = sandbox.clone_template("missing-gates");
    let hook = ["hook", "pre-commit"];
    let out = env_run(
        &root,
        ENGINE,
        &hook,
        &[("PATH", sandbox.0.to_str().unwrap())],
    );
    check(
        "pre-commit fails closed when a required utility is absent",
        out.0 == 1 && out.1.to_lowercase().contains("git"),
        out,
    );
    fs::remove_file(root.join("protocol/coordination-files.txt")).unwrap();
    authority_ok(
        &run(&root, &hook),
        "pre-commit fails closed when a gate helper is absent",
        Some(1),
        &["protocol/coordination-files.txt is missing"],
    );
}
