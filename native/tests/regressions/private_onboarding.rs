use super::*;

const MODEL: &str = "test-model";
const LEAST: &[&str] = &["-LeastFile", "isolated close fixture"];
const RECOVERY: &[&str] = &[
    "-Release",
    "-Force",
    "-Model",
    MODEL,
    "-AbandonmentReason",
    "Fixture operation finished",
];

fn with_env(root: &Path, program: &str, args: &[&str], env: &[(&str, &str)]) -> (i32, String) {
    result(
        command(program, root, args)
            .envs(env.iter().copied())
            .output()
            .unwrap(),
    )
}

fn append(root: &Path, path: &str, addition: &str) {
    let old = fs::read_to_string(root.join(path)).unwrap();
    put(root, path, format!("{old}\n{addition}\n"));
}

fn replace(root: &Path, path: &str, old: &str, new: &str) {
    let before = fs::read_to_string(root.join(path)).unwrap();
    assert!(before.contains(old), "missing fixture text {old:?}");
    put(root, path, before.replace(old, new));
}

fn config(root: &Path, key: &str) -> String {
    let out = with_env(root, "git", &["config", "--local", "--get", key], &[]);
    assert!(out.0 == 0 || out.0 == 1, "{out:?}");
    out.1.trim().into()
}

fn unset(root: &Path, key: &str) {
    let out = with_env(root, "git", &["config", "--local", "--unset-all", key], &[]);
    assert!(out.0 == 0 || out.0 == 5, "{out:?}");
}

fn close(root: &Path, args: &[&str]) -> (i32, String) {
    run(
        root,
        &[&["close-session", "-NoHandoff", "-Model", MODEL], args].concat(),
    )
}

fn expect(out: &(i32, String), name: &str, code: i32) {
    authority_ok(out, name, Some(code), &[]);
}

fn token(output: &str) -> &str {
    output
        .lines()
        .find_map(|line| line.strip_prefix("session token:"))
        .map(str::trim)
        .unwrap()
}

fn install(root: &Path) {
    fs::create_dir_all(root.join("bin")).unwrap();
    fs::copy(
        ENGINE,
        root.join(if cfg!(windows) {
            "bin/mdr-kernel.exe"
        } else {
            "bin/mdr-kernel"
        }),
    )
    .unwrap();
}

struct Private {
    sandbox: Sandbox,
    root: PathBuf,
    remote: PathBuf,
}

