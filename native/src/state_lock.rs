//! Structured advisory lock for coordination files. Port of `mdr-kernel state-lock`: the same JSON
//! body, identity rules, token sealing, `-Wait` queueing and recovery refusals, with the same
//! messages. `mdr-kernel state-lock` hands its arguments here when the binary is installed.

use crate::args::Args;
use crate::git::{common_top, git, lossy, top};
use regex::Regex;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const STALE_AFTER_MINUTES: i64 = 120;
pub const VALUES: [&str; 6] = [
    "AbandonmentReason",
    "TimeoutSec",
    "Model",
    "Token",
    "Objective",
    "Root",
];
pub const SWITCHES: [&str; 6] = [
    "Acquire",
    "Release",
    "Heartbeat",
    "CheckOwner",
    "Force",
    "Wait",
];

// ---------- time: .NET round-trip ("o") strings in UTC ----------

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

pub(crate) fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Seconds since the epoch, as f64, for `now`.
pub(crate) fn now_seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// `2026-11-23T18:13:44.2801234+00:00`, the shape PowerShell writes with `ToString('o')`.
pub(crate) fn iso(seconds: f64) -> String {
    let whole = seconds.floor() as i64;
    let ticks = ((seconds - whole as f64) * 1e7).floor() as i64;
    let (y, m, d) = civil_from_days(whole.div_euclid(86_400));
    let s = whole.rem_euclid(86_400);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{ticks:07}+00:00",
        s / 3600,
        s % 3600 / 60,
        s % 60
    )
}

/// Epoch seconds for an ISO 8601 date-time. A string with no offset is read as UTC.
pub(crate) fn parse_iso(text: &str) -> Option<f64> {
    static SHAPE: OnceLock<Regex> = OnceLock::new();
    let shape = SHAPE.get_or_init(|| {
        Regex::new(
            r"^(\d{4})-(\d{2})-(\d{2})[T ](\d{2}):(\d{2}):(\d{2})(\.\d+)?(Z|[+-]\d{2}:?\d{2})?$",
        )
        .unwrap()
    });
    let c = shape.captures(text.trim())?;
    let n = |i: usize| c[i].parse::<i64>().ok();
    let days = days_from_civil(n(1)?, n(2)?, n(3)?);
    let mut seconds = (days * 86_400 + n(4)? * 3600 + n(5)? * 60 + n(6)?) as f64;
    if let Some(fraction) = c.get(7) {
        seconds += format!("0{}", fraction.as_str()).parse::<f64>().ok()?;
    }
    if let Some(offset) = c.get(8).map(|m| m.as_str()).filter(|o| *o != "Z") {
        let digits: String = offset[1..].chars().filter(char::is_ascii_digit).collect();
        let minutes = digits[..2].parse::<i64>().ok()? * 60 + digits[2..].parse::<i64>().ok()?;
        seconds -= (if offset.starts_with('-') {
            -minutes
        } else {
            minutes
        } * 60) as f64;
    }
    Some(seconds)
}

// ---------- identity ----------

/// `[Environment]::MachineName`: the NetBIOS name on Windows, the host name before any dot elsewhere.
fn host_name() -> String {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetComputerNameW(buffer: *mut u16, size: *mut u32) -> i32;
        }
        let mut buffer = [0u16; 256];
        let mut size = buffer.len() as u32;
        // SAFETY: the buffer is writable for `size` UTF-16 units; nothing is retained.
        if unsafe { GetComputerNameW(buffer.as_mut_ptr(), &mut size) } != 0 {
            return String::from_utf16_lossy(&buffer[..size as usize]);
        }
        std::env::var("COMPUTERNAME").unwrap_or_default()
    }
    #[cfg(not(windows))]
    {
        let name = std::fs::read_to_string("/proc/sys/kernel/hostname")
            .ok()
            .or_else(|| {
                std::process::Command::new("hostname")
                    .output()
                    .ok()
                    .map(|o| lossy(o.stdout))
            })
            .unwrap_or_default();
        name.trim().split('.').next().unwrap_or("").to_string()
    }
}

/// 32 hex characters from the OS-seeded hasher keys (the shape of `[guid]::NewGuid().ToString('N')`).
fn new_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let part = || {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        );
        h.write_u32(std::process::id());
        h.finish()
    };
    format!("{:016x}{:016x}", part(), part())
}

fn trimmed(value: Option<&str>) -> Option<String> {
    value
        .filter(|v| !v.is_empty())
        .map(|v| v.trim().to_string())
}

