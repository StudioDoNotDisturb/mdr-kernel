//! Action-time recall. A port of `mdr-kernel brain-recall`: match an objective against
//! `protocol/recall-gates.json` and print the required reads, recurrence-counted traps, stop signs
//! and cited rulings before a model acts. Also `-CheckGates`, `-Ruling`, `-Increment`, `-Json`,
//! `-Brief`, `-Header` and `-Quiet`, with the same exit codes (2 = BLOCKED).
//!
//!   mdr-kernel brain-recall -Objective <text> | -FromStdin | -GateId <id> | -Ruling <D#|text>
//!       | -CheckGates [-GatesPath <file>] | -Increment <trap id>  [-Brief|-Json|-Header|-Quiet]

use crate::args::Args;
use crate::git::top;
use regex::Regex;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

const MAX_OBJECTIVE: usize = 131_072;
const BRIEF_BUDGET: usize = 1400;
const VALUES: [&str; 6] = [
    "Objective",
    "GatesPath",
    "Ruling",
    "GateId",
    "Increment",
    "Root",
];
const SWITCHES: [&str; 6] = [
    "FromStdin",
    "Brief",
    "CheckGates",
    "Json",
    "Quiet",
    "Header",
];

/// What the script writes and how it exits.
pub struct Outcome {
    pub code: i32,
    pub out: String,
    pub err: String,
}

fn blocked(reason: &str) -> Outcome {
    Outcome {
        code: 2,
        out: String::new(),
        err: format!("RECALL_STATUS=BLOCKED reason={reason}\n"),
    }
}

fn done(code: i32, out: String) -> Outcome {
    Outcome {
        code,
        out,
        err: String::new(),
    }
}

/// UTF-16 length, as .NET `String.Length` counts.
fn units(s: &str) -> usize {
    s.encode_utf16().count()
}

fn read_text(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    Some(String::from_utf8_lossy(bytes).into_owned())
}

/// The file `ConvertTo-Json` writes: serde's pretty form with the platform newline.
pub(crate) fn to_json(value: &Value) -> String {
    let text = serde_json::to_string_pretty(value).unwrap_or_default();
    if cfg!(windows) {
        text.replace('\n', "\r\n")
    } else {
        text
    }
}

// ---------- validation (v1 schema) ----------

/// .NET `\w`: letters, digits, connector punctuation and combining marks.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || unicode_category_mark(c)
}

/// Combining marks count as word characters in .NET `\w`.
fn unicode_category_mark(c: char) -> bool {
    static MARK: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    MARK.get_or_init(|| Regex::new(r"^\p{M}$").unwrap())
        .is_match(c.encode_utf8(&mut [0; 4]))
}

/// The hyphen-aware whole-term match: "post-launch analysis" must not match "launch analysis".
fn term_hits(objective_lower: &str, term: &str) -> bool {
    let t = term.trim().to_lowercase();
    if t.is_empty() || !objective_lower.contains(&t) {
        return false;
    }
    let edge = |c: Option<char>| c.is_none_or(|c| !(is_word(c) || c == '-'));
    objective_lower.char_indices().any(|(i, _)| {
        objective_lower[i..].starts_with(&t)
            && edge(objective_lower[..i].chars().next_back())
            && edge(objective_lower[i + t.len()..].chars().next())
    })
}

fn require_string(value: &Value, at: &str) -> Result<(), String> {
    match value.as_str() {
        Some(s) if !s.trim().is_empty() => Ok(()),
        _ => Err(format!("{at} must be a nonempty string")),
    }
}

fn require_strings(value: &Value, at: &str) -> Result<(), String> {
    let items = value
        .as_array()
        .ok_or_else(|| format!("{at} must be an array of strings"))?;
    items.iter().try_for_each(|i| require_string(i, at))
}

fn require_count(value: &Value, at: &str) -> Result<(), String> {
    match value.as_i64() {
        Some(n) if (0..=i64::from(i32::MAX)).contains(&n) => Ok(()),
        _ => Err(format!("{at} must be a nonnegative Int32 integer")),
    }
}

fn require_id(value: &Value, at: &str) -> Result<(), String> {
    require_string(value, at)?;
    let safe = Regex::new(r"^[A-Za-z0-9][A-Za-z0-9_-]{0,79}$").unwrap();
    if !safe.is_match(value.as_str().unwrap_or("")) {
        return Err(format!(
            "{at} must be a safe identifier of at most 80 characters"
        ));
    }
    Ok(())
}

fn object<'a>(value: &'a Value, at: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{at} must be an object"))
}

