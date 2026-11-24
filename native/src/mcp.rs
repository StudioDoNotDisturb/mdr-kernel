//! The built-in MCP server: read-only, typed access to recall and history search for a host that
//! has no shell. Newline-delimited JSON-RPC 2.0 on stdio. Every tool answers in process from this
//! binary, so the server cannot report healthy while a backend is missing, and nothing floats with
//! an upstream publish: the tools are pinned to the build.
//!
//!   mdr-kernel mcp [-Root <memory>]

use crate::args::Args;
use crate::git::top;
use serde_json::{json, Map, Value};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

const PROTOCOLS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
/// Characters returned to the model per call.
const OUTPUT_LIMIT: usize = 24_000;
const INSTRUCTIONS: &str = "Read-only access to this memory's recall gates and history search. Gates and STATE.md govern; a tool result is evidence, not a ruling.";

fn tools() -> Value {
    json!([
        {
            "name": "recall_gates",
            "description": "Match an objective (the operator's request, in their words) against this memory's recall gates. Returns the matched gates with their required reads and known traps. CLEAR is not a green light.",
            "inputSchema": {
                "type": "object",
                "properties": { "objective": { "type": "string", "maxLength": 8000, "description": "What is about to be done, in plain words." } },
                "required": ["objective"],
                "additionalProperties": false
            }
        },
        {
            "name": "get_gate_traps",
            "description": "Print one recall gate in full by id: every trap, required read, ruling and stop sign.",
            "inputSchema": {
                "type": "object",
                "properties": { "gate_id": { "type": "string", "pattern": "^[A-Za-z0-9][A-Za-z0-9_-]{0,79}$", "description": "Gate id as shown in a recall banner." } },
                "required": ["gate_id"],
                "additionalProperties": false
            }
        },
        {
            "name": "find_history",
            "description": "Lexical search over STATE, the decision log and handoffs. Returns evidence pointers (path:line), not answers; STATE and the rule files govern.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "maxLength": 4096, "description": "Distinctive words: a project name, a tool, a ruling id." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 10, "description": "Rows to return (default 5)." }
                },
                "required": ["query"],
                "additionalProperties": false
            }
        }
    ])
}

fn text_arg(input: &Map<String, Value>, name: &str, max: usize) -> Result<String, String> {
    let value = input
        .get(name)
        .and_then(Value::as_str)
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| format!("{name} must be a non-empty string"))?;
    if value.encode_utf16().count() > max {
        return Err(format!("{name} exceeds {max} characters"));
    }
    Ok(value.to_string())
}

fn clip(body: &str) -> String {
    let body = body.trim_end();
    let count = body.chars().count();
    if count <= OUTPUT_LIMIT {
        return body.to_string();
    }
    let shown: String = body.chars().take(OUTPUT_LIMIT).collect();
    format!(
        "{shown}\n[truncated: {} more characters; narrow the request]",
        count - OUTPUT_LIMIT
    )
}

/// One recall call in process: (ok, text).
fn recall(root: &Path, argv: Vec<String>) -> (bool, String) {
    let parsed = Args::parse(
        &argv,
        &[
            "Objective",
            "GatesPath",
            "Ruling",
            "GateId",
            "Increment",
            "Root",
        ],
        &[
            "FromStdin",
            "Brief",
            "CheckGates",
            "Json",
            "Quiet",
            "Header",
        ],
    );
    let o = match parsed {
        Ok(args) => crate::recall::run(root, &args, None),
        Err(e) => return (false, e),
    };
    if o.code == 0 {
        return (true, clip(&o.out));
    }
    let parts: Vec<String> = [
        o.out.trim().to_string(),
        o.err.trim().to_string(),
        format!("(exit {})", o.code),
    ]
    .into_iter()
    .filter(|p| !p.is_empty())
    .collect();
    (false, clip(&parts.join("\n")))
}