fn env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|v| !v.is_empty())
        .map(|v| v.trim().to_string())
}

// ---------- the lock record ----------

#[derive(Default)]
struct Record {
    model: String,
    host: String,
    objective: String,
    acquired_at: String,
    heartbeat_at: String,
    base_commit: String,
    token: String,
}

fn text(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

/// None when the file is missing or empty; a legacy "model timestamp" body is tolerated.
fn read_record(path: &Path) -> Option<Record> {
    let raw = std::fs::read(path).ok().map(lossy)?;
    let raw = raw.trim_start_matches('\u{feff}').trim();
    if raw.is_empty() {
        return None;
    }
    if let Ok(value) = serde_json::from_str::<Value>(raw) {
        if value.is_object() {
            return Some(Record {
                model: text(&value, "model"),
                host: text(&value, "host"),
                objective: text(&value, "objective"),
                acquired_at: text(&value, "acquired_at"),
                heartbeat_at: text(&value, "heartbeat_at"),
                base_commit: text(&value, "base_commit"),
                token: text(&value, "token"),
            });
        }
    }
    let parts: Vec<&str> = raw.split_whitespace().collect();
    let last = parts.last().copied().unwrap_or("").to_string();
    Some(Record {
        model: parts.first().copied().unwrap_or("").to_string(),
        acquired_at: last.clone(),
        heartbeat_at: last,
        ..Record::default()
    })
}

fn age_minutes(record: &Record) -> Option<i64> {
    let stamp = if record.heartbeat_at.is_empty() {
        &record.acquired_at
    } else {
        &record.heartbeat_at
    };
    parse_iso(stamp).map(|t| ((now_seconds() - t) / 60.0).round_ties_even() as i64)
}

fn describe(record: &Record) -> String {
    let heartbeat = age_minutes(record).map_or("heartbeat unreadable".to_string(), |a| {
        format!("heartbeat {a}m ago")
    });
    let on = if record.host.is_empty() {
        String::new()
    } else {
        format!(" on {}", record.host)
    };
    let objective = if record.objective.is_empty() {
        "; objective: MISSING".to_string()
    } else {
        format!("; objective: {}", record.objective)
    };
    let acquired = parse_iso(&record.acquired_at).map_or_else(|| "?".to_string(), iso);
    format!(
        "{}{on} (acquired {acquired}, {heartbeat}, base {}{objective})",
        record.model, record.base_commit
    )
}

struct Session {
    root: PathBuf,
    lock: PathBuf,
    model: String,
    explicit: bool,
    host: String,
    token: Option<String>,
    objective: Option<String>,
}

impl Session {
    /// Same model (+host when recorded), compared case-insensitively as PowerShell's -eq does.
    fn base_match(&self, r: &Record) -> bool {
        r.model.to_lowercase() == self.model.to_lowercase()
            && (r.host.is_empty() || r.host.to_lowercase() == self.host.to_lowercase())
    }

    fn mine(&self, r: &Record) -> bool {
        self.base_match(r)
            && self.explicit
            && self
                .token
                .as_deref()
                .is_some_and(|t| !t.is_empty() && t == r.token)
    }

    fn other_session(&self, r: &Record) -> bool {
        self.base_match(r) && !self.mine(r)
    }

    fn body(&self, acquired_at: Option<&str>, token: &str, existing_objective: &str) -> String {
        let now = iso(now_seconds());
        let acquired = acquired_at
            .and_then(parse_iso)
            .map_or_else(|| now.clone(), iso);
        let base = git(&self.root, &["rev-parse", "--short", "HEAD"])
            .map(lossy)
            .unwrap_or_default();
        let objective = match self.objective.as_deref().filter(|o| !o.is_empty()) {
            Some(o) => Value::from(o),
            None if !existing_objective.is_empty() => Value::from(existing_objective),
            None => Value::Null,
        };
        let fields = [
            ("model", Value::from(self.model.as_str())),
            ("host", Value::from(self.host.as_str())),
            ("pid", Value::from(std::process::id())),
            ("objective", objective),
            ("acquired_at", Value::from(acquired)),
            ("heartbeat_at", Value::from(now)),
            ("base_commit", Value::from(base.trim())),
            ("token", Value::from(token)),
        ];
        let lines: Vec<String> = fields
            .iter()
            .map(|(k, v)| format!("  \"{k}\": {v}"))
            .collect();
        format!("{{\n{}\n}}", lines.join(",\n"))
    }
}

/// The report: exit code and the lines PowerShell would have written.
pub struct Report {
    pub code: i32,
    pub out: String,
}

pub fn run(cwd: &Path, args: &Args) -> Result<Report, String> {
    let root = match args.value("Root") {
        Some(r) => PathBuf::from(r),
        None => top(cwd)?,
    };
    let lock = common_top(&root)
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| root.clone())
        .join("STATE.lock");
    let (model, source) = if let Some(m) = trimmed(args.value("Model")).filter(|m| !m.is_empty()) {
        (m, "param")
    } else if let Some(m) = env("BRAIN_MODEL") {
        (m, "env")
    } else {
        match git(&root, &["config", "brain.model"]).map(lossy) {
            Ok(m) if !m.trim().is_empty() => (m.trim().to_string(), "config"),
            _ => (String::new(), "unknown"),
        }
    };
    let model = if model.is_empty() {
        "unknown".to_string()
    } else {
        model
    };
    let session = Session {
        explicit: matches!(source, "param" | "env"),
        host: host_name(),
        token: trimmed(args.value("Token")).or_else(|| env("BRAIN_LOCK_TOKEN")),
        objective: trimmed(args.value("Objective")).or_else(|| env("BRAIN_OBJECTIVE")),
        root,
        lock,
        model,
    };
    let mut out = Vec::new();
    let code = act(&session, args, source, &mut out);
    Ok(Report {
        code,
        out: out.join("\n") + if out.is_empty() { "" } else { "\n" },
    })
}

