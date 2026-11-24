//! Decision history validation and deterministic compilation of live operator rulings.

use crate::{args::Args, git};
use regex::Regex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

const LOG: &str = "DECISIONS.md";
const POLICY: &str = "protocol/decision-authority.json";
const LEDGER: &str = "protocol/decision-ratifications.jsonl";
const GATES: &str = "protocol/recall-gates.json";
const OUTPUT: &str = "protocol/rulings.json";
const VALIDATION_FAILED: &str = "Decision validation failed; no rulings compiled.\n";
const SURFACES: [&str; 6] = [
    "terminal",
    "chat",
    "operator terminal",
    "in person",
    "email",
    "phone",
];
const FIELDS: [(&str, &str); 3] = [
    ("Supersedes", "superseded"),
    ("Supersedes in part", "superseded in part"),
    ("Retires", "retired"),
];

static HEADING: LazyLock<Regex> = LazyLock::new(|| regex(r"(?m)^## D(\d+)\b[^\r\n]*"));
static STAMP: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r"(?m)^_Status: (superseded|superseded in part|retired) by D(\d+) \((\d{4}-\d{2}-\d{2})\)_\r?$",
    )
});
static STAMP_LINE: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r"(?m)^_Status: (?:superseded|superseded in part|retired) by D\d+ \(\d{4}-\d{2}-\d{2}\)_\r?(?:\n|\z)",
    )
});
static ID: LazyLock<Regex> = LazyLock::new(|| regex(r"(?i)^D(\d+)$"));
static AUTHORITY: LazyLock<Regex> = LazyLock::new(|| field("Authority"));
static TIMESTAMP: LazyLock<Regex> = LazyLock::new(|| {
    regex(
        r"(?i)^\s*(\d{4})[-/](\d{1,2})[-/](\d{1,2})(?:[T ](\d{1,2}):(\d{2})(?::(\d{2})(?:\.\d+)?)?)?\s*(?:Z|([+-])(\d{1,2}):?(\d{2}))?\s*$",
    )
});

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid decision regex")
}

fn field(name: &str) -> Regex {
    regex(&format!(
        r"(?m)^\*\*{}:\*\*[ \t]*([^\r\n]*?)[ \t]*\r?$",
        regex::escape(name)
    ))
}

fn value(v: &Value, key: &str) -> String {
    property(v, key)
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| {
            if property(v, key).is_null() {
                String::new()
            } else {
                property(v, key).to_string()
            }
        })
}

fn property<'a>(v: &'a Value, key: &str) -> &'a Value {
    v.as_object()
        .and_then(|object| object.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)))
        .map_or(&Value::Null, |(_, value)| value)
}

fn sequence(v: &Value) -> impl Iterator<Item = &Value> {
    v.as_array()
        .map_or_else(|| std::slice::from_ref(v), Vec::as_slice)
        .iter()
}

#[derive(Default)]
pub struct Report {
    pub code: i32,
    pub out: String,
    pub err: String,
}

impl Report {
    fn line(&mut self, line: &str) {
        self.out.push_str(line);
        self.out.push('\n');
    }

    fn fail(mut self, status: &str, errors: impl IntoIterator<Item = String>) -> Self {
        self.code = 1;
        self.line(status);
        for error in errors {
            self.line(&format!("ERROR={error}"));
        }
        self
    }

    fn exception(mut self, error: &str) -> Self {
        self.code = 1;
        self.err = format!("{error}\n");
        self
    }

    pub fn emit(&self) {
        if cfg!(windows) {
            print!("{}", self.out.replace('\n', "\r\n"));
            eprint!("{}", self.err.replace('\n', "\r\n"));
        } else {
            print!("{}", self.out);
            eprint!("{}", self.err);
        }
    }
}

#[derive(Default)]
struct History<'a> {
    path: Option<&'a str>,
    from_index: bool,
    baseline: Option<&'a str>,
    authority_from: Option<i32>,
    ratifications: Option<&'a str>,
    ratifications_baseline: Option<&'a str>,
    hash_of: Option<&'a str>,
}

#[derive(Default)]
struct Build<'a> {
    check: bool,
    from_index: bool,
    decisions: Option<&'a str>,
    ratifications: Option<&'a str>,
    gates: Option<&'a str>,
    out: Option<&'a str>,
}

fn canonical(path: &Path, root: &Path) -> bool {
    crate::args::absolute(path)
        .to_string_lossy()
        .eq_ignore_ascii_case(&crate::args::absolute(&root.join(LOG)).to_string_lossy())
}

fn file_text(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    // File.ReadAllText autodetects a BOM on working-tree files.
    for (bom, little) in [([0xff, 0xfe], true), ([0xfe, 0xff], false)] {
        if let Some(body) = bytes.strip_prefix(&bom) {
            let (words, tail) = body.as_chunks::<2>();
            let mut result = String::from_utf16_lossy(
                &words
                    .iter()
                    .map(|&word| {
                        if little {
                            u16::from_le_bytes(word)
                        } else {
                            u16::from_be_bytes(word)
                        }
                    })
                    .collect::<Vec<_>>(),
            );
            if !tail.is_empty() {
                result.push(char::REPLACEMENT_CHARACTER);
            }
            return Some(result);
        }
    }
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
    Some(String::from_utf8_lossy(bytes).into_owned())
}

fn blob(root: &Path, spec: &str, strict: bool) -> Result<Option<String>, String> {
    let Ok(bytes) = git::git(root, &["cat-file", "blob", spec]) else {
        return Ok(None);
    };
    let text = if strict {
        String::from_utf8(bytes).map_err(|_| format!("{spec} is not valid UTF-8."))?
    } else {
        git::lossy(bytes)
    };
    Ok(Some(text.trim_start_matches('\u{feff}').to_owned()))
}

fn normalized(text: &str) -> String {
    text.replace("\r\n", "\n")
}

