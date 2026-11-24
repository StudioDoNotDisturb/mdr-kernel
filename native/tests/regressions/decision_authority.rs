use super::*;
use serde_json::{json, Value};

const EMPTY: &str =
    "# Decisions\n\n_No decisions yet._\n\n---\n\n## SYSTEM NODE LINKS\nDirectory Path: ./\n";
const ONE: &str = "## D1 - 2026-01-01 - One\n\n**Authority:** proposed\n\n**Decision:** Keep useful evidence.\n\n";
const TWO: &str = "## D2 - 2026-01-02 - Two\n\n**Authority:** operator-ruled\n\n**Operator said (chat, 2026-01-02):** \"Keep every verified receipt intact.\"\n\n**Decision:** Preserve the full evidence, including uncertainty.\n\n";
const QUOTE: &str = "\"Keep every verified receipt intact.\"";
const OPERATOR: &str =
    "**Operator said (chat, 2026-01-02):** \"Keep every verified receipt intact.\"";
const POLICY: &str = "protocol/decision-authority.json";
const GATES: &str = "protocol/recall-gates.json";
const LEDGER: &str = "protocol/decision-ratifications.jsonl";
const RULINGS: &str = "protocol/rulings.json";
const CHECK: &str = "check-decision-history";
const ADD: &str = "add-decision";
const BUILD: &str = "build-rulings";
const INIT: &str = "initialize-decision-authority";

fn scaffold() -> &'static str {
    include_str!("../../src/onboard.rs")
        .split_once("const DECISIONS: &str = \"")
        .unwrap()
        .1
        .split_once("\";")
        .unwrap()
        .0
}

fn ruled(id: usize, extra: &str, stamp: &str) -> String {
    let stamp = if stamp.is_empty() {
        String::new()
    } else {
        format!("{stamp}\n")
    };
    format!("## D{id} - 2026-01-0{id} - Rule {id}\n{stamp}\n**Authority:** operator-ruled\n\n**Operator said (chat, 2026-01-0{id}):** \"Rule number {id} is what I want.\"\n\n**Decision:** Rule {id} body text.\n\n{extra}")
}

fn invoke(root: &Path, name: &str, args: &[&str]) -> (i32, String) {
    let mut argv = vec![name, "-Root", root.to_str().unwrap()];
    argv.extend(args);
    run(root, &argv)
}

fn read(root: &Path, path: &str) -> String {
    fs::read_to_string(root.join(path)).unwrap()
}

fn expect_entry(root: &Path, text: &str, name: &str, passes: bool, reason: &str, extra: &[&str]) {
    put(root, "case.md", text);
    let mut args = vec!["-Path", "case.md"];
    args.extend(extra);
    let out = invoke(root, CHECK, &args);
    authority_ok(&out, name, passes.then_some(0), &[reason]);
}

fn hash(root: &Path, path: &str, id: &str) -> String {
    let out = invoke(root, CHECK, &["-Path", path, "-HashOf", id]);
    assert_eq!(out.0, 0, "{out:?}");
    let hash = out
        .1
        .lines()
        .find_map(|line| line.strip_prefix("ENTRY_SHA256="))
        .unwrap();
    assert_eq!(hash.len(), 64, "{out:?}");
    hash.to_owned()
}

fn rat(id: &str, sha: &str) -> String {
    json!({"id":id,"entry_sha256":sha,"ratified_at":"2026-01-05T00:00:00Z","quote":"synthetic test fixture","surface":"terminal"}).to_string()
}

fn add(root: &Path, path: &str, title: &str, body: &str) -> (i32, String) {
    invoke(root, ADD, &["-Path", path, "-Title", title, "-Body", body])
}

fn compiled(root: &Path, path: &str) -> Value {
    serde_json::from_str(&read(root, path)).unwrap()
}

fn gate_ids(root: &Path, gates: &[(&str, &str)]) {
    put(root, GATES, json!({"gates": gates.iter().map(|(id, decision)|json!({"id":id,"rulings":[decision]})).collect::<Vec<_>>()}).to_string());
}

fn bom_crlf(text: &str) -> String {
    format!("\u{feff}{}", text.replace('\n', "\r\n"))
}

