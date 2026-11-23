mod args;
mod envelope;
mod find;
mod fingerprint;
mod git;
mod hook;
mod least_code;
mod state_lock;

/// `find` usage; `find --help` prints it.
pub const USAGE: &str =
    "Usage: mdr-kernel find --query <text> [--root <memory>] [--limit 1..10] [--json]";
const COMMANDS: &str =
    "Usage: mdr-kernel find --query <text> [--root <memory>] [--limit 1..10] [--json]
       mdr-kernel envelope check <file>... | staged | count [--list]
       mdr-kernel hook pre-commit | prepare-commit-msg <file> | commit-msg <file>
       mdr-kernel state-lock [-Acquire|-Release|-Heartbeat|-CheckOwner] [-Model <id>] ...
       mdr-kernel install";

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let code = match argv.split_first() {
        None => {
            println!("{COMMANDS}");
            0
        }
        Some((cmd, [])) if matches!(cmd.as_str(), "--help" | "-h" | "help") => {
            println!("{COMMANDS}");
            0
        }
        Some((cmd, [])) if cmd == "--version" => {
            println!("mdr-kernel {}", env!("CARGO_PKG_VERSION"));
            0
        }
        Some((cmd, args)) if cmd == "find" => find::cli(args),
        Some((cmd, args)) if cmd == "envelope" => envelope::cli(args),
        Some((cmd, args)) if cmd == "state-lock" => state_lock::cli(args),
        Some((cmd, _)) if matches!(cmd.as_str(), "hook" | "install") => hook::cli(&argv),
        Some((cmd, _)) => {
            eprintln!("Unknown command: {cmd}\n{COMMANDS}");
            2
        }
    };
    std::process::exit(code);
}
