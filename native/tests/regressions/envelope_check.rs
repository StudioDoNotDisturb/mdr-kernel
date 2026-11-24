use super::*;

fn doc(title: &str, context: &str) -> String {
    format!("Title: {title}\nType: Test\nContext: {context}\n\n# T\n\nbody\n")
}

#[test]
fn envelope_check() {
    let sandbox = Sandbox::new();
    let good = "Use when checking the envelope checker against its own fixtures.";
    let cases = [
        ("compliant header passes", doc("A short name", good), 0),
        ("title at 100 chars passes", doc(&"x".repeat(100), good), 0),
        ("title at 101 chars fails", doc(&"x".repeat(101), good), 1),
        ("context at 300 chars passes", doc("T", &format!("Use {}.", "x".repeat(295))), 0),
        ("context at 301 chars fails", doc("T", &format!("Use {}.", "x".repeat(296))), 1),
        ("two short sentences fail", doc("T", "Use when testing. Written 2026-09-25."), 1),
        ("abbreviations and initials do not split a sentence", doc("T", "Use when comparing e.g. No. 0412 vs. the U.S. plan; see p. 4 of Dr. Smith's note."), 0),
        ("decimals do not split a sentence", doc("T", "Version 2.5 of the file, at 3.14 scale."), 0),
        ("missing envelope is not this check's signal", "# No envelope\n\nbody\n".into(), 0),
        ("a wrapped Context is joined and checked", "Title: T\nType: Test\nContext: Use when testing a Context that\nwraps onto a second line. And a third sentence.\n\n# T\n".into(), 1),
        ("text after the blank line is body, not Context", "Title: T\nType: Test\nContext: Use when testing.\n\nBody text. More text.\n".into(), 0),
        ("YAML front matter is checked", format!("---\nTitle: \"{}\"\nType: Skill\nContext: One. Two.\nname: x\n---\n\n# S\n", "y".repeat(120)), 1),
        ("compliant YAML front matter passes", "---\nTitle: 'Short'\nType: Skill\nContext: 'Use when testing.'\n---\n".into(), 0),
    ];
    for (name, text, expected) in cases {
        put(&sandbox.0, "case.md", text);
        let actual = run(&sandbox.0, &["envelope", "check", "case.md"]);
        check(name, actual.0 == expected, actual);
    }

    let repo = sandbox.repo("repo");
    put(&repo, "ok.md", doc("Short", good));
    git(&repo, &["add", "ok.md"]);
    let out = run(&repo, &["envelope", "staged"]);
    check("staged: compliant file passes", out.0 == 0, out);
    put(
        &repo,
        "bad.md",
        doc(&"z".repeat(150), "Written today. Nothing here is doctrine."),
    );
    git(&repo, &["add", "bad.md"]);
    let out = run(&repo, &["envelope", "staged"]);
    check("staged: overlong header blocks the commit", out.0 == 1, out);
    let out = run(&repo, &["envelope", "count"]);
    check(
        "count reports tracked files over the limits",
        out.0 == 0 && out.1.contains("ENVELOPE_LENGTH over=1 tracked=2"),
        out,
    );
    put(&repo, "bad.md", doc("Short", good));
    let out = run(&repo, &["envelope", "staged"]);
    check(
        "staged: an unstaged fix cannot mask the staged blob",
        out.0 == 1,
        out,
    );
}
