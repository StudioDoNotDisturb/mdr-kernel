//! Options and memory-root discovery for the history search CLI.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct Args {
    values: HashMap<String, String>,
    switches: HashMap<String, bool>,
}

fn norm(name: &str) -> String {
    name.trim_start_matches('-')
        .chars()
        .filter(|c| *c != '-' && *c != '_')
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn truthy(v: &str) -> bool {
    !matches!(
        v.trim().trim_start_matches('$').to_lowercase().as_str(),
        "false" | "0" | ""
    )
}

impl Args {
    /// `value_params` take the next argument as their value; `switch_params` are booleans.
    pub fn parse(
        argv: &[String],
        value_params: &[&str],
        switch_params: &[&str],
    ) -> Result<Args, String> {
        let values_known: Vec<String> = value_params.iter().map(|n| norm(n)).collect();
        let switches_known: Vec<String> = switch_params.iter().map(|n| norm(n)).collect();
        let mut values = HashMap::new();
        let mut switches = HashMap::new();
        let mut i = 0;
        while i < argv.len() {
            let raw = &argv[i];
            if !raw.starts_with('-') || raw == "-" {
                return Err(format!("Unexpected positional argument: {raw}"));
            }
            let (name_part, inline) = match raw.split_once(':') {
                Some((n, v))
                    if switches_known.contains(&norm(n)) || values_known.contains(&norm(n)) =>
                {
                    (n, Some(v))
                }
                _ => match raw.split_once('=') {
                    Some((n, v)) if raw.starts_with("--") => (n, Some(v)),
                    _ => (raw.as_str(), None),
                },
            };
            let name = norm(name_part);
            if switches_known.contains(&name) {
                switches.insert(name, inline.map(truthy).unwrap_or(true));
            } else if values_known.contains(&name) {
                let value = match inline {
                    Some(v) => v.to_string(),
                    None => {
                        i += 1;
                        match argv.get(i) {
                            Some(next) if next.starts_with("--") => {
                                return Err(format!(
                                    "Missing value for {raw} (next is the option {next}; write {raw}={next} to use it as the value)"
                                ))
                            }
                            next => next.cloned().ok_or_else(|| format!("Missing value for {raw}"))?,
                        }
                    }
                };
                values.insert(name, value);
            } else {
                return Err(format!("Unknown parameter: {raw}"));
            }
            i += 1;
        }
        Ok(Args { values, switches })
    }

    pub fn value(&self, name: &str) -> Option<&str> {
        self.values.get(&norm(name)).map(|s| s.as_str())
    }

    pub fn switch(&self, name: &str) -> bool {
        self.switches.get(&norm(name)).copied().unwrap_or(false)
    }

    pub fn root(&self) -> Result<PathBuf, String> {
        let root = match self.value("root") {
            Some(root) => PathBuf::from(root),
            None => std::env::current_dir()
                .map_err(|e| e.to_string())?
                .ancestors()
                .find(|p| p.join(".brain-root").is_file())
                .map(Path::to_path_buf)
                .ok_or("Cannot locate a configured memory. Run setup or pass --root.")?,
        };
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        if !root.join(".brain-root").is_file() {
            return Err("Selected root is missing .brain-root. Run private setup first.".into());
        }
        Ok(root)
    }
}

/// Full path with native separators and no trailing separator (the .NET `GetFullPath` shape).
pub fn absolute(p: &Path) -> PathBuf {
    let full = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let s = full.to_string_lossy();
    let trimmed = s.trim_end_matches(['\\', '/']);
    if trimmed.len() < 3 {
        full.clone()
    } else {
        PathBuf::from(trimmed)
    }
}
