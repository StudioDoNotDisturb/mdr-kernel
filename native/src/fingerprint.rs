// Shared with build.rs (included there): the SHA-256 of the binary's own compiler inputs. The hook
// compares the fingerprint embedded at build time with the staged sources, so a binary built from
// other sources never guards a commit.
use sha2::{Digest, Sha256};
use std::path::Path;

/// Repository-relative compiler inputs.
pub fn compiler_input(name: &str) -> bool {
    matches!(
        name,
        "native/Cargo.toml" | "native/Cargo.lock" | "native/build.rs"
    ) || (name.starts_with("native/src/") && name.ends_with(".rs"))
}

pub fn digest(files: &[(String, Vec<u8>)]) -> String {
    let mut ordered: Vec<_> = files.iter().collect();
    ordered.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hash = Sha256::new();
    for (name, bytes) in ordered {
        // A Windows checkout may use CRLF; a lone CR stays significant.
        let normalized: Vec<u8> = bytes
            .iter()
            .enumerate()
            .filter(|(i, b)| !(**b == b'\r' && bytes.get(i + 1) == Some(&b'\n')))
            .map(|(_, b)| *b)
            .collect();
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update((normalized.len() as u64).to_le_bytes());
        hash.update(normalized);
    }
    format!("{:x}", hash.finalize())
}

/// Fingerprint of the compiler inputs on disk under `native` (the crate directory).
#[allow(dead_code)] // used by build.rs only
pub fn source_fingerprint(native: &Path) -> Option<String> {
    fn walk(dir: &Path, rel: &str, out: &mut Vec<(String, Vec<u8>)>) -> Option<()> {
        for entry in std::fs::read_dir(dir).ok()? {
            let entry = entry.ok()?;
            let name = format!("{rel}/{}", entry.file_name().to_str()?);
            if entry.file_type().ok()?.is_dir() {
                walk(&entry.path(), &name, out)?;
            } else if compiler_input(&name) {
                out.push((name, std::fs::read(entry.path()).ok()?));
            }
        }
        Some(())
    }
    let mut files = Vec::new();
    for top in ["Cargo.toml", "Cargo.lock", "build.rs"] {
        files.push((
            format!("native/{top}"),
            std::fs::read(native.join(top)).ok()?,
        ));
    }
    walk(&native.join("src"), "native/src", &mut files)?;
    Some(digest(&files))
}
