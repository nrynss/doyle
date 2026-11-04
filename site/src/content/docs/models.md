---
title: Meta models, one job each
description: How Doyle uses Muse Spark Open, Muse Spark, and Muse Wonder for curation, re-checking, screen reading, and migration.
---

Doyle assigns specialized Meta models to distinct tasks across the truth-maintenance lifecycle. Each model handles a single responsibility.

Deterministic code handles parsing, diffing, and runtime holds. Models evaluate semantic intent, visual layouts, and repair proposals.

## The four model roles

Doyle deploys four distinct model configurations:

### 1. Muse Spark Open on Baseten: the curator

`meta-models/muse-spark-open-1026` runs on Baseten. It powers `doyle curate --live`.

Implementation details live in [`src/models/baseten.rs`](https://github.com/nrynss/doyle/blob/main/src/models/baseten.rs) and [`src/curate.rs`](https://github.com/nrynss/doyle/blob/main/src/curate.rs).

The curator reads unlinked memories from adapter storage. It compares each natural language statement against extracted contract, COBOL, or screen elements.

When it identifies a match, it writes a `Citation` record containing the cited element identifiers and a numerical confidence score.

### 2. Muse Spark on Meta Model API: the re-check

`muse-spark-1.3` runs through the Meta Model API. It powers `doyle check --live` and `doyle diffrun --live`.

Implementation details live in [`src/models/meta.rs`](https://github.com/nrynss/doyle/blob/main/src/models/meta.rs), [`src/prompts/recheck.rs`](https://github.com/nrynss/doyle/blob/main/src/prompts/recheck.rs), and [`src/check.rs`](https://github.com/nrynss/doyle/blob/main/src/check.rs).

When an evidence source detects a breaking change, the re-check evaluates the flagged memory alongside old and new definitions.

The model generates a `Proposal` recommending one of three repair actions:

- **`correct`:** Supplies updated claim text reflecting the new reality.
- **`retire`:** Recommends deleting the memory if an endpoint or record was permanently removed.
- **`keep`:** Recommends keeping the claim if the behavioral change does not alter its truth.

Every proposal includes an explanation for the human reviewer.

### 3. Muse Wonder 1.0 on Meta Model API: Doyle's eyes

`muse-wonder-1.0` serves as Doyle's vision engine during `doyle screens read --live`.

Implementation details live in [`src/models/vision.rs`](https://github.com/nrynss/doyle/blob/main/src/models/vision.rs) and [`docs/wonder-vision.md`](https://github.com/nrynss/doyle/blob/main/docs/wonder-vision.md).

For static terminal screens, Wonder inspects 3270 green-screen PNG images in a single multimodal call. It extracts visible fields, labels, status lines, and function key hints.

A console version's stills and its training video go to Wonder together, in one call per version (`image_url` and `video_url` parts). Wonder returns the screens, their elements, and the video's steps with their time spans. Doyle diffs two such reads. It reads no pixels itself.

### 4. Muse Wonder 1.0 in Muse Code: the developer agent

`muse-wonder-1.0` also drives the autonomous developer agent inside Muse Code.

It executes API migration steps and COBOL billing cutover tasks. The agent reads memories from Muse Code storage and [Lambo](https://github.com/nrynss/lambo) graph databases.

During each session, [`src/guard.rs`](https://github.com/nrynss/doyle/blob/main/src/guard.rs) monitors tool use. The guard prevents the agent from acting on unverified assumptions until a human signs off.

## Where Doyle stays deterministic

Doyle relies on models only for perception and synthesis. Core verification remains strictly deterministic:

- **Contract diffing:** Handled deterministically by Vimanam using schema ASTs.
- **Legacy COBOL diffing:** Handled by Doyle's COBOL parser through exact record offset and statement comparisons.
- **Console screen diffing:** Handled by hashing normalized visual elements and comparing grid positions.
- **Differential run blame:** Handled by intersecting and subtracting GnuCOBOL `-ftrace` execution logs.
- **Runtime enforcement:** Handled by `doyle guard` through fast token and path matching.

Zero model calls occur during runtime tool interception.

## Offline replay recordings

Doyle runs full test suites and offline demonstrations without live API keys or network access.

Recorded model interactions live in [`testdata/replies/`](https://github.com/nrynss/doyle/tree/main/testdata/replies):

- `curator/`: Recorded contract citation responses.
- `curator-legacy/`: Recorded COBOL paragraph and field citations.
- `curator-screens/`: Recorded screen element citations.
- `recheck/`: Recorded repair proposals for flagged claims.
- `vision/`: Recorded multimodal terminal reads and video step transcriptions.

During offline runs, Doyle keys each request by a 64-bit FNV-1a hash ([`src/models/mod.rs`](https://github.com/nrynss/doyle/blob/main/src/models/mod.rs)). The hash covers the model ID, the prompts, the canonical response schema and the budget. Doyle serves the recorded reply with that key, and a missing recording fails loudly.

Setting `DOYLE_REPLAY_DIR` or `DOYLE_RECHECK_REPLAY_DIR` redirects model clients to replay directories. This guarantees fast, deterministic verification across all environments.
