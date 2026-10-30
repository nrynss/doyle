//! Stub for the `serve` command; filled in by T6.2.

pub mod logo;

use std::process::ExitCode;

use crate::cli;

/// Run the `serve` command.
pub fn run(_cli: &cli::Cli) -> ExitCode {
    eprintln!("doyle serve: not implemented yet");
    ExitCode::from(2)
}