fn digest(text: &str) -> String {
    let normalized = normalized(text);
    let digest = Sha256::digest(normalized.trim_end().as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn entry_hash(segment: &str) -> String {
    digest(&STAMP_LINE.replace_all(segment, ""))
}

fn segments(raw: &str) -> Result<BTreeMap<i32, &str>, String> {
    let matches: Vec<_> = HEADING.captures_iter(raw).collect();
    let mut entries = BTreeMap::new();
    if let Some(last) = matches.last() {
        let start = last.get(0).unwrap().start();
        let template = raw[start..]
            .find("<!-- Template for a new entry")
            .map(|i| start + i);
        let footer = regex(r"(?m)^---[ \t]*\r?\n(?:[ \t]*\r?\n)*## SYSTEM NODE LINKS")
            .find_at(raw, start)
            .map(|m| m.start());
        let tail = template
            .into_iter()
            .chain(footer)
            .min()
            .unwrap_or(raw.len());
        for (i, heading) in matches.iter().enumerate() {
            let id = number(&heading[1])?;
            let end = matches
                .get(i + 1)
                .map_or(tail, |h| h.get(0).unwrap().start());
            entries
                .entry(id)
                .or_insert(&raw[heading.get(0).unwrap().start()..end]);
        }
    }
    Ok(entries)
}

fn number(text: &str) -> Result<i32, String> {
    text.parse()
        .map_err(|_| format!("Cannot convert value \"{text}\" to type \"System.Int32\"."))
}

fn structure(raw: &str, errors: &mut Vec<String>) -> Result<usize, String> {
    let heads: Vec<_> = HEADING.captures_iter(raw).collect();
    if heads.is_empty() {
        if !raw.to_lowercase().contains("_no decisions yet._")
            || regex(r"(?im)^\*\*Decision").is_match(raw)
        {
            errors.push("Empty decision logs must explicitly say _No decisions yet._ and contain no decision bodies.".into());
        }
        return Ok(0);
    }
    let mut ids = Vec::new();
    let mut seen = BTreeSet::new();
    for heading in &heads {
        let id = number(&heading[1])?;
        ids.push(id);
        if !seen.insert(id) {
            errors.push(format!("Duplicate decision id D{id}."));
        }
    }
    for pair in ids.windows(2) {
        if pair[0] <= pair[1] {
            errors.push(format!(
                "Decision order is not strictly descending: D{} precedes D{}.",
                pair[0], pair[1]
            ));
        }
    }
    for (old, new) in seen.iter().zip(seen.iter().skip(1)).rev() {
        for missing in (old + 1..*new).rev() {
            errors.push(format!("Missing decision id D{missing}."));
        }
    }
    let min = *seen.first().unwrap();
    if min != 1 {
        errors.push(format!("Decision history ends at D{min} instead of D1."));
    }
    let body_start = regex(r"(?m)^\*\*Decision(?: \([^\r\n]+\))?:\*\*");
    if regex(r"(?im)^\*\*Decision(?: \([^\r\n]+\))?:\*\*")
        .is_match(&raw[..heads[0].get(0).unwrap().start()])
    {
        errors.push("Anonymous decision body appears before the first decision heading.".into());
    }
    let template = regex(r"(?s)<!--\s*Template for a new entry.*?-->");
    let separator = regex(r"(?m)^---\s*$");
    for (i, heading) in heads.iter().enumerate() {
        let start = heading.get(0).unwrap().end();
        let end = heads
            .get(i + 1)
            .map_or(raw.len(), |h| h.get(0).unwrap().start());
        let body = &raw[start..end];
        let body = if i + 1 == heads.len() {
            template.replace_all(body, "")
        } else {
            body.into()
        };
        let id = ids[i];
        if separator.replace_all(&body, "").trim().is_empty() {
            errors.push(format!("D{id} has no body."));
            continue;
        }
        match body_start.find_iter(&body).count() {
            0 => errors.push(format!("D{id} needs one **Decision:** body.")),
            1 => (),
            count => errors.push(format!(
                "D{id} contains {count} decision bodies; at least one body is anonymous."
            )),
        }
    }
    Ok(heads.len())
}

fn authority_boundary(
    root: &Path,
    opts: &History<'_>,
    errors: &mut Vec<String>,
) -> Result<i32, String> {
    let default = opts.authority_from.unwrap_or(1);
    let raw = if opts.from_index {
        blob(root, &format!(":{POLICY}"), true)?
    } else {
        file_text(&root.join(POLICY))
    };
    let old = if opts.from_index {
        blob(root, &format!("HEAD:{POLICY}"), true)?
    } else {
        None
    };
    let Some(raw) = raw.filter(|s| !s.is_empty()) else {
        if old.is_some_and(|s| !s.is_empty()) {
            errors.push("The committed decision authority boundary cannot be deleted.".into());
        }
        return Ok(default);
    };
    let from = serde_json::from_str::<Value>(&raw).ok().and_then(|policy| {
        let version = property(&policy, "schema_version");
        let from = property(&policy, "authority_from").as_i64()?;
        (version.as_f64() == Some(1.0) || version.as_str() == Some("1") || version == true)
            .then_some(from)
            .and_then(|n| i32::try_from(n).ok())
            .filter(|&n| n >= 1)
    });
    let Some(from) = from else {
        errors.push("Invalid protocol/decision-authority.json; expected schema_version 1 and positive integer authority_from.".into());
        return Ok(default);
    };
    if opts.from_index {
        if let Some(old) = old.filter(|s| !s.is_empty()) {
            if normalized(&old).trim_end() != normalized(&raw).trim_end() {
                errors.push("The committed decision authority boundary is immutable.".into());
            }
        } else {
            let old_log = blob(root, "HEAD:DECISIONS.md", true)?.unwrap_or_default();
            let mut max = 0;
            for heading in HEADING.captures_iter(&old_log) {
                max = max.max(number(&heading[1])?);
            }
            if from > max.saturating_add(1) {
                errors.push(
                    "Migration can exempt only decision entries already committed in HEAD.".into(),
                );
            }
        }
    }
    Ok(from)
}

fn records(text: &str) -> Vec<&str> {
    text.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .filter(|line| !line.trim().is_empty())
        .collect()
}

fn valid_timestamp(text: &str) -> bool {
    let Some(parts) = TIMESTAMP.captures(text) else {
        return false;
    };
    let part = |n| {
        parts
            .get(n)
            .and_then(|m| m.as_str().parse::<u32>().ok())
            .unwrap_or(0)
    };
    let (year, month, day) = (part(1), part(2), part(3));
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => 0,
    };
    year > 0
        && (1..=days).contains(&day)
        && part(4) < 24
        && part(5) < 60
        && part(6) < 60
        && part(8) * 60 + part(9) <= 14 * 60
        && part(9) < 60
}

fn ratifications(
    current: &str,
    baseline: Option<&str>,
    entries: &BTreeMap<i32, &str>,
    errors: &mut Vec<String>,
) -> Result<(HashSet<i32>, usize), String> {
    let lines = records(current);
    if baseline.is_some_and(|old| !lines.starts_with(&records(old))) {
        errors.push(format!(
            "{LEDGER} is append-only: an existing ratification record was changed or removed."
        ));
    }
    let hash = regex(r"(?i)^[0-9a-f]{64}$");
    let mut ratified = HashSet::new();
    let mut count = 0;
    for (i, line) in lines.into_iter().enumerate() {
        let label = format!("{LEDGER} line {}", i + 1);
        let Ok(rec) = serde_json::from_str::<Value>(line) else {
            errors.push(format!("{label}: not valid JSON."));
            continue;
        };
        let rid = value(&rec, "id");
        let id = ID.captures(&rid);
        let sha = value(&rec, "entry_sha256");
        let surface = value(&rec, "surface");
        let mut problems = Vec::new();
        if id.is_none() {
            problems.push("id must look like D123".into());
        }
        if !hash.is_match(&sha) {
            problems.push("entry_sha256 must be 64 lowercase hex characters".into());
        }
        if !valid_timestamp(&value(&rec, "ratified_at")) {
            problems.push("ratified_at must be an ISO timestamp".into());
        }
        if !SURFACES.iter().any(|s| s.eq_ignore_ascii_case(&surface)) {
            problems.push(format!("surface must be one of: {}", SURFACES.join(", ")));
        }
        if !property(&rec, "quote").is_string() {
            problems.push("quote must be a string (empty is allowed)".into());
        }
        if !problems.is_empty() {
            errors.push(format!("{label}: {}.", problems.join("; ")));
            continue;
        }
        let id = number(&id.unwrap()[1])?;
        let Some(segment) = entries.get(&id) else {
            errors.push(format!("{label}: ratifies {rid}, which does not exist."));
            continue;
        };
        if !entry_hash(segment).eq_ignore_ascii_case(&sha) {
            errors.push(format!("{label}: {rid} no longer matches the hash it was ratified at - the entry changed after ratification."));
            continue;
        }
        ratified.insert(id);
        count += 1;
    }
    Ok((ratified, count))
}

fn authority(
    entries: &BTreeMap<i32, &str>,
    from: i32,
    ratified: &HashSet<i32>,
    errors: &mut Vec<String>,
) -> Result<(usize, usize), String> {
    let quote = regex(
        r#"(?im)^\*\*Operator said \((?:chat|terminal|operator terminal|in person|email|phone)\b[^)\r\n]*?\d{4}-\d{2}-\d{2}[^)\r\n]*\):\*\* ["“]([^\r\n]*)"#,
    );
    // The legacy regex counts UTF-16 code units in the quoted text.
    let has_quote = |text: &str| {
        quote.captures_iter(text).any(|line| {
            line[1]
                .encode_utf16()
                .skip(12)
                .any(|unit| matches!(unit, 0x22 | 0x201d))
        })
    };
    let date = regex(r"^## D\d+ [-—] (\d{4}-\d{2}-\d{2})");
    let list = regex(r"(?i)^D\d+(?:\s*,\s*D\d+)*$");
    let proposed_line = regex(r"(?im)^\*\*Authority:\*\*[ \t]*proposed[ \t]*\r?$");
    let fields = FIELDS.map(|(name, kind)| (name, kind, field(name)));
    let mut claims = BTreeSet::new();
    let mut checked = 0;
    for (&id, text) in entries.range(from..).rev() {
        checked += 1;
        let lines: Vec<_> = AUTHORITY.captures_iter(text).collect();
        if lines.len() != 1 {
            errors.push(format!("D{id} needs exactly one **Authority:** line (operator-ruled or proposed); found {}.", lines.len()));
            continue;
        }
        let class = &lines[0][1];
        match class.to_ascii_lowercase().as_str() {
            "operator-ruled" if !has_quote(text) => errors.push(format!("D{id} is operator-ruled but has no **Operator said (<surface>, YYYY-MM-DD):** \"<quote of 12+ characters>\" line.")),
            "operator-ruled" | "proposed" => (),
            "operator-ratified" => errors.push(format!("D{id} types operator-ratified; that class is set only by the operator's ratification record, never typed.")),
            "record" => errors.push(format!("D{id} is a record; a work report belongs in a handoff, not the decision log.")),
            _ => errors.push(format!("D{id} has unknown authority '{class}'; use operator-ruled or proposed.")),
        }
        for (name, kind, field) in &fields {
            let mut lines = field.captures_iter(text);
            let Some(line) = lines.next() else { continue };
            if lines.next().is_some() {
                errors.push(format!("D{id} has more than one **{name}:** line."));
                continue;
            }
            if !list.is_match(&line[1]) {
                errors.push(format!(
                    "D{id} **{name}:** must be a comma-separated list of ids like D12, D34."
                ));
                continue;
            }
            for target in line[1]
                .split(',')
                .map(str::trim)
                .filter(|s| s.starts_with('D'))
            {
                let target = number(&target[1..])?;
                let Some(older) = entries.get(&target).filter(|_| target < id) else {
                    errors.push(format!("D{id} lists D{target} under **{name}:**, which is not an older existing entry."));
                    continue;
                };
                claims.insert((target, *kind, id));
                if class.eq_ignore_ascii_case("proposed") {
                    if *kind == "retired" {
                        errors.push(format!("D{id} is proposed and cannot retire D{target}; only an operator ruling retires."));
                    } else if ratified.contains(&target)
                        || target < from
                        || !proposed_line.is_match(older)
                    {
                        errors.push(format!("D{id} is proposed and cannot supersede D{target}, which is not an unratified proposal; a ruling yields only to a ruling."));
                    }
                }
            }
        }
    }
    let mut stamps = 0;
    let mut stamped = BTreeSet::new();
    for (&id, text) in entries.iter().rev() {
        for stamp in STAMP.captures_iter(text) {
            stamps += 1;
            let kind = stamp.get(1).unwrap().as_str();
            let by = number(&stamp[2])?;
            if !claims.contains(&(id, kind, by)) {
                errors.push(format!("D{id} carries '_Status: {kind} by D{by}_' but D{by} does not list D{id} under the matching field."));
            } else if let Some(by_date) = date.captures(entries[&by]) {
                if stamp[3] != by_date[1] {
                    errors.push(format!(
                        "D{id} stamp '{kind} by D{by}' is dated {}, not D{by}'s date {}.",
                        &stamp[3], &by_date[1]
                    ));
                }
            }
            if stamp[2] == by.to_string() {
                stamped.insert((id, kind, by));
            }
        }
    }
    for (target, kind, by) in claims.difference(&stamped) {
        errors.push(format!("D{by} lists D{target} as {kind} but D{target} has no '_Status: {kind} by D{by} (date)_' stamp; append with mdr-kernel add-decision."));
    }
    Ok((checked, stamps))
}

fn append_only(
    before: &str,
    after: &BTreeMap<i32, &str>,
    label: &str,
    errors: &mut Vec<String>,
) -> Result<(), String> {
    for (id, old) in segments(before)?.into_iter().rev() {
        let Some(&new) = after.get(&id) else {
            errors.push(format!(
                "D{id} exists in {label} but was removed; existing entries are append-only."
            ));
            continue;
        };
        if new == old {
            continue;
        }
        let mut stamps: Vec<_> = STAMP_LINE
            .find_iter(new)
            .map(|m| m.as_str().trim_end_matches(['\r', '\n']))
            .collect();
        let stamps_kept = STAMP_LINE.find_iter(old).all(|stamp| {
            let text = stamp.as_str().trim_end_matches(['\r', '\n']);
            if let Some(at) = stamps.iter().position(|&s| s == text) {
                stamps.remove(at);
                true
            } else {
                false
            }
        });
        let old = STAMP_LINE.replace_all(old, "");
        let new = STAMP_LINE.replace_all(new, "");
        if !stamps_kept || old != new {
            let first = old
                .split('\n')
                .zip(new.split('\n'))
                .take_while(|(a, b)| a == b)
                .count()
                + 1;
            errors.push(format!("D{id} differs from {label} (first difference at entry line {first}); existing entries are append-only - record a correction as a new entry."));
        }
    }
    Ok(())
}

fn check_history(root: &Path, opts: &History<'_>) -> Report {
    history(root, opts).unwrap_or_else(|error| Report::default().exception(&error))
}

fn history(root: &Path, opts: &History<'_>) -> Result<Report, String> {
    let report = Report::default();
    let path = opts.path.map_or_else(|| root.join(LOG), PathBuf::from);
    let canonical = canonical(&path, root);
    let mut errors = Vec::new();
    let from = if opts.from_index || canonical {
        authority_boundary(root, opts, &mut errors)?
    } else {
        opts.authority_from.unwrap_or(1)
    };
    let raw = if opts.from_index {
        blob(root, ":DECISIONS.md", true)?
            .ok_or_else(|| "DECISIONS.md is absent from the Git index.".into())
    } else {
        file_text(&path).ok_or_else(|| format!("Decision log not found: {}", path.display()))
    };
    let raw = match raw {
        Ok(text) => text,
        Err(e) => return Ok(report.fail("DECISION_HISTORY_STATUS=FAIL", [e])),
    };
    let label = if opts.baseline.is_some() {
        "baseline"
    } else {
        "HEAD"
    };
    let baseline = match opts.baseline {
        Some(path) => match file_text(Path::new(path)) {
            Some(text) => Some(text),
            None => {
                return Ok(report.fail(
                    "DECISION_HISTORY_STATUS=FAIL",
                    [format!("Baseline decision log not found: {path}")],
                ))
            }
        },
        None if opts.from_index => blob(root, "HEAD:DECISIONS.md", true)?,
        None => None,
    };
    let count = structure(&raw, &mut errors)?;
    let entries = segments(&raw)?;
    if let Some(wanted) = opts.hash_of {
        let id = wanted.strip_prefix(['D', 'd']).unwrap_or(wanted);
        let entry = id
            .parse::<i32>()
            .ok()
            .filter(|_| id.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|id| entries.get(&id).map(|text| (id, text)));
        return Ok(match entry {
            Some((id, text)) => Report {
                out: format!(
                    "ENTRY_HASH_STATUS=OK\nENTRY_ID=D{id}\nENTRY_HEADING={}\nENTRY_SHA256={}\n",
                    text.split('\n').next().unwrap_or("").trim_end_matches('\r'),
                    entry_hash(text)
                ),
                ..report
            },
            None => report.fail("ENTRY_HASH_STATUS=FAIL", [format!("No entry {wanted}.")]),
        });
    }
    let rat_text = match opts.ratifications {
        Some(path) => file_text(Path::new(path)),
        None if opts.from_index => blob(root, &format!(":{LEDGER}"), true)?,
        None if canonical => file_text(&root.join(LEDGER)),
        None => None,
    };
    let rat_base = match opts.ratifications_baseline {
        Some(path) => file_text(Path::new(path)),
        None if opts.from_index && opts.ratifications.is_none() => {
            blob(root, &format!("HEAD:{LEDGER}"), true)?
        }
        None => None,
    };
    let (ratified, rat_count) = ratifications(
        rat_text.as_deref().unwrap_or_default(),
        rat_base.as_deref(),
        &entries,
        &mut errors,
    )?;
    let (checked, stamps) = authority(&entries, from, &ratified, &mut errors)?;
    let mut append = "not-checked".to_owned();
    if let Some(before) = baseline {
        append_only(&before, &entries, label, &mut errors)?;
        append = format!("checked baseline={label}");
    }
    if !errors.is_empty() {
        return Ok(report.fail("DECISION_HISTORY_STATUS=FAIL", errors));
    }
    Ok(Report {
        out: format!("DECISION_HISTORY_STATUS=OK decisions={count} authority_from={from} append_only={append} authority_checked={checked} stamps={stamps} ratified={rat_count}\n"),
        ..report
    })
}

fn source(root: &Path, relative: &str, path: Option<&str>, indexed: bool) -> String {
    if indexed && path.is_none() {
        return blob(root, &format!(":{relative}"), false)
            .ok()
            .flatten()
            .unwrap_or_default();
    }
    file_text(&path.map_or_else(|| root.join(relative), PathBuf::from)).unwrap_or_default()
}

fn unique_gates(cites: &[String]) -> Vec<&str> {
    let mut result: Vec<_> = cites.iter().map(String::as_str).collect();
    // Sort-Object puts underscore before hyphen in the gate identifier alphabet.
    result.sort_by_cached_key(|name| name.to_ascii_lowercase().replace('_', "!"));
    result.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    result
}

fn cited_gates(raw: &str) -> Result<BTreeMap<i32, Vec<String>>, String> {
    let mut result: BTreeMap<i32, Vec<String>> = BTreeMap::new();
    if raw.is_empty() {
        return Ok(result);
    }
    let doc: Value =
        serde_json::from_str(raw).map_err(|e| format!("Invalid recall gates JSON: {e}"))?;
    for gate in sequence(property(&doc, "gates")) {
        let gate_id = value(gate, "id");
        for citation in sequence(property(gate, "rulings")) {
            if citation.is_null() || citation == false || citation == 0 || citation == "" {
                continue;
            }
            let id = citation
                .as_str()
                .and_then(|s| ID.captures(s))
                .map_or(Ok(0), |cap| number(&cap[1]))?;
            result.entry(id).or_default().push(gate_id.clone());
        }
    }
    Ok(result)
}

fn clip_rules_out(text: &str) -> String {
    let mut previous = ' ';
    let end = text
        .char_indices()
        .find_map(|(i, c)| {
            let ends_sentence = matches!(previous, '.' | ';') && c.is_whitespace();
            previous = c;
            ends_sentence.then_some(i)
        })
        .unwrap_or(text.len());
    let compact = text[..end].split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.encode_utf16().count() <= 200 {
        return compact;
    }
    let prefix = String::from_utf16_lossy(&compact.encode_utf16().take(197).collect::<Vec<_>>());
    format!("{}...", prefix.trim_end())
}

fn live_rulings(
    decisions: &str,
    ledger: &str,
    cites: &BTreeMap<i32, Vec<String>>,
) -> Result<BTreeMap<i32, Value>, String> {
    let heading = regex(r"(?m)^## D(\d+) [-—] \d{4}-\d{2}-\d{2} [-—] ([^\r\n]+)");
    let heads: Vec<_> = heading.captures_iter(decisions).collect();
    let ratified: HashSet<_> = records(ledger)
        .into_iter()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|record| {
            ID.captures(&value(&record, "id"))
                .and_then(|id| id[1].parse::<i32>().ok())
        })
        .collect();
    let authority = regex(r"(?m)^\*\*Authority:\*\*[ \t]*(\S+)");
    let quote = regex(r"(?im)^\*\*Operator said \(");
    let superseded = regex(r"(?im)^_Status: (?:superseded|retired) by D\d+ ");
    let partial = regex(r"(?im)^_Status: superseded in part by D\d+ ");
    let decision_field = regex(r"(?m)^\*\*Decision(?: \([^\r\n]+\))?:\*\*[ \t]*");
    let rule_end = regex(r"(?m)^(?:\*\*[A-Z][^\r\n]*?:\*\*|---|## SYSTEM NODE LINKS)");
    let rules_out = regex(r"(?ms)^\*\*(?:What it )?[Rr]ules out:\*\*\s*(.+?)(?:\r?\n\r?\n|\z)");
    let mut rulings = BTreeMap::new();
    for (i, head) in heads.iter().enumerate() {
        let start = head.get(0).unwrap().start();
        let end = heads
            .get(i + 1)
            .map_or(decisions.len(), |h| h.get(0).unwrap().start());
        let text = &decisions[start..end];
        let id = number(&head[1])?;
        if superseded.is_match(text) {
            continue;
        }
        let class = if ratified.contains(&id) {
            "operator-ratified"
        } else if authority
            .captures(text)
            .is_some_and(|a| a[1].eq_ignore_ascii_case("operator-ruled"))
            && quote.is_match(text)
        {
            "operator-ruled"
        } else {
            continue;
        };
        let title = head[2].trim();
        let mut rule = decision_field
            .find(text)
            .map_or(title, |matched| {
                let start = matched.end();
                let end = rule_end
                    .find_at(text, start)
                    .map_or(text.len(), |m| m.start());
                text[start..end].trim()
            })
            .to_owned();
        if partial.is_match(text) {
            rule.push_str(" (superseded in part; see the stamp)");
        }
        let rules_out = rules_out
            .captures(text)
            .map_or_else(String::new, |m| clip_rules_out(&m[1]));
        let gates = cites.get(&id).map_or_else(Vec::new, |g| unique_gates(g));
        // Assignment from a PowerShell pipeline unwraps zero or one matches.
        let gates = match gates.as_slice() {
            [] => Value::Null,
            [gate] => json!(gate),
            _ => json!(gates),
        };
        rulings.insert(
            id,
            json!({
                "id": format!("D{id}"), "class": class, "title": title,
                "rule": rule, "rules_out": rules_out, "gates": gates,
            }),
        );
    }
    Ok(rulings)
}

