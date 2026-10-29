//! Command-line parsing for `doyle`. Hand-rolled, no external dependency.
//!
//! Shared options (state directory, adapters, verbosity) apply to every
//! command. Help text follows `docs/architecture.md`'s command table.

/// A parsed invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cli {
    /// The command to run.
    pub command: Command,
    /// Options shared across commands.
    pub common: CommonOpts,
}

/// One of `doyle`'s commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Read memories through the adapters and cite each against the contract.
    Curate,
    /// Record a citation by hand.
    Cite,
    /// Diff two contract versions, flag cited memories, write proposals.
    Check {
        /// Old contract version.
        old: String,
        /// New contract version.
        new: String,
        /// Output format: markdown or json.
        format: String,
    },
    /// Run the review page and the judges' page.
    Serve,
    /// Run the Muse Code hook.
    Guard,
}

/// Options shared by every command.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommonOpts {
    /// State directory (`.doyle/` by default).
    pub state_dir: Option<String>,
    /// Adapters to use (comma-separated).
    pub adapters: Vec<String>,
    /// Verbose output.
    pub verbose: bool,
}

/// The result of parsing the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseResult {
    /// Run this parsed invocation.
    Run(Cli),
    /// Print help (for the whole tool or one command) and exit successfully.
    Help(Option<String>),
    /// Print the version and exit successfully.
    Version,
    /// Print this error and exit with code 2.
    Error(String),
}

/// Parse the process arguments.
pub fn parse() -> ParseResult {
    let args: Vec<String> = std::env::args().skip(1).collect();
    parse_from(&args)
}

/// Parse an explicit argument list (for tests).
pub fn parse_from(args: &[String]) -> ParseResult {
    if args.is_empty() {
        return ParseResult::Help(None);
    }

    match args[0].as_str() {
        "-h" | "--help" => return ParseResult::Help(None),
        "-V" | "--version" => return ParseResult::Version,
        "curate" | "cite" | "check" | "serve" | "guard" => {}
        other => return ParseResult::Error(format!("doyle: unknown command: {other}")),
    }

    let command_name = args[0].clone();
    let rest = &args[1..];

    if rest.iter().any(|a| a == "-h" || a == "--help") {
        return ParseResult::Help(Some(command_name));
    }

    // check-specific: -f/--format <markdown|json>
    let (format, parse_rest) = if command_name == "check" {
        match extract_check_format(rest) {
            Ok(v) => v,
            Err(e) => return ParseResult::Error(format!("doyle: {e}")),
        }
    } else {
        (String::from("markdown"), rest.to_vec())
    };

    let (common, positionals) = match parse_common_and_positionals(&parse_rest) {
        Ok(v) => v,
        Err(e) => return ParseResult::Error(format!("doyle: {e}")),
    };

    let command = match command_name.as_str() {
        "curate" => {
            if !positionals.is_empty() {
                return ParseResult::Error(String::from(
                    "doyle curate: unexpected positional arguments",
                ));
            }
            Command::Curate
        }
        "cite" => {
            if !positionals.is_empty() {
                return ParseResult::Error(String::from(
                    "doyle cite: unexpected positional arguments",
                ));
            }
            Command::Cite
        }
        "check" => {
            if positionals.len() != 2 {
                return ParseResult::Error(String::from(
                    "doyle check: requires OLD and NEW contract paths",
                ));
            }
            Command::Check {
                old: positionals[0].clone(),
                new: positionals[1].clone(),
                format,
            }
        }
        "serve" => {
            if !positionals.is_empty() {
                return ParseResult::Error(String::from(
                    "doyle serve: unexpected positional arguments",
                ));
            }
            Command::Serve
        }
        "guard" => {
            if !positionals.is_empty() {
                return ParseResult::Error(String::from(
                    "doyle guard: unexpected positional arguments",
                ));
            }
            Command::Guard
        }
        _ => return ParseResult::Error(format!("doyle: unknown command: {command_name}")),
    };

    ParseResult::Run(Cli { command, common })
}

fn extract_check_format(args: &[String]) -> Result<(String, Vec<String>), String> {
    let mut format = String::from("markdown");
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "-f" || a == "--format" {
            i += 1;
            if i >= args.len() {
                return Err(String::from("missing value for --format"));
            }
            let v = args[i].clone();
            if v != "markdown" && v != "json" {
                return Err(format!("unknown format: {v}"));
            }
            format = v;
        } else if let Some(value) = a.strip_prefix("--format=") {
            if value != "markdown" && value != "json" {
                return Err(format!("unknown format: {value}"));
            }
            format = value.to_string();
        } else if let Some(value) = a.strip_prefix("-f=") {
            if value != "markdown" && value != "json" {
                return Err(format!("unknown format: {value}"));
            }
            format = value.to_string();
        } else {
            out.push(a.clone());
        }
        i += 1;
    }
    Ok((format, out))
}