/// Run one tool. `Err` is an input error; `Ok((ok, text))` is the tool's own result.
fn call(root: &Path, name: &str, input: &Map<String, Value>) -> Result<(bool, String), String> {
    let allowed: &[&str] = match name {
        "recall_gates" => &["objective"],
        "get_gate_traps" => &["gate_id"],
        "find_history" => &["query", "limit"],
        _ => return Err(format!("Unknown tool: {name}")),
    };
    let extra: Vec<&str> = input
        .keys()
        .map(String::as_str)
        .filter(|k| !allowed.contains(k))
        .collect();
    if !extra.is_empty() {
        return Err(format!("Unknown argument: {}", extra.join(", ")));
    }
    // A binary built from other native/ sources than the indexed ones must not answer.
    crate::hook::fresh_in_repo(root)?;
    match name {
        "recall_gates" => {
            let objective = text_arg(input, "objective", 8000)?;
            Ok(recall(root, vec![format!("-Objective:{objective}")]))
        }
        "get_gate_traps" => {
            let id = text_arg(input, "gate_id", 80)?;
            if !regex::Regex::new(r"^[A-Za-z0-9][A-Za-z0-9_-]{0,79}$")
                .unwrap()
                .is_match(&id)
            {
                return Err("gate_id is malformed: letters, digits, _ and - only, starting with a letter or digit".into());
            }
            Ok(recall(root, vec![format!("-GateId:{id}")]))
        }
        _ => {
            let query = text_arg(input, "query", 4096)?;
            let limit = match input.get("limit") {
                None => 5,
                Some(v) => v
                    .as_u64()
                    .filter(|n| (1..=10).contains(n))
                    .ok_or("limit must be an integer from 1 to 10")?
                    as usize,
            };
            match crate::find::search(&query, limit, root, None) {
                Ok(found) => Ok((true, clip(&crate::find::render(&found.rows).join("\n")))),
                Err(e) => Ok((false, clip(&e))),
            }
        }
    }
}

/// One JSON-RPC message in, at most one reply out (a notification gets none).
pub fn handle(root: &Path, message: &Value) -> Option<Value> {
    let Some(object) = message.as_object() else {
        return Some(
            json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32600, "message": "Invalid request: expected a JSON object"}}),
        );
    };
    let id = object.get("id").cloned();
    let fail = |code: i64, text: String| {
        Some(
            json!({"jsonrpc": "2.0", "id": id.clone().unwrap_or(Value::Null), "error": {"code": code, "message": text}}),
        )
    };
    let Some(method) = object.get("method").and_then(Value::as_str) else {
        return fail(-32600, "Invalid request".into());
    };
    let id = id.clone()?; // notifications (initialized, cancelled) need no reply
    let params = object.get("params").cloned().unwrap_or_else(|| json!({}));
    let reply = |result: Value| Some(json!({"jsonrpc": "2.0", "id": id, "result": result}));
    match method {
        "initialize" => {
            let asked = params["protocolVersion"].as_str().unwrap_or("");
            let version = PROTOCOLS
                .iter()
                .find(|p| **p == asked)
                .unwrap_or(&PROTOCOLS[0]);
            reply(json!({
                "protocolVersion": version,
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "mdr-kernel", "version": env!("CARGO_PKG_VERSION")},
                "instructions": INSTRUCTIONS,
            }))
        }
        "ping" => reply(json!({})),
        "tools/list" => reply(json!({"tools": tools()})),
        "tools/call" => {
            let name = params["name"].as_str().unwrap_or("");
            if !matches!(name, "recall_gates" | "get_gate_traps" | "find_history") {
                return fail(-32602, format!("Unknown tool: {}", params["name"]));
            }
            let empty = Map::new();
            let input = match params.get("arguments") {
                None | Some(Value::Null) => &empty,
                Some(Value::Object(m)) => m,
                Some(_) => {
                    return reply(
                        json!({"content": [{"type": "text", "text": "arguments must be an object"}], "isError": true}),
                    )
                }
            };
            let (ok, text) = call(root, name, input).unwrap_or_else(|e| (false, e));
            reply(json!({"content": [{"type": "text", "text": text}], "isError": !ok}))
        }
        other => fail(-32601, format!("Method not found: {other}")),
    }
}

pub fn cli(argv: &[String]) -> i32 {
    let root = (|| -> Result<PathBuf, String> {
        let args = Args::parse(argv, &["Root"], &[])?;
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        args.value("Root")
            .map_or_else(|| top(&cwd), |r| Ok(PathBuf::from(r)))
            .map(|r| crate::args::absolute(&r))
    })();
    let root = match root {
        Ok(r) => r,
        Err(e) => {
            eprintln!("X mdr-kernel mcp: {e}");
            return 2;
        }
    };
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(line) {
            Ok(message) => handle(&root, &message),
            Err(_) => Some(
                json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": "Parse error"}}),
            ),
        };
        if let Some(response) = response {
            if writeln!(stdout, "{response}")
                .and_then(|()| stdout.flush())
                .is_err()
            {
                break;
            }
        }
    }
    0
}