/// Compile after checking authority, or compare the expected bytes for -Check.
fn build(root: &Path, opts: &Build<'_>) -> Report {
    let mut report = check_history(
        root,
        &History {
            path: opts.decisions,
            from_index: opts.from_index,
            ratifications: opts.ratifications,
            ..History::default()
        },
    );
    if report.code != 0 {
        report.err.push_str(VALIDATION_FAILED);
        return report;
    }
    match compile(root, opts, &mut report) {
        Ok(()) => report,
        Err(error) => report.exception(&error),
    }
}

/// The pre-commit's two gates, judged on the index: decision history, then compiled rulings.
pub fn history_from_index(root: &Path) -> Report {
    check_history(
        root,
        &History {
            from_index: true,
            ..History::default()
        },
    )
}

pub fn rulings_check_from_index(root: &Path) -> Report {
    build(
        root,
        &Build {
            check: true,
            from_index: true,
            ..Build::default()
        },
    )
}

pub fn build_default(root: &Path, check: bool) -> Report {
    build(
        root,
        &Build {
            check,
            ..Build::default()
        },
    )
}

fn compile(root: &Path, opts: &Build<'_>, report: &mut Report) -> Result<(), String> {
    let decisions = source(root, LOG, opts.decisions, opts.from_index);
    let fixture = opts
        .decisions
        .is_some_and(|path| !canonical(Path::new(path), root));
    let ledger = if fixture && opts.ratifications.is_none() {
        String::new()
    } else {
        source(root, LEDGER, opts.ratifications, opts.from_index)
    };
    let gates = source(root, GATES, opts.gates, opts.from_index);
    if decisions.is_empty() {
        return Err("DECISIONS.md not found.".into());
    }
    let cites = cited_gates(&gates)?;
    let rulings = live_rulings(&decisions, &ledger, &cites)?;
    let bad: Vec<_> = cites
        .iter()
        .filter(|(id, _)| !rulings.contains_key(id))
        .map(|(id, gates)| format!("D{id} (cited by {})", unique_gates(gates).join(", ")))
        .collect();
    if !bad.is_empty() {
        report.code = 1;
        report.line("RULINGS_STATUS=FAIL");
        report.line(&format!("ERROR=recall gates cite ids that are not live rulings (retired, superseded, proposed or unknown): {}", bad.join("; ")));
        return Ok(());
    }
    let rulings: Vec<_> = rulings.into_values().rev().collect();
    let total = rulings.len();
    let ratified = rulings
        .iter()
        .filter(|r| r["class"] == "operator-ratified")
        .count();
    let ruled = total - ratified;
    let doc = json!({
        "description": "Generated by mdr-kernel build-rulings. Do not edit by hand. Unclassified legacy entries are not rulings.",
        "sources": { "decisions": digest(&decisions), "ratifications": digest(&ledger), "gates": digest(&gates) },
        "counts": { "operator-ratified": ratified, "operator-ruled": ruled },
        "total": total,
        "rulings": rulings,
    });
    // ConvertTo-Json escapes these additional line terminators. The script folds its
    // platform line endings to LF and appends one LF before writing UTF-8 without a BOM.
    let mut rendered = serde_json::to_string_pretty(&doc).expect("serializable rulings");
    rendered = rendered
        .replace('\u{0085}', "\\u0085")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    rendered.push('\n');
    let status = if opts.check {
        if normalized(&source(root, OUTPUT, opts.out, opts.from_index)) != rendered {
            report.code = 1;
            report.line("RULINGS_STATUS=STALE");
            report.line("ERROR=protocol/rulings.json does not match its sources; run: mdr-kernel build-rulings");
            return Ok(());
        }
        "OK"
    } else {
        let path = opts.out.map_or_else(|| root.join(OUTPUT), PathBuf::from);
        fs::write(&path, rendered)
            .map_err(|err| format!("Could not write {}: {err}", path.display()))?;
        "WRITTEN"
    };
    report.line(&format!(
        "RULINGS_STATUS={status} total={total} operator-ratified={ratified} operator-ruled={ruled}"
    ));
    Ok(())
}

