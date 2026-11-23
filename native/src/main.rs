mod args;
mod find;

pub const USAGE: &str =
    "Usage: mdr-kernel find --query <text> [--root <memory>] [--limit 1..10] [--json]";

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    match argv.split_first() {
        None => println!("{USAGE}"),
        Some((cmd, [])) if matches!(cmd.as_str(), "--help" | "-h" | "help") => println!("{USAGE}"),
        Some((cmd, [])) if cmd == "--version" => {
            println!("mdr-kernel {}", env!("CARGO_PKG_VERSION"))
        }
        Some((cmd, args)) if cmd == "find" => std::process::exit(find::cli(args)),
        Some((cmd, _)) => {
            eprintln!("Unknown command: {cmd}\n{USAGE}");
            std::process::exit(2);
        }
    }
}