fn validate(data: &Value) -> Result<(), String> {
    let root = object(data, "root")?;
    match root.get("version").and_then(Value::as_i64) {
        Some(1) => {}
        Some(_) => return Err("unsupported version (expected 1)".into()),
        None => return Err("version must be integer 1".into()),
    }
    let gates = root
        .get("gates")
        .and_then(Value::as_array)
        .filter(|g| g.len() <= 128)
        .ok_or("gates must be an array of at most 128 gates")?;
    // PowerShell hashtables compare keys without case.
    let (mut gate_ids, mut trap_ids) = (HashSet::new(), HashSet::new());
    let null = Value::Null;
    let decision = Regex::new(r"^D[1-9][0-9]*$").unwrap();
    for gate in gates {
        let g = object(gate, "gate")?;
        let field = |k: &str| g.get(k).unwrap_or(&null);
        require_id(field("id"), "gate.id")?;
        if !gate_ids.insert(field("id").as_str().unwrap_or("").to_lowercase()) {
            return Err("duplicate gate id".into());
        }
        require_string(field("label"), "gate.label")?;
        require_count(field("times_recurred"), "gate.times_recurred")?;
        for k in ["triggers", "required_reads", "stop_signs"] {
            require_strings(field(k), &format!("gate.{k}"))?;
        }
        for k in ["aliases", "rulings"] {
            if g.contains_key(k) {
                require_strings(field(k), &format!("gate.{k}"))?;
            }
        }
        if let Some(ids) = field("rulings").as_array() {
            if ids
                .iter()
                .any(|id| !decision.is_match(id.as_str().unwrap_or("")))
            {
                return Err("gate.rulings must contain decision IDs such as D1".into());
            }
        }
        if g.contains_key("context_rules") {
            let rules = field("context_rules")
                .as_array()
                .ok_or("gate.context_rules must be an array")?;
            for rule in rules {
                let r = object(rule, "context rule")?;
                let any = r.get("any").unwrap_or(&null);
                let with = r.get("with").unwrap_or(&null);
                require_strings(any, "context rule.any")?;
                require_strings(with, "context rule.with")?;
                if any.as_array().is_some_and(Vec::is_empty)
                    || with.as_array().is_some_and(Vec::is_empty)
                {
                    return Err("context rule sides must not be empty".into());
                }
            }
        }
        let traps = field("traps")
            .as_array()
            .ok_or("gate.traps must be an array")?;
        for trap in traps {
            let t = object(trap, "trap")?;
            let tf = |k: &str| t.get(k).unwrap_or(&null);
            require_id(tf("id"), "trap.id")?;
            if !trap_ids.insert(tf("id").as_str().unwrap_or("").to_lowercase()) {
                return Err("duplicate trap id".into());
            }
            require_count(tf("times_recurred"), "trap.times_recurred")?;
            for k in ["symptom", "wrong_move", "right_move"] {
                require_string(tf(k), &format!("trap.{k}"))?;
            }
            for k in ["evidence_refs", "triggers"] {
                if t.contains_key(k) {
                    require_strings(tf(k), &format!("trap.{k}"))?;
                }
            }
        }
    }
    Ok(())
}

/// Read and validate the gate document. `Err` carries the BLOCKED reason.
pub fn load(path: &Path) -> Result<Value, &'static str> {
    if !path.exists() {
        return Err("missing-gate-data (run mdr-kernel bootstrap-clone)");
    }
    if std::fs::metadata(path).map_or(true, |m| m.len() > 2_097_152) {
        return Err(INVALID_GATES);
    }
    parse_gates(&std::fs::read(path).map_err(|_| INVALID_GATES)?)
}

const INVALID_GATES: &str =
    "invalid-gate-data (check v1 field types, unique IDs, arrays and nonnegative counts)";

pub fn parse_gates(bytes: &[u8]) -> Result<Value, &'static str> {
    // Parse errors can echo document contents. Report a stable code, never the data.
    if bytes.len() > 2_097_152 {
        return Err(INVALID_GATES);
    }
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let data: Value =
        serde_json::from_str(&String::from_utf8_lossy(bytes)).map_err(|_| INVALID_GATES)?;
    validate(&data).map_err(|_| INVALID_GATES)?;
    Ok(data)
}

/// `-CheckGates`: the doctor and the pre-commit run this on every check.
pub fn check_gates(path: &Path) -> Outcome {
    match load(path) {
        Ok(data) => done(
            0,
            format!("RECALL_GATES_STATUS=VALID gates={}\n", gates(&data).len()),
        ),
        Err(reason) => blocked(reason),
    }
}