pub fn cli(command: &str, argv: &[String]) -> i32 {
    let result = (|| -> Result<Report, String> {
        let build_command = command == "build-rulings";
        let (values, switches): (&[&str], &[&str]) = if build_command {
            (
                &[
                    "Root",
                    "DecisionsPath",
                    "RatificationsPath",
                    "GatesPath",
                    "OutPath",
                ],
                &["Check", "FromIndex", "ScriptHandoff"],
            )
        } else {
            (
                &[
                    "Root",
                    "Path",
                    "BaselinePath",
                    "AuthorityFrom",
                    "RatificationsPath",
                    "RatificationsBaselinePath",
                    "HashOf",
                ],
                &["FromIndex"],
            )
        };
        let args = Args::parse(argv, values, switches)?;
        let path = |name| args.value(name).filter(|s| !s.is_empty());
        let cwd = std::env::current_dir().map_err(|err| err.to_string())?;
        let root = path("Root").map_or_else(|| git::top(&cwd), |p| Ok(PathBuf::from(p)))?;
        let root = crate::args::absolute(&root);
        crate::hook::fresh_in_repo(&root)?;
        let from_index = args.switch("FromIndex");
        let ratifications = path("RatificationsPath");
        if build_command {
            let mut report = build(
                &root,
                &Build {
                    check: args.switch("Check"),
                    from_index,
                    decisions: path("DecisionsPath"),
                    ratifications,
                    gates: path("GatesPath"),
                    out: path("OutPath"),
                },
            );
            if args.switch("ScriptHandoff") {
                if let Some(err) = report.err.strip_suffix(VALIDATION_FAILED) {
                    report.err = err.to_owned();
                    report.code = 3;
                }
            }
            Ok(report)
        } else {
            let authority_from = args
                .value("AuthorityFrom")
                .map(|s| {
                    number(s.trim())
                        .ok()
                        .filter(|&n| n >= 1)
                        .ok_or("-AuthorityFrom must be in the range 1 to 2147483647.")
                })
                .transpose()?;
            Ok(check_history(
                &root,
                &History {
                    path: path("Path"),
                    from_index,
                    baseline: path("BaselinePath"),
                    authority_from,
                    ratifications,
                    ratifications_baseline: path("RatificationsBaselinePath"),
                    hash_of: path("HashOf"),
                },
            ))
        }
    })();
    let report = result.unwrap_or_else(|err| Report::default().exception(&err));
    report.emit();
    report.code
}

