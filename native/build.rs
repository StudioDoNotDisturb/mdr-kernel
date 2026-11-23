// Embeds the binary's source fingerprint at build time (see src/fingerprint.rs).
include!("src/fingerprint.rs");

fn main() {
    let dir =
        std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let fingerprint = source_fingerprint(&dir).unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=MDR_SOURCE_FINGERPRINT={fingerprint}");
    for watched in ["src", "Cargo.toml", "Cargo.lock", "build.rs"] {
        println!("cargo:rerun-if-changed={watched}");
    }
}
