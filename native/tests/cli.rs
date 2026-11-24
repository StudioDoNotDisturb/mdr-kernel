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
    // PowerShell-style names get the ":" form in the hint, and that form parses.
    refuses(
        fixture.run(&fixture.0, &["find", "-Query", "--json"]),
        "write -Query:--json to use it",
    );
    refuses(
        fixture.run(&fixture.0, &["find", "-Query:--json"]),
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

/// The prompt hook's receipts: one session is not re-sent what it was shown, every other case is.
#[test]
fn hook_receipts_suppress_repeats_and_rearm() {
    use std::io::Write;
    use std::process::Stdio;

    let fixture = Fixture::new();
    let root = fixture.memory("memory");
    fs::create_dir_all(root.join("protocol")).unwrap();
    let example =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../protocol/recall-gates.example.json");
    let gates = root.join("protocol/recall-gates.json");
    fs::copy(&example, &gates).unwrap();
    let temp = fixture.0.join("temp");
    fs::create_dir_all(&temp).unwrap();
    let hook = |payload: &str| -> String {
        let mut child = Command::new(env!("CARGO_BIN_EXE_mdr-kernel"))
            .args(["hook-recall", "-Root", &root.to_string_lossy()])
            .env("TMP", &temp)
            .env("TEMP", &temp)
            .env("TMPDIR", &temp)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(payload.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap()
    };
    let ask = |session: &str, extra: &str| {
        hook(&format!(
            r#"{{"session_id":"{session}","prompt":"deploy again"{extra}}}"#
        ))
    };
    assert!(
        ask("one", "").contains("EXAMPLE-DOMAIN"),
        "first prompt delivers"
    );
    assert_eq!(ask("one", ""), "", "same session, same traps: suppressed");
    assert!(
        ask("two", "").contains("EXAMPLE-DOMAIN"),
        "another session gets its own"
    );
    assert_eq!(
        hook(r#"{"session_id":"one","hook_event_name":"SessionStart","source":"compact"}"#),
        "",
        "SessionStart is silent"
    );
    assert!(
        ask("one", "").contains("EXAMPLE-DOMAIN"),
        "SessionStart re-arms"
    );
    assert!(
        ask("one", r#","checkpoint":"c2""#).contains("EXAMPLE-DOMAIN"),
        "a new checkpoint re-arms"
    );
    assert_eq!(ask("one", r#","checkpoint":"c2""#), "");
    // A changed trap is a new fingerprint and is delivered again.
    let text = fs::read_to_string(&gates).unwrap();
    fs::write(
        &gates,
        text.replacen("\"times_recurred\": 0", "\"times_recurred\": 3", 1),
    )
    .unwrap();
    assert!(ask("one", r#","checkpoint":"c2""#).contains("RECALL_STATUS=MATCHED"));
    // No session id: nothing is ever suppressed.
    let anonymous = r#"{"prompt":"deploy again"}"#;
    assert!(hook(anonymous).contains("EXAMPLE-DOMAIN"));
    assert!(hook(anonymous).contains("EXAMPLE-DOMAIN"));
    // A corrupt receipt file repeats the reminder instead of hiding it.
    for entry in fs::read_dir(temp.join("mdr-kernel-recall-delivery")).unwrap() {
        fs::write(entry.unwrap().path(), "not json").unwrap();
    }
    assert!(ask("two", "").contains("EXAMPLE-DOMAIN"));
}

/// The built-in MCP server, end to end over stdio: one request per line, one reply per request,
/// none for a notification, and real tool results, not just a handshake.
#[test]
fn mcp_server_answers_over_stdio() {
    use serde_json::Value;
    use std::io::Write;
    use std::process::Stdio;

    let fixture = Fixture::new();
    let root = fixture.memory("memory");
    fs::create_dir_all(root.join("protocol")).unwrap();
    let example =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../protocol/recall-gates.example.json");
    fs::copy(&example, root.join("protocol/recall-gates.json")).unwrap();
    fs::write(
        root.join("STATE.md"),
        "Title: STATE\n\nquartz harbor plan\n",
    )
    .unwrap();
    let requests = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"recall_gates","arguments":{"objective":"deploy again"}}}"#,
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"get_gate_traps","arguments":{"gate_id":"NOT-HERE"}}}"#,
        r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"find_history","arguments":{"query":"quartz","limit":3}}}"#,
        r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"recall_gates","arguments":{"objective":"x","extra":1}}}"#,
        r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"nope"}}"#,
        r#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"get_gate_traps","arguments":{"gate_id":"-bad id"}}}"#,
        r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"find_history","arguments":{"query":"quartz","limit":11}}}"#,
        "not json",
        "[1,2]",
        r#"{"jsonrpc":"2.0","id":10,"method":"ping"}"#,
        r#"{"jsonrpc":"2.0","id":11,"method":"resources/list"}"#,
        r#"{"jsonrpc":"2.0","id":12,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}"#,
    ];
    let mut child = Command::new(env!("CARGO_BIN_EXE_mdr-kernel"))
        .args(["mcp", "-Root", &root.to_string_lossy()])
        .env("XDG_CACHE_HOME", fixture.0.join("cache"))
        .env("LOCALAPPDATA", fixture.0.join("cache"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all((requests.join("\n") + "\n").as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let replies: Vec<Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    // 15 lines in, one notification: 14 replies.
    assert_eq!(replies.len(), 14, "{replies:#?}");
    let by_id = |id: i64| replies.iter().find(|r| r["id"] == id).unwrap();
    let text = |id: i64| {
        by_id(id)["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let is_error = |id: i64| by_id(id)["result"]["isError"].as_bool().unwrap();

    assert_eq!(by_id(1)["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(by_id(1)["result"]["serverInfo"]["name"], "mdr-kernel");
    let names: Vec<&str> = by_id(2)["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["recall_gates", "get_gate_traps", "find_history"]);
    assert!(
        !is_error(3)
            && text(3).contains("RECALL_STATUS=MATCHED")
            && text(3).contains("EXAMPLE-DOMAIN")
    );
    assert!(is_error(4) && text(4).contains("unknown-gate-id"));
    assert!(!is_error(5) && text(5).contains("STATE.md"), "{}", text(5));
    assert!(is_error(6) && text(6).contains("Unknown argument: extra"));
    assert_eq!(by_id(7)["error"]["code"], -32602);
    assert!(is_error(8) && text(8).contains("gate_id is malformed"));
    assert!(is_error(9) && text(9).contains("limit must be an integer from 1 to 10"));
    let nulls: Vec<i64> = replies
        .iter()
        .filter(|r| r["id"].is_null())
        .map(|r| r["error"]["code"].as_i64().unwrap())
        .collect();
    assert_eq!(
        nulls,
        [-32700, -32600],
        "parse error, then a batch array refused"
    );
    assert_eq!(by_id(10)["result"], serde_json::json!({}));
    assert_eq!(by_id(11)["error"]["code"], -32601);
    assert_eq!(by_id(12)["result"]["protocolVersion"], "2025-06-18");
}

fn lifecycle_git(root: &Path, args: &[&str], old: bool) {
    let mut command = Command::new("git");
    command
        .current_dir(root)
        .args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "core.hooksPath=.git/no-hooks",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args);
    for key in ["GIT_AUTHOR_DATE", "GIT_COMMITTER_DATE"] {
        command.env_remove(key);
        if old {
            command.env(key, "2000-01-01T00:00:00Z");
        }
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn live_archive(fixture: &Fixture, root: &Path, option: &str, file: &str) -> Output {
    fixture.run(
        root,
        &[
            "archive-adopted",
            option,
            file,
            "-Model:lifecycle",
            "-Token:fixture-token",
        ],
    )
}

fn lifecycle_lock(root: &Path) {
    fs::write(
        root.join("STATE.lock"),
        r#"{"model":"lifecycle","token":"fixture-token"}"#,
    )
    .unwrap();
}

fn lifecycle_document(root: &Path, path: &str, state: &str) {
    fs::write(
        root.join(path),
        format!(
            "Title: [{state}] Example\nType: Note\nContext: Use when testing.\n\nPreserve me.\n"
        ),
    )
    .unwrap();
}

#[test]
fn lifecycle_lists_only_eligible_records_and_dispositions_and_moves_exact_paths() {
    let fixture = Fixture::new();
    let root = fixture.memory("lifecycle");
    lifecycle_git(&root, &["init", "-q", "-b", "main"], false);
    for (path, state) in [
        ("handoffs/old record.md", "Record"),
        ("handoffs/edited.md", "Record"),
        ("handoffs/unindexed.md", "Record"),
        ("cited record.md", "Record"),
        ("draft.md", "Draft"),
        ("open.md", "Open"),
        ("pending.md", "Adopted?"),
        ("ingested.md", "Ingested"),
        ("superseded.md", "Superseded"),
        ("rejected.md", "Rejected"),
        ("living.md", "Living"),
        ("already.md", "Archived"),
    ] {
        lifecycle_document(&root, path, state);
    }
    let handoff = "\u{feff}Title: [Record] Old record\r\nType: Handoff\r\nContext: Use when testing.\r\n\r\nBody \u{201c}preserved\u{201d}  \r\n";
    fs::write(root.join("handoffs/old record.md"), handoff).unwrap();
    let index = "| Date | File | What it covers | Status |\r\n|---|---|---|---|\r\n| Jan | [old record.md](old%20record.md) | Example | Done |\r\n| Jan | edited.md | Example | Done |\r\n";
    fs::write(root.join("handoffs/INDEX.md"), index).unwrap();
    fs::write(
        root.join("STATE.md"),
        "Title: [Open] State\n\nSee cited record.md.\n",
    )
    .unwrap();
    lifecycle_git(&root, &["add", "."], true);
    lifecycle_git(&root, &["commit", "-qm", "Old docs"], true);
    lifecycle_document(&root, "fresh.md", "Record");
    lifecycle_git(&root, &["add", "fresh.md"], false);
    lifecycle_git(&root, &["commit", "-qm", "Fresh record"], false);
    fs::write(
        root.join("handoffs/edited.md"),
        "Title: [Record] Edited since commit\n",
    )
    .unwrap();
    assert_eq!(
        stdout(fixture.run(&root, &["find-stale", "list"])),
        "STALE_COUNT=5\nhandoffs/old record.md [Record]\nhandoffs/unindexed.md [Record]\ningested.md [Ingested]\nrejected.md [Rejected]\nsuperseded.md [Superseded]\n"
    );
    refuses(
        live_archive(&fixture, &root, "-Adopt", "ingested.md"),
        "your own STATE.lock",
    );
    lifecycle_lock(&root);
    refuses(
        fixture.run(
            &root,
            &[
                "archive-adopted",
                "-Adopt:ingested.md",
                "-Model:lifecycle",
                "-Token:someone-else",
            ],
        ),
        "LOCK_OWNER=UNPROVEN",
    );
    refuses(
        fixture.run(
            &root,
            &[
                "archive-adopted",
                "-Adopt:ingested.md",
                "-Model:other",
                "-Token:fixture-token",
            ],
        ),
        "LOCK_OWNER=UNPROVEN",
    );

    for (option, name, message) in [
        ("-Adopt", "draft.md", "[Draft] is ineligible"),
        ("-Adopt", "living.md", "[Living] is ineligible"),
        ("-Adopt", "already.md", "[Archived] is ineligible"),
        ("-Adopt", "fresh.md", "[Record] is ineligible"),
        (
            "-Expired",
            "draft.md",
            "unchanged [Record] older than 30 days",
        ),
        (
            "-Expired",
            "fresh.md",
            "unchanged [Record] older than 30 days",
        ),
        (
            "-Expired",
            "handoffs/edited.md",
            "unchanged [Record] older than 30 days",
        ),
        ("-Expired", "handoffs/unindexed.md", "missing INDEX row"),
        (
            "-Expired",
            "cited record.md",
            "still cited by main file STATE.md",
        ),
    ] {
        refuses(live_archive(&fixture, &root, option, name), message);
        assert!(root.join(name).is_file());
    }
    for name in [
        "open.md",
        "pending.md",
        "ingested.md",
        "superseded.md",
        "rejected.md",
    ] {
        assert!(stdout(live_archive(&fixture, &root, "-Adopt", name)).contains("ARCHIVED="));
        assert!(!root.join(name).exists());
        assert!(fs::read_to_string(root.join("archive").join(name))
            .unwrap()
            .starts_with("Title: [Archived] Example\n"));
    }
    let named = format!("-Expired:{}", root.join("handoffs/old record.md").display());
    assert_eq!(
        stdout(fixture.run(
            &root,
            &[
                "archive-adopted",
                &named,
                "-Model:lifecycle",
                "-Token:fixture-token"
            ]
        )),
        "ARCHIVED=handoffs/old record.md -> archive/handoffs/old record.md\n"
    );
    assert!(!root.join("handoffs/old record.md").exists());
    assert_eq!(
        fs::read(root.join("archive/handoffs/old record.md")).unwrap(),
        handoff.replace("[Record]", "[Archived]").as_bytes()
    );
    assert_eq!(
        fs::read(root.join("handoffs/INDEX.md")).unwrap(),
        index
            .replace(
                "(old%20record.md) | Example | Done",
                "(../archive/handoffs/old%20record.md) | Example | Archived"
            )
            .as_bytes()
    );
    assert_eq!(
        stdout(fixture.run(&root, &["find-stale", "list"])),
        "STALE_COUNT=1\nhandoffs/unindexed.md [Record]\n"
    );
}

#[test]
fn lifecycle_excludes_main_citations_and_checks_every_index_reference() {
    let fixture = Fixture::new();
    let root = fixture.memory("citations");
    lifecycle_git(&root, &["init", "-q", "-b", "main"], false);
    lifecycle_document(&root, "handoffs/target.md", "Superseded");
    let index_path = root.join("handoffs/INDEX.md");
    let index = "| Date | File | Status |\n|---|---|---|\n| Jan | target.md | Open |\n";
    fs::write(&index_path, index).unwrap();
    lifecycle_git(&root, &["add", "."], false);
    lifecycle_git(&root, &["commit", "-qm", "Source docs"], false);
    lifecycle_lock(&root);
    let path = "handoffs/target.md";
    for (file, body) in [
        ("STATE.md", "Next: [revisit](handoffs/target.md#next)\n"),
        (
            "living.md",
            "Title: [Living] Main\n\n[[handoffs/target|Current]]\n",
        ),
        ("protocol/links.json", "{\"entry\":\"handoffs/target.md\"}"),
        ("README.md", "Read target.md for details.\n"),
    ] {
        fs::create_dir_all(root.join(file).parent().unwrap()).unwrap();
        fs::write(root.join(file), body).unwrap();
        assert_eq!(
            stdout(fixture.run(&root, &["find-stale", "list"])),
            "STALE_COUNT=1\nhandoffs/target.md [Superseded]\n"
        );
        refuses(
            live_archive(&fixture, &root, "-Adopt", path),
            &format!("still cited by main file {file}"),
        );
        assert!(root.join(path).is_file());
        fs::remove_file(root.join(file)).unwrap();
    }
    fs::write(&index_path, format!("{index}\nSee target.md.\n")).unwrap();
    refuses(
        live_archive(&fixture, &root, "-Adopt", path),
        "still cited by main file handoffs/INDEX.md",
    );
    fs::write(&index_path, index.replace("Open", "See target.md")).unwrap();
    refuses(
        live_archive(&fixture, &root, "-Adopt", path),
        "another citation in its INDEX row",
    );
    fs::write(
        &index_path,
        format!("{index}| Jan | `target.md` | Completed |\n"),
    )
    .unwrap();
    refuses(
        live_archive(&fixture, &root, "-Adopt", path),
        "multiple INDEX rows",
    );
    fs::write(&index_path, "| Date | File | Status |\n").unwrap();
    refuses(
        live_archive(&fixture, &root, "-Adopt", path),
        "missing INDEX row",
    );
    fs::write(&index_path, index).unwrap();
    fs::write(
        root.join("history.md"),
        "Title: [Record] Historical\n\nSee target.md.\n",
    )
    .unwrap();
    stdout(live_archive(&fixture, &root, "-Adopt", path));
    assert!(fs::read_to_string(index_path)
        .unwrap()
        .contains("| Jan | ../archive/handoffs/target.md | Archived |"));
}

#[test]
fn lifecycle_refuses_unsafe_or_unreadable_paths_and_preserves_existing_targets() {
    let fixture = Fixture::new();
    let root = fixture.memory("refusals");
    lifecycle_git(&root, &["init", "-q", "-b", "main"], false);
    for name in ["one.md", "two.md", "one, two.md"] {
        lifecycle_document(&root, name, "Rejected");
    }
    fs::write(root.join("no-title.md"), "# [Ingested] in Markdown body\n").unwrap();
    fs::write(
        root.join("ambiguous.md"),
        "Title: [Open] One\nTitle: [Rejected] Two\n",
    )
    .unwrap();
    fs::write(root.join("unreadable.md"), [0xff]).unwrap();
    lifecycle_lock(&root);
    refuses(fixture.run(&root, &["find-stale"]), "find-stale list");
    refuses(
        fixture.run(&root, &["archive-adopted"]),
        "choose exactly one",
    );
    refuses(
        fixture.run(
            &root,
            &["archive-adopted", "-Adopt:one.md", "-Expired:two.md"],
        ),
        "choose exactly one",
    );
    for (name, message) in [
        ("../outside.md", "one exact file"),
        ("handoffs/../one.md", "one exact file"),
        ("*.md", "one exact file"),
        ("one.md:stream", "one exact file"),
        (".git/target.md", "one exact file"),
        ("handoffs/INDEX.md", "outside archive/ and INDEX.md"),
        ("archive/one.md", "outside archive/ and INDEX.md"),
        ("missing.md", "not a readable regular file"),
        ("no-title.md", "missing or ambiguous Title state"),
        ("ambiguous.md", "missing or ambiguous Title state"),
        ("unreadable.md", "unreadable.md"),
    ] {
        refuses(live_archive(&fixture, &root, "-Adopt", name), message);
    }
    let outside = fixture.0.join("outside.md");
    lifecycle_document(&fixture.0, "outside.md", "Rejected");
    refuses(
        live_archive(&fixture, &root, "-Adopt", outside.to_str().unwrap()),
        "inside the repository",
    );
    assert!(outside.exists());
    refuses(fixture.run(&root, &["find-stale", "list"]), "unreadable.md");
    refuses(
        live_archive(&fixture, &root, "-Adopt", "one.md"),
        "unreadable.md",
    );
    fs::remove_file(root.join("unreadable.md")).unwrap();
    fs::create_dir(root.join("archive")).unwrap();
    fs::write(root.join("archive/one.md"), "Existing archive contents\n").unwrap();
    refuses(
        live_archive(&fixture, &root, "-Adopt", "one.md"),
        "already exists",
    );
    assert_eq!(
        fs::read_to_string(root.join("archive/one.md")).unwrap(),
        "Existing archive contents\n"
    );
    assert!(root.join("one.md").exists());
    stdout(live_archive(&fixture, &root, "-Adopt", "one, two.md"));
    assert!(root.join("one.md").exists());
    assert!(root.join("two.md").exists());
    assert!(root.join("archive/one, two.md").exists());
}

#[cfg(any(unix, windows))]
fn lifecycle_directory_link(link: &Path, destination: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(destination, link).unwrap();
    #[cfg(windows)]
    {
        let output = Command::new("cmd")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(link)
            .arg(destination)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[cfg(any(unix, windows))]
#[test]
fn lifecycle_refuses_links_in_source_archive_and_root_paths() {
    let fixture = Fixture::new();
    let root = fixture.memory("links");
    let outside = fixture.memory("outside");
    lifecycle_git(&root, &["init", "-q", "-b", "main"], false);
    lifecycle_document(&root, "source.md", "Rejected");
    lifecycle_document(&outside, "target.md", "Rejected");
    lifecycle_lock(&root);
    lifecycle_directory_link(&root.join("linked"), &outside);
    refuses(
        live_archive(&fixture, &root, "-Adopt", "linked/target.md"),
        "link or junction",
    );
    let linked_root = fixture.0.join("linked-root");
    lifecycle_directory_link(&linked_root, &root);
    refuses(
        fixture.run(
            &root,
            &["find-stale", "list", "-Root", linked_root.to_str().unwrap()],
        ),
        "link or junction",
    );
    lifecycle_directory_link(&root.join("archive"), &outside);
    refuses(
        live_archive(&fixture, &root, "-Adopt", "source.md"),
        "link or junction",
    );
    assert!(outside.join("target.md").exists());
    assert!(!outside.join("source.md").exists());
    assert!(root.join("source.md").exists());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.join("target.md"), root.join("direct.md")).unwrap();
        refuses(
            live_archive(&fixture, &root, "-Adopt", "direct.md"),
            "link or junction",
        );
    }
}

#[cfg(windows)]
#[test]
fn lifecycle_serializes_operations_and_rolls_back_a_failed_index_replace() {
    use std::os::windows::fs::OpenOptionsExt;

    let fixture = Fixture::new();
    let root = fixture.memory("rollback");
    lifecycle_git(&root, &["init", "-q", "-b", "main"], false);
    lifecycle_lock(&root);
    let source = "handoffs/target.md";
    lifecycle_document(&root, source, "Rejected");
    let original = fs::read(root.join(source)).unwrap();
    let index_path = root.join("handoffs/INDEX.md");
    let index = "| File | Status |\n|---|---|\n| target.md | Done |\n";
    fs::write(&index_path, index).unwrap();

    let guard = fs::File::create(root.join(".git/mdr-lifecycle.lock")).unwrap();
    guard.lock().unwrap();
    refuses(
        live_archive(&fixture, &root, "-Adopt", source),
        "could not acquire archive operation lock",
    );
    drop(guard);

    // Allow the command to validate the index, then make replacement fail.
    let index_handle = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&index_path)
        .unwrap();
    refuses(live_archive(&fixture, &root, "-Adopt", source), "INDEX.md");
    drop(index_handle);
    assert_eq!(fs::read(root.join(source)).unwrap(), original);
    assert_eq!(fs::read_to_string(index_path).unwrap(), index);
    assert!(!root.join("archive").join(source).exists());
    stdout(live_archive(&fixture, &root, "-Adopt", source));
}
