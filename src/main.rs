//! Doyle: truth maintenance for agent memory.

use std::process::ExitCode;

const USAGE: &str = "\
doyle: truth maintenance for agent memory

Usage:
  doyle check <OLD> <NEW>   Flag the memories a contract change has made stale
  doyle serve               Run the review page
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("check") | Some("serve") => {
            eprintln!("doyle {}: not implemented yet", args[0]);
            ExitCode::from(2)
        }
        Some("-h") | Some("--help") => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some("-V") | Some("--version") => {
            println!("doyle {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        _ => {
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
