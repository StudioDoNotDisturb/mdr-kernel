mod approval;
mod args;
mod close;
mod decisions;
mod doctor;
mod doors;
mod envelope;
mod find;
mod fingerprint;
mod git;
mod health;
mod hook;
mod least_code;
mod mcp;
mod node_map;
mod onboard;
mod recall;
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
       mdr-kernel state-budget [-Path <file>] [-MaxLines 90] [-MaxKB 10] [-Top 5] [-Detail]
       mdr-kernel secret-audit [-CheckScannerCoverage]
       mdr-kernel setup-private -Model <id> [-LocalOnly | -ConfirmPrivateRemote]
       mdr-kernel bootstrap-clone -Model <id> [-CommitScaffold]
       mdr-kernel brain-recall -Objective <text> | -FromStdin | -GateId <id> | -Ruling <id> | -CheckGates | -Increment <trap> [-Brief|-Json|-Header|-Quiet]
       mdr-kernel hook-recall (UserPromptSubmit JSON on stdin) | prepare-work -Objective <text> | -FromStdin
       mdr-kernel sync-doors [-Check [-Staged]]
       mdr-kernel check-decision-history [-Path <file>] [-FromIndex] [-BaselinePath <file>] [-AuthorityFrom <id>] [-RatificationsPath <file>] [-RatificationsBaselinePath <file>] [-HashOf <id>]
       mdr-kernel add-decision -Title <one line> -Body <text> | -BodyPath <file> [-Path <log>] | initialize-decision-authority [-PreserveLegacy]
       mdr-kernel build-rulings [-Check] [-FromIndex] [-DecisionsPath <file>] [-RatificationsPath <file>] [-GatesPath <file>] [-OutPath <file>]
       mdr-kernel ratify-decision -Id <D#> [-Quote <words>]   (operator terminal only)
       mdr-kernel write-approval -Propose | -Approve | -List | -Show | -ValidateStagedApprovals ...
       mdr-kernel update-node-map [-Check] [-Path <file> [-Repair [-Apply]]]
       mdr-kernel mcp   (read-only MCP server on stdio: recall_gates, get_gate_traps, find_history)
       mdr-kernel doctor [-HardOnly] [-StaleDays 14] [-StateMaxLines 90] [-StateMaxKB 10]
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
        Some((cmd, args)) if matches!(cmd.as_str(), "state-budget" | "secret-audit") => {
            health::cli(cmd, args)
        }
        Some((cmd, args)) if cmd == "doctor" => doctor::cli(args),
        Some((cmd, args)) if cmd == "mcp" => mcp::cli(args),
        Some((cmd, args)) if cmd == "write-approval" => approval::cli(args),
        Some((cmd, args)) if cmd == "ratify-decision" => decisions::ratify_cli(args),
        Some((cmd, args)) if cmd == "sync-doors" => doors::cli(args),
        Some((cmd, args))
            if matches!(
                cmd.as_str(),
                "add-decision" | "initialize-decision-authority"
            ) =>
        {
            decisions::write_cli(cmd, args)
        }
        Some((cmd, args)) if matches!(cmd.as_str(), "check-decision-history" | "build-rulings") => {
            decisions::cli(cmd, args)
        }
        Some((cmd, args)) if cmd == "update-node-map" => node_map::cli(args),
        Some((cmd, args)) if cmd == "brain-recall" => recall::cli(args),
        Some((cmd, args)) if matches!(cmd.as_str(), "hook-recall" | "prepare-work") => {
            recall::adapter_cli(cmd, args)
        }
        Some((cmd, args)) if matches!(cmd.as_str(), "setup-private" | "bootstrap-clone") => {
            onboard::cli(cmd, args)
        }
        Some((cmd, _)) if matches!(cmd.as_str(), "hook" | "install") => hook::cli(&argv),
        Some((cmd, _)) => {
            eprintln!("Unknown command: {cmd}\n{COMMANDS}");
            2
        }
    };
    std::process::exit(code);
}