#[test]
fn decision_structure_history_and_ratification() {
    let sandbox = Sandbox::new();
    let root = &sandbox.0;
    let base = format!("{TWO}{ONE}");
    let with_one = |entry: String| entry + ONE;
    let retired = |day| ruled(2, "", &format!("_Status: retired by D3 (2026-01-0{day})_"));
    let one_stamped = ONE.replace("One\n", "One\n_Status: superseded by D3 (2026-01-03)_\n");
    let proposed_three =
        "## D3 - 2026-01-03 - Three\n\n**Authority:** proposed\n\n**Decision:** Replace it.\n\n";
    let cases = [
        ("empty new log passes", EMPTY.into(), true, ""),
        ("bootstrap scaffold passes", scaffold().into(), true, ""),
        ("proposal passes", ONE.into(), true, ""),
        ("quoted ruling passes", base.clone(), true, ""),
        (
            "duplicate id refused",
            ONE.repeat(2),
            false,
            "Duplicate decision id D1",
        ),
        (
            "gap refused",
            with_one(TWO.replace("D2", "D3")),
            false,
            "Missing decision id D2",
        ),
        (
            "wrong order refused",
            format!("{ONE}{TWO}"),
            false,
            "not strictly descending: D1 precedes D2",
        ),
        (
            "order checked across 3 entries",
            format!("{}{ONE}{TWO}", ruled(3, "", "")),
            false,
            "D1 precedes D2",
        ),
        (
            "em dash heading passes",
            with_one(TWO.replace(" - 2026-01-02 - ", " — 2026-01-02 — ")),
            true,
            "",
        ),
        (
            "curly-quoted operator words pass",
            with_one(TWO.replace(QUOTE, "“Keep every verified receipt intact.”")),
            true,
            "",
        ),
        (
            "curly-quoted operator words use UTF-16 length",
            with_one(TWO.replace(QUOTE, "“🧾🧾🧾🧾🧾🧾”")),
            true,
            "",
        ),
        (
            "unclassified new entry refused",
            ONE.replace("**Authority:** proposed", ""),
            false,
            "needs exactly one **Authority:** line (operator-ruled or proposed); found 0",
        ),
        (
            "duplicate Authority lines refused",
            ONE.replace(
                "**Authority:** proposed",
                "**Authority:** proposed\n\n**Authority:** operator-ruled",
            ),
            false,
            "needs exactly one **Authority:** line (operator-ruled or proposed); found 2",
        ),
        (
            "invented ruling without quote refused",
            with_one(TWO.replace(OPERATOR, "")),
            false,
            "operator-ruled but has no **Operator said",
        ),
        (
            "work report refused",
            ONE.replace("proposed", "record"),
            false,
            "work report belongs in a handoff",
        ),
        (
            "typed ratification refused",
            ONE.replace("proposed", "operator-ratified"),
            false,
            "never typed",
        ),
        (
            "missing decision body refused",
            ONE.replace("**Decision:**", "**Why:**"),
            false,
            "D1 needs one **Decision:** body",
        ),
        (
            "proposal cannot supersede a ruling (checker)",
            format!(
                "{proposed_three}**Supersedes:** D2\n\n{}{ONE}",
                TWO.replace("Two\n", "Two\n_Status: superseded by D3 (2026-01-03)_\n")
            ),
            false,
            "D3 is proposed and cannot supersede D2",
        ),
        (
            "proposal cannot retire even a proposal",
            format!(
                "{proposed_three}**Retires:** D1\n\n{TWO}{}",
                ONE.replace("One\n", "One\n_Status: retired by D3 (2026-01-03)_\n")
            ),
            false,
            "D3 is proposed and cannot retire D1",
        ),
        (
            "stamp without a backing field refused",
            format!("{}{}{ONE}", ruled(3, "", ""), retired(3)),
            false,
            "D3 does not list D2",
        ),
        (
            "field without a stamp refused",
            format!(
                "{}{}{ONE}",
                ruled(3, "**Retires:** D2\n\n", ""),
                ruled(2, "", "")
            ),
            false,
            "D2 has no '_Status: retired by D3",
        ),
        (
            "stamp dated differently from its entry refused",
            format!("{}{}{ONE}", ruled(3, "**Retires:** D2\n\n", ""), retired(9)),
            false,
            "dated 2026-01-09, not D3",
        ),
    ];
    for (name, text, passes, reason) in cases {
        expect_entry(root, &text, name, passes, reason, &[]);
    }

    put(root, POLICY, r#"{"schema_version":1,"authority_from":2}"#);
    let legacy =
        format!("{TWO}## D1 - 2026-01-01 - Old\n\n**Decision:** An old unclassified entry.\n\n");
    expect_entry(
        root,
        &legacy,
        "repository policy never applies to a fixture path",
        false,
        "D1 needs exactly one **Authority:** line",
        &[],
    );
    expect_entry(
        root,
        &legacy,
        "explicit -AuthorityFrom applies to a fixture path",
        true,
        "",
        &["-AuthorityFrom", "2"],
    );
    fs::remove_file(root.join(POLICY)).unwrap();
    put(root, "lf.md", &base);
    put(root, "crlf.md", base.replace('\n', "\r\n"));
    put(root, "bom.md", bom_crlf(&base));
    let digest = hash(root, "lf.md", "D2");
    check(
        "LF, CRLF and BOM logs hash identically",
        digest == hash(root, "crlf.md", "D2") && digest == hash(root, "bom.md", "D2"),
        &digest,
    );
    put(
        root,
        "stamped.md",
        format!(
            "{}{}{ONE}",
            ruled(3, "**Supersedes in part:** D2\n\n", ""),
            TWO.replace(
                "Two\n",
                "Two\n_Status: superseded in part by D3 (2026-01-03)_\n"
            )
        ),
    );
    check(
        "stamp under the heading is excluded from the hash",
        digest == hash(root, "stamped.md", "D2"),
        &digest,
    );
    put(root, "scaffold.md", scaffold());
    let proposal = "**Authority:** proposed\n\n**Decision:** Record the observed behavior.";
    let out = add(root, "scaffold.md", "First proposal", proposal);
    let appended = read(root, "scaffold.md");
    check(
        "first append to the bootstrap scaffold works",
        out.0 == 0
            && appended.lines().any(|line| line.starts_with("## D1 - "))
            && !appended.contains("_No decisions yet._"),
        out,
    );
    let digest = hash(root, "scaffold.md", "D1");
    put(
        root,
        "scaffold2.md",
        appended.replace(
            "Next Sequential File: STATE.md",
            "Next Sequential File: README.md",
        ),
    );
    check(
        "footer edits do not move the oldest entry hash",
        digest == hash(root, "scaffold2.md", "D1"),
        &digest,
    );
    put(
        root,
        "scaffold3.md",
        appended.replace("---\n\n## SYSTEM NODE LINKS", "---\n## SYSTEM NODE LINKS"),
    );
    check(
        "footer without a blank line is still excluded",
        digest == hash(root, "scaffold3.md", "D1"),
        &digest,
    );
    put(root, "crlfbom.md", bom_crlf(&base));
    let out = add(
        root,
        "crlfbom.md",
        "Third",
        "**Authority:** proposed\n\n**Decision:** Keep the line endings.",
    );
    let after = read(root, "crlfbom.md");
    check(
        "add-decision preserves CRLF and BOM",
        out.0 == 0
            && after.starts_with("\u{feff}## D3 - ")
            && !after.replace("\r\n", "").contains('\n'),
        out,
    );
    put(root, "case.md", &base);
    authority_ok(
        &add(
            root,
            "case.md",
            "Bad replacement",
            "**Authority:** proposed\n\n**Decision:** Replace the ruling.\n\n**Supersedes:** D2",
        ),
        "proposal cannot supersede ruling (add-decision)",
        None,
        &["D3 is proposed and cannot supersede D2", "rolled back"],
    );
    check(
        "failed append restores exact bytes",
        fs::read(root.join("case.md")).unwrap() == base.as_bytes(),
        &base,
    );
    put(root, "case.md", EMPTY);
    let out = add(root, "case.md", "First proposal", proposal);
    check(
        "first append to empty log works",
        out.0 == 0 && read(root, "case.md").contains("## D1"),
        out,
    );
    let out = add(
        root,
        "case.md",
        "Second proposal",
        "**Authority:** proposed\n\n**Decision:** Refine the observation.\n\n**Supersedes:** D1",
    );
    check(
        "proposal can supersede proposal",
        out.0 == 0 && read(root, "case.md").contains("_Status: superseded by D2"),
        out,
    );

    put(root, "case.md", &base);
    let line = rat("D1", &hash(root, "case.md", "D1"));
    put(root, "rat.jsonl", &line);
    let rat_args = ["-RatificationsPath", "rat.jsonl"];
    authority_ok(
        &invoke(
            root,
            CHECK,
            &["-Path", "case.md", "-RatificationsPath", "rat.jsonl"],
        ),
        "valid ratification accepted",
        Some(0),
        &["ratified=1"],
    );
    put(root, "forged.jsonl", rat("D1", &"ab".repeat(32)));
    authority_ok(
        &invoke(
            root,
            CHECK,
            &["-Path", "case.md", "-RatificationsPath", "forged.jsonl"],
        ),
        "forged ratification hash refused",
        None,
        &["D1 no longer matches the hash it was ratified at"],
    );
    expect_entry(
        root,
        &base.replace("useful", "edited"),
        "entry edited after ratification loses it",
        false,
        "D1 no longer matches",
        &rat_args,
    );
    expect_entry(
        root,
        &format!("{proposed_three}**Supersedes:** D1\n\n{TWO}{one_stamped}"),
        "ratified proposal cannot be superseded by a proposal",
        false,
        "D3 is proposed and cannot supersede D1",
        &rat_args,
    );
    expect_entry(
        root,
        &format!(
            "{}{TWO}{one_stamped}",
            ruled(3, "**Supersedes:** D1\n\n", "")
        ),
        "ratification survives a later supersession stamp",
        true,
        "ratified=1",
        &rat_args,
    );
    put(root, "case.md", &base);
    put(
        root,
        "rat-base.jsonl",
        format!("{line}\n{}", rat("D2", &hash(root, "case.md", "D2"))),
    );
    authority_ok(
        &invoke(
            root,
            CHECK,
            &[
                "-Path",
                "case.md",
                "-RatificationsPath",
                "rat.jsonl",
                "-RatificationsBaselinePath",
                "rat-base.jsonl",
            ],
        ),
        "ratification ledger is append-only",
        None,
        &["is append-only: an existing ratification record was changed or removed"],
    );

    put(root, "DECISIONS.md", &base);
    let out = invoke(root, "ratify-decision", &["-Id", "D1"]);
    check(
        "ratify refuses redirected stdin and writes nothing",
        out.0 == 3
            && out.1.contains("RATIFICATION_STATUS=REFUSED")
            && out
                .1
                .to_lowercase()
                .contains("standard input is redirected")
            && !root.join(LEDGER).exists(),
        out,
    );
}

#[test]
fn decision_rulings_and_index() {
    let sandbox = Sandbox::new();
    let root = &sandbox.0;
    let base = format!("{TWO}{ONE}");
    let build = |args| invoke(root, BUILD, args);
    put(root, "DECISIONS.md", &base);
    gate_ids(root, &[("A-1", "D2"), ("A_1", "D2")]);
    let out = build(&[]);
    let doc = compiled(root, RULINGS);
    check(
        "compiler excludes unratified proposal and sorts gate IDs",
        out.0 == 0
            && doc["total"] == 1
            && doc["rulings"][0]["id"] == "D2"
            && doc["rulings"][0]["gates"] == json!(["A_1", "A-1"]),
        out,
    );
    authority_ok(
        &build(&["-Check"]),
        "fresh compiled output passes -Check",
        Some(0),
        &["RULINGS_STATUS=OK total=1"],
    );
    put(root, "DECISIONS.md", format!("{}{base}", ruled(3, "", "")));
    authority_ok(
        &build(&["-Check"]),
        "stale compiled output fails -Check after a decision change",
        Some(1),
        &["RULINGS_STATUS=STALE"],
    );
    gate_ids(root, &[("TEST", "D1")]);
    authority_ok(
        &build(&[]),
        "gate cannot cite proposal",
        None,
        &["not live rulings", "D1 (cited by TEST)"],
    );
    put(
        root,
        "DECISIONS.md",
        format!(
            "{}{}{}{}",
            ruled(4, "**Supersedes in part:** D3\n\n", ""),
            ruled(
                3,
                "**Supersedes:** D2\n\n",
                "_Status: superseded in part by D4 (2026-01-04)_"
            ),
            ruled(
                2,
                "**Retires:** D1\n\n",
                "_Status: superseded by D3 (2026-01-03)_"
            ),
            ruled(1, "", "_Status: retired by D2 (2026-01-02)_")
        ),
    );
    gate_ids(root, &[]);
    let out = build(&[]);
    let doc = compiled(root, RULINGS);
    let ids: Vec<_> = doc["rulings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["id"].as_str().unwrap())
        .collect();
    check(
        "compiler drops superseded/retired and annotates superseded in part",
        out.0 == 0
            && ids == ["D4", "D3"]
            && doc["rulings"][1]["rule"]
                .as_str()
                .unwrap()
                .contains("superseded in part"),
        out,
    );
    gate_ids(
        root,
        &[
            ("OLD", "D1"),
            ("REPLACED", "D2"),
            ("GHOST", "D9"),
            ("LIVE", "D3"),
        ],
    );
    let out = build(&[]);
    check(
        "gate citing retired, superseded or unknown ids fails compilation",
        out.0 == 1
            && [
                "D1 (cited by OLD)",
                "D2 (cited by REPLACED)",
                "D9 (cited by GHOST)",
            ]
            .iter()
            .all(|needle| out.1.contains(needle))
            && !out.1.contains("D3 (cited"),
        out,
    );
    gate_ids(root, &[]);
    put(root, "DECISIONS.md", EMPTY);
    let out = build(&[]);
    check(
        "empty log compiles to total=0",
        out.0 == 0 && compiled(root, RULINGS)["total"] == 0,
        out,
    );
    put(root, "DECISIONS.md", &base);
    put(root, LEDGER, rat("D1", &"0".repeat(64)));
    authority_ok(
        &build(&[]),
        "compiler refuses forged hash",
        None,
        &["no longer matches"],
    );
    put(root, "case.md", &base);
    let out = build(&["-DecisionsPath", "case.md", "-OutPath", "case-rulings.json"]);
    let fixture = compiled(root, "case-rulings.json");
    check(
        "fixture compile ignores the live ledger",
        out.0 == 0 && fixture["total"] == 1 && fixture["rulings"][0]["id"] == "D2",
        out,
    );
    put(root, LEDGER, "");
    put(root, POLICY, r#"{"schema_version":1,"authority_from":3}"#);
    put(root, "DECISIONS.md", base.replace(OPERATOR, ""));
    let out = build(&[]);
    check(
        "legacy operator-ruled without operator words is not compiled",
        out.0 == 0 && compiled(root, RULINGS)["total"] == 0,
        out,
    );
    fs::remove_file(root.join(POLICY)).unwrap();
    put(root, "DECISIONS.md", bom_crlf(&base));
    git(root, &["init", "-q", "-b", "main"]);
    configure(root);
    stage_compiler_inputs(root);
    let out = build(&[]);
    assert_eq!(out.0, 0, "{out:?}");
    git(root, &["add", "--", "DECISIONS.md", "protocol"]);
    authority_ok(
        &build(&["-Check", "-FromIndex"]),
        "BOM/CRLF log: staged -Check agrees with the working-tree build",
        Some(0),
        &["RULINGS_STATUS=OK"],
    );
    git(root, &["commit", "-qm", "baseline"]);
    let staged = || invoke(root, CHECK, &["-FromIndex"]);
    put(
        root,
        "DECISIONS.md",
        bom_crlf(&base.replace("useful", "quietly revised")),
    );
    git(root, &["add", "--", "DECISIONS.md"]);
    let out = staged();
    check(
        "silent old-body rewrite caught against HEAD",
        out.0 != 0 && out.1.contains("D1 differs from HEAD") && out.1.contains("append-only"),
        out,
    );
    git(root, &["reset", "-q", "--", "DECISIONS.md"]);
    put(root, "DECISIONS.md", bom_crlf(&base));
    for (policy, name, code, message) in [
        (
            r#"{"schema_version":1,"authority_from":4}"#,
            "migration cannot exempt future entries",
            None,
            "only decision entries already committed",
        ),
        (
            r#"{"schema_version":2,"authority_from":3}"#,
            "unknown policy schema refused",
            None,
            "Invalid protocol/decision-authority.json",
        ),
        (
            r#"{"schema_version":1,"authority_from":3}"#,
            "explicit legacy boundary preserves history",
            Some(0),
            "",
        ),
    ] {
        put(root, POLICY, policy);
        git(root, &["add", "--", POLICY]);
        authority_ok(&staged(), name, code, &[message]);
    }
    git(root, &["commit", "-qm", "migration"]);
    put(root, POLICY, r#"{"schema_version":1,"authority_from":5}"#);
    git(root, &["add", "--", POLICY]);
    authority_ok(
        &staged(),
        "committed boundary cannot move",
        None,
        &["boundary is immutable"],
    );
    git(root, &["rm", "-q", "--cached", "--", POLICY]);
    authority_ok(
        &staged(),
        "committed boundary cannot be deleted",
        None,
        &["boundary cannot be deleted"],
    );
}

#[test]
fn decision_initializer() {
    let sandbox = Sandbox::new();
    let root = &sandbox.repo("initializer");
    stage_compiler_inputs(root);
    put(root, "DECISIONS.md", scaffold());
    gate_ids(root, &[]);
    let out = invoke(
        root,
        INIT,
        &["-Model", "fixture-model", "-Token", "not-a-token"],
    );
    check(
        "initializer requires the lock",
        out.0 != 0
            && out
                .1
                .contains("Own STATE.lock before initializing authority")
            && !root.join(POLICY).exists(),
        out,
    );
    let lock = invoke(
        root,
        "state-lock",
        &[
            "-Acquire",
            "-Model",
            "fixture-model",
            "-Objective",
            "decision authority test",
        ],
    );
    let token = lock
        .1
        .lines()
        .find_map(|line| line.trim().strip_prefix("session token: "))
        .unwrap_or_else(|| panic!("{lock:?}"));
    let init = |preserve| {
        let mut args = vec!["-Model", "fixture-model", "-Token", token];
        if preserve {
            args.push("-PreserveLegacy");
        }
        invoke(root, INIT, &args)
    };
    let out = init(false);
    check(
        "empty log starts at D1 with the bootstrap policy form",
        out.0 == 0
            && out.1.contains("AUTHORITY_INITIALIZED from=D1")
            && read(root, POLICY).trim() == r#"{"schema_version":1,"authority_from":1}"#,
        out,
    );
    authority_ok(
        &init(false),
        "initializer never rewrites a boundary",
        None,
        &["already exists; it is immutable"],
    );
    fs::remove_file(root.join(POLICY)).unwrap();
    git(root, &["add", "--", GATES]);
    git(root, &["commit", "-qm", "gates"]);
    let legacy = format!("{TWO}{ONE}")
        .replace("**Authority:** operator-ruled", "")
        .replace("**Authority:** proposed", "");
    put(root, "DECISIONS.md", &legacy);
    let out = init(false);
    check(
        "existing history needs -PreserveLegacy",
        out.0 != 0
            && out.1.contains("explicitly use -PreserveLegacy")
            && !root.join(POLICY).exists(),
        out,
    );
    let out = init(true);
    check(
        "uncommitted history cannot be preserved",
        out.0 != 0
            && out.1.contains("Commit existing history unchanged")
            && !root.join(POLICY).exists(),
        out,
    );
    git(root, &["add", "--", "DECISIONS.md"]);
    git(root, &["commit", "-qm", "legacy"]);
    put(root, "DECISIONS.md", legacy.replace("useful", "dirty"));
    let out = init(true);
    check(
        "dirty DECISIONS.md refused",
        out.0 != 0
            && out.1.contains("Commit existing history unchanged")
            && !root.join(POLICY).exists(),
        out,
    );
    git(root, &["checkout", "-q", "--", "DECISIONS.md"]);
    let out = init(true);
    check(
        "legacy history preserved from max+1 without ratification",
        out.0 == 0
            && out.1.contains("AUTHORITY_INITIALIZED from=D3")
            && !root.join(LEDGER).exists()
            && compiled(root, RULINGS)["total"] == 0,
        out,
    );
    authority_ok(
        &invoke(root, CHECK, &[]),
        "canonical log honours the initialized boundary",
        Some(0),
        &["authority_checked=0"],
    );
    assert_eq!(
        invoke(
            root,
            "state-lock",
            &["-Release", "-Model", "fixture-model", "-Token", token]
        )
        .0,
        0
    );
}