fn act(s: &Session, args: &Args, source: &str, out: &mut Vec<String>) -> i32 {
    let mut say = |line: String| out.push(line);
    let actions = ["Acquire", "Release", "Heartbeat", "CheckOwner"]
        .iter()
        .filter(|a| args.switch(a))
        .count();
    if actions > 1 || (args.switch("Force") && !args.switch("Release")) {
        say("state-lock: choose one action; -Force is only valid with -Release".into());
        return 1;
    }
    if actions > 0 && (!s.explicit || s.model == "unknown") {
        say(format!(
            "state-lock: refusing ownership operation — identity resolved from '{source}' ('{}'), not from this session. Pass -Model or set BRAIN_MODEL.",
            s.model
        ));
        return 1;
    }
    if args.switch("Acquire") {
        let token = s
            .token
            .clone()
            .filter(|t| !t.is_empty())
            .unwrap_or_else(new_token);
        let timeout = args
            .value("TimeoutSec")
            .and_then(|t| t.parse::<i64>().ok())
            .unwrap_or(300);
        let deadline = now_seconds() + timeout.max(5) as f64;
        let mut announced = false;
        loop {
            let created = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&s.lock);
            if let Ok(mut file) = created {
                use std::io::Write;
                if file.write_all(s.body(None, &token, "").as_bytes()).is_err() {
                    say("state-lock: could not write STATE.lock".into());
                    return 1;
                }
                say(format!("acquired STATE.lock as {}@{}", s.model, s.host));
                match s.objective.as_deref().filter(|o| !o.is_empty()) {
                    Some(o) => say(format!("objective: {o}")),
                    None => say("warning: no objective recorded; pass -Objective or set BRAIN_OBJECTIVE so parallel sessions know your scope".into()),
                }
                if s.token.as_deref().is_none_or(str::is_empty) {
                    say(format!("session token: {token}"));
                    say("to seal same-model concurrency, export it for this session:".into());
                    say(format!("  PowerShell:  $env:BRAIN_LOCK_TOKEN='{token}'"));
                    say(format!("  bash:        export BRAIN_LOCK_TOKEN={token}"));
                }
                return 0;
            }
            let record = read_record(&s.lock);
            if let Some(r) = record.as_ref().filter(|r| s.mine(r)) {
                if std::fs::write(
                    &s.lock,
                    s.body(Some(&r.acquired_at), &r.token, &r.objective),
                )
                .is_err()
                {
                    say("state-lock: could not refresh STATE.lock".into());
                    return 1;
                }
                say(format!(
                    "STATE.lock already yours ({}@{}) — heartbeat refreshed",
                    s.model, s.host
                ));
                return 0;
            }
            let shown = record
                .as_ref()
                .map_or_else(|| "(unreadable)".to_string(), describe);
            if args.switch("Wait") && now_seconds() < deadline {
                if !announced {
                    // Printed at once: a waiting caller should see why it waits.
                    println!("STATE.lock busy -> {shown}; polling every 5s (budget {timeout}s)...");
                    announced = true;
                }
                std::thread::sleep(Duration::from_secs(5));
                continue;
            }
            let age = record.as_ref().and_then(age_minutes);
            let stale = age.is_some_and(|a| a > STALE_AFTER_MINUTES);
            if record.as_ref().is_some_and(|r| s.other_session(r)) {
                say(format!(
                    "STATE.lock held by another {} session (token mismatch) -> {shown}",
                    s.model
                ));
                say("Same-model acquires are fail-closed: a live partner lock is never silently taken over.".into());
                say("  your own lock?      re-run with -Token <token>   (printed by YOUR -Acquire; if you cannot produce it, treat the lock as not yours)".into());
                say("  waiting your turn?  re-run with -Acquire -Wait   (polls every 5s until the holder releases)".into());
                if stale {
                    say(format!("  heartbeat is {}m old (stale past {STALE_AFTER_MINUTES}m), but abandonment is NOT PROVEN. Obtain independent evidence or operator direction before recovery.", age.unwrap_or_default()));
                } else {
                    let how_old =
                        age.map_or_else(|| "not readable as".to_string(), |a| format!("only {a}m"));
                    say(format!("  holder dead?        NOT PROVEN — heartbeat is {how_old} old. The holder is presumed ALIVE; -Release -Force refuses fresh-heartbeat locks. Queue with -Wait instead."));
                }
            } else {
                say(format!("STATE.lock already held -> {shown}"));
                if stale {
                    say(format!("heartbeat is {}m old — stale, not proof of abandonment. Obtain independent evidence or operator direction before recovery.", age.unwrap_or_default()));
                } else if !args.switch("Wait") {
                    say("to queue for it instead of failing: re-run with -Acquire -Wait".into());
                }
            }
            if args.switch("Wait") {
                say(format!(
                    "gave up after {timeout}s (-Wait budget); re-run to keep waiting"
                ));
            }
            return 1;
        }
    }
    let record = read_record(&s.lock);
    if args.switch("CheckOwner") {
        if record.as_ref().is_some_and(|r| s.mine(r)) {
            say("LOCK_OWNER=VERIFIED".into());
            return 0;
        }
        say("LOCK_OWNER=UNPROVEN — carry YOUR acquired token and explicit model identity; never copy a token from the lock file".into());
        return 1;
    }
    if args.switch("Heartbeat") {
        let Some(r) = record else {
            say("no STATE.lock to heartbeat".into());
            return 1;
        };
        if !s.mine(&r) {
            if s.other_session(&r) {
                say(format!(
                    "STATE.lock held by another {} session (token mismatch) — not refreshing",
                    s.model
                ));
            } else {
                say(format!(
                    "STATE.lock held by {}, not {} — not refreshing",
                    r.model, s.model
                ));
            }
            return 1;
        }
        if std::fs::write(
            &s.lock,
            s.body(Some(&r.acquired_at), &r.token, &r.objective),
        )
        .is_err()
        {
            say("state-lock: could not refresh STATE.lock".into());
            return 1;
        }
        say(format!("heartbeat refreshed ({}@{})", s.model, s.host));
        return 0;
    }
    if args.switch("Release") {
        if !s.lock.exists() {
            say("no STATE.lock to release".into());
            return 0;
        }
        if record.as_ref().is_some_and(|r| s.mine(r)) {
            return match std::fs::remove_file(&s.lock) {
                Ok(()) => {
                    say("released STATE.lock".into());
                    0
                }
                Err(e) => {
                    say(format!("state-lock: could not remove STATE.lock: {e}"));
                    1
                }
            };
        }
        if args.switch("Force") {
            let reason = args
                .value("AbandonmentReason")
                .unwrap_or("")
                .trim()
                .to_string();
            if reason.is_empty() {
                say("refusing -Release -Force: -AbandonmentReason must record independent evidence or operator direction. Age, model name, and a short-lived PID do not prove abandonment.".into());
                return 1;
            }
            let age = record.as_ref().and_then(age_minutes);
            if age.is_some_and(|a| a <= STALE_AFTER_MINUTES)
                && std::env::var("MDR_FORCE_LIVE_LOCK").as_deref() != Ok("1")
            {
                say(format!("refusing -Release -Force: heartbeat is only {}m old (stale threshold {STALE_AFTER_MINUTES}m) — the holder is presumed ALIVE:", age.unwrap_or_default()));
                say(format!(
                    "  {}",
                    record.as_ref().map_or_else(String::new, describe)
                ));
                say("  queue instead:      mdr-kernel state-lock -Acquire -Wait".into());
                say("  your own lock?      mdr-kernel state-lock -Release -Token <token>   (printed by YOUR -Acquire)".into());
                say("  abandonment independently established despite the fresh heartbeat? set MDR_FORCE_LIVE_LOCK=1 and supply -AbandonmentReason.".into());
                return 1;
            }
            if let Err(e) = std::fs::remove_file(&s.lock) {
                say(format!("state-lock: could not remove STATE.lock: {e}"));
                return 1;
            }
            say(format!(
                "FORCE-released STATE.lock. Recorded recovery reason: {}",
                args.value("AbandonmentReason").unwrap_or("")
            ));
            return 0;
        }
        if record.as_ref().is_some_and(|r| s.other_session(r)) {
            say(format!(
                "STATE.lock held by another {} session (token mismatch) — not releasing.",
                s.model
            ));
            say("Set BRAIN_LOCK_TOKEN / pass -Token <token> to release your own; recovery requires independent evidence or operator direction.".into());
            return 1;
        }
        let shown = record.as_ref().map_or_else(String::new, describe);
        say(format!(
            "STATE.lock held by {shown} — not yours ({}@{}).",
            s.model, s.host
        ));
        say("Wait for the holder. Forced recovery requires independent evidence or operator direction recorded with -AbandonmentReason.".into());
        return 1;
    }
    match record {
        Some(r) => {
            say(format!("STATE.lock held -> {}", describe(&r)));
            if let Some(age) = age_minutes(&r).filter(|a| *a > STALE_AFTER_MINUTES) {
                say(format!(
                    "heartbeat is {age}m old — stale, not proof of abandonment; see brain-doctor"
                ));
            }
        }
        None if s.lock.exists() => {
            say("STATE.lock: held but unreadable; ownership and abandonment are unproven".into())
        }
        None => say("STATE.lock: free".into()),
    }
    0
}

