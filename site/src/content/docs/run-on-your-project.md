---
title: Run on your own project
description: Install Doyle, configure keys and environment variables, curate memories, run checks in CI, and wire the Muse Code guard hook.
---

This guide shows how to install Doyle, configure credentials, curate agent memories, run checks in CI, and wire the runtime guard.

## Install

Doyle requires the toolchain and Rust edition pinned in [`rust-toolchain.toml`](https://github.com/nrynss/doyle/blob/main/rust-toolchain.toml).
Install Rust via `rustup` if you have not installed it already.

Clone the repository and build the release binary:

```bash
git clone https://github.com/nrynss/doyle.git
cd doyle
cargo build --release
```

Copy the compiled executable `target/release/doyle` onto your `PATH`.

```bash
install -m 755 target/release/doyle /usr/local/bin/doyle
```

Verify the installation by running the help flag:

```bash
doyle --help
```

## Keys

Doyle connects to model providers for curation and re-check tasks.
The tool reads two API keys when running in live mode:

- `BASETEN_API_KEY`: used by `doyle curate --live` to call the curator model on Baseten.
- `MODEL_API_KEY`: used by `doyle check --live`, `doyle diffrun --live`, and `doyle screens read --live` to call Meta models.

Doyle resolves keys through [`src/models/keys.rs`](https://github.com/nrynss/doyle/blob/main/src/models/keys.rs).
The loader checks environment variables first.
If a variable is unset, Doyle queries the system keyring via `secret-tool`:

```bash
secret-tool lookup service baseten
secret-tool lookup service meta-model-api
```

Keys are never stored in repository files or written into logs.
Doyle scrubs credential variables from sub-process environments before launching agent sessions.

## Configuration

There is no configuration file.
The file `.gitignore` lists `doyle.toml`, but nothing in the binary reads it.
Every setting comes directly from command-line flags or environment variables.

### Shared CLI flags

These options apply to all subcommands:

- `-s, --state-dir <DIR>`: state directory for citation and flag logs (default `.doyle`).
- `-a, --adapters <LIST>`: comma-separated list of adapters (default `muse-code,lambo`).
- `--project <DIR>`: project root directory for Muse Code note scanning (default current working directory).
- `-v, --verbose`: enables detailed status output.
- `-h, --help`: prints command help.
- `-V, --version`: prints binary version information.

### Subcommand flags

Specific commands accept targeted flags:

- `doyle curate`: `--contract <FILE>` specifies the contract to curate against. Flag `--dry-run` prints discovered citations without writing. Flag `--live` queries the curator model.
- `doyle check`: accepts two positional versions. Flag `--no-recheck` suppresses model proposals. Flag `--live` calls the live re-check model. Option `-f, --format <markdown|json>` selects output formatting.
- `doyle diffrun`: accepts a COBOL program and a run manifest. Flags `--no-recheck`, `--live`, and `-f, --format <markdown|json>` match `check`.
- `doyle screens read`: uses independent flag parsing in [`src/cli.rs`](https://github.com/nrynss/doyle/blob/main/src/cli.rs). Flags include `--console <NAME>`, `--version <V>`, repeatable `--screen <NAME=PNG>`, optional `--recording <NAME=MP4>`, optional `--duration-s <SECONDS>`, `--live`, and output path `-o, --output <FILE>`.

### Environment variables

Doyle inspects several environment variables:

- `BASETEN_API_KEY`: API key for Baseten model calls.
- `MODEL_API_KEY`: API key for Meta Model API calls.
- `LAMBO_URL`: URL for [Lambo](https://github.com/nrynss/lambo) MCP streamable HTTP (default `http://127.0.0.1:7700/mcp/s/doyle`).
- `DOYLE_SERVE_PORT`: port for `doyle serve` (default `8080`).
- `DOYLE_REPLAY_DIR`: directory containing recorded model responses.
- `DOYLE_RECORD_DIR`: directory where live model responses are saved.
- `DOYLE_RECHECK_REPLAY_DIR`: directory containing recorded re-check replies.

The manual citation command `doyle cite` also checks fallback environment variables defined in [`src/cite.rs`](https://github.com/nrynss/doyle/blob/main/src/cite.rs):

- `DOYLE_CITE_ADAPTER`: default adapter name.
- `DOYLE_CITE_MEMORY`: target memory identifier.
- `DOYLE_CITE_CONTRACT`: path to the referenced contract.
- `DOYLE_CITE_ELEMENT`: comma-separated element identifiers or native locations.
- `DOYLE_CITE_CLAIM`: claim text describing the memory belief.
- `DOYLE_CITE_RECORDED_BY`: author attribution string.
- `DOYLE_CITE_CONFIDENCE`: float confidence score between 0.0 and 1.0.

## Curating existing memories

To index beliefs from existing memories, point `doyle curate` at your API contract:

```bash
doyle curate --contract api/v1.yaml --live
```

You can also curate against a legacy COBOL program or a screen inventory file:

```bash
doyle curate --contract legacy/BILLBATCH.cbl --live
doyle curate --contract screens-v1.json --live
```

The command scans memory systems registered in `--adapters`.
It asks the curator model to identify which contract entities each memory describes.
It then appends citation records to the local state directory.

## Putting `doyle check` in CI

Run `doyle check` in CI pipelines to prevent code from shipping against broken beliefs.
Add this step to your GitHub Actions workflow:

```yaml
- name: Run Doyle contract check
  run: |
    doyle check api/v1.yaml api/v2.yaml --no-recheck
```

The `--no-recheck` flag skips model proposal generation.
The command diffs the contracts and evaluates stored citations.
If any cited element has changed, `doyle check` exits with code `1`, failing the job immediately.

## Running `doyle diffrun`

The `doyle diffrun` command compares execution outputs between a legacy COBOL program and a rewrite.
It requires a JSON manifest describing the run:

```json
{
  "inputs": "patients.dat",
  "cobol_report": "report.txt",
  "rewrite_report": "rewrite.txt",
  "record_paths": "paths.txt",
  "rewrite_artifacts": [
    "wonder-bill.rs",
    "wonder-bill",
    "bin/billbatch"
  ]
}
```

The `rewrite_artifacts` array lists source files and compiled binaries for the rewrite.
When execution produces output discrepancies, Doyle flags the legacy paragraphs responsible.
The guard uses `rewrite_artifacts` to block agent tools from compiling or shipping those files while the flag stays open.

Run the differential check:

```bash
doyle diffrun legacy/BILLBATCH.cbl run-manifest.json --live
```

## Reading screens and video

When changes affect visual interfaces, use `doyle screens read` to parse screenshots and recordings:

```bash
doyle screens read \
  --console billing \
  --version v1 \
  --screen menu=screens/menu-v1.png \
  --screen lookup=screens/lookup-v1.png \
  --recording training=video/operator-guide.mp4 \
  --duration-s 45 \
  --live \
  -o screens-v1.json
```

Generate a second inventory file for the redesigned screens:

```bash
doyle screens read \
  --console billing \
  --version v2 \
  --screen menu=screens/menu-v2.png \
  --screen lookup=screens/lookup-v2.png \
  --live \
  -o screens-v2.json
```

Diff both screen inventory files with `doyle check`:

```bash
doyle check screens-v1.json screens-v2.json --live
```

Doyle flags any memories citing altered screen elements or removed confirmation steps.

## Wiring `.muse/hooks.json`

To block agent actions on stale memories, configure Muse Code hooks.
Create or edit `.muse/hooks.json` inside your project directory:

```json
{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "/abs/path/to/doyle guard --state-dir /abs/path/to/.doyle"
          }
        ]
      }
    ],
    "PreToolUse": [
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "/abs/path/to/doyle guard --state-dir /abs/path/to/.doyle"
          }
        ]
      }
    ]
  }
}
```

Both paths must be absolute so the hook resolves correctly when the agent changes its working directory.
Muse Code loads project hooks only in trusted workspaces.
Ensure you run `muse` with `--trust-workspace` or trust the directory beforehand.

## Running `doyle serve` and Lambo

Launch the review server to inspect flags and record human decisions:

```bash
doyle serve --state-dir .doyle
```

On startup, `doyle serve` prints a secret reviewer link to standard error:

```text
Review at http://127.0.0.1:8080/?reviewer=<token>
```

Open this link in a browser to review proposals and approve corrections.
To connect Doyle to a running Lambo memory service, pass the endpoint URL via `LAMBO_URL`:

```bash
export LAMBO_URL="http://127.0.0.1:7700/mcp/s/doyle"
doyle serve --state-dir .doyle
```

Approved decisions update notes in Muse Code and record contradiction links in Lambo.
