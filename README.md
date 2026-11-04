# Doyle

**Truth maintenance for agent memory.**

[**▶ Demo video**](https://www.youtube.com/watch?v=cBD3CQrDqUc) · [**Docs**](https://nrynss.github.io/doyle/) · [Judge walkthrough](https://nrynss.github.io/doyle/judge-walkthrough/) · [The thread on X](https://x.com/nrynssnryn/status/2118070593144893873)

Built by Narayan SS.

Coding agents remember what they learn: how an API pages, what a billing batch charges, which keys retire an old console. Every one of those memories rests on something outside it. When that thing changes, or was misread in the first place, nothing tells the agent. It keeps acting on the stale belief, and so does every other agent reading the same memory.

Doyle records what each memory rests on. When the evidence changes, Doyle flags every memory resting on it, has a Meta model propose a correction, and sends the correction to a person. Until the person has decided, Doyle's guard holds the agent off the stale step. The correction is then written back into every memory system the agent reads.

> Entry for the Meta Global AI Developer Hackathon (Cerebral Valley), Indie track. The gate passes with no key, and the demo below runs fully offline from a fresh clone.

## The two stories, as recorded

Both are live, recorded Muse Code sessions with `muse-wonder-1.0`, run once without Doyle and once with it. The numbers are the measured ones; each links to its record.

**1. The Quiet Migration.** A veterinary clinic chain moves 4,812 patient records to a new platform. The agent remembers that `GET /patients` returns everyone in one call; the vendor API now pages at 100. Without Doyle, Wonder probed past the stale memory and lost nothing, but the stale belief survived. With Doyle, both memories were flagged, the guard held the export, a person approved the correction, and it landed in Muse Code and [Lambo](https://github.com/nrynss/lambo). 0 records lost in any run, never claimed as "losses averted". [`evidence/measures.md`](evidence/measures.md)

**2. The COBOL cutover.** Wonder ports a COBOL billing batch (4,812 accounts) to Rust, ships it, and retires the green-screen console. Its memory says the 1.5% late fee has "no exceptions"; the COBOL exempts vaccination visits. Without Doyle, Wonder found the bug itself and shipped correctly, and both stale memories survived. With Doyle:

- Doyle flagged the memories from three kinds of evidence: the COBOL diff, both programs run on all 4,812 accounts (535 mismatches, $8,308.80 overcharged, traced to paragraph `LOG-EXEMPT`), and Wonder's own reading of the console screens and training video.
- The guard denied the rewrite and the ship script. Wonder worked out the exemption from the COBOL and stopped to ask a person.
- After the review, session two worked from the corrected memory, shipped a two-line fix, and Doyle's differential run found 0 mismatches.
- 9 flags, 3 of them false alarms, all shown on the review page. In this run the operator stood in for the reviewer, as the record says.

[`evidence/legacy/with-doyle/`](evidence/legacy/with-doyle/README.md) · [`evidence/legacy/without-doyle/`](evidence/legacy/without-doyle/README.md)

## How it works

1. **Cite.** A curator model (Muse Spark Open on Baseten) reads each memory, in Muse Code and in Lambo, and records which evidence elements it rests on. Every element has a stable ID: `ve2_` for API contracts (through Vimanam), `le2_` for COBOL programs, `se2_` for console screens.
2. **Flag.** When evidence changes, Doyle diffs the two versions into change facts and flags every memory citing a changed element. The evidence can be a contract, a COBOL program, a differential run (the batch and a rewrite on the same input, each mismatch traced to the paragraphs that ran), or console screens that Muse Wonder read from stills and a video. A flag is computed, never a model's word.
3. **Re-check.** Muse Spark 1.3 reads each flagged memory with the change and proposes a correction, a retirement, or keep.
4. **Guard.** `doyle guard` is a Muse Code hook. It denies any step that touches a flagged element and gives the agent the reason. It is deterministic: no model runs on this path.
5. **Review.** A person decides on the review page, which only the reviewer's link can change. The correction is written back through each memory adapter and the stale note is marked superseded.

Architecture: [`docs/architecture.md`](docs/architecture.md) · the guard: [`docs/guard.md`](docs/guard.md) · the full docs: [nrynss.github.io/doyle](https://nrynss.github.io/doyle/)

## Run the offline demo

From a fresh clone, with no keys, no network (after the first build) and no running server. Install Rust via [rustup](https://rustup.rs) first; `rust-toolchain.toml` pins the toolchain and `rustup` installs it on first use. The first `cargo build` downloads crates from crates.io once; everything after that is offline.

```sh
git clone https://github.com/nrynss/doyle && cd doyle
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

Story 1, quoted from [`evidence/measures.md`](evidence/measures.md) ("What the judges' page may claim, at most"). How each was counted, the files behind it, and the confounds are in that file and its `recompute.py`:

2 memories flagged; 2 stale memories left behind without Doyle; 0 records lost in any run (never "losses averted"); 6 guard denials in session one (never "damage prevented"); re-check cost about even (114.2 s vs 111.7 s of work, 149 vs 136 approvals); escalations 1 / 2 / 0 / 0+3 as listed; 0 false alarms with the granularity confound stated; 2 of 2 citations right; 2 of 2 proposals accepted unedited by a person.

Story 2, from [`evidence/legacy/with-doyle/README.md`](evidence/legacy/with-doyle/README.md): 9 flags (3 false alarms, counted); 4,812 accounts compared, 535 mismatches and $8,308.80 overcharged before the fix, 0 after; 4 guard denials in session one; old 4,812, new 4,812, nothing lost.

## What is new here, and what is not

The hackathon's new-work rule draws this line, and so does this file. Everything presented as built during the event is Doyle; everything else is an existing dependency, never vendored or copied.

**Built here (Doyle, this repository):** the `doyle` binary and all of its behaviour — the citation index, flagging, proposals, sign-off, the review and judges' pages, the Muse Code hook, the curator and re-check prompts, the adapters' Doyle-side code, and every fixture and recording under `testdata/` and `evidence/`.

**Existing dependencies (not built here):**

| Dependency | What it is | How Doyle uses it |
|---|---|---|
| [Vimanam](https://github.com/noemaforge/vimanam) | An existing API-contract library (crates.io `vimanam` 2.x) | Used as a library with default features off: the contract model, stable element IDs (`ve2_…`), and the diff. Doyle links it; no code is copied |
| [Lambo](https://github.com/nrynss/lambo) | An existing open-source memory system | Reached as a client of a running Lambo server over MCP. Doyle records corrections `against` stale memories; Lambo itself is untouched |
| Muse Code built-in memory | The memory the demo agent already writes and recalls | Read and annotated in place through an adapter; owned by Muse, not by Doyle |
| Muse Wonder (`muse-wonder-1.0`) | The coding agent in the demo | Runs the migration. Doyle never migrates; it keeps what Wonder believes true |
| Meta Model API (Muse Spark 1.3, the re-check; Muse Wonder, the screen reader) and Baseten (Muse Spark Open, the curator) | Model endpoints | Called live only with `--live` and keys; offline they are replaced by committed recordings |

## Beyond the offline demo

- **Live model calls.** `doyle curate --live` and `doyle check --live` need keys, from the environment or the system keyring, never from a committed file (names only in `.env.example`): `MODEL_API_KEY` for the Meta Model API, `BASETEN_API_KEY` for Baseten. Lambo corrections need a running Lambo server (`LAMBO_URL`, default `http://127.0.0.1:7700/mcp/s/doyle`).
- **The recorded live runs.** Story 2: `evidence/legacy/with-doyle/` and `evidence/legacy/without-doyle/`. Story 1: `evidence/with-doyle/`, `evidence/without-doyle/`, `evidence/with-doyle-self-approved/` (the run where the agent approved its own correction, published as it happened, and the fix it caused) and `evidence/without-doyle-quiet/` each have a README narrating the run as it happened, failures included.
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

## Built with Meta's models

Doyle was built during the event by agents working under the repository's task protocol ([`AGENTS.md`](AGENTS.md), [`dev-diary/`](dev-diary/README.md)), one task, one branch, one worktree, every commit through the gate. Meta's models did most of it: of the 55 tasks whose claim named an agent, Muse Spark 1.3 claimed 26 and Muse Wonder 3. Each claim is in the dev-diary's history.

## The name

Doyle is named after Jon Doyle, whose paper *A Truth Maintenance System* (*Artificial Intelligence* 12(3), 231–272, 1979, [doi:10.1016/0004-3702(79)90008-0](https://doi.org/10.1016/0004-3702(79)90008-0)) set out the idea this tool applies to LLM agent memory. In a truth maintenance system, every belief keeps its justification, the reasons it is held. When a justification fails, the beliefs resting on it are withdrawn, and so is everything that rested on them.

Doyle maps that onto agent memory directly:

| In the 1979 paper | In Doyle |
|---|---|
| A belief | A memory an agent keeps, in Muse Code or Lambo |
| Its justification | A citation: the evidence elements the memory rests on |
| A justification fails | A cited element changes in a contract, COBOL program, differential run or console screen |
| The belief is withdrawn | The memory is flagged, and the guard holds the agent off it |

The one step the paper leaves to the system, Doyle gives to a person: a flagged memory is corrected, retired or kept only by the reviewer's decision. The name is a tribute; this project is not affiliated with or endorsed by Jon Doyle.

## Licence

Apache-2.0. See [`LICENSE`](LICENSE).