// ---------- writers: add-decision and initialize-decision-authority ----------

/// The script's `throw`: the message on stderr, exit 1.
fn thrown(message: &str) -> i32 {
    eprintln!("{message}");
    1
}

fn owner(root: &Path, model: &str, token: &str) -> Result<bool, String> {
    let mut argv = vec!["-CheckOwner", "-Model", model];
    if !token.is_empty() {
        argv.extend(["-Token", token]);
    }
    let report = crate::hook::lock_port(root, &argv)?;
    print!("{}", report.out);
    Ok(report.code == 0)
}

fn verify(root: &Path, path: &str, authority_from: i32) -> bool {
    let report = check_history(
        root,
        &History {
            path: Some(path),
            authority_from: Some(authority_from),
            ..History::default()
        },
    );
    report.emit();
    report.code == 0
}

fn utc_today() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let (y, m, d) = crate::state_lock::civil_from_days(seconds.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Prepend a newest-first entry, stamp supersession targets in the same write, verify the
/// whole result, and roll back on failure. Keeps the file's BOM and line-ending style.
fn add(root: &Path, args: &Args) -> Result<i32, String> {
    let title = args.value("Title").unwrap_or("");
    let body_path = args.value("BodyPath");
    if args.value("Title").is_none() || (args.value("Body").is_some() == body_path.is_some()) {
        return Err("Specify -Title and exactly one of -Body or -BodyPath.".into());
    }
    let canonical_log = root.join(LOG);
    let target = match args.value("Path").filter(|p| !p.is_empty()) {
        Some(p) => crate::args::absolute(Path::new(p)),
        None => crate::args::absolute(&canonical_log),
    };
    let is_canonical = target
        .to_string_lossy()
        .eq_ignore_ascii_case(&crate::args::absolute(&canonical_log).to_string_lossy());
    if title.trim().is_empty() || title.contains(['\r', '\n']) {
        return Ok(thrown("Title must be one non-empty line."));
    }
    let body = match body_path {
        Some(p) => file_text(Path::new(p))
            .ok_or_else(|| format!("Cannot find path {p} because it does not exist."))?,
        None => args.value("Body").unwrap_or("").to_string(),
    };
    if body.trim().is_empty() {
        return Ok(thrown("Decision body must not be empty."));
    }
    if !regex(r"(?im)^\*\*Decision(?: \([^\r\n]+\))?:\*\*").is_match(&body) {
        return Ok(thrown(
            "Decision body must contain a top-level **Decision:** marker.",
        ));
    }
    let env = |k: &str| std::env::var(k).unwrap_or_default();
    if is_canonical {
        let model = args
            .value("Model")
            .map_or_else(|| env("BRAIN_MODEL"), str::to_string);
        if model.trim().is_empty() {
            return Ok(thrown(
                "BRAIN_MODEL or -Model is required for a canonical decision append.",
            ));
        }
        let token = args
            .value("LockToken")
            .map_or_else(|| env("BRAIN_LOCK_TOKEN"), str::to_string);
        if !owner(root, &model, &token)? {
            return Ok(thrown(
                "Your sealed STATE.lock is required before editing DECISIONS.md.",
            ));
        }
    }
    let authority_from: i32 = args.value("AuthorityFrom").map_or(Ok(1), |v| {
        v.trim()
            .parse()
            .map_err(|_| format!("-AuthorityFrom needs a whole number, not {v}"))
    })?;
    let target_text = target.to_string_lossy().into_owned();
    if !verify(root, &target_text, authority_from) {
        return Ok(thrown(
            "Existing decision history is invalid; append refused.",
        ));
    }
    let original = fs::read(&target).map_err(|e| format!("{}: {e}", target.display()))?;
    let bom = original.starts_with(b"\xEF\xBB\xBF");
    let raw = String::from_utf8(original[if bom { 3 } else { 0 }..].to_vec())
        .map_err(|_| "DECISIONS.md is not valid UTF-8.".to_string())?;
    let first = regex(r"(?m)^## D(?<id>\d+)\b[^\r\n]*").captures(&raw);
    let next = match &first {
        Some(c) => c["id"].parse::<i64>().map_err(|e| e.to_string())? + 1,
        None => 1,
    };
    let eol = if raw.contains("\r\n") { "\r\n" } else { "\n" };
    let body = regex(r"\r\n|\r|\n")
        .replace_all(&body, eol)
        .trim_end_matches(['\r', '\n'])
        .to_string();
    let today = utc_today();
    let entry = format!(
        "## D{next} - {today} - {}{eol}{eol}{body}{eol}{eol}",
        title.trim()
    );
    let mut updated = match &first {
        Some(c) => {
            let at = c.get(0).map_or(0, |m| m.start());
            format!("{}{entry}{}", &raw[..at], &raw[at..])
        }
        None => {
            const EMPTY: &str = "_No decisions yet._";
            let Some(at) = raw.find(EMPTY) else {
                return Ok(thrown("Empty log marker missing."));
            };
            format!(
                "{}{}{}",
                &raw[..at],
                entry.trim_end(),
                &raw[at + EMPTY.len()..]
            )
        }
    };
    // Stamp every target a supersession field names, directly under its heading line.
    for (field, kind) in [
        ("Supersedes", "superseded"),
        ("Supersedes in part", "superseded in part"),
        ("Retires", "retired"),
    ] {
        let line = regex(&format!(
            r"(?m)^\*\*{}:\*\*[ \t]*(?<v>[^\r\n]*?)[ \t]*\r?$",
            regex::escape(field)
        ));
        let Some(found) = line.captures(&body) else {
            continue;
        };
        let ids: BTreeSet<i64> = regex(r"D(\d+)")
            .captures_iter(&found["v"])
            .filter_map(|c| c[1].parse().ok())
            .collect();
        for id in ids {
            let heading = regex(&format!(r"(?m)^## D{id}\b[^\r\n]*\r?\n"));
            let Some(h) = heading.find(&updated) else {
                return Ok(thrown(&format!(
                    "**{field}:** names D{id}, which does not exist."
                )));
            };
            let at = h.end();
            updated.insert_str(at, &format!("_Status: {kind} by D{next} ({today})_{eol}"));
        }
    }
    let mut payload = if bom {
        b"\xEF\xBB\xBF".to_vec()
    } else {
        Vec::new()
    };
    payload.extend_from_slice(updated.as_bytes());
    let dir = target.parent().unwrap_or(root);
    let aside = dir.join(format!(".DECISIONS.{}.tmp", std::process::id()));
    fs::write(&aside, &payload).map_err(|e| format!("{}: {e}", aside.display()))?;
    if let Err(e) = fs::rename(&aside, &target) {
        let _ = fs::remove_file(&aside);
        return Err(format!("{}: {e}", target.display()));
    }
    if !verify(root, &target_text, authority_from) {
        let _ = fs::write(&target, &original);
        return Ok(thrown("Appended decision failed verification (structure, authority or supersession); rolled back."));
    }
    println!(
        "DECISION_APPEND_STATUS=OK id=D{next} path={}",
        target.display()
    );
    // Keep protocol/rulings.json current for the canonical log; fixtures are left alone.
    if is_canonical {
        let rulings = build_default(root, false);
        rulings.emit();
        if rulings.code != 0 {
            println!("WARNING: protocol/rulings.json was not rebuilt; run mdr-kernel build-rulings before committing.");
        }
    }
    Ok(0)
}

/// Write the immutable authority boundary. A fresh log starts at D1; an existing history needs
/// -PreserveLegacy, must be committed unchanged, and is never ratified by this step.
fn initialize(root: &Path, args: &Args) -> Result<i32, String> {
    let target = root.join("protocol/decision-authority.json");
    if target.exists() {
        return Ok(thrown(
            "Authority boundary already exists; it is immutable.",
        ));
    }
    let env = |k: &str| std::env::var(k).unwrap_or_default();
    let model = args
        .value("Model")
        .map_or_else(|| env("BRAIN_MODEL"), str::to_string);
    let token = args
        .value("Token")
        .map_or_else(|| env("BRAIN_LOCK_TOKEN"), str::to_string);
    if !owner(root, &model, &token)? {
        return Ok(thrown("Own STATE.lock before initializing authority."));
    }
    let log = root.join(LOG);
    let Ok(bytes) = fs::read(&log) else {
        return Ok(thrown("DECISIONS.md is missing; bootstrap first."));
    };
    let raw =
        String::from_utf8_lossy(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes)).into_owned();
    let ids: Vec<i64> = regex(r"(?m)^## D(\d+)\b")
        .captures_iter(&raw)
        .filter_map(|c| c[1].parse().ok())
        .collect();
    let mut next = 1;
    if let Some(max) = ids.iter().max() {
        if !args.switch("PreserveLegacy") {
            return Ok(thrown("Existing history: review it, then explicitly use -PreserveLegacy; no old entry is ratified."));
        }
        // Only committed, unchanged history may be preserved.
        let quiet = |a: &[&str]| {
            std::process::Command::new("git")
                .current_dir(root)
                .args(a)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        };
        if !quiet(&["cat-file", "-e", "HEAD:DECISIONS.md"])
            || !quiet(&["diff", "--quiet", "HEAD", "--", "DECISIONS.md"])
        {
            return Ok(thrown(
                "Commit existing history unchanged before migration.",
            ));
        }
        next = max + 1;
    }
    let from = i32::try_from(next).map_err(|_| "decision id out of range".to_string())?;
    if !verify(root, &log.to_string_lossy(), from) {
        return Ok(thrown(
            "Existing history is structurally invalid; migration refused.",
        ));
    }
    fs::write(
        &target,
        format!("{{\"schema_version\":1,\"authority_from\":{next}}}\n"),
    )
    .map_err(|e| format!("{}: {e}", target.display()))?;
    let rulings = build_default(root, false);
    rulings.emit();
    if rulings.code != 0 {
        return Ok(thrown(
            "Boundary written but rulings failed; inspect before committing.",
        ));
    }
    println!("AUTHORITY_INITIALIZED from=D{next} legacy=unclassified");
    Ok(0)
}