fn gates(data: &Value) -> &[Value] {
    data["gates"].as_array().map_or(&[], Vec::as_slice)
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

// ---------- rulings ----------

fn source_digest(root: &Path, rel: &str) -> String {
    let path = root.join(rel);
    let source = if path.is_file() {
        read_text(&path).unwrap_or_default()
    } else {
        String::new()
    };
    let normalized = source.replace("\r\n", "\n");
    let digest = Sha256::digest(normalized.trim_end().as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// The compiled ruling index, keyed by lower-case id; checked fresh against its sources.
fn load_rulings(root: &Path, data: &Value) -> Result<HashMap<String, Value>, ()> {
    let doc: Value = read_text(&root.join("protocol/rulings.json"))
        .and_then(|t| serde_json::from_str(&t).ok())
        .ok_or(())?;
    let sources = doc.as_object().and_then(|d| d.get("sources")).ok_or(())?;
    sources.as_object().ok_or(())?;
    for (key, rel) in [
        ("decisions", "DECISIONS.md"),
        ("ratifications", "protocol/decision-ratifications.jsonl"),
        ("gates", "protocol/recall-gates.json"),
    ] {
        if sources[key].as_str() != Some(source_digest(root, rel).as_str()) {
            return Err(());
        }
    }
    let decision = Regex::new(r"^D[1-9][0-9]*$").unwrap();
    let mut by_id = HashMap::new();
    for r in doc["rulings"].as_array().ok_or(())? {
        r.as_object().ok_or(())?;
        let id = r["id"]
            .as_str()
            .filter(|id| decision.is_match(id))
            .ok_or(())?;
        if by_id.contains_key(&id.to_lowercase()) {
            return Err(());
        }
        require_string(&r["rule"], "rule").map_err(|_| ())?;
        let class = r["class"].as_str().unwrap_or("").to_lowercase();
        if class != "operator-ruled" && class != "operator-ratified" {
            return Err(());
        }
        by_id.insert(id.to_lowercase(), r.clone());
    }
    for gate in gates(data) {
        for id in strings(&gate["rulings"]) {
            if !by_id.contains_key(&id.to_lowercase()) {
                return Err(());
            }
        }
    }
    Ok(by_id)
}

fn ruling_line(r: &Value) -> String {
    format!(
        "{} [{}] {}",
        text(&r["id"]),
        text(&r["class"]),
        text(&r["rule"])
    )
}

// ---------- the command ----------

struct Matched<'a> {
    gate: &'a Value,
    triggers: Vec<String>,
    rulings: Vec<Value>,
}

pub fn run(root: &Path, args: &Args, stdin: Option<String>) -> Outcome {
    run_seen(root, args, stdin, &HashSet::new(), &mut Vec::new())
}

/// `run`, with the hook's delivery receipts: in `-Brief`, traps already in `seen` are not
/// re-sent, and `delivered` receives the fingerprints this banner actually printed.
fn run_seen(
    root: &Path,
    args: &Args,
    stdin: Option<String>,
    seen: &HashSet<String>,
    delivered: &mut Vec<String>,
) -> Outcome {
    let mut gate_path = root.join("protocol/recall-gates.json");
    if args.has("GatesPath") {
        let others = [
            "Objective",
            "FromStdin",
            "Brief",
            "Ruling",
            "Json",
            "Quiet",
            "GateId",
            "Header",
            "Increment",
        ];
        let path = args.value("GatesPath").unwrap_or("");
        if !args.switch("CheckGates")
            || path.trim().is_empty()
            || others.iter().any(|o| args.has(o))
        {
            return blocked("gates-path-requires-check-only");
        }
        gate_path = PathBuf::from(path);
    }
    let objective = match stdin {
        Some(text) => text,
        None => args.value("Objective").unwrap_or("").to_string(),
    };
    if units(&objective) > MAX_OBJECTIVE {
        return blocked("objective-too-large (maximum 131072 characters)");
    }
    let given = |k: &str| args.value(k).is_some_and(|v| !v.is_empty());
    if !given("Increment")
        && !given("GateId")
        && !given("Ruling")
        && !args.switch("CheckGates")
        && objective.trim().is_empty()
    {
        return blocked(
            "Provide -Objective, -FromStdin, -GateId, -Ruling, -CheckGates or -Increment.",
        );
    }
    let mut data = match load(&gate_path) {
        Ok(d) => d,
        Err(reason) => return blocked(reason),
    };
    if args.switch("CheckGates") {
        return done(
            0,
            format!("RECALL_GATES_STATUS=VALID gates={}\n", gates(&data).len()),
        );
    }

    // The ruling index is optional until a gate cites one or a lookup is asked for.
    let ruling = args.value("Ruling").filter(|r| !r.is_empty());
    let cites = gates(&data)
        .iter()
        .any(|g| g["rulings"].as_array().is_some_and(|r| !r.is_empty()));
    let mut rulings = HashMap::new();
    if ruling.is_some() || cites {
        match load_rulings(root, &data) {
            Ok(r) => rulings = r,
            Err(()) => return blocked(
                "invalid-rulings (run mdr-kernel build-rulings -Check and repair before acting)",
            ),
        }
    }
    if let Some(query) = ruling {
        let query = query.trim();
        let numeric = Regex::new(r"(?i)^D?\d+$").unwrap();
        let hits: Vec<&Value> = if numeric.is_match(query) {
            let id = format!("d{}", query.trim_start_matches(['D', 'd']));
            rulings.get(&id).into_iter().collect()
        } else {
            let q = query.to_lowercase();
            let mut hits: Vec<&Value> = rulings
                .values()
                .filter(|r| text(&r["rule"]).to_lowercase().contains(&q))
                .collect();
            hits.sort_by_key(|r| text(&r["id"]).to_lowercase());
            hits
        };
        if hits.is_empty() {
            return done(1, "RULING_STATUS=NONE (no live ruling matched)\n".into());
        }
        let mut out = format!("RULING_STATUS=FOUND count={}\n", hits.len());
        for r in hits {
            let _ = writeln!(out, "{}", ruling_line(r));
            if let Some(rules_out) = r["rules_out"].as_str().filter(|s| !s.is_empty()) {
                let _ = writeln!(out, "  rules out: {rules_out}");
            }
        }
        return done(0, out);
    }

    // Record a recurrence: bump the trap and its gate, write the document back.
    if let Some(trap_id) = args.value("Increment").filter(|i| !i.is_empty()) {
        return increment(&gate_path, &mut data, trap_id);
    }

    let lower = objective.to_lowercase();
    let gate_id = args.value("GateId").filter(|g| !g.is_empty());
    let mut matched = Vec::new();
    for gate in gates(&data) {
        let triggers = if let Some(id) = gate_id {
            if !text(&gate["id"]).eq_ignore_ascii_case(id) {
                continue;
            }
            vec!["explicit-gate-id".to_string()]
        } else {
            // Triggers and aliases match identically; synonyms grow in the JSON.
            let mut hits: Vec<String> = strings(&gate["triggers"])
                .into_iter()
                .chain(strings(&gate["aliases"]))
                .filter(|t| term_hits(&lower, t))
                .collect();
            for rule in gate["context_rules"].as_array().into_iter().flatten() {
                let left: Vec<String> = strings(&rule["any"])
                    .into_iter()
                    .filter(|t| term_hits(&lower, t))
                    .collect();
                let right: Vec<String> = strings(&rule["with"])
                    .into_iter()
                    .filter(|t| term_hits(&lower, t))
                    .collect();
                if let (Some(l), Some(r)) = (left.first(), right.first()) {
                    hits.push(format!("context:{l} + {r}"));
                }
            }
            if hits.is_empty() {
                continue;
            }
            hits
        };
        let cited = strings(&gate["rulings"])
            .iter()
            .filter_map(|id| rulings.get(&id.to_lowercase()).cloned())
            .collect();
        matched.push(Matched {
            gate,
            triggers,
            rulings: cited,
        });
    }
    if gate_id.is_some() && matched.is_empty() {
        return blocked("unknown-gate-id");
    }
    let status = if matched.is_empty() {
        "CLEAR"
    } else {
        "MATCHED"
    };

    if args.switch("Json") {
        let gates: Vec<Value> = matched
            .iter()
            .map(|m| {
                let g = m.gate;
                serde_json::json!({
                    "id": g["id"],
                    "label": g["label"],
                    "times_recurred": g["times_recurred"],
                    "matched_triggers": m.triggers,
                    "required_reads": g["required_reads"],
                    "traps": g["traps"],
                    "stop_signs": g["stop_signs"],
                    "rulings": m.rulings,
                })
            })
            .collect();
        let result = serde_json::json!({
            "status": status,
            "objective": objective,
            "matched_count": matched.len(),
            "gates": gates,
        });
        // Stdout stays LF like every other engine report; only the rewritten file keeps CRLF.
        return done(
            0,
            serde_json::to_string_pretty(&result).unwrap_or_default() + "\n",
        );
    }
    if args.switch("Quiet") {
        return done(0, String::new());
    }
    if args.switch("Brief") {
        return match brief(&matched, seen) {
            Ok((banner, sent)) => {
                *delivered = sent;
                done(0, banner)
            }
            Err(()) => blocked(
                "compact-budget-exceeded (use full recall; no matched gate was silently omitted)",
            ),
        };
    }
    if args.switch("Header") {
        return done(0, header(&matched));
    }
    done(0, full(status, &objective, &matched))
}

fn traps(gate: &Value) -> &[Value] {
    gate["traps"].as_array().map_or(&[], Vec::as_slice)
}

/// Healthy CLEAR prints nothing. Every matched gate and the full-read instruction come before
/// optional detail, so a budget cut never erases a later gate or the recovery path.
fn brief(matched: &[Matched], seen: &HashSet<String>) -> Result<(String, Vec<String>), ()> {
    // Receipts suppress delivery only, after matching: a gate whose traps were all delivered to
    // this session drops out; a gate with no traps cannot be receipted and always shows.
    fn pending<'a>(m: &Matched<'a>, seen: &HashSet<String>) -> Vec<&'a Value> {
        traps(m.gate)
            .iter()
            .filter(|t| !seen.contains(&delivery_key(m.gate, t)))
            .collect()
    }
    let matched: Vec<&Matched> = matched
        .iter()
        .filter(|m| traps(m.gate).is_empty() || !pending(m, seen).is_empty())
        .collect();
    if matched.is_empty() {
        return Ok((String::new(), Vec::new()));
    }
    let mut lines = vec![format!(
        "RECALL_STATUS=MATCHED gates={} - read before acting",
        matched.len()
    )];
    for m in &matched {
        let mut line = format!(
            "- {} {}x ({} traps, {} reads)",
            text(&m.gate["id"]),
            text(&m.gate["times_recurred"]),
            traps(m.gate).len(),
            strings(&m.gate["required_reads"]).len()
        );
        if !m.rulings.is_empty() {
            let ids: Vec<String> = m.rulings.iter().map(|r| text(&r["id"])).collect();
            line += &format!("; rulings: {}", ids.join(","));
        }
        lines.push(line);
    }
    lines.push("For EACH gate ID: mdr-kernel brain-recall -GateId <ID>".into());
    lines.push("NEEDS_READ=Full output has all traps, rulings and required reads; this reminder is not write authorization.".into());
    let mut banner = lines.join("\n");
    if units(&banner) > BRIEF_BUDGET {
        return Err(());
    }
    let mut sent = Vec::new();
    for m in &matched {
        for trap in pending(m, seen) {
            let detail = format!(
                "  {}/{} {}x",
                text(&m.gate["id"]),
                text(&trap["id"]),
                text(&trap["times_recurred"])
            );
            // Only a trap whose line was printed counts as delivered.
            if units(&banner) + units(&detail) < BRIEF_BUDGET {
                banner = banner + "\n" + &detail;
                sent.push(delivery_key(m.gate, trap));
            }
        }
    }
    Ok((banner + "\n", sent))
}

