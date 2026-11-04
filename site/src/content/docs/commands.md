---
title: Commands reference
description: Verbatim CLI help output and exit codes for doyle and all seven subcommands.
---

This page provides the verbatim command help text and exit codes for the `doyle` executable.

## Exit codes

The executable signals outcome states through standard process exit codes:

- `0`: Success. The command completed without error and found no stale memories.
- `1`: Stale memories detected. Command `doyle check` or `doyle diffrun` found open flags on cited memories. CI pipelines rely on this code to halt builds.
- `2`: Failure. Doyle encountered a syntax error, unknown flag, missing input, or backend fault.

## Top-level CLI

Run `doyle --help` to inspect top-level commands and shared flags.

```text
doyle: truth maintenance for agent memory

Usage:
  doyle <COMMAND> [OPTIONS]

Commands:
  curate   Read memories through the adapters, cite each against the current contract, write citations
  cite     Record a citation by hand
  check    Diff two versions, flag cited memories, run the re-check, write proposals
  diffrun  Compare a COBOL batch report with a rewrite report over the same inputs, flag cited memories, write flags
  screens  Read a console version's screens and recording with the vision model, write the version file check reads
  serve    The review page and the judges' page
  guard    The Muse Code hook

Shared options:
  -s, --state-dir <DIR>   State directory (.doyle/ by default)
  -a, --adapters <LIST>   Adapters to use, comma-separated
      --project <DIR>     Muse Code project root (default: the working directory)
  -v, --verbose           Verbose output
  -h, --help              Print help
  -V, --version           Print version
```

## `doyle curate`

The `curate` command reads memories across registered adapters and asks the curator model to link them to contract entities.

```text
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
```

## `doyle cite`

The `cite` command records a citation manually from standard input when you want to bypass automated model curation.

```text
doyle cite: record a citation by hand

Usage:
  doyle cite [OPTIONS]

Shared options:
  -s, --state-dir <DIR>   State directory (.doyle/ by default)
  -a, --adapters <LIST>   Adapters to use, comma-separated
      --project <DIR>     Muse Code project root (default: the working directory)
  -v, --verbose           Verbose output
  -h, --help              Print help
```

## `doyle check`

The `check` command diffs two versions of a contract, COBOL program, or screen inventory.
It flags memories citing changed elements and generates fix proposals.

```text
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
```

## `doyle diffrun`

The `diffrun` command compares batch execution outputs between legacy COBOL and a modern rewrite.
It traces mismatched outputs back to source paragraphs and flags memories resting on them.

```text
doyle diffrun: compare a COBOL batch report with a rewrite report, flag cited memories, write flags

Usage:
  doyle diffrun <PROGRAM> <RUN> [OPTIONS]

Evidence by execution: PROGRAM is the COBOL batch (a `.cbl` path; its
paragraphs name the blamed elements through the legacy index), RUN is a
run manifest JSON naming the recorded inputs and both reports plus the
per-record paragraph paths (or a raw trace):

  {"inputs": "patients.dat", "cobol_report": "report.txt",
   "rewrite_report": "rewrite.txt", "record_paths": "paths.txt",
   "rewrite_artifacts": ["wonder-bill.rs", "wonder-bill", "bin/billbatch"]}

or with "trace", "record_start" and "record_end" instead of
"record_paths". "rewrite_artifacts" names the rewrite's own files
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
```

## `doyle screens read`

The `screens read` command calls the vision model to extract elements, positions, and steps from UI images and recordings.

```text
doyle screens: read a console version with the vision model

Usage:
  doyle screens read --console NAME --version V --screen NAME=PNG [--screen NAME=PNG ...]
                     [--recording NAME=MP4 --duration-s SECONDS] [--live] -o FILE

One muse-wonder-1.0 call reads the version's stills (and its training
recording, when given) into screens, elements and steps, and writes FILE in
the version shape `doyle check` and `doyle curate` read (console, screens,
steps, recording). The stated cost is printed before the call, the reply's
usage and every warning (for example dropped step-screen names) after it.

Without --live the reply is replayed from DOYLE_REPLAY_DIR (default
testdata/replies/vision); a miss is an error naming the request key and
writes nothing. With --live the call goes to the Meta Model API with the
key from MODEL_API_KEY or the keyring, and when DOYLE_RECORD_DIR is set the
reply is recorded there so it can be replayed later.

Options:
      --console <NAME>      The console (scopes every element ID)
      --version <V>         The version label (names the request)
      --screen <NAME=PNG>   A still frame; repeat for each screen
      --recording <NAME=MP4>  The training recording
      --duration-s <SECONDS>  The recording's duration (with --recording)
      --live                Call the live model instead of replaying
  -o, --output <FILE>       Where to write the version file
  -h, --help                Print help
```

## `doyle serve`

The `serve` command hosts the local human review interface and the judges evidence overview.

```text
doyle serve: the review page and the judges' page

Usage:
  doyle serve [OPTIONS]

Shared options:
  -s, --state-dir <DIR>   State directory (.doyle/ by default)
  -a, --adapters <LIST>   Adapters to use, comma-separated
      --project <DIR>     Muse Code project root (default: the working directory)
  -v, --verbose           Verbose output
  -h, --help              Print help
```

## `doyle guard`

The `guard` command executes as a Muse Code lifecycle hook to block tool calls that touch elements under open flags.

```text
doyle guard: the Muse Code hook

Usage:
  doyle guard [OPTIONS]

Shared options:
  -s, --state-dir <DIR>   State directory (.doyle/ by default)
  -a, --adapters <LIST>   Adapters to use, comma-separated
      --project <DIR>     Muse Code project root (default: the working directory)
  -v, --verbose           Verbose output
  -h, --help              Print help
```
