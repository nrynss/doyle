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
    /// Diff two versions, flag cited memories, write proposals.
    Check {
        /// Old version.
        old: String,
        /// New version.
        new: String,
        /// Output format: markdown or json.
        format: String,
        /// Skip the re-check (no model calls).
        no_recheck: bool,
    },
    /// Compare a COBOL batch report with a rewrite report, flag cited
    /// memories, write flags.
    Diffrun {
        /// COBOL batch program (.cbl; copybooks resolve beside it).
        program: String,
        /// Run manifest JSON (inputs, reports, paths; see help).
        run: String,
        /// Output format: markdown or json.
        format: String,
        /// Skip the re-check (no model calls).
        no_recheck: bool,
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
    /// Dry run: print what would be cited, write nothing (curate).
    pub dry_run: bool,
    /// Use live model clients instead of replay (curate, check, diffrun).
    pub live: bool,
    /// Muse Code project root (curate, check, serve; default: the cwd).
    pub project: Option<String>,
    /// Contract file to curate against (curate; default: the demo v1 path).
    pub contract: Option<String>,
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
        "curate" | "cite" | "check" | "diffrun" | "serve" | "guard" => {}
        other => return ParseResult::Error(format!("doyle: unknown command: {other}")),
    }

    let command_name = args[0].clone();
    let rest = &args[1..];

    if rest.iter().any(|a| a == "-h" || a == "--help") {
        return ParseResult::Help(Some(command_name));
    }

    // curate/check/diffrun-specific: --live (curate, check, diffrun), --contract <FILE>
    // (curate). Other commands leave these in place and the shared parser
    // rejects them as unknown options.
    let (live, contract, command_rest) = match extract_command_flags(&command_name, rest) {
        Ok(v) => v,
        Err(e) => return ParseResult::Error(format!("doyle: {e}")),
    };

    // check/diffrun-specific: -f/--format <markdown|json>, --no-recheck
    let (format, no_recheck, parse_rest) = if command_name == "check" || command_name == "diffrun" {
        match extract_check_format(&command_rest) {
            Ok(v) => v,
            Err(e) => return ParseResult::Error(format!("doyle: {e}")),
        }
    } else {
        (String::from("markdown"), false, command_rest)
    };

    let (mut common, positionals) = match parse_common_and_positionals(&parse_rest) {
        Ok(v) => v,
        Err(e) => return ParseResult::Error(format!("doyle: {e}")),
    };
    common.live = live;
    common.contract = contract;

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
                    "doyle check: requires OLD and NEW version paths",
                ));
            }
            Command::Check {
                old: positionals[0].clone(),
                new: positionals[1].clone(),
                format,
                no_recheck,
            }
        }
        "diffrun" => {
            if positionals.len() != 2 {
                return ParseResult::Error(String::from(
                    "doyle diffrun: requires PROGRAM and RUN manifest paths",
                ));
            }
            Command::Diffrun {
                program: positionals[0].clone(),
                run: positionals[1].clone(),
                format,
                no_recheck,
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

fn extract_command_flags(
    command_name: &str,
    args: &[String],
) -> Result<(bool, Option<String>, Vec<String>), String> {
    let allow_live =
        command_name == "curate" || command_name == "check" || command_name == "diffrun";
    let allow_contract = command_name == "curate";
    let mut live = false;
    let mut contract: Option<String> = None;
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "--live" && allow_live {
            live = true;
        } else if (a == "--contract" || a.starts_with("--contract=")) && allow_contract {
            let value = if let Some(v) = a.strip_prefix("--contract=") {
                v.to_string()
            } else {
                i += 1;
                if i >= args.len() {
                    return Err(String::from("missing value for --contract"));
                }
                args[i].clone()
            };
            contract = Some(value);
        } else {
            out.push(a.clone());
        }
        i += 1;
    }
    Ok((live, contract, out))
}

fn extract_check_format(args: &[String]) -> Result<(String, bool, Vec<String>), String> {
    let mut format = String::from("markdown");
    let mut no_recheck = false;
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
        } else if a == "--no-recheck" {
            no_recheck = true;
        } else {
            out.push(a.clone());
        }
        i += 1;
    }
    Ok((format, no_recheck, out))
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
        } else if a == "--project" {
            i += 1;
            if i >= args.len() {
                return Err(String::from("missing value for --project"));
            }
            common.project = Some(args[i].clone());
        } else if let Some(value) = a.strip_prefix("--project=") {
            common.project = Some(value.to_string());
        } else if a == "--dry-run" {
            common.dry_run = true;
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
  check    Diff two versions, flag cited memories, run the re-check, write proposals
  diffrun  Compare a COBOL batch report with a rewrite report over the same inputs, flag cited memories, write flags
  serve    The review page and the judges' page
  guard    The Muse Code hook

Shared options:
  -s, --state-dir <DIR>   State directory (.doyle/ by default)
  -a, --adapters <LIST>   Adapters to use, comma-separated
      --project <DIR>     Muse Code project root (default: the working directory)
  -v, --verbose           Verbose output
  -h, --help              Print help
  -V, --version           Print version
";

/// Help for `doyle curate`.
pub const CURATE_HELP: &str = "\
doyle curate: read memories through the adapters, cite each against the current contract, write citations

Usage:
  doyle curate [OPTIONS]

Options:
      --live                Use live model clients (needs keys) instead of replay
      --contract <FILE>     Contract to curate against (default: testdata/contracts/patients-v1.yaml)

Shared options:
  -s, --state-dir <DIR>   State directory (.doyle/ by default)
  -a, --adapters <LIST>   Adapters to use, comma-separated
      --project <DIR>     Muse Code project root (default: the working directory)
  -v, --verbose           Verbose output
  --dry-run               Print what would be cited, write nothing
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
      --project <DIR>     Muse Code project root (default: the working directory)
  -v, --verbose           Verbose output
  -h, --help              Print help
";

/// Help for `doyle check`.
pub const CHECK_HELP: &str = "\
doyle check: diff two versions, flag cited memories, run the re-check, write proposals

Usage:
  doyle check <OLD> <NEW> [OPTIONS]

The source is picked from the file extensions: two `.cbl` programs diff
as legacy COBOL (copybooks resolve beside each program, as LegacyIndex
loads them); two `.json` files with a screens list diff as console
screens (the review page's version shape: console, screens, steps and
the recording); anything else diffs as API contracts. Flags go to the
same store through flag_citations either way, and the exit code says
whether anything was flagged, for CI.

The re-check proposes a correction, retirement or keep for every open
flag without a proposal yet: from recorded replies by default (no key,
no network; a flag with no recording gets one stderr line naming the
miss), live from Muse Spark with --live. --no-recheck stays silent and
makes no model calls.

Options:
  -f, --format <FORMAT>   Output format: markdown or json (default: markdown)
      --no-recheck         Skip the re-check (no model calls)
      --live                Run the re-check with live clients (needs keys) instead of replay
  -s, --state-dir <DIR>   State directory (.doyle/ by default)
  -a, --adapters <LIST>   Adapters to use, comma-separated
      --project <DIR>     Muse Code project root (default: the working directory)
  -v, --verbose           Verbose output
  -h, --help              Print help
";

/// Help for `doyle diffrun`.
pub const DIFRUN_HELP: &str = "\
doyle diffrun: compare a COBOL batch report with a rewrite report, flag cited memories, write flags

Usage:
  doyle diffrun <PROGRAM> <RUN> [OPTIONS]

Evidence by execution: PROGRAM is the COBOL batch (a `.cbl` path; its
paragraphs name the blamed elements through the legacy index), RUN is a
run manifest JSON naming the recorded inputs and both reports plus the
per-record paragraph paths (or a raw trace):

  {\"inputs\": \"patients.dat\", \"cobol_report\": \"report.txt\",
   \"rewrite_report\": \"rewrite.txt\", \"record_paths\": \"paths.txt\",
   \"rewrite_artifacts\": [\"wonder-bill.rs\", \"wonder-bill\", \"bin/billbatch\"]}

or with \"trace\", \"record_start\" and \"record_end\" instead of
\"record_paths\". \"rewrite_artifacts\" names the rewrite's own files
(the source, the built binary, the ship script/target): while the flag
is open the guard denies a tool call naming one of them. When the
manifest names no rewrite_artifacts, the flag is still written but
the guard cannot hold any step for it. Relative paths resolve beside
the manifest. Every
compared record must appear in the inputs, both reports and the paths;
any drift is an error, never a silent pass. The exit code says whether
anything was flagged, for CI. The flag's old version is PROGRAM and its
new version is RUN, the way the review page reads them.

Each mismatching record blames the paragraphs that ran only on
mismatched paths; a memory citing one is flagged with the inputs and
both outputs as its evidence. The re-check proposes a correction,
retirement or keep for every open flag without a proposal yet: from
recorded replies by default (no key, no network; a flag with no
recording gets one stderr line naming the miss), live from Muse Spark
with --live.

Options:
  -f, --format <FORMAT>   Output format: markdown or json (default: markdown)
      --no-recheck         Skip the re-check (no model calls)
      --live               Run the re-check with live clients (needs keys) instead of replay
  -s, --state-dir <DIR>   State directory (.doyle/ by default)
  -a, --adapters <LIST>   Adapters to use, comma-separated
      --project <DIR>     Muse Code project root (default: the working directory)
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
      --project <DIR>     Muse Code project root (default: the working directory)
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
      --project <DIR>     Muse Code project root (default: the working directory)
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
        Some("diffrun") => DIFRUN_HELP,
        Some("serve") => SERVE_HELP,
        Some("guard") => GUARD_HELP,
        _ => TOP_HELP,
    }
}

/// Exit code used by stub commands.
pub const STUB_EXIT_CODE: u8 = 2;
