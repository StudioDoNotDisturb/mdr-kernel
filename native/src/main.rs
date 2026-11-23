mod args;
mod close;
mod envelope;
mod find;
mod fingerprint;
mod git;
mod hook;
mod least_code;
mod remote_privacy;
mod session;
mod state_lock;

/// `find` usage; `find --help` prints it.
pub const USAGE: &str =
    "Usage: mdr-kernel find --query <text> [--root <memory>] [--limit 1..10] [--json]";
const COMMANDS: &str =
    "Usage: mdr-kernel find --query <text> [--root <memory>] [--limit 1..10] [--json]
       mdr-kernel envelope check <file>... | staged | count [--list]
       mdr-kernel hook pre-commit | prepare-commit-msg <file> | commit-msg <file> | pre-push <remote> <url>
       mdr-kernel state-lock [-Acquire|-Release|-Heartbeat|-CheckOwner] [-Model <id>] ...
       mdr-kernel remote-privacy [-RemoteUrl <url>] [-Approve] [-RequireApproved] [-ConfirmPrivateRemote] [-Quiet]
       mdr-kernel start-session [-Model <id>] [-FullDoctor] | session-stop
       mdr-kernel close-session [-Topic <slug>|-NoHandoff] [-Model <id>] [-Token <t>] [-Also a,b] [-OfflineCheckpoint] [-DryRun] ...
       mdr-kernel sync-private [-PreserveWorkingTree]
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
        Some((cmd, args)) if cmd == "remote-privacy" => remote_privacy::cli(args),
        Some((cmd, args)) if matches!(cmd.as_str(), "start-session" | "session-stop") => {
            session::cli(cmd, args)
        }
        Some((cmd, args)) if matches!(cmd.as_str(), "close-session" | "sync-private") => {
            close::cli(cmd, args)
        }
        Some((cmd, _)) if matches!(cmd.as_str(), "hook" | "install") => hook::cli(&argv),
        Some((cmd, _)) => {
            eprintln!("Unknown command: {cmd}\n{COMMANDS}");
            2
        }
    };
    std::process::exit(code);
}
