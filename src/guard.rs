//! Stub for the `guard` command; filled in by T6.1.

use std::process::ExitCode;

use crate::cli;

/// Run the `guard` command.
pub fn run(_cli: &cli::Cli) -> ExitCode {
    eprintln!("doyle guard: not implemented yet");
    ExitCode::from(2)
}
