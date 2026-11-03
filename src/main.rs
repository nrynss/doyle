//! Doyle: truth maintenance for agent memory.

use std::process::ExitCode;

use doyle_tms::cli;

fn main() -> ExitCode {
    match cli::parse() {
        cli::ParseResult::Help(command) => {
            print!("{}", cli::help_text(command.as_deref()));
            ExitCode::SUCCESS
        }
        cli::ParseResult::Version => {
            println!("doyle {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        cli::ParseResult::Error(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
        cli::ParseResult::Run(cli_value) => dispatch(cli_value),
    }
}

fn dispatch(cli_value: cli::Cli) -> ExitCode {
    match cli_value.command {
        cli::Command::Curate => doyle_tms::curate::run(&cli_value),
        cli::Command::Cite => doyle_tms::cite::run(&cli_value),
        cli::Command::Check { .. } => doyle_tms::check::run(&cli_value),
        cli::Command::Diffrun { .. } => doyle_tms::diffrun_cmd::run(&cli_value),
        cli::Command::Serve => doyle_tms::serve::run(&cli_value),
        cli::Command::Guard => doyle_tms::guard::run(&cli_value),
    }
}
