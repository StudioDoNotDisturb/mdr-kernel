use super::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::Write;

const GATES: &str = "protocol/recall-gates.json";
const EXAMPLE: &str = "protocol/recall-gates.example.json";

struct Reply {
    code: i32,
    out: String,
    err: String,
}

impl Reply {
    fn verify(&self, name: &str, condition: bool) {
        check(name, condition, (self.code, &self.out, &self.err));
    }

    fn success(&self, name: &str, needles: &[&str]) {
        self.verify(
            name,
            self.code == 0 && needles.iter().all(|s| self.out.contains(s)),
        );
    }

    fn clear(&self, name: &str) {
        self.verify(
            name,
            self.code == 0 && self.out.is_empty() && self.err.is_empty(),
        );
    }

    fn blocked(&self, name: &str, reason: &str) {
        self.verify(
            name,
            self.code == 2
                && self
                    .err
                    .contains(&format!("RECALL_STATUS=BLOCKED reason={reason}")),
        );
    }

    fn context(&self) -> String {
        let data: Value =
            serde_json::from_str(&self.out).unwrap_or_else(|e| panic!("{e}: {}", self.out));
        assert_eq!(
            data["hookSpecificOutput"]["hookEventName"],
            "UserPromptSubmit"
        );
        data["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn degraded(&self, name: &str, reason: &str) {
        self.verify(
            name,
            self.code == 0 && self.err.is_empty() && !self.out.contains("PRIVATE-CANARY") && {
                let text = self.context();
                text.contains(&format!("RECALL_HOOK_STATUS=DEGRADED reason={reason};"))
                    && text.contains("prepare-work")
            },
        );
    }
}

fn invoke(root: &Path, name: &str, args: &[&str], input: &str) -> Reply {
    let mut child = command(ENGINE, root, &[])
        .arg(name)
        .arg("-Root")
        .arg(root)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut pipe = child.stdin.take().unwrap();
    // An over-budget reader may close stdin before the writer finishes.
    if let Err(error) = pipe.write_all(input.as_bytes()) {
        assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
    }
    drop(pipe);
    let output = child.wait_with_output().unwrap();
    Reply {
        code: output.status.code().unwrap(),
        out: String::from_utf8(output.stdout).unwrap(),
        err: String::from_utf8(output.stderr).unwrap(),
    }
}

fn save(root: &Path, data: &Value) {
    put(root, GATES, serde_json::to_vec_pretty(data).unwrap());
}

fn rulings(root: &Path, class: &str, id: &str) {
    let mut sources = json!({});
    for (key, path) in [
        ("decisions", "DECISIONS.md"),
        ("ratifications", "protocol/decision-ratifications.jsonl"),
        ("gates", GATES),
    ] {
        let source = fs::read_to_string(root.join(path))
            .unwrap_or_default()
            .replace("\r\n", "\n");
        sources[key] = json!(format!(
            "{:x}",
            Sha256::digest(source.trim_end().as_bytes())
        ));
    }
    put(root, "protocol/rulings.json", json!({
        "sources": sources,
        "rulings": [{
            "id": id,
            "class": class,
            "rule": "Read every required source. Preserve the complete operator rule, including this final clause.",
            "rules_out": "Assuming a brief reminder is permission."
        }]
    }).to_string());
}

#[test]
fn recall_schema_matching_and_budgets() {
    let fixture = Sandbox::new();
    let root = &fixture.0;
    let seed = fs::read_to_string(template().join(EXAMPLE))
        .unwrap()
        .replace("\r\n", "\n");
    let example: Value = serde_json::from_str(&seed).unwrap();
    let recall = |args: &[&str]| invoke(root, "brain-recall", args, "");
    let hook = |input| invoke(root, "hook-recall", &[], input);
    let reset = || put(root, GATES, &seed);
    reset();
    put(root, EXAMPLE, &seed);
    recall(&["-CheckGates"]).success(
        "example v1 schema passes",
        &["RECALL_GATES_STATUS=VALID gates=3"],
    );
    put(root, GATES, "invalid runtime document");
    let reply = recall(&["-CheckGates", "-GatesPath", EXAMPLE]);
    reply.verify(
        "explicit schema path validates example independently without changing runtime data",
        reply.code == 0
            && reply.out.contains("RECALL_GATES_STATUS=VALID gates=3")
            && fs::read_to_string(root.join(GATES)).unwrap() == "invalid runtime document",
    );
    recall(&["-GatesPath", EXAMPLE, "-Objective", "deploy"]).blocked(
        "schema path cannot redirect normal recall",
        "gates-path-requires-check-only",
    );
    recall(&[
        "-CheckGates",
        "-GatesPath",
        EXAMPLE,
        "-Increment",
        "STORED-BUT-UNUSED",
    ])
    .blocked(
        "schema path cannot accompany increment even with CheckGates",
        "gates-path-requires-check-only",
    );
    reset();
    recall(&["-Objective", "polish the typography", "-Brief"])
        .clear("healthy brief CLEAR is empty without error");
    let reply = recall(&["-Objective", "deploy again", "-Json"]);
    let data: Value = serde_json::from_str(&reply.out).unwrap();
    reply.verify(
        "normal match preserves both gate records",
        reply.code == 0
            && data["status"] == "MATCHED"
            && data["matched_count"] == 2
            && data["gates"][1]["id"] == "EXAMPLE-DOMAIN",
    );
    recall(&["-GateId", "AGENT-EVIDENCE"]).success(
        "direct lookup includes trap and required read",
        &["AGENT-FABRICATED-COMPLETION", "Required reads:"],
    );
    recall(&["-GateId", "NOT-HERE"]).blocked(
        "unknown gate fails with the intended diagnostic",
        "unknown-gate-id",
    );
    for objective in [
        "champagne and fortune",
        "pre-deploy check",
        "post-roll back-check",
    ] {
        recall(&["-Objective", objective, "-Brief"])
            .clear(&format!("literal boundaries clear: {objective}"));
    }
    recall(&["-Objective", "(roll back) now", "-Brief"]).success(
        "phrase with punctuation boundaries matches",
        &["EXAMPLE-DOMAIN"],
    );
    for (position, left, right) in [
        ("beginning", 0, 100_000),
        ("middle", 50_000, 50_000),
        ("end", 100_000, 0),
    ] {
        let text = format!("{} deploy {}", "x".repeat(left), "x".repeat(right));
        invoke(root, "brain-recall", &["-FromStdin", "-Brief"], &text).success(
            &format!("100k stdin trigger at {position} survives"),
            &["EXAMPLE-DOMAIN"],
        );
    }
    invoke(
        root,
        "brain-recall",
        &["-FromStdin", "-Brief"],
        &format!("{} deploy", "x".repeat(131_065)),
    )
    .success(
        "objective exactly at 131072-character boundary passes",
        &["EXAMPLE-DOMAIN"],
    );
    invoke(
        root,
        "brain-recall",
        &["-FromStdin", "-Brief"],
        &"x".repeat(131_073),
    )
    .blocked(
        "objective over boundary is rejected, not truncated",
        "objective-too-large",
    );
    for bad in [
        "broken-json",
        "[]",
        r#"[{"version":1,"gates":[]}]"#,
        r#""scalar""#,
        r#"{"version":1,"gates":null}"#,
        r#"{"version":2,"gates":[]}"#,
    ] {
        put(root, GATES, bad);
        recall(&["-CheckGates"]).blocked(
            &format!("invalid schema rejected: {bad}"),
            "invalid-gate-data",
        );
    }
    for kind in [
        "scalar-triggers",
        "negative-count",
        "string-count",
        "fraction-count",
        "duplicate-gate",
        "duplicate-trap",
        "missing-right-move",
        "unsafe-id",
        "empty-context-side",
    ] {
        let mut data = example.clone();
        match kind {
            "scalar-triggers" => data["gates"][0]["triggers"] = json!("again"),
            "negative-count" => data["gates"][0]["times_recurred"] = json!(-1),
            "string-count" => data["gates"][0]["times_recurred"] = json!("1"),
            "fraction-count" => data["gates"][0]["times_recurred"] = json!(1.5),
            "duplicate-gate" => data["gates"][1]["id"] = data["gates"][0]["id"].clone(),
            "duplicate-trap" => {
                data["gates"][1]["traps"][0]["id"] = data["gates"][0]["traps"][0]["id"].clone()
            }
            "missing-right-move" => {
                data["gates"][0]["traps"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("right_move");
            }
            "unsafe-id" => data["gates"][0]["id"] = json!("G; command"),
            "empty-context-side" => {
                data["gates"][0]["context_rules"] = json!([{"any":[],"with":["workflow"]}])
            }
            _ => unreachable!(),
        }
        save(root, &data);
        recall(&["-Objective", "ordinary work"])
            .blocked(&format!("schema blocks {kind}"), "invalid-gate-data");
    }
    save(root, &json!({"version":1,"gates":[]}));
    recall(&["-Objective", "anything", "-Brief"])
        .clear("deliberate empty registry is healthy CLEAR");
    let mut paired = example.clone();
    paired["gates"][0]["triggers"] = json!([]);
    paired["gates"][0]["aliases"] = json!([]);
    paired["gates"][0]["context_rules"] = json!([{"any":["remember"],"with":["workflow"]}]);
    save(root, &paired);
    recall(&["-Objective", "remember the workflow", "-Brief"]).success(
        "paired context needs and accepts both sides",
        &["RECURRING-WORK"],
    );
    recall(&["-Objective", "remember a birthday", "-Brief"])
        .clear("one context side alone stays CLEAR");

    let gates = |ids: Vec<String>, reads: &[&str]| {
        json!({"version":1,"gates":ids.into_iter().enumerate().map(|(i,id)| json!({
        "id": id, "label": "Fixture", "times_recurred": i, "triggers": ["signal"], "required_reads": reads, "stop_signs": [], "traps": []
    })).collect::<Vec<_>>()})
    };
    let ids = (1..=18).map(|i| format!("GATE-{i}")).collect::<Vec<_>>();
    save(root, &gates(ids.clone(), &["reference/INDEX.md"]));
    let reply = recall(&["-Objective", "signal", "-Brief"]);
    reply.verify(
        "compact output preserves every matched gate and full-read path",
        reply.code == 0
            && reply.out.trim().encode_utf16().count() <= 1400
            && ids.iter().all(|id| {
                reply
                    .out
                    .lines()
                    .any(|line| line.starts_with(&format!("- {id} ")))
            })
            && reply.out.contains("NEEDS_READ=")
            && reply.out.contains("For EACH gate ID:"),
    );
    save(
        root,
        &gates(
            (1..=40)
                .map(|i| format!("G{i}{}", "X".repeat(70)))
                .collect(),
            &[],
        ),
    );
    recall(&["-Objective", "signal", "-Brief"]).blocked(
        "unrepresentable compact gate list fails visibly",
        "compact-budget-exceeded",
    );
    hook(r#"{"prompt":"signal"}"#).degraded(
        "hook degrades instead of dropping overflow gates",
        "matcher-failed",
    );
    reset();
    let reply = recall(&["-Increment", "STORED-BUT-UNUSED"]);
    let mut data: Value = serde_json::from_slice(&fs::read(root.join(GATES)).unwrap()).unwrap();
    reply.verify(
        "recurrence mutation increments trap and gate exactly once",
        reply.code == 0
            && data["gates"][0]["times_recurred"] == 1
            && data["gates"][0]["traps"][0]["times_recurred"] == 1,
    );
    data["gates"][0]["traps"][0]["times_recurred"] = json!(i32::MAX);
    save(root, &data);
    let before = fs::read(root.join(GATES)).unwrap();
    let reply = recall(&["-Increment", "STORED-BUT-UNUSED"]);
    reply.blocked(
        "recurrence overflow fails without changing data",
        "recurrence-count-overflow",
    );
    check(
        "recurrence overflow fails without changing data",
        fs::read(root.join(GATES)).unwrap() == before,
        "gate bytes",
    );
}

#[test]
fn recall_hook_fallback_and_rulings() {
    let fixture = Sandbox::new();
    let root = &fixture.0;
    let seed = fs::read_to_string(template().join(EXAMPLE))
        .unwrap()
        .replace("\r\n", "\n");
    let example: Value = serde_json::from_str(&seed).unwrap();
    let recall = |args: &[&str]| invoke(root, "brain-recall", args, "");
    let hook = |input: &str| invoke(root, "hook-recall", &[], input);
    let fallback = |args: &[&str], input: &str| invoke(root, "prepare-work", args, input);
    let reset = || put(root, GATES, &seed);
    reset();
    hook(r#"{"user_input":"polish typography"}"#).clear("hook healthy CLEAR is empty exit zero");
    let reply = hook(r#"{"user_input":"deploy again"}"#);
    let context = reply.context();
    reply.verify(
        "hook MATCH returns valid typed context with all matching gates",
        reply.code == 0 && context.contains("EXAMPLE-DOMAIN") && context.contains("RECURRING-WORK"),
    );
    hook(r#"{"user_input":"polish typography","prompt":"deploy"}"#)
        .clear("nonblank user_input takes precedence over prompt");
    let reply = hook(r#"{"prompt":"ship\u0000it\u0007 \u001b[0m deploy\u0000"}"#);
    reply.verify(
        "embedded NUL and control characters still match without error",
        reply.code == 0 && reply.err.is_empty() && reply.out.contains("EXAMPLE-DOMAIN"),
    );
    hook(r#"{"prompt":"\u0000\u0001\u0002"}"#).clear("control-only prompt is healthy CLEAR");
    hook(r#"{"user_input":null,"prompt":"deploy"}"#)
        .success("null user_input can use string prompt", &["EXAMPLE-DOMAIN"]);
    hook(r#"{"prompt":"deploy"}"#).success("prompt field is supported", &["EXAMPLE-DOMAIN"]);
    for (payload, reason) in [
        ("", "empty-input"),
        (r#"{"prompt": "PRIVATE-CANARY""#, "invalid-json"),
        (r#"["deploy"]"#, "payload-not-object"),
        (r#"[{"prompt":"deploy"}]"#, "payload-not-object"),
        (r#""deploy""#, "payload-not-object"),
        ("null", "payload-not-object"),
        (r#"{"prompt":42}"#, "prompt-not-string"),
        (
            r#"{"user_input":{},"prompt":"deploy"}"#,
            "prompt-not-string",
        ),
        (r#"{"prompt":" "}"#, "missing-prompt"),
    ] {
        hook(payload).degraded(
            &format!("hook visibly degrades: {reason} ({payload:?})"),
            reason,
        );
    }
    let long = format!("{} deploy", "x".repeat(100_000));
    hook(&json!({"prompt":long}).to_string()).success(
        "hook preserves a late trigger in long prompt",
        &["EXAMPLE-DOMAIN"],
    );
    hook(&json!({"prompt":format!("PRIVATE-CANARY {}", "x".repeat(131_073))}).to_string())
        .degraded(
            "hook bounds decoded prompt without echoing it",
            "prompt-too-large",
        );
    hook(&" ".repeat(1_048_577)).degraded("hook bounds raw payload", "payload-too-large");
    put(root, GATES, r#"{"version":1,"gates":null}"#);
    hook(r#"{"prompt":"ordinary"}"#).degraded(
        "malformed gate schema is visible through adapter",
        "matcher-failed",
    );
    let reply = fallback(&["-Objective", "ordinary"], "");
    reply.verify(
        "fallback propagates malformed schema failure without READY",
        reply.code == 2
            && reply.err.contains("PREPARE_WORK_STATUS=BLOCKED")
            && !reply.out.contains("PREPARE_WORK_STATUS=READY"),
    );
    reset();
    fallback(&["-FromStdin"], &long).success(
        "fallback supports long stdin and full required reads",
        &[
            "EXAMPLE-DOMAIN",
            "Required reads:",
            "PREPARE_WORK_STATUS=READY",
        ],
    );

    let mut data = example.clone();
    data["gates"][1]["rulings"] = json!(["D7"]);
    save(root, &data);
    let deploy = || recall(&["-Objective", "deploy"]);
    deploy().blocked(
        "a missing cited ruling index blocks, never silently drops",
        "invalid-rulings",
    );
    rulings(root, "operator-ruled", "D7");
    deploy().success(
        "full recall exposes complete cited ruling",
        &["D7 [operator-ruled]", "including this final clause."],
    );
    recall(&["-Objective", "deploy", "-Brief"]).success(
        "compact recall retains cited ruling IDs and explicit expansion requirement",
        &["rulings: D7", "NEEDS_READ=", "2 reads"],
    );
    recall(&["-Ruling", "7"]).success(
        "direct ruling ID lookup returns full rule",
        &[
            "RULING_STATUS=FOUND count=1",
            "including this final clause.",
        ],
    );
    recall(&["-Ruling", "required source"])
        .success("ruling words use literal substring search", &["D7"]);
    let reply = recall(&["-Ruling", "*"]);
    reply.verify(
        "ruling words are not wildcard patterns",
        reply.code == 1 && reply.out.contains("RULING_STATUS=NONE"),
    );
    rulings(root, "proposed", "D7");
    deploy().blocked(
        "proposal cannot enter live ruling output",
        "invalid-rulings",
    );
    rulings(root, "operator-ruled", "D8");
    deploy().blocked("unknown cited decision blocks recall", "invalid-rulings");
    rulings(root, "operator-ruled", "D7");
    put(root, "DECISIONS.md", "changed after compilation");
    deploy().blocked(
        "stale decision source hash blocks recall",
        "invalid-rulings",
    );
    reset();
    recall(&["-Objective", "deploy", "-Brief"]).success(
        "uncited optional ruling index does not block generic gates",
        &["EXAMPLE-DOMAIN"],
    );

    let mut data = example.clone();
    data["gates"][1]["aliases"]
        .as_array_mut()
        .unwrap()
        .extend([json!("überprüfen"), json!("部署")]);
    save(root, &data);
    recall(&["-Objective", "bitte überprüfen", "-Brief"]).success(
        "non-ASCII trigger matches from an argument",
        &["EXAMPLE-DOMAIN"],
    );
    invoke(
        root,
        "brain-recall",
        &["-FromStdin", "-Brief"],
        "bitte überprüfen",
    )
    .success(
        "non-ASCII trigger matches from UTF-8 stdin",
        &["EXAMPLE-DOMAIN"],
    );
    hook(r#"{"prompt":"请部署"}"#).clear(
        "non-ASCII CJK trigger in raw UTF-8 hook JSON stays CLEAR at a word-character boundary",
    );
    let reply = hook(r#"{"prompt":"请 部署 一下"}"#);
    reply.verify(
        "non-ASCII CJK trigger in raw UTF-8 hook JSON matches",
        reply.code == 0 && reply.err.is_empty() && reply.out.contains("EXAMPLE-DOMAIN"),
    );
    hook(r#"{"prompt":"bitte \u00fcberpr\u00fcfen"}"#)
        .success("escaped non-ASCII hook prompt matches", &["EXAMPLE-DOMAIN"]);
    let reply = fallback(&["-FromStdin"], "bitte überprüfen");
    reply.verify(
        "fallback matches non-ASCII stdin and reports READY last",
        reply.code == 0
            && reply.out.contains("EXAMPLE-DOMAIN")
            && reply
                .out
                .lines()
                .last()
                .unwrap_or("")
                .contains("PREPARE_WORK_STATUS=READY"),
    );
    put(
        root,
        GATES,
        format!("\u{feff}{}", seed.replace('\n', "\r\n")),
    );
    let validate = recall(&["-CheckGates"]);
    let reply = recall(&["-Objective", "deploy", "-Brief"]);
    reply.verify(
        "BOM and CRLF gate file validates and matches",
        reply.code == 0
            && reply.out.contains("EXAMPLE-DOMAIN")
            && validate.code == 0
            && validate.out.contains("VALID gates=3"),
    );

    let mut data = example;
    data["x_adopter"] = ('a'..='m')
        .rev()
        .fold(json!("deep"), |value, key| json!({key.to_string():value}));
    data["gates"][1]["traps"][0]["triggers"] = json!(["solo"]);
    save(root, &data);
    let reply = recall(&["-Increment", "EXAMPLE-TRAP"]);
    let after: Value = serde_json::from_slice(&fs::read(root.join(GATES)).unwrap()).unwrap();
    let kept = after["description"] == data["description"]
        && after["x_adopter"] == data["x_adopter"]
        && after
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>()
            == ["version", "updated", "description", "gates", "x_adopter"]
        && regex::Regex::new(r"^\d{4}-\d{2}-\d{2}$")
            .unwrap()
            .is_match(after["updated"].as_str().unwrap());
    reply.verify(
        "increment round trip keeps fields, order and single-item arrays and stays valid",
        reply.code == 0
            && kept
            && after["gates"][1]["traps"][0]["triggers"] == json!(["solo"])
            && after["gates"][1]["traps"][0]["times_recurred"] == 1
            && recall(&["-CheckGates"]).code == 0,
    );
    reset();

    let reply = hook(r#"{"prompt":"deploy"}"#);
    reply.verify(
        "engine: hook answers from the gates with the matcher script missing",
        reply.code == 0 && reply.out.contains("EXAMPLE-DOMAIN") && !reply.out.contains("DEGRADED"),
    );
    fallback(&["-Objective", "deploy"], "").success(
        "engine: fallback is READY with the matcher script missing",
        &["EXAMPLE-DOMAIN", "PREPARE_WORK_STATUS=READY"],
    );
    let impostor = ["bin/brain-recall.", "ps1"].concat();
    for (body, name) in [
        (
            "exit 2",
            "engine: a failing matcher script does not change the answer",
        ),
        (
            r#"[Console]::Error.WriteLine("warning"); exit 0"#,
            "engine: a noisy matcher script does not change the answer",
        ),
        (
            r#"Write-Output "unexpected output""#,
            "engine: unexpected matcher-script output does not reach the hook",
        ),
    ] {
        put(root, &impostor, body);
        hook(r#"{"prompt":"ordinary"}"#).clear(name);
    }
    let pid = root.join("matcher.pid");
    put(
        root,
        &impostor,
        format!(
            "[IO.File]::WriteAllText('{}', [string]$PID); Start-Sleep -Seconds 60",
            pid.display()
        ),
    );
    let start = std::time::Instant::now();
    let reply = hook(r#"{"prompt":"ordinary"}"#);
    reply.verify(
        "engine: a hanging matcher script is never started and the hook answers at once",
        reply.code == 0
            && reply.out.is_empty()
            && reply.err.is_empty()
            && !pid.exists()
            && start.elapsed().as_secs() < 20,
    );
}