// ---------- receipts: one session is not re-sent the traps it was already shown ----------

const RECEIPT_MAX_BYTES: u64 = 131_072;
const RECEIPT_MAX_KEYS: usize = 1024;

fn sha_hex(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A changed gate (its rules, reads or recurrence counts) or trap is a new fingerprint.
fn delivery_key(gate: &Value, trap: &Value) -> String {
    sha_hex(&format!("mdr-kernel-recall-v1|{gate}|{trap}"))
}

fn session_of(payload: &Map<String, Value>) -> Option<String> {
    match field(payload, "session_id") {
        Some(Value::String(s)) if !s.trim().is_empty() => Some(s.clone()),
        Some(Value::Number(n)) => Some(n.to_string()),
        _ => None,
    }
}

/// The per-session receipt file, under the OS temp folder, named by a hash of the memory root
/// and the session: no prompt text or raw session id is written anywhere.
fn receipt_path(root: &Path, session: &str) -> Option<PathBuf> {
    let root = root.canonicalize().ok()?;
    let key = sha_hex(&format!("{}|{session}", root.display()));
    Some(
        std::env::temp_dir()
            .join("mdr-kernel-recall-delivery")
            .join(format!("{key}.json")),
    )
}

/// A link (or Windows junction) at the receipt folder or file could redirect a write.
fn linked(path: &Path) -> bool {
    [Some(path), path.parent()].into_iter().flatten().any(|p| {
        std::fs::symlink_metadata(p).is_ok_and(|m| {
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                m.file_type().is_symlink() || m.file_attributes() & 0x400 != 0
            }
            #[cfg(not(windows))]
            {
                m.file_type().is_symlink()
            }
        })
    })
}

