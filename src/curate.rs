//! Stub for the `curate` command; filled in by T5.1.

use std::process::ExitCode;

use crate::cli;

/// Run the `curate` command.
pub fn run(_cli: &cli::Cli) -> ExitCode {
    eprintln!("doyle curate: not implemented yet");
    ExitCode::from(2)
}