fn parse_common_and_positionals(args: &[String]) -> Result<(CommonOpts, Vec<String>), String> {
    let mut common = CommonOpts::default();
    let mut positionals = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "--state-dir" || a == "-s" {
            i += 1;
            if i >= args.len() {
                return Err(String::from("missing value for --state-dir"));
            }
            common.state_dir = Some(args[i].clone());
        } else if let Some(value) = a.strip_prefix("--state-dir=") {
            common.state_dir = Some(value.to_string());
        } else if a == "--adapters" || a == "-a" {
            i += 1;
            if i >= args.len() {
                return Err(String::from("missing value for --adapters"));
            }
            for part in args[i].split(',') {
                let trimmed = part.trim();
                if !trimmed.is_empty() {
                    common.adapters.push(trimmed.to_string());
                }
            }
        } else if let Some(value) = a.strip_prefix("--adapters=") {
            for part in value.split(',') {
                let trimmed = part.trim();
                if !trimmed.is_empty() {
                    common.adapters.push(trimmed.to_string());
                }
            }
        } else if a == "--verbose" || a == "-v" {
            common.verbose = true;
        } else if a.starts_with('-') {
            return Err(format!("unknown option: {a}"));
        } else {
            positionals.push(a.clone());
        }
        i += 1;
    }
    Ok((common, positionals))
}

/// Top-level help text.
pub const TOP_HELP: &str = "\
doyle: truth maintenance for agent memory

Usage:
  doyle <COMMAND> [OPTIONS]

Commands:
  curate   Read memories through the adapters, cite each against the current contract, write citations
  cite     Record a citation by hand
  check    Diff two contract versions, flag cited memories, run the re-check, write proposals
  serve    The review page and the judges' page
  guard    The Muse Code hook

Shared options:
  -s, --state-dir <DIR>   State directory (.doyle/ by default)
  -a, --adapters <LIST>   Adapters to use, comma-separated
  -v, --verbose           Verbose output
  -h, --help              Print help
  -V, --version           Print version
";

/// Help for `doyle curate`.
pub const CURATE_HELP: &str = "\
doyle curate: read memories through the adapters, cite each against the current contract, write citations

Usage:
  doyle curate [OPTIONS]

Shared options:
  -s, --state-dir <DIR>   State directory (.doyle/ by default)
  -a, --adapters <LIST>   Adapters to use, comma-separated
  -v, --verbose           Verbose output
  -h, --help              Print help
";

/// Help for `doyle cite`.
pub const CITE_HELP: &str = "\
doyle cite: record a citation by hand

Usage:
  doyle cite [OPTIONS]

Shared options:
  -s, --state-dir <DIR>   State directory (.doyle/ by default)
  -a, --adapters <LIST>   Adapters to use, comma-separated
  -v, --verbose           Verbose output
  -h, --help              Print help
";

/// Help for `doyle check`.
pub const CHECK_HELP: &str = "\
doyle check: diff two contract versions, flag cited memories, run the re-check, write proposals

Usage:
  doyle check <OLD> <NEW> [OPTIONS]

Options:
  -f, --format <FORMAT>   Output format: markdown or json (default: markdown)
  -s, --state-dir <DIR>   State directory (.doyle/ by default)
  -a, --adapters <LIST>   Adapters to use, comma-separated
  -v, --verbose           Verbose output
  -h, --help              Print help
";

/// Help for `doyle serve`.
pub const SERVE_HELP: &str = "\
doyle serve: the review page and the judges' page

Usage:
  doyle serve [OPTIONS]

Shared options:
  -s, --state-dir <DIR>   State directory (.doyle/ by default)
  -a, --adapters <LIST>   Adapters to use, comma-separated
  -v, --verbose           Verbose output
  -h, --help              Print help
";

/// Help for `doyle guard`.
pub const GUARD_HELP: &str = "\
doyle guard: the Muse Code hook

Usage:
  doyle guard [OPTIONS]

Shared options:
  -s, --state-dir <DIR>   State directory (.doyle/ by default)
  -a, --adapters <LIST>   Adapters to use, comma-separated
  -v, --verbose           Verbose output
  -h, --help              Print help
";

/// Print the help text for `command` (or top-level when `None`).
pub fn help_text(command: Option<&str>) -> &'static str {
    match command {
        None => TOP_HELP,
        Some("curate") => CURATE_HELP,
        Some("cite") => CITE_HELP,
        Some("check") => CHECK_HELP,
        Some("serve") => SERVE_HELP,
        Some("guard") => GUARD_HELP,
        _ => TOP_HELP,
    }
}

/// Exit code used by stub commands.
pub const STUB_EXIT_CODE: u8 = 2;
