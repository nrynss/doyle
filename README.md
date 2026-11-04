# Doyle

**Truth maintenance for agent memory.**

Every memory an agent keeps rests on something outside it: an API contract, a schema, a dependency version. When that changes, the memory goes stale, and the agent keeps acting on it. Doyle records what each memory rests on. When a source changes, Doyle flags every memory resting on it, has a model propose a correction, sends the correction to a person for sign-off, and keeps agents from acting on the stale version until it is settled.

The demo story is the Quiet Migration (`product.md`, `evidence/`): a veterinary clinic chain's 4,812 patient records, a stale `GET /patients` memory from an earlier session, and a vendor API that now pages at 100. The recorded runs show what happened with Doyle (the guard held the export, a person approved, the correction landed in both memory systems) and without it (the agent probed past the stale memory both times, so 0 records were lost in any run — and the stale belief survived in memory). The numbers below are the measured ones, never more.

> **Status:** entry for the Meta Global AI Developer Hackathon (Indie track). The binary builds, the gate passes keyless, and the demo below runs fully offline from a fresh clone. Live model calls, live services and the video are separate (below).

## What is new here, and what is not

The hackathon's new-work rule draws this line, and so does this file. Everything presented as built during the event is Doyle; everything else is an existing dependency, never vendored or copied.

**Built here (Doyle, this repository):** the `doyle` binary and all of its behaviour — the citation index, flagging, proposals, sign-off, the review and judges' pages, the Muse Code hook, the curator and re-check prompts, the adapters' Doyle-side code, and every fixture and recording under `testdata/` and `evidence/`.

**Existing dependencies (not built here):**

