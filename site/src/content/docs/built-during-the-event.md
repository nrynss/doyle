---
title: Built during the event
description: What was written for Doyle during the hackathon versus existing dependencies, and how the repository was built.
---

This page details what code was written specifically for Doyle during the hackathon, which external dependencies were used, and how the project was engineered.

## What is new in Doyle

Every line of Doyle's source code was created from scratch for this event.
No code was vendored or copied from earlier repositories.

New components created in this repository include:

- The entire `doyle` Rust crate and executable located in [`src/`](https://github.com/nrynss/doyle/tree/main/src).
- The truth-maintenance core: citation indexing, change detection, flagging, proposal generation, decision logging, and the append-only state store.
- Four distinct evidence sources: the Vimanam contract bridge, the COBOL parser and differ, the vision reader and screen differ, and the differential run execution parser.
- The memory adapters for Muse Code file memory and [Lambo](https://github.com/nrynss/lambo) graph memory.
- The HTTP model clients for Baseten and the Meta Model API, alongside offline replay engines for testing.
- The `doyle serve` web server, including the human review interface and the judges evidence overview.
- The `doyle guard` runtime hook preventing agents from executing commands based on stale facts.
- The mock veterinary patient services located in [`demo/`](https://github.com/nrynss/doyle/tree/main/demo).
- All offline test fixtures and recorded runs in [`testdata/`](https://github.com/nrynss/doyle/tree/main/testdata) and [`evidence/`](https://github.com/nrynss/doyle/tree/main/evidence).

## Existing dependencies

Doyle connects to external services and crates without vendoring their source code:

- `vimanam` (version 2.x): imported from crates.io as a library for OpenAPI contract parsing and semantic diffing.
- Lambo: an external graph memory service reached over HTTP MCP streams, never linked as a library.
- Rust ecosystem crates: `serde` and `serde_json` for serialization, and `ureq` for blocking HTTP requests.
- GnuCOBOL (`cobc`): used to compile COBOL batches and capture execution traces.
- Muse Code: Meta's coding agent, running `muse-wonder-1.0`. Doyle plugs into it as a hook and through its memory files.
- The Meta Model API (`muse-spark-1.3`, `muse-wonder-1.0`) and Baseten (`meta-models/muse-spark-open-1026`): the model endpoints Doyle calls with `--live`.

## How the repository was built

The project used a structured agent orchestration system described in [`dev-diary/ORCHESTRATOR.md`](https://github.com/nrynss/doyle/blob/main/dev-diary/ORCHESTRATOR.md).
An orchestrator dispatched distinct tasks to autonomous agents working in isolated git worktrees.

Quality control was enforced by a strict gating script on every commit:

- Running [`tools/check.sh`](https://github.com/nrynss/doyle/blob/main/tools/check.sh) verified formatting with `cargo fmt --check`.
- Running clippy with `-D warnings` denied all warnings.
- The test suite verified functionality offline against recorded fixtures.

Every task outcome, architectural decision, and live run transcript was logged in [`dev-diary/README.md`](https://github.com/nrynss/doyle/blob/main/dev-diary/README.md) across Phases 0 through 9.
All claims in this documentation trace directly to committed code and recorded evidence.
