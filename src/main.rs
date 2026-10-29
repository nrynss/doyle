//! Doyle: truth maintenance for agent memory.

// The adapters have no in-crate user until P3 builds the real ones;
// the allow keeps the gate quiet until then.
#[allow(dead_code)]
mod adapters;

// The contract index has no in-crate user until T2.3 wires up `doyle check`;
// the allow keeps the gate quiet until then.
#[allow(dead_code)]
mod contract;

// The core has no in-crate user until T1.5 turns this into a library;
// the allow keeps the gate quiet until then.
#[allow(dead_code)]
mod core;

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