/// What this session was already shown. Any doubt (a link, size, version, another checkpoint,
/// a malformed key) yields an empty set: a reminder repeats, a new one never hides.
fn load_receipts(path: &Path, checkpoint: Option<&str>) -> HashSet<String> {
    let read = || -> Option<HashSet<String>> {
        if linked(path) || std::fs::metadata(path).ok()?.len() > RECEIPT_MAX_BYTES {
            return None;
        }
        let data: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
        if data["version"] != 1 || data["checkpoint"].as_str() != checkpoint {
            return None;
        }
        let keys = data["seen"]
            .as_array()
            .filter(|k| k.len() <= RECEIPT_MAX_KEYS)?;
        keys.iter()
            .map(|k| {
                k.as_str()
                    .filter(|s| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()))
                    .map(str::to_string)
            })
            .collect()
    };
    read().unwrap_or_default()
}

/// Best effort and atomic (write aside, then rename). A failed save only repeats a reminder.
fn save_receipts(path: &Path, seen: &HashSet<String>, checkpoint: Option<&str>) {
    let save = || -> Option<()> {
        let dir = path.parent()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir)
                .ok()?;
        }
        #[cfg(not(unix))]
        std::fs::create_dir_all(dir).ok()?;
        if linked(path) {
            return None;
        }
        let mut keys: Vec<&String> = seen.iter().collect();
        keys.sort();
        keys.truncate(RECEIPT_MAX_KEYS);
        let data = serde_json::json!({"version": 1, "seen": keys, "checkpoint": checkpoint});
        let aside = path.with_extension(format!("{}.tmp", std::process::id()));
        std::fs::write(&aside, data.to_string()).ok()?;
        std::fs::rename(&aside, path)
            .or_else(|_| std::fs::remove_file(&aside))
            .ok()
    };
    let _ = save();
}