/// `mdr-kernel add-decision` and `mdr-kernel initialize-decision-authority`.
pub fn write_cli(command: &str, argv: &[String]) -> i32 {
    let result = (|| -> Result<i32, String> {
        let (values, switches): (&[&str], &[&str]) = if command == "add-decision" {
            (
                &[
                    "Title",
                    "Body",
                    "BodyPath",
                    "Path",
                    "Model",
                    "LockToken",
                    "AuthorityFrom",
                    "Root",
                ],
                &[],
            )
        } else {
            (&["Model", "Token", "Root"], &["PreserveLegacy"])
        };
        let args = Args::parse(argv, values, switches)?;
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        let root = args
            .value("Root")
            .map_or_else(|| git::top(&cwd), |p| Ok(PathBuf::from(p)))
            .map(|r| crate::args::absolute(&r))?;
        crate::hook::fresh_in_repo(&root)?;
        if command == "add-decision" {
            add(&root, &args)
        } else {
            initialize(&root, &args)
        }
    })();
    result.unwrap_or_else(|e| thrown(&e))
}

// ---------- ratify-decision: an operator act, in the operator's own terminal ----------

/// The entry's exact bytes as the checker hashes them: (heading, sha256), or the report.
fn hashed_entry(root: &Path, id: &str) -> Result<(String, String), Report> {
    let report = check_history(
        root,
        &History {
            hash_of: Some(id),
            ..History::default()
        },
    );
    let field = |name: &str| {
        report
            .out
            .lines()
            .find_map(|l| l.strip_prefix(name).map(str::to_string))
    };
    match (field("ENTRY_HEADING="), field("ENTRY_SHA256=")) {
        (Some(heading), Some(sha)) if report.code == 0 => Ok((heading, sha)),
        _ => Err(report),
    }
}

