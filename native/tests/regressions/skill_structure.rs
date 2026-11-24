use super::*;
use serde_json::Value;

#[test]
fn skill_structure() {
    for path in ["skills/mdr-kernel/SKILL.md", "_templates/skill-template.md"] {
        let bytes = fs::read(template().join(path)).unwrap();
        check(
            &format!("{path} has no BOM"),
            !bytes.starts_with(b"\xef\xbb\xbf"),
            path,
        );
        let text = std::str::from_utf8(&bytes).unwrap().replace("\r\n", "\n");
        let body = text.strip_prefix("---\n");
        check(
            &format!("{path} frontmatter opens at byte one"),
            body.is_some(),
            path,
        );
        let end = body.and_then(|text| text.split_once("\n---\n"));
        check(
            &format!("{path} frontmatter block closes"),
            end.is_some(),
            path,
        );
        let (body, _) = end.unwrap();
        for key in [
            "Title",
            "Type",
            "Context",
            "name",
            "description",
            "compatibility",
        ] {
            check(
                &format!("{path} frontmatter has '{key}'"),
                body.lines()
                    .any(|line| line.starts_with(&format!("{key}:"))),
                path,
            );
        }
    }

    let path = template().join("skills/mdr-kernel/evals/trigger-queries.json");
    check("trigger-queries.json exists", path.is_file(), &path);
    let evals: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    for section in [
        "should_trigger",
        "should_not_trigger",
        "output_quality_assertions",
    ] {
        let entries = evals[section].as_array();
        check(
            &format!("trigger-queries.json has '{section}'"),
            entries.is_some(),
            section,
        );
        if section == "output_quality_assertions" {
            continue;
        }
        let entries = entries.unwrap();
        check(
            &format!("trigger-queries.json '{section}' is non-empty"),
            !entries.is_empty(),
            section,
        );
        for (index, entry) in entries.iter().enumerate() {
            check(
                &format!(
                    "trigger-queries.json '{section}' entry {} has query+why",
                    index + 1
                ),
                ["query", "why"]
                    .iter()
                    .all(|key| entry[key].as_str().is_some_and(|value| !value.is_empty())),
                entry,
            );
        }
    }
}