| Dependency | What it is | How Doyle uses it |
|---|---|---|
| [Vimanam](https://github.com/noemaforge/vimanam) | An existing API-contract library (crates.io `vimanam` 2.x) | Used as a library with default features off: the contract model, stable element IDs (`ve2_…`), and the diff. Doyle links it; no code is copied |
| Lambo | An existing open-source memory system | Reached as a client of a running Lambo server over MCP. Doyle records corrections `against` stale memories; Lambo itself is untouched |
| Muse Code built-in memory | The memory the demo agent already writes and recalls | Read and annotated in place through an adapter; owned by Muse, not by Doyle |
| Muse Wonder (`muse-wonder-1.0`) | The coding agent in the demo | Runs the migration. Doyle never migrates; it keeps what Wonder believes true |
| Meta Model API (Muse Spark, the re-check) and Baseten (Muse Spark Open, the curator) | Model endpoints | Called live only with `--live` and keys; offline they are replaced by committed recordings |

## Run the offline demo

From a fresh clone, with no keys, no network (after the first build) and no running server. Install Rust via [rustup](https://rustup.rs) first; `rust-toolchain.toml` pins the toolchain and `rustup` installs it on first use. The first `cargo build` downloads crates from crates.io once; everything after that is offline.

```sh
git clone <this-repo> doyle && cd doyle
```

### 1. The gate

```sh
./tools/check.sh
```

Runs `rustfmt --check`, `clippy` with warnings denied, `cargo build` and the full test suite. It needs no key, no network and no running server: tests run on `testdata/` and recorded replies. Live tests are `#[ignore]`d and never run here.

### 2. Build

```sh
cargo build --locked
```

The binary is `target/debug/doyle`. `doyle --help` lists the seven commands; each takes `--help`.

### 3. Record what a memory rests on, then flag it (`cite`, `check`)

These two commands are the core loop, run from the repository root against a scratch state directory so the clone stays clean:

```sh
S=/tmp/doyle-demo-state && rm -rf $S
echo '{"adapter": "muse-code", "memory_id": "M1", "contract": "testdata/contracts/patients-v1.yaml", "elements": ["GET /patients"], "claim": "GET /patients returns every patient in one response", "recorded_by": "manual"}' \
  | ./target/debug/doyle cite -s $S
./target/debug/doyle check testdata/contracts/patients-v1.yaml testdata/contracts/patients-v2.yaml -s $S
echo "check exit: $?"
```

`cite` records that memory M1 rests on `GET /patients`. `check` diffs the two contract versions through Vimanam, flags M1 (exit code 1: something was flagged), and attaches the re-check's proposal — replayed offline from the committed recording of what Muse Spark actually answered (`testdata/replies/recheck/`), so no key or network is involved. With `--no-recheck` it flags without proposing; with `--live` it calls the real model instead (needs `MODEL_API_KEY`, see below).

`doyle curate` is the same citation step done by the curator model instead of by hand; offline it needs `DOYLE_REPLAY_DIR`, live it needs `--live` plus `BASETEN_API_KEY`.

### 4. Review it (`serve`, the judges' page)

```sh
S=/tmp/doyle-demo-state
DOYLE_SERVE_PORT=8080 ./target/debug/doyle serve -s $S
```

Run this from the repository root: the judges' pages read the committed `evidence/` directory relative to the working directory. Then, in a browser or with `curl`:

- `http://127.0.0.1:8080/` — the review page: the flag from step 3, the change behind it, the proposed correction, and the sessions that relied on the memory. Approving writes the correction back through the adapters and needs the reviewer link printed on serve's stderr, which only a person holds; the offline demo leaves the flag open.
- `http://127.0.0.1:8080/judges` — "For judges: start here": the recorded runs with and without Doyle, the measured numbers, the session logs, and the review page itself. Every evidence link is served by this process from `evidence/`; nothing here calls a model, the network, or a key.

### 5. Block the stale step (`guard`)

In another shell, with serve still running or not (the guard reads the state directory, not the server):

```sh
S=/tmp/doyle-demo-state
echo '{"hook_event_name":"PreToolUse","tool_name":"bash","tool_input":{"command":"curl -s http://127.0.0.1:8091/patients | head -c 2000"},"session_id":"demo"}' \
  | ./target/debug/doyle guard -s $S
```

The guard denies the `GET /patients` action with the reason — the stale memory, the match, the breaking change — and names the review page instead of settling anything itself. It is deterministic: no model call on this path. Registration as a Muse Code `PreToolUse` / `SessionStart` hook is in `docs/guard.md`.

## The measured numbers

Quoted from `evidence/measures.md` ("What the judges' page may claim, at most"), which is what the judges' page is pinned to. How each was counted, the files behind it, and the confounds are in that file and its `recompute.py`:

2 memories flagged; 2 stale memories left behind without Doyle; 0 records lost in any run (never "losses averted"); 6 guard denials in session one (never "damage prevented"); re-check cost about even (114.2 s vs 111.7 s of work, 149 vs 136 approvals, confounds as above); escalations 1 / 2 / 0 / 0+3 as listed; 0 false alarms with the granularity confound stated; 2 of 2 citations right; 2 of 2 proposals accepted unedited by a person. Demo only; no second-upgrade numbers exist.

## Beyond the offline demo

- **Live model calls.** `doyle curate --live` and `doyle check --live` need keys, from the environment or the system keyring, never from a committed file (names only in `.env.example`): `MODEL_API_KEY` for the Meta Model API, `BASETEN_API_KEY` for Baseten. Lambo corrections need a running Lambo server (`LAMBO_URL`, default `http://127.0.0.1:7700/mcp/s/doyle`).
- **The recorded live runs.** `evidence/with-doyle/`, `evidence/without-doyle/`, `evidence/with-doyle-self-approved/` (the run where the agent approved its own correction, published as it happened, and the fix it caused) and `evidence/without-doyle-quiet/` each have a README narrating the run as it happened, failures included.
- **The live services.** `demo/` holds the old and new patient-record services and the migration scripts (`demo/README.md`).
- **The design.** `product.md` is the product, `docs/architecture.md` the build, `dev-diary/` the work log, `testdata/README.md` the fixtures.

## Commands

| Command | Does |
|---|---|
| `doyle curate` | Read memories through the adapters, have the curator cite each against the current contract, write citations |
| `doyle cite` | Record a citation by hand (for tests and for memories the curator misses) |
| `doyle check OLD NEW` | Diff two contract versions, flag cited memories, run the re-check, write proposals. Exit code 1 when anything was flagged, for CI |
| `doyle diffrun PROGRAM RUN` | Read a differential run (the COBOL batch against a rewrite, from a manifest), trace each mismatched output to the paragraphs that ran, and flag memories resting on them |
| `doyle screens read` | Have `muse-wonder-1.0` read console screens into an element list with stable IDs, the input to a screens check |
| `doyle serve` | The review page and the judges' page. A decision writes the correction back through the adapters |
| `doyle guard` | The Muse Code hook. `PreToolUse`: deny an action that touches an element with an unsettled flag, with the reason. `SessionStart`: a short notice of open flags |

## The name

Doyle is named after Jon Doyle, whose *A Truth Maintenance System* (Artificial Intelligence 12(3), 1979) set out the idea this tool applies to LLM agent memory: every belief keeps its justification, and when a justification fails, the beliefs resting on it are withdrawn. The name is a tribute; this project is not affiliated with or endorsed by him.

## Licence

Apache-2.0. See [`LICENSE`](LICENSE).