impl Private {
    fn new() -> Self {
        let sandbox = Sandbox::new();
        let root = sandbox.clone_template("local");
        let remote = sandbox.0.join("private.git");
        git(
            &sandbox.0,
            &[
                "clone",
                "-q",
                "--bare",
                "--",
                root.to_str().unwrap(),
                remote.to_str().unwrap(),
            ],
        );
        git(
            &root,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        git(&root, &["config", "core.hooksPath", ".githooks"]);
        Self {
            sandbox,
            root,
            remote,
        }
    }

    fn setup(&self) -> (i32, String) {
        run(
            &self.root,
            &["setup-private", "-Model", MODEL, "-ConfirmPrivateRemote"],
        )
    }

    fn clone(&self, name: &str) -> PathBuf {
        let path = self.sandbox.0.join(name);
        git(
            &self.sandbox.0,
            &[
                "clone",
                "-q",
                "--",
                self.remote.to_str().unwrap(),
                path.to_str().unwrap(),
            ],
        );
        git(&path, &["branch", "-M", "main"]);
        configure(&path);
        install(&path);
        path
    }

    fn remote_head(&self) -> String {
        git(&self.remote, &["rev-parse", "main"])
    }

    fn remote_file(&self, path: &str) -> String {
        git(&self.remote, &["show", &format!("main:{path}")])
    }

    fn set_origin(&self, url: &str) {
        git(&self.root, &["remote", "set-url", "origin", url]);
    }

    fn pending(&self) -> bool {
        config(&self.root, "brain.pendingSync") == "true"
    }

    fn discard_failed_close(&self) {
        let out = with_env(
            &self.root,
            ENGINE,
            &[&["state-lock"], RECOVERY].concat(),
            &[("MDR_FORCE_LIVE_LOCK", "1")],
        );
        assert_eq!(out.0, 0, "{out:?}");
        git(&self.root, &["reset", "--hard", "-q", "origin/main"]);
        git(&self.root, &["stash", "clear"]);
        unset(&self.root, "brain.pendingSync");
    }
}

#[test]
fn setup_start_and_destination_boundaries() {
    let f = Private::new();
    let root = &f.root;
    check(
        "fresh clone ships phase-zero doors routing to setup-private",
        ["CLAUDE.md", "AGENTS.md", "GEMINI.md"].iter().all(|name| {
            let door = fs::read_to_string(root.join(name)).unwrap();
            door.contains("setup-private")
                && door.contains("phase-zero")
                && !door.contains("BEGIN BRAIN-CONTRACT")
        }),
        root,
    );
    let out = run(root, &["start-session", "-Model", MODEL]);
    check(
        "pre-setup startup blocks and routes to setup-private",
        out.0 == 1
            && out.1.contains("STARTUP_STATUS=BLOCKED")
            && out.1.contains("setup-private")
            && !out.1.contains("NOTE=STATE.md is missing - run"),
        out,
    );
    authority_ok(
        &run(root, &["setup-private"]),
        "missing model id fails with an actionable command",
        Some(1),
        &["missing -Model", "model id", "setup-private"],
    );
    f.set_origin("https://github.com/StudioDoNotDisturb/mdr-kernel.git");
    expect(
        &run(
            root,
            &["bootstrap-clone", "-Model", MODEL, "-CommitScaffold"],
        ),
        "direct upstream bootstrap fails before memory is created",
        1,
    );
    check(
        "refused bootstrap created no STATE.md",
        !root.join("STATE.md").exists(),
        root,
    );
    f.set_origin(f.remote.to_str().unwrap());
    let before = f.remote_head();
    let out = f.setup();
    expect(
        &out,
        "one-command private-remote setup verifies, bootstraps, and pushes",
        0,
    );
    authority_ok(
        &out,
        "first setup prints the induction badge and readiness tagline",
        Some(0),
        &["REFINER: TEST-MODEL", "PLEASE ENJOY EACH SESSION EQUALLY"],
    );
    check(
        "setup pushed the initial private checkpoint",
        before != f.remote_head(),
        before,
    );
    check(
        "setup regenerated the phase-zero doors into full contract doors",
        ["CLAUDE.md", "AGENTS.md", "GEMINI.md"].iter().all(|name| {
            fs::read_to_string(root.join(name))
                .unwrap()
                .contains("BEGIN BRAIN-CONTRACT")
        }),
        root,
    );
    check(
        "setup leaves git config brain.model unset (display-only)",
        config(root, "brain.model").is_empty(),
        root,
    );
    let second = f.clone("second");
    let setup = ["setup-private", "-Model", MODEL, "-ConfirmPrivateRemote"];
    let out = run(&second, &setup);
    expect(
        &out,
        "a second clone runs setup once to recreate local approval and hooks",
        0,
    );
    check(
        "second-clone setup skips the induction badge (scaffold came from history)",
        !out.1.contains("REFINER:"),
        out,
    );
    check(
        "second clone has local destination approval and hooks",
        !config(&second, "brain.approvedRemote").is_empty()
            && config(&second, "core.hooksPath") == ".githooks",
        &second,
    );
    expect(
        &run(&second, &setup),
        "setup rerun in the same clone is idempotent",
        0,
    );
    let evil = f.sandbox.0.join("other.git");
    git(
        &f.sandbox.0,
        &[
            "clone",
            "-q",
            "--bare",
            "--",
            root.to_str().unwrap(),
            evil.to_str().unwrap(),
        ],
    );
    let evil_before = git(&evil, &["rev-parse", "main"]);
    git(root, &["remote", "add", "evil", evil.to_str().unwrap()]);
    git(root, &["config", "remote.pushDefault", "evil"]);
    append(
        root,
        "STATE.md",
        "Private-onboarding close-set battery marker.",
    );
    put(root, "stray-download.bin", "must-not-be-committed");
    append(root, "CONTRIBUTING.md", "Parallel-session battery marker.");
    expect(
        &with_env(
            root,
            ENGINE,
            &["close-session", "-NoHandoff"],
            &[("BRAIN_MODEL", MODEL)],
        ),
        "close-session pushes private main without sweeping a stray file",
        0,
    );
    check(
        "stray file stayed local and untracked",
        root.join("stray-download.bin").exists()
            && git(root, &["ls-files", "--", "stray-download.bin"]).is_empty(),
        root,
    );
    check(
        "shared close sync preserved an unrelated tracked edit without pushing it",
        fs::read_to_string(root.join("CONTRIBUTING.md"))
            .unwrap()
            .contains("Parallel-session battery marker")
            && !f
                .remote_file("CONTRIBUTING.md")
                .contains("Parallel-session battery marker"),
        root,
    );
    git(root, &["restore", "CONTRIBUTING.md"]);
    authority_ok(
        &run(root, &["start-session", "-Model", MODEL]),
        "startup stays READY with an untracked stray file present",
        Some(0),
        &["STARTUP_STATUS=READY"],
    );
    check(
        "close-session pushed the owned STATE update",
        f.remote_file("STATE.md")
            .contains("close-set battery marker"),
        root,
    );
    check(
        "explicit origin/main ignores divergent remote.pushDefault",
        evil_before == git(&evil, &["rev-parse", "main"]),
        &evil,
    );
    f.set_origin(evil.to_str().unwrap());
    expect(
        &run(
            root,
            &[
                "remote-privacy",
                "-RequireApproved",
                "-ConfirmPrivateRemote",
            ],
        ),
        "approved destination pin refuses a remote swap",
        1,
    );
    let out = with_env(root, "git", &["push", "origin", "main"], &[]);
    check(
        "pre-push propagates the remote-swap refusal",
        out.0 == 1 && out.1.contains("REMOTE_PRIVACY_STATUS=UNVERIFIED_HOST"),
        out,
    );
    check(
        "blocked push left swapped remote unchanged",
        evil_before == git(&evil, &["rev-parse", "main"]),
        &evil,
    );
    f.set_origin(f.remote.to_str().unwrap());
    let pin = config(root, "brain.approvedRemote");
    git(
        root,
        &["config", "brain.approvedRemote", &pin.to_uppercase()],
    );
    expect(
        &run(
            root,
            &[
                "remote-privacy",
                "-RequireApproved",
                "-ConfirmPrivateRemote",
            ],
        ),
        "approval pin comparison is case-sensitive",
        1,
    );
    git(root, &["config", "brain.approvedRemote", &pin]);
    git(root, &["remote", "remove", "origin"]);
    authority_ok(
        &run(root, &["remote-privacy"]),
        "privacy reports MISSING_ORIGIN instead of LOCAL_ONLY when the pin is set",
        Some(1),
        &["REMOTE_PRIVACY_STATUS=MISSING_ORIGIN"],
    );
    authority_ok(
        &run(root, &["doctor", "-HardOnly"]),
        "doctor hard-fails a private-remote memory with no origin",
        Some(1),
        &["origin is missing"],
    );
    authority_ok(
        &run(root, &["start-session", "-Model", MODEL]),
        "startup reports BLOCKED while the private remote is missing",
        Some(1),
        &["STARTUP_STATUS=BLOCKED"],
    );
    append(root, "STATE.md", "Missing-origin fixture marker.");
    let out = close(root, &[]);
    check(
        "close refuses the degraded memory and records durable pending-sync",
        out.0 == 1 && out.1.contains("origin is missing") && f.pending(),
        out,
    );
    git(
        root,
        &["remote", "add", "origin", f.remote.to_str().unwrap()],
    );
    unset(root, "brain.pendingSync");
    git(root, &["restore", "STATE.md"]);
    expect(
        &run(root, &["doctor", "-HardOnly"]),
        "doctor recovers after the origin is restored",
        0,
    );

    let local = f.clone("local-only");
    git(&local, &["remote", "remove", "origin"]);
    expect(
        &run(&local, &["setup-private", "-Model", MODEL, "-LocalOnly"]),
        "local-only setup succeeds",
        0,
    );
    expect(
        &close(&local, &["-OfflineCheckpoint"]),
        "local-only mode refuses redundant OfflineCheckpoint",
        1,
    );
    git(&local, &["switch", "-qc", "side"]);
    expect(
        &close(&local, &[]),
        "local-only close refuses a non-main branch",
        1,
    );
}

#[test]
fn offline_checkpoint_and_recovery() {
    let f = Private::new();
    let root = &f.root;
    assert_eq!(f.setup().0, 0);
    append(root, "STATE.md", "Offline checkpoint battery marker.");
    let before = f.remote_head();
    let pin = config(root, "brain.approvedRemote");
    unset(root, "brain.approvedRemote");
    authority_ok(
        &close(root, &["-OfflineCheckpoint", "-DryRun"]),
        "offline checkpoint refuses a clone without prior destination approval",
        Some(1),
        &["requires a destination approved earlier"],
    );
    git(root, &["config", "brain.approvedRemote", &pin]);
    f.set_origin("https://github.com/example/private-memory.git");
    let out = close(root, &["-OfflineCheckpoint", "-DryRun"]);
    check(
        "offline checkpoint refuses an origin that no longer matches its approval pin",
        out.0 == 1
            && out.1.contains("origin changed since approval")
            && !out.1.contains("REMOTE_PRIVACY_STATUS="),
        out,
    );
    git(
        root,
        &[
            "config",
            "brain.approvedRemote",
            "github:example/private-memory",
        ],
    );
    let out = with_env(
        root,
        ENGINE,
        &["close-session", "-NoHandoff", "-OfflineCheckpoint"],
        &[
            ("BRAIN_MODEL", MODEL),
            ("HTTP_PROXY", "http://127.0.0.1:1"),
            ("HTTPS_PROXY", "http://127.0.0.1:1"),
            ("ALL_PROXY", "http://127.0.0.1:1"),
        ],
    );
    check(
        "offline GitHub checkpoint uses no live privacy probe",
        out.0 == 0
            && out
                .1
                .contains("existing private destination approval found")
            && !out.1.contains("REMOTE_PRIVACY_STATUS="),
        out,
    );
    f.set_origin(f.remote.to_str().unwrap());
    git(root, &["config", "brain.approvedRemote", &pin]);
    check(
        "offline checkpoint left remote unchanged and marked pending sync",
        before == f.remote_head() && f.pending(),
        root,
    );
    let out = run(root, &["doctor"]);
    check(
        "doctor names exact pending-sync recovery command",
        out.1
            .lines()
            .any(|line| line.contains("PENDING PRIVATE SYNC") && line.contains("sync-private")),
        out,
    );
    expect(
        &run(root, &["sync-private"]),
        "sync-private live-verifies and recovers offline checkpoint",
        0,
    );
    check(
        "recovery pushed checkpoint and cleared pending marker",
        f.remote_file("STATE.md")
            .contains("Offline checkpoint battery marker")
            && config(root, "brain.pendingSync").is_empty(),
        root,
    );
}

#[test]
fn concurrent_sync_and_rejected_push() {
    let f = Private::new();
    let root = &f.root;
    assert_eq!(f.setup().0, 0);
    let peer = f.clone("peer");
    let peer_checkpoint = |path: &str| {
        git(&peer, &["add", "--", path]);
        git(
            &peer,
            &[
                "-c",
                "core.hooksPath=.no-hooks",
                "commit",
                "-qm",
                "peer checkpoint",
            ],
        );
        git(
            &peer,
            &[
                "-c",
                "core.hooksPath=.no-hooks",
                "push",
                "-q",
                "origin",
                "main",
            ],
        );
    };
    put(&peer, "reference/peer-battery.md", "peer checkpoint");
    peer_checkpoint("reference/peer-battery.md");
    append(root, "STATE.md", "Automatic retry battery marker.");
    expect(
        &close(root, &[]),
        "close-session delegates shared sync and rebases a non-conflicting checkpoint",
        0,
    );
    check(
        "single retry delivered local checkpoint",
        f.remote_file("STATE.md")
            .contains("Automatic retry battery marker"),
        root,
    );
    git(&peer, &["pull", "-q", "--rebase"]);
    replace(
        &peer,
        "CONTRIBUTING.md",
        "# Contributing to mdr-kernel",
        "# Remote contributing to mdr-kernel",
    );
    peer_checkpoint("CONTRIBUTING.md");
    replace(
        root,
        "CONTRIBUTING.md",
        "# Contributing to mdr-kernel",
        "# Local contributing to mdr-kernel",
    );
    append(root, "STATE.md", "Autostash safety battery marker.");
    let before = f.remote_head();
    let out = close(root, &[]);
    expect(&out, "autostash restoration conflict fails before push", 1);
    check(
        "autostash conflict kept the remote unchanged and retained the lock",
        before == f.remote_head()
            && git(root, &["diff", "--name-only", "--diff-filter=U"]).contains("CONTRIBUTING.md")
            && root.join("STATE.lock").exists(),
        out,
    );
    f.discard_failed_close();
    git(&peer, &["pull", "-q", "--rebase"]);
    replace(
        &peer,
        "STATE.md",
        "Automatic retry battery marker.",
        "Remote conflict battery marker.",
    );
    peer_checkpoint("STATE.md");
    replace(
        root,
        "STATE.md",
        "Automatic retry battery marker.",
        "Local conflict battery marker.",
    );
    let out = close(root, &[]);
    expect(&out, "conflicting retry fails closed after one attempt", 1);
    let rebase = with_env(
        root,
        "git",
        &["rev-parse", "--verify", "-q", "REBASE_HEAD"],
        &[],
    );
    check(
        "conflict was aborted, lock retained, and pending sync stayed loud",
        rebase.0 != 0 && root.join("STATE.lock").exists() && f.pending(),
        out,
    );
    f.discard_failed_close();
    put(
        &f.remote,
        "hooks/pre-receive",
        "#!/bin/sh\necho 'fixture remote rejects pushes' >&2\nexit 1\n",
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            f.remote.join("hooks/pre-receive"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    append(root, "STATE.md", "Push rejection battery marker.");
    let out = close(root, &[]);
    let ahead: usize = git(root, &["rev-list", "--count", "origin/main..HEAD"])
        .trim()
        .parse()
        .unwrap();
    check(
        "rejected push keeps the lock, the local commit, and loud pending sync",
        out.0 == 1
            && out.1.contains("STATE.lock kept")
            && root.join("STATE.lock").exists()
            && ahead >= 1
            && f.pending(),
        &out,
    );
    let release = run(
        root,
        &[
            "state-lock",
            "-Release",
            "-Model",
            MODEL,
            "-Token",
            token(&out.1),
        ],
    );
    check(
        "kept lock is released by the token close printed",
        release.0 == 0 && !root.join("STATE.lock").exists(),
        release,
    );
}

#[test]
fn close_paths_and_approval() {
    let f = Private::new();
    let root = &f.root;
    assert_eq!(f.setup().0, 0);
    let also = |paths: &str| close(root, &[LEAST, &["-Also", paths]].concat());
    let tracked = |path: &str| !git(root, &["ls-files", "--", path]).is_empty();
    let clean = || git(root, &["status", "--porcelain", "--untracked-files=no"]).is_empty();
    put(root, "notes-a.md", "battery also-a");
    put(root, "notes-b.md", "battery also-b");
    expect(
        &also("notes-a.md,notes-b.md"),
        "close accepts multiple -Also paths in comma form",
        0,
    );
    check(
        "comma-form -Also committed both named paths",
        tracked("notes-a.md") && tracked("notes-b.md"),
        root,
    );
    git(root, &["rm", "-q", "notes-b.md"]);
    append(root, "STATE.md", "Split-close battery marker.");
    expect(
        &also("notes-b.md"),
        "close accepts a pre-staged deletion named in -Also without splitting",
        0,
    );
    let show = git(root, &["show", "--name-status", "--format=", "HEAD"]);
    check(
        "deletion and owned edits landed in ONE close commit with a clean tree",
        show.contains("D\tnotes-b.md") && show.contains("M\tSTATE.md") && clean(),
        show,
    );
    put(root, "notes-c.md", "battery also-c");
    let out = also("notes-c.md,ghost-note.md");
    check(
        "close warns on a no-match -Also path and still commits the matched one",
        out.0 == 0
            && out
                .1
                .contains("-Also 'ghost-note.md' matched no pending change")
            && !out.1.contains("-Also 'notes-c.md' matched no pending")
            && tracked("notes-c.md"),
        out,
    );
    let out = run(root, &["write-approval", "-Propose"]);
    check(
        "write-approval renders argument errors as [FAIL], not raw exceptions",
        out.0 == 1
            && out.1.contains("[FAIL] -Propose requires")
            && !["Exception", "At line", "CategoryInfo", "panicked"]
                .iter()
                .any(|needle| out.1.contains(needle)),
        out,
    );
    let draft = f.sandbox.0.join("draft.md");
    fs::write(&draft, "Battery reference note body.").unwrap();
    let propose = |target| {
        vec![
            "write-approval",
            "-Propose",
            "-Target",
            target,
            "-Source",
            draft.to_str().unwrap(),
            "-Reason",
            "battery flow",
        ]
    };
    let model = [("BRAIN_MODEL", MODEL)];
    let out = run(root, &propose("reference/battery-note.md"));
    let no_proposals = fs::read_dir(root.join(".write-proposals")).map_or_else(
        |error| error.kind() == std::io::ErrorKind::NotFound,
        |items| items.count() == 0,
    );
    check(
        "write-approval refuses to propose without an explicit session identity",
        out.0 == 1 && out.1.contains("[FAIL]") && out.1.contains("BRAIN_MODEL") && no_proposals,
        out,
    );
    let approve = |target| {
        let out = with_env(root, ENGINE, &propose(target), &model);
        assert_eq!(out.0, 0, "{out:?}");
        let id = out
            .1
            .lines()
            .find_map(|line| line.strip_prefix("PROPOSAL_ID="))
            .unwrap();
        let out = with_env(
            root,
            ENGINE,
            &[
                "write-approval",
                "-Approve",
                "-ProposalId",
                id,
                "-Reason",
                "battery approve",
            ],
            &model,
        );
        assert_eq!(out.0, 0, "{out:?}");
    };
    approve("reference/battery-note.md");
    append(
        root,
        "reference/INDEX.md",
        "| battery-note.md | battery write-approval flow note |",
    );
    expect(
        &also("reference/battery-note.md"),
        "write-approval flow closes with file, INDEX row, and auto-ridden audit",
        0,
    );
    check(
        "approved note and its audit entry both reached the private remote",
        f.remote_file("reference/battery-note.md")
            .contains("Battery reference note")
            && f.remote_file("inbox/write-approval-audit.jsonl")
                .contains("battery-note"),
        root,
    );
    fs::write(&draft, "Approved swap note body.").unwrap();
    approve("reference/swap-note.md");
    let path = "reference/swap-note.md";
    let audit = "inbox/write-approval-audit.jsonl";
    let approved = fs::read(root.join(path)).unwrap();
    put(root, path, "Different content staged after approval.");
    git(root, &["add", "--", path, audit]);
    let validate = || {
        with_env(
            root,
            ENGINE,
            &["write-approval", "-ValidateStagedApprovals"],
            &model,
        )
    };
    let swapped = validate();
    put(root, path, approved);
    git(root, &["add", "--", path]);
    let restored = validate();
    check(
        "approval gate refuses content swapped after approval and passes the approved bytes",
        swapped.0 == 1
            && swapped
                .1
                .contains("staged content is not the approved proposal")
            && restored.0 == 0,
        (swapped, restored),
    );
    git(root, &["reset", "-q", "--", path, audit]);
    git(root, &["restore", "--", audit]);
    fs::remove_file(root.join(path)).unwrap();
    let docs = git(root, &["ls-files", "--", "docs/*.md"]);
    let original = docs.lines().next().unwrap();
    let moved = "reference/moved-note.md";
    git(root, &["mv", "--", original, moved]);
    let out = validate();
    git(root, &["mv", "--", moved, original]);
    check(
        "approval gate treats a move into a gated path as a new gated file",
        out.0 == 1 && out.1.contains(moved) && clean(),
        out,
    );
    let out = close(root, &["-DryRun"]);
    check(
        "close does not count the write-approval audit log as an unfolded inbox note",
        out.0 == 0 && !out.1.contains("unfolded note"),
        out,
    );
    put(root, "reference/rogue-note.md", "rogue direct write");
    append(
        root,
        "reference/INDEX.md",
        "| rogue-note.md | rogue row for battery |",
    );
    authority_ok(
        &close(root, &[]),
        "close refuses an INDEX row pointing at a left-behind file",
        Some(1),
        &["referencing left-behind"],
    );
    git(root, &["reset", "--hard", "-q"]);
    fs::remove_file(root.join("reference/rogue-note.md")).unwrap();
    let deep = "cp2-newdir/deep-note.md";
    put(root, deep, "cp2 deep also");
    let out = also(deep);
    check(
        "-Also stages a file inside a fully-untracked directory",
        out.0 == 0 && tracked(deep) && !out.1.contains("matched no pending change"),
        out,
    );
}

fn lock(root: &Path, args: &[&str]) -> (i32, String) {
    run(root, &[&["state-lock"], args].concat())
}

fn lock_record(root: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(root.join("STATE.lock")).unwrap()).unwrap()
}

#[test]
fn identity_and_lock_wait() {
    let f = Private::new();
    let root = &f.root;
    assert_eq!(f.setup().0, 0);
    let lock_path = root.join("STATE.lock");
    let model = [("BRAIN_MODEL", MODEL)];
    git(root, &["config", "brain.model", MODEL]);
    authority_ok(
        &run(root, &["start-session", "-Model", "someone-else"]),
        "startup names the identity divergence and the exact seal command",
        Some(0),
        &[
            "fallback identity is 'test-model'",
            "are refused",
            "$env:BRAIN_MODEL = 'someone-else'",
        ],
    );
    let out = lock(root, &["-Acquire", "-Objective", "cp2 battery"]);
    check(
        "acquire refuses a config-fallback identity and creates no lock",
        out.0 == 1
            && out.1.contains("refusing ownership operation")
            && out.1.contains("identity resolved from 'config'")
            && !lock_path.exists(),
        out,
    );
    append(root, "STATE.md", "CP2 close identity battery marker.");
    let unsealed = ["close-session", "-NoHandoff"];
    authority_ok(
        &run(root, &unsealed),
        "close refuses a config-only committing identity",
        Some(1),
        &["identity must be explicit (resolved from 'config'"],
    );
    expect(
        &close(root, &[]),
        "close with explicit -Model completes end to end",
        0,
    );
    let message = git(root, &["log", "-1", "--format=%B"]);
    check(
        "-Model close stamped the Brain-Model trailer",
        message.contains("Brain-Model: test-model"),
        message,
    );
    let acquire = |objective| {
        lock(
            root,
            &["-Acquire", "-Model", MODEL, "-Objective", objective],
        )
    };
    let out = acquire("ownership battery");
    let seal = lock_record(root);
    let original = seal["token"].as_str().unwrap();
    check(
        "explicit -Model acquire records identity and a session token",
        out.0 == 0 && seal["model"] == MODEL && !original.is_empty(),
        out,
    );
    append(root, "STATE.md", "CP2 token battery marker.");
    let out = with_env(root, ENGINE, &unsealed, &model);
    check(
        "close refuses to adopt a live same-model lock token",
        out.0 == 1
            && out.1.contains("LOCK_OWNER=UNPROVEN")
            && lock_record(root)["token"] == original,
        out,
    );
    let proof = [("BRAIN_MODEL", MODEL), ("BRAIN_LOCK_TOKEN", original)];
    expect(
        &with_env(root, ENGINE, &unsealed, &proof),
        "holder with the correct token closes over its own lock",
        0,
    );
    let out = acquire("token parameter battery");
    let seal = lock_record(root);
    let second = seal["token"].as_str().unwrap();
    check(
        "second acquire for the token-param leg holds a sealed lock",
        out.0 == 0 && !second.is_empty(),
        out,
    );
    append(root, "STATE.md", "CP2 token-param battery marker.");
    expect(
        &with_env(
            root,
            ENGINE,
            &["close-session", "-NoHandoff", "-Token", second],
            &model,
        ),
        "holder recovers with the -Token parameter alone (no env var)",
        0,
    );
    let out = acquire("same-model refusal");
    let seal = lock_record(root);
    check(
        "drive2 leg holds a sealed lock",
        out.0 == 0
            && seal["token"]
                .as_str()
                .is_some_and(|value| !value.is_empty()),
        out,
    );
    let again = ["-Acquire", "-Model", MODEL];
    let out = lock(root, &again);
    check(
        "live same-model refusal steers to -Token/-Wait with no takeover recipe",
        out.0 == 1
            && ["-Acquire -Wait", "-Token <token>", "NOT PROVEN"]
                .iter()
                .all(|needle| out.1.contains(needle))
            && !out.1.contains("take it over"),
        out,
    );
    let force = ["-Release", "-Force", "-Model", MODEL];
    let out = lock(root, &force);
    check(
        "-Release -Force refuses a fresh-heartbeat lock and leaves it in place",
        out.0 == 1 && out.1.contains("refusing -Release -Force") && lock_path.exists(),
        out,
    );
    expect(
        &with_env(
            root,
            ENGINE,
            &[&["state-lock"], RECOVERY].concat(),
            &[("MDR_FORCE_LIVE_LOCK", "1")],
        ),
        "fresh-lock recovery requires valve plus independent abandonment evidence",
        0,
    );
    assert_eq!(acquire("stale heartbeat").0, 0);
    let mut stale = lock_record(root);
    stale["heartbeat_at"] = "2000-01-01T00:00:00Z".into();
    fs::write(&lock_path, serde_json::to_vec(&stale).unwrap()).unwrap();
    authority_ok(
        &lock(root, &again),
        "stale same-model refusal requires independent evidence",
        Some(1),
        &["stale past 120m", "NOT PROVEN", "independent evidence"],
    );
    expect(
        &lock(root, &force),
        "stale lock refuses force without evidence",
        1,
    );
    expect(
        &lock(root, RECOVERY),
        "stale lock recovers with independent evidence and no live valve",
        0,
    );
    assert_eq!(acquire("wait battery").0, 0);
    let seal = lock_record(root);
    let waiting_for = seal["token"].as_str().unwrap();
    authority_ok(
        &lock(
            root,
            &["-Acquire", "-Wait", "-TimeoutSec", "6", "-Model", MODEL],
        ),
        "-Wait polls, then gives up on budget against a live holder",
        Some(1),
        &["polling every 5s", "gave up after 6s"],
    );
    std::thread::scope(|scope| {
        let release = scope.spawn(|| {
            std::thread::sleep(std::time::Duration::from_secs(2));
            lock(root, &["-Release", "-Token", waiting_for, "-Model", MODEL])
        });
        let out = lock(
            root,
            &[
                "-Acquire",
                "-Wait",
                "-TimeoutSec",
                "30",
                "-Model",
                MODEL,
                "-Objective",
                "wait winner",
            ],
        );
        let won = lock_record(root);
        check(
            "-Wait acquires with a fresh token once the holder releases",
            out.0 == 0 && won["token"] != waiting_for,
            out,
        );
        assert_eq!(release.join().unwrap().0, 0);
        assert_eq!(
            lock(
                root,
                &[
                    "-Release",
                    "-Token",
                    won["token"].as_str().unwrap(),
                    "-Model",
                    MODEL
                ]
            )
            .0,
            0
        );
    });
}

#[test]
fn handoff_dates_and_staged_coordination() {
    let f = Private::new();
    let root = &f.root;
    assert_eq!(f.setup().0, 0);
    let missing = run(
        root,
        &["close-session", "-Topic", "no-such-topic", "-Model", MODEL],
    );
    authority_ok(
        &missing,
        "missing-handoff failure names the accepted window and -SessionDate",
        Some(1),
        &["accepted dates for -Topic", "-SessionDate <yyyy-MM-dd>"],
    );
    if root.join("STATE.lock").exists() {
        let seal = lock_record(root);
        assert_eq!(
            lock(
                root,
                &[
                    "-Release",
                    "-Token",
                    seal["token"].as_str().unwrap(),
                    "-Model",
                    MODEL
                ]
            )
            .0,
            0
        );
    }
    // The date list is UTC today, host-local today if different, then UTC minus/plus one.
    let dates = missing
        .1
        .split_once("accepted dates for -Topic 'no-such-topic': ")
        .unwrap()
        .1
        .split_once(';')
        .unwrap()
        .0;
    let today = dates.split(", ").next().unwrap();
    let yesterday = dates.split(", ").find(|date| *date < today).unwrap();
    let name = format!("{yesterday}-datetol.md");
    put(root,&format!("handoffs/{name}"),format!("Title: Handoff - {yesterday} - datetol\nType: Handoff / Session Closeout\nContext: Date-tolerance battery handoff.\n\nBattery body.\n\n## SYSTEM NODE LINKS\n\nDirectory Path: handoffs/\nNext Sequential File: INDEX.md\nRelated Workflows: STATE.md\nAI Execution Prompts: ../protocol/CONTRACT.md\n"));
    append(
        root,
        "handoffs/INDEX.md",
        &format!("| {yesterday} | {name} | Date tolerance | Closed |"),
    );
    let out = run(
        root,
        &["close-session", "-Topic", "datetol", "-Model", MODEL],
    );
    authority_ok(
        &out,
        "close accepts a UTC-1-dated handoff and says which date it used",
        Some(0),
        &[&format!("handoff date {yesterday} accepted")],
    );
    let contract = "protocol/CONTRACT.md";
    replace(
        root,
        contract,
        "mdr-kernel contract - every model follows this",
        "Staged drift fixture. mdr-kernel contract - every model follows this",
    );
    assert_eq!(run(root, &["sync-doors"]).0, 0);
    git(root, &["add", contract]);
    let tree = run(root, &["sync-doors", "-Check"]);
    let staged = run(root, &["sync-doors", "-Check", "-Staged"]);
    check(
        "staged-blob door check catches drift the working-tree check misses",
        tree.0 == 0 && staged.0 == 1,
        (tree, staged),
    );
    let model = [("BRAIN_MODEL", MODEL)];
    let out = with_env(root, "git", &["commit", "-m", "drift attempt"], &model);
    check(
        "pre-commit blocks the partially staged contract/door commit",
        out.0 != 0 && out.1.contains("STAGED contract/door set"),
        out,
    );
    git(root, &["reset", "--hard", "-q"]);
    let policy = fs::read_to_string(root.join("protocol/coordination-files.txt")).unwrap();
    let identity = "native/src/remote_privacy.rs";
    let audit = "native/src/health.rs";
    let protected = |path: &str| policy.lines().any(|line| line.trim() == path);
    check(
        "pre-push/remote-identity/secret-audit are on the coordination allowlist",
        protected(".githooks/pre-push")
            && ((protected(identity) && protected(audit))
                || (protected("native/src/remote_privacy.rs") && protected("native/src/hook.rs"))),
        policy,
    );
    append(root, ".githooks/pre-push", "# fixture enforcement edit");
    git(root, &["add", ".githooks/pre-push"]);
    let out = with_env(root, "git", &["commit", "-m", "helper attempt"], &model);
    check(
        "committing an enforcement helper without the lock is blocked",
        out.0 != 0 && out.1.contains("STATE.lock is not held"),
        out,
    );
}
