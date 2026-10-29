//! Stub for the `check` command; filled in by T5.2.

use std::process::ExitCode;

use crate::cli;

/// Run the `check` command.
pub fn run(_cli: &cli::Cli) -> ExitCode {
    eprintln!("doyle check: not implemented yet");
    ExitCode::from(2)
}