pub fn cli(argv: &[String]) -> i32 {
    let report = Args::parse(argv, &VALUES, &SWITCHES).and_then(|args| {
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        let root = args
            .value("Root")
            .map_or_else(|| top(&cwd), |r| Ok(PathBuf::from(r)))?;
        crate::hook::fresh(&root)?;
        run(&cwd, &args)
    });
    match report {
        Ok(r) => {
            print!("{}", r.out);
            r.code
        }
        Err(e) => {
            eprintln!("state-lock: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_round_trips_and_reads_offsets() {
        let t = 1_795_000_000.123_456_7;
        let text = iso(t);
        assert!(text.ends_with("+00:00") && text.contains('T'), "{text}");
        assert!((parse_iso(&text).unwrap() - t).abs() < 1e-6);
        assert_eq!(parse_iso("1970-01-01T00:00:00Z"), Some(0.0));
        assert_eq!(parse_iso("1970-01-01T01:00:00+01:00"), Some(0.0));
        assert_eq!(parse_iso("1970-01-01T00:00:00-0130"), Some(5400.0));
        assert_eq!(civil_from_days(days_from_civil(2024, 2, 29)), (2024, 2, 29));
        assert!(parse_iso("not a time").is_none());
    }

    #[test]
    fn legacy_and_json_records() {
        let dir = std::env::temp_dir().join(format!("mdr-lock-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("STATE.lock");
        std::fs::write(&path, "old-model 2026-01-01T00:00:00Z\n").unwrap();
        let r = read_record(&path).unwrap();
        assert_eq!(
            (r.model.as_str(), r.acquired_at.as_str()),
            ("old-model", "2026-01-01T00:00:00Z")
        );
        std::fs::write(
            &path,
            "{\"model\":\"m\",\"host\":null,\"token\":\"t\",\"pid\":5}",
        )
        .unwrap();
        let r = read_record(&path).unwrap();
        assert_eq!(
            (r.model.as_str(), r.host.as_str(), r.token.as_str()),
            ("m", "", "t")
        );
        std::fs::write(&path, "  ").unwrap();
        assert!(read_record(&path).is_none());
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(new_token().len(), 32);
        assert_ne!(new_token(), new_token());
    }
}