/// The tool-time banner a fragile script prints at the moment of action.
fn header(matched: &[Matched]) -> String {
    let mut out = String::new();
    for m in matched {
        let id = text(&m.gate["id"]);
        let worst = traps(m.gate)
            .iter()
            .filter_map(|t| t["times_recurred"].as_i64())
            .max()
            .unwrap_or(0)
            .max(0);
        let _ = writeln!(out, "##### KNOWN TRAPS [{id}] - gate recurred {}x, worst trap {worst}x. Read before acting. #####", text(&m.gate["times_recurred"]));
        for t in traps(m.gate) {
            let _ = writeln!(
                out,
                "- [{}] {}x  WRONG: {}  RIGHT: {}",
                text(&t["id"]),
                text(&t["times_recurred"]),
                text(&t["wrong_move"]),
                text(&t["right_move"])
            );
        }
        let _ = writeln!(
            out,
            "  reads: {}",
            strings(&m.gate["required_reads"]).join("; ")
        );
        for r in &m.rulings {
            let _ = writeln!(out, "  ruling: {}", ruling_line(r));
        }
        let _ = writeln!(
            out,
            "  full: mdr-kernel brain-recall -GateId {id} -Objective '(tool)'"
        );
        let _ = writeln!(out, "##### END KNOWN TRAPS [{id}] #####");
    }
    out
}

fn full(status: &str, objective: &str, matched: &[Matched]) -> String {
    let mut out = format!("RECALL_STATUS={status}\nOBJECTIVE={objective}\n");
    if matched.is_empty() {
        // CLEAR is not a green light: rule out a reworded recurring task before acting.
        out.push_str("NOTE=No recall gate matched. CLEAR is not a green light: confirm this work is genuinely novel, not a reworded recurring/stuck task. If it touches a known-fragile domain or the operator called it recurring, rephrase with the domain words and run again, or read the gate it should have hit.\n");
        return out;
    }
    let ids: Vec<String> = matched.iter().map(|m| text(&m.gate["id"])).collect();
    let _ = writeln!(out, "MATCHED_GATES={}", ids.join(", "));
    out.push_str("ACTION_REQUIRED=Read required sources before acting; state the matched traps and how they change the next move.\n");
    for m in matched {
        let g = m.gate;
        let _ = writeln!(out, "\n== {}: {} ==", text(&g["id"]), text(&g["label"]));
        let _ = writeln!(out, "RECURRED={}", text(&g["times_recurred"]));
        let _ = writeln!(out, "Matched triggers: {}", m.triggers.join(", "));
        for r in &m.rulings {
            let _ = writeln!(out, "Ruling in force: {}", ruling_line(r));
        }
        out.push_str("Required reads:\n");
        for read in strings(&g["required_reads"]) {
            let _ = writeln!(out, "- {read}");
        }
        out.push_str("Known traps:\n");
        for t in traps(g) {
            let _ = writeln!(
                out,
                "- [{}] RECURRED {}x",
                text(&t["id"]),
                text(&t["times_recurred"])
            );
            for (key, label) in [
                ("symptom", "symptom"),
                ("wrong_move", "wrong"),
                ("right_move", "right"),
            ] {
                let value = text(&t[key]);
                if !value.is_empty() {
                    let _ = writeln!(out, "  {label}: {value}");
                }
            }
        }
        out.push_str("Stop signs:\n");
        for sign in strings(&g["stop_signs"]) {
            let _ = writeln!(out, "- {sign}");
        }
    }
    out
}

fn increment(path: &Path, data: &mut Value, trap_id: &str) -> Outcome {
    let found = gates(data).iter().enumerate().find_map(|(g, gate)| {
        traps(gate)
            .iter()
            .position(|t| text(&t["id"]).eq_ignore_ascii_case(trap_id))
            .map(|t| (g, t))
    });
    let Some((g, t)) = found else {
        return blocked("unknown-trap-id");
    };
    let max = i64::from(i32::MAX);
    let count = |v: &Value| v.as_i64().unwrap_or(0);
    let gate = &mut data["gates"][g];
    if count(&gate["traps"][t]["times_recurred"]) == max || count(&gate["times_recurred"]) == max {
        return blocked("recurrence-count-overflow");
    }
    let trap_count = count(&gate["traps"][t]["times_recurred"]) + 1;
    let gate_count = count(&gate["times_recurred"]) + 1;
    gate["traps"][t]["times_recurred"] = trap_count.into();
    gate["times_recurred"] = gate_count.into();
    let (gate_name, trap_name) = (text(&gate["id"]), text(&gate["traps"][t]["id"]));
    // An existing `updated` keeps its place; a new one goes at the end.
    if let Some(root) = data.as_object_mut() {
        root.insert("updated".into(), crate::close::local_date().into());
    }
    if let Err(e) = std::fs::write(path, to_json(data) + "\n") {
        return Outcome {
            code: 1,
            out: String::new(),
            err: format!("{}: {e}\n", path.display()),
        };
    }
    done(0, format!("INCREMENTED trap {trap_name} -> {trap_count}x (gate {gate_name} -> {gate_count}x)\nNEXT: commit protocol/recall-gates.json under STATE.lock (pre-commit enforces it) and record the evidence in DECISIONS.md or a handoff - a count without evidence is just another note.\n"))
}

