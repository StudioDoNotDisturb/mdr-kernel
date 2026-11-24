use super::*;
use serde_json::{json, Value};

fn md(title: &str, dir: &str, next: &str, related: &str) -> String {
    format!("Title: {title}\nType: Test Fixture\nContext: Use when testing node-map retrieval coordinates.\n\n# {title}\n\n---\n\n## SYSTEM NODE LINKS\n\nDirectory Path: {dir}\nNext Sequential File: {next}\nRelated Workflows: {related}\nAI Execution Prompts: none\n")
}

fn load(root: &Path) -> Value {
    serde_json::from_slice(&fs::read(root.join("maps/node-map.json")).unwrap()).unwrap()
}

fn node<'a>(map: &'a Value, path: &str) -> &'a Value {
    map["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["relPath"] == path)
        .unwrap()
}

#[test]
fn node_map_retrieval() {
    let sandbox = Sandbox::new();
    let root = &sandbox.clone_template("clone");
    let setup = run(
        root,
        &["setup-private", "-Model", "test-model", "-LocalOnly"],
    );
    assert_eq!(setup.0, 0, "{setup:?}");
    let target = "specs/cp3-target.md";
    let source = "docs/cp3-source.md";
    let relative = format!("../{target}");
    put(root, target, md("CP3 Target", "specs/", "none", "none"));
    let original = md("CP3 Source", "docs/", &relative, &relative);
    put(root, source, &original);
    let out = run(
        root,
        &[
            "close-session",
            "-Model",
            "test-model",
            "-NoHandoff",
            "-Also",
            &format!("{source},{target}"),
            "-LeastFile",
            "fixture files for this node-map close test",
        ],
    );
    authority_ok(
        &out,
        "close stages new Markdown before node-map generation",
        Some(0),
        &[],
    );
    let map = load(root);
    let source_node = node(&map, source);
    check(
        "new Markdown is recorded as tracked retrieval truth",
        source_node["tracked"] == true,
        source_node,
    );
    check(
        "source-relative link stores its canonical root coordinate",
        source_node["resolvedLinks"]["nextSequentialFile"] == target,
        source_node,
    );
    let bridge = fs::read_to_string(root.join("maps/OBSIDIAN-NODE-LINKS.md")).unwrap();
    check(
        "exported bridge links stay inside the repository",
        bridge.contains(&format!("[{target}](../{target})"))
            && !bridge.contains(&format!("[{target}](../../{target})")),
        source_node,
    );
    let skill = node(&map, "_templates/skill-template.md");
    check(
        "intentional skill-template placeholder is explicitly waived",
        skill["issues"] == json!([])
            && skill["waivedIssues"]
                .as_array()
                .unwrap()
                .contains(&json!("broken-link:skills/<skill-name>/")),
        skill,
    );
    let markdown = node(&map, "_templates/markdown-file-template.md");
    check(
        "instructional node-field placeholders are explicitly waived",
        markdown["issues"] == json!([]) && markdown["waivedIssues"].as_array().unwrap().len() == 4,
        markdown,
    );
    let map_run = |args: &[&str]| {
        let mut argv = vec!["update-node-map"];
        argv.extend_from_slice(args);
        run(root, &argv)
    };
    authority_ok(
        &map_run(&["-Check"]),
        "generated maps pass -Check right after close",
        Some(0),
        &["node-map check passed"],
    );
    put(
        root,
        source,
        original.replace(
            "Use when testing node-map retrieval coordinates.",
            "Use when changing the context to trigger drift.",
        ),
    );
    authority_ok(
        &run(root, &["doctor"]),
        "doctor detects exact node-map parity drift",
        Some(0),
        &["node map differs from tracked Markdown", "update-node-map"],
    );
    put(root, "inbox/write-approval-audit.jsonl", "{}\n");
    let audit = run(root, &["doctor"]);
    let count = regex::Regex::new(r"inbox/:\s+\d+ note").unwrap();
    check(
        "durable write-approval audit log is not an inbox note",
        audit.0 == 0 && !count.is_match(&audit.1),
        audit,
    );

    let make_node = |path, next| put(root, path, md("Fixture", "docs/", next, "../STATE.md"));
    let has_issue = regex::Regex::new(r"New node-map issues:[^\r\n]*broken-link:gone\.md").unwrap();
    make_node("docs/set-fresh.md", "gone.md");
    let out = map_run(&["-Check"]);
    check(
        "check reports a new issue when the committed map has none",
        has_issue.is_match(&out.1) && !out.1.contains("null-valued"),
        out,
    );
    fs::remove_file(root.join("docs/set-fresh.md")).unwrap();
    make_node("docs/set-one.md", "gone.md.bak");
    assert_eq!(map_run(&[]).0, 0);
    make_node("docs/set-one.md", "gone.md");
    let out = map_run(&["-Check"]);
    check(
        "a new issue that is a substring of the one known issue is still reported",
        has_issue.is_match(&out.1)
            && !out.1.lines().any(|line| {
                line.starts_with("New node-map issues:") && line.contains("broken-link:gone.md.bak")
            }),
        out,
    );
    fs::remove_file(root.join("docs/set-one.md")).unwrap();
    make_node("docs/café-note.md", "../STATE.md");
    make_node("docs/set-link.md", "café-note.md");
    git(
        root,
        &["add", "--", "docs/café-note.md", "docs/set-link.md"],
    );
    assert_eq!(map_run(&[]).0, 0);
    let map = load(root);
    let linker = node(&map, "docs/set-link.md");
    check(
        "a tracked non-ASCII file name is tracked and links to it resolve",
        node(&map, "docs/café-note.md")["tracked"] == true
            && !linker["issues"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item.as_str().unwrap().starts_with("broken-link")),
        linker,
    );
    git(
        root,
        &[
            "rm",
            "-q",
            "--cached",
            "--",
            "docs/café-note.md",
            "docs/set-link.md",
        ],
    );
    for file in ["docs/café-note.md", "docs/set-link.md"] {
        fs::remove_file(root.join(file)).unwrap();
    }

    let second = sandbox.repo("second-root");
    put(&second, "README.md", md("Fixture", "./", "none", "none"));
    let roots_path = "maps/node-roots.json";
    let mut roots: Value =
        serde_json::from_slice(&fs::read(root.join(roots_path)).unwrap()).unwrap();
    roots.as_array_mut().unwrap().push(json!({
        "name": "second", "path": second, "writePolicy": "read-only", "trackGit":true,
        "excludes": [], "waivedLegacy": [], "waivedIssues": []
    }));
    put(root, roots_path, serde_json::to_vec(&roots).unwrap());
    let out = map_run(&[]);
    let second = load(root)["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["root"] == "second")
        .cloned();
    check(
        "a git root with Markdown but nothing tracked generates without crashing",
        out.0 == 0
            && !out.1.contains("null-valued")
            && second.is_some_and(|file| file["tracked"] == false),
        out,
    );
}