/// Everything after the terminal gate. `ask` stands in for the operator's keyboard, so the
/// confirmation logic can be tested without driving any terminal.
fn ratify_steps(
    root: &Path,
    id: &str,
    quote: Option<&str>,
    surface: &str,
    model: &str,
    token: &str,
    ask: &mut dyn FnMut(&str) -> String,
) -> Result<i32, String> {
    let digits = regex(r"(?i)^D?(\d+)$")
        .captures(id.trim())
        .map(|c| c[1].to_string())
        .ok_or_else(|| format!("Id must look like D123, not {id}."))?;
    let id = format!("D{digits}");
    let (heading, sha) = match hashed_entry(root, &id) {
        Ok(found) => found,
        Err(report) => {
            report.emit();
            return Err(format!("Could not hash {id}."));
        }
    };
    let ledger = root.join(LEDGER);
    if let Ok(existing) = fs::read_to_string(&ledger) {
        for line in existing.lines().filter(|l| !l.trim().is_empty()) {
            let Ok(record) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if value(&record, "id").eq_ignore_ascii_case(&id)
                && value(&record, "entry_sha256") == sha
            {
                println!(
                    "RATIFICATION_STATUS=ALREADY id={id} ratified_at={}",
                    value(&record, "ratified_at")
                );
                return Ok(0);
            }
        }
    }
    println!("\n  {heading}\n  SHA-256 {sha}\n");
    let quote = match quote {
        Some(q) => q.to_string(),
        None => ask("Your own words for the record (optional; press Enter to skip)"),
    };
    let phrase = format!("RATIFY {id}");
    if ask(&format!("Type '{phrase}' to ratify this exact entry")) != phrase {
        println!(
            "RATIFICATION_STATUS=CANCELLED (confirmation phrase did not match; nothing written)"
        );
        return Ok(1);
    }
    if !owner(root, model, token)? {
        return Err(
            "Own STATE.lock in your operator terminal before recording ratification.".into(),
        );
    }
    if hashed_entry(root, &id).ok().map(|(_, now)| now).as_deref() != Some(sha.as_str()) {
        return Err("Entry changed while you reviewed it; rerun and review the new bytes.".into());
    }
    let history = check_history(root, &History::default());
    history.emit();
    if history.code != 0 {
        return Err("Decision history is invalid; no ratification written.".into());
    }
    let seconds = crate::state_lock::now_seconds() as i64;
    let (y, m, d) = crate::state_lock::civil_from_days(seconds.div_euclid(86_400));
    let s = seconds.rem_euclid(86_400);
    let at = format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        s / 3600,
        s % 3600 / 60,
        s % 60
    );
    let record = json!({"id": id, "entry_sha256": sha, "ratified_at": at, "quote": quote, "surface": surface});
    let current = fs::read_to_string(&ledger).unwrap_or_default();
    let prefix = if !current.is_empty() && !current.ends_with('\n') {
        "\n"
    } else {
        ""
    };
    use std::io::Write;
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&ledger)
        .and_then(|mut f| f.write_all(format!("{prefix}{record}\n").as_bytes()))
        .map_err(|e| format!("{}: {e}", ledger.display()))?;
    if check_history(root, &History::default()).code != 0 {
        println!("WARNING: the decision checker now fails; run mdr-kernel check-decision-history to see why.");
    }
    if build_default(root, false).code != 0 {
        println!("WARNING: protocol/rulings.json was not rebuilt; the next session must run mdr-kernel build-rulings.");
    }
    println!("RATIFICATION_STATUS=RECORDED id={id} sha256={sha}");
    println!("The next session commits protocol/decision-ratifications.jsonl at its close.");
    Ok(0)
}