/// Read stdin as UTF-8 (BOM dropped), one character past the budget so overflow is visible.
fn read_stdin(max_chars: usize) -> Result<String, ()> {
    use std::io::Read;
    let mut bytes = Vec::new();
    // Four bytes per character bound one character past the budget, so overflow stays visible.
    let limit = (max_chars as u64 + 1) * 4 + 3;
    std::io::stdin()
        .take(limit)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

// ---------- adapters: the UserPromptSubmit hook and the prepare-work fallback ----------

const HOOK_PAYLOAD_LIMIT: usize = 1_048_576;

fn hook_wire(context: &str) -> String {
    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "UserPromptSubmit",
            "additionalContext": context,
        }
    })
    .to_string()
        + "\n"
}

fn hook_degraded(reason: &str) -> String {
    hook_wire(&format!("RECALL_HOOK_STATUS=DEGRADED reason={reason}; automatic recall was not evaluated. Before acting run mdr-kernel prepare-work -Objective '<operator objective>' (or pipe the complete objective with -FromStdin)."))
}

/// PowerShell property access ignores case: `$payload.user_input` also reads `User_Input`.
fn field<'a>(object: &'a Map<String, Value>, name: &str) -> Option<&'a Value> {
    object
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v)
}

/// The hook: bounded typed input, complete recall in process, visible degradation. Healthy
/// CLEAR prints nothing. Payload text never reaches the degraded context.
pub fn hook(root: &Path, raw: &str) -> String {
    if units(raw) > HOOK_PAYLOAD_LIMIT {
        return hook_degraded("payload-too-large");
    }
    if raw.is_empty() {
        return hook_degraded("empty-input");
    }
    // ConvertFrom-Json turns blank text into $null, which is not an object.
    let payload = if raw.trim().is_empty() {
        Value::Null
    } else {
        match serde_json::from_str::<Value>(raw) {
            Ok(v) => v,
            Err(_) => return hook_degraded("invalid-json"),
        }
    };
    let Some(object) = payload.as_object() else {
        return hook_degraded("payload-not-object");
    };
    // SessionStart (a new session, a resume, or the restart after compaction) re-arms every
    // reminder for that session, silently.
    let receipts = session_of(object).and_then(|s| receipt_path(root, &s));
    let checkpoint = field(object, "checkpoint").and_then(Value::as_str);
    if field(object, "hook_event_name")
        .and_then(Value::as_str)
        .is_some_and(|e| e.eq_ignore_ascii_case("SessionStart"))
    {
        if let Some(path) = &receipts {
            save_receipts(path, &HashSet::new(), None);
        }
        return String::new();
    }
    let prompt = match field(object, "user_input") {
        None | Some(Value::Null) => field(object, "prompt"),
        Some(Value::String(s)) if s.trim().is_empty() => field(object, "prompt"),
        other => other,
    };
    let Some(prompt) = prompt.and_then(Value::as_str) else {
        return hook_degraded("prompt-not-string");
    };
    if prompt.trim().is_empty() {
        return hook_degraded("missing-prompt");
    }
    if units(prompt) > MAX_OBJECTIVE {
        return hook_degraded("prompt-too-large");
    }
    let args = match Args::parse(&["-FromStdin".into(), "-Brief".into()], &VALUES, &SWITCHES) {
        Ok(a) => a,
        Err(_) => return hook_degraded("adapter-error"),
    };
    let mut seen = receipts
        .as_deref()
        .map(|p| load_receipts(p, checkpoint))
        .unwrap_or_default();
    let mut delivered = Vec::new();
    let outcome = run_seen(root, &args, Some(prompt.to_string()), &seen, &mut delivered);
    if let (Some(path), false) = (&receipts, delivered.is_empty()) {
        seen.extend(delivered);
        save_receipts(path, &seen, checkpoint);
    }
    if outcome.code != 0 || !outcome.err.trim().is_empty() {
        return hook_degraded("matcher-failed");
    }
    let banner = outcome.out.trim();
    if banner.is_empty() {
        return String::new();
    }
    if units(banner) > BRIEF_BUDGET || !banner.starts_with("RECALL_STATUS=MATCHED ") {
        return hook_degraded("invalid-matcher-output");
    }
    hook_wire(banner)
}

