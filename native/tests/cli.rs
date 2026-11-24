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