/// `mdr-kernel ratify-decision -Id <D#>`. Refuses unless standard input and output are a real
/// terminal: ratification is the operator's act, never a model tool call's. The PowerShell
/// wrapper refuses non-interactive and -NonInteractive callers before it hands off.
pub fn ratify_cli(argv: &[String]) -> i32 {
    use std::io::IsTerminal;
    let mut reasons = Vec::new();
    if !std::io::stdin().is_terminal() {
        reasons.push("standard input is redirected");
    }
    if !std::io::stdout().is_terminal() {
        reasons.push("standard output is redirected");
    }
    if !reasons.is_empty() {
        println!("RATIFICATION_STATUS=REFUSED");
        println!("REASON={}", reasons.join("; "));
        println!(
            "Ratification is an operator act. Run this command yourself in your own terminal."
        );
        return 3;
    }
    let result = (|| -> Result<i32, String> {
        let args = Args::parse(
            argv,
            &["Id", "Quote", "Model", "Token", "Surface", "Root"],
            &[],
        )?;
        let surface = args.value("Surface").unwrap_or("terminal");
        const SURFACES: [&str; 6] = [
            "terminal",
            "chat",
            "operator terminal",
            "in person",
            "email",
            "phone",
        ];
        if !SURFACES.iter().any(|s| s.eq_ignore_ascii_case(surface)) {
            return Err(format!("-Surface must be one of: {}", SURFACES.join(", ")));
        }
        let id = args.value("Id").ok_or("-Id is required.")?;
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        let root = args
            .value("Root")
            .map_or_else(|| git::top(&cwd), |p| Ok(PathBuf::from(p)))
            .map(|r| crate::args::absolute(&r))?;
        crate::hook::fresh_in_repo(&root)?;
        let env = |k: &str| std::env::var(k).unwrap_or_default();
        let model = args
            .value("Model")
            .map_or_else(|| env("BRAIN_MODEL"), str::to_string);
        let token = args
            .value("Token")
            .map_or_else(|| env("BRAIN_LOCK_TOKEN"), str::to_string);
        let mut ask = |prompt: &str| -> String {
            use std::io::Write;
            print!("{prompt}: ");
            let _ = std::io::stdout().flush();
            let mut line = String::new();
            let _ = std::io::stdin().read_line(&mut line);
            line.trim_end_matches(['\r', '\n']).to_string()
        };
        ratify_steps(
            &root,
            id,
            args.value("Quote"),
            surface,
            &model,
            &token,
            &mut ask,
        )
    })();
    result.unwrap_or_else(|e| thrown(&e))
}

#[cfg(test)]
mod ratify_tests {
    use super::*;

    fn fixture(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("mdr-ratify-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("protocol")).unwrap();
        fs::write(
            root.join("DECISIONS.md"),
            "# Decisions\n\n## D1 - 2026-11-23 - Keep receipts\n\n**Authority:** proposed\n\n**Decision:** Keep every verified receipt.\n",
        )
        .unwrap();
        root
    }

    #[test]
    fn a_wrong_phrase_writes_nothing() {
        let root = fixture("cancel");
        let mut answers = vec!["".to_string(), "ratify d1".to_string()].into_iter();
        let code = ratify_steps(&root, "D1", None, "terminal", "m", "", &mut |_| {
            answers.next().unwrap_or_default()
        });
        assert_eq!(code, Ok(1));
        assert!(!root.join(LEDGER).exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_unknown_id_and_a_bad_id_refuse() {
        let root = fixture("unknown");
        let mut never = |_: &str| -> String { panic!("must not prompt") };
        assert!(ratify_steps(&root, "D9", None, "terminal", "m", "", &mut never).is_err());
        assert!(ratify_steps(&root, "x1", None, "terminal", "m", "", &mut never).is_err());
        assert!(!root.join(LEDGER).exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_right_phrase_still_needs_the_lock() {
        let root = fixture("lock");
        let mut answers = vec!["RATIFY D1".to_string()].into_iter();
        let result = ratify_steps(
            &root,
            "D1",
            Some("my words"),
            "terminal",
            "m",
            "",
            &mut |_| answers.next().unwrap_or_default(),
        );
        assert!(
            result.is_err(),
            "no STATE.lock is held, so nothing may be recorded"
        );
        assert!(!root.join(LEDGER).exists());
        let _ = fs::remove_dir_all(&root);
    }
}
