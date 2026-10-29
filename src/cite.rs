//! Stub for the `cite` command; filled in by T5.3.

use std::process::ExitCode;

use crate::cli;

/// Run the `cite` command.
pub fn run(_cli: &cli::Cli) -> ExitCode {
    eprintln!("doyle cite: not implemented yet");
    ExitCode::from(2)
}