/// The fallback for a surface without a proven prompt hook: full recall, then READY or BLOCKED.
pub fn prepare_work(root: &Path, objective: Option<&str>, stdin: Option<String>) -> Outcome {
    let mut out = String::from("RECALL_SURFACE=fallback-wrapper status=RUNNING\n");
    let argv: Vec<String> = match (objective, &stdin) {
        (_, Some(_)) => vec!["-FromStdin".into()],
        (Some(o), None) => vec![format!("-Objective:{o}")],
        (None, None) => Vec::new(),
    };
    let recall = match Args::parse(&argv, &VALUES, &SWITCHES) {
        Ok(args) => run(root, &args, stdin),
        Err(e) => blocked(&e),
    };
    out.push_str(&recall.out);
    if recall.code != 0 {
        return Outcome {
            code: recall.code,
            out,
            err: recall.err + "PREPARE_WORK_STATUS=BLOCKED reason=recall-failed\n",
        };
    }
    out.push_str(
        "PREPARE_WORK_STATUS=READY (read any matched gates and required sources before acting)\n",
    );
    done(0, out)
}

fn root_arg(args: &Args) -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let root = args
        .value("Root")
        .map_or_else(|| top(&cwd), |r| Ok(PathBuf::from(r)))
        .map(|r| crate::args::absolute(&r))?;
    crate::hook::fresh_in_repo(&root)?;
    Ok(root)
}

/// `mdr-kernel hook-recall` (JSON on stdin, always exit 0) and `mdr-kernel prepare-work`.
pub fn adapter_cli(command: &str, argv: &[String]) -> i32 {
    if command == "hook-recall" {
        // A failure before recall still continues the prompt, visibly degraded.
        let wire = Args::parse(argv, &["Root"], &[])
            .and_then(|args| root_arg(&args))
            .map_or_else(
                |_| hook_degraded("adapter-error"),
                |root| match read_stdin(HOOK_PAYLOAD_LIMIT) {
                    Ok(raw) => hook(&root, &raw),
                    Err(()) => hook_degraded("adapter-error"),
                },
            );
        print!("{wire}");
        return 0;
    }
    let result = (|| -> Result<Outcome, String> {
        let args = Args::parse(argv, &["Objective", "Root"], &["FromStdin"])?;
        let objective = args.value("Objective");
        match (objective, args.switch("FromStdin")) {
            (Some(_), true) => return Err("Parameter set cannot be resolved using the specified named parameters.".into()),
            (None, false) => return Err("Cannot process command because of one or more missing mandatory parameters: Objective.".into()),
            (Some(""), false) => return Err("Cannot validate argument on parameter 'Objective'. The argument is null or empty.".into()),
            _ => {}
        }
        let root = root_arg(&args)?;
        let stdin = if args.switch("FromStdin") {
            Some(read_stdin(MAX_OBJECTIVE).map_err(|()| "could not read stdin".to_string())?)
        } else {
            None
        };
        Ok(prepare_work(&root, objective, stdin))
    })();
    match result {
        Ok(o) => {
            print!("{}", o.out);
            eprint!("{}", o.err);
            o.code
        }
        Err(e) => {
            eprintln!("X mdr-kernel prepare-work: {e}");
            1
        }
    }
}

pub fn cli(argv: &[String]) -> i32 {
    let result = (|| -> Result<Outcome, String> {
        let args = Args::parse(argv, &VALUES, &SWITCHES)?;
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        let root = args
            .value("Root")
            .map_or_else(|| top(&cwd), |r| Ok(PathBuf::from(r)))
            .map(|r| crate::args::absolute(&r))?;
        crate::hook::fresh_in_repo(&root)?;
        let stdin = if args.switch("FromStdin") {
            match read_stdin(MAX_OBJECTIVE) {
                Ok(text) => Some(text),
                Err(()) => return Ok(blocked("objective-too-large (maximum 131072 characters)")),
            }
        } else {
            None
        };
        Ok(run(&root, &args, stdin))
    })();
    match result {
        Ok(o) => {
            print!("{}", o.out);
            eprint!("{}", o.err);
            o.code
        }
        Err(e) => {
            eprintln!("X mdr-kernel brain-recall: {e}");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terms_match_whole_and_hyphen_aware() {
        assert!(term_hits("please deploy now", "deploy"));
        assert!(term_hits("(roll back) now", "roll back"));
        assert!(!term_hits("post-launch analysis", "launch analysis"));
        assert!(!term_hits("redeploy", "deploy"));
        assert!(!term_hits("deploy_x", "deploy"));
        assert!(term_hits("aaa deploy", "  Deploy "));
        assert!(!term_hits("anything", "  "));
    }

    #[test]
    fn schema_rejects_bad_types() {
        let good = serde_json::json!({"version": 1, "gates": []});
        assert!(validate(&good).is_ok());
        for bad in [
            serde_json::json!({"version": 1.0, "gates": []}),
            serde_json::json!({"version": 2, "gates": []}),
            serde_json::json!({"version": 1, "gates": {}}),
            serde_json::json!([]),
        ] {
            assert!(validate(&bad).is_err(), "{bad}");
        }
    }
}
