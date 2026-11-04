---
title: The review page
description: How doyle serve renders evidence for all four flag kinds, protects reviewer sign-off, and writes corrections back to memory.
---

The `doyle serve` command starts a local web server for reviewing memory flags.
It renders every view directly from local state without external frameworks or build steps.
The server runs on standard library networking primitives and handles requests one at a time.
Every view works offline without network access or model credentials.

The script [`site/scripts/capture-screenshots.mjs`](https://github.com/nrynss/doyle/blob/main/site/scripts/capture-screenshots.mjs) builds an isolated scratch state from committed fixtures.
It runs the full offline demo pipeline and captures every screenshot on this page.

## The review queue

The review queue at `/` lists all open and settled memory flags.
Each card displays the memory claim, the storage adapter, and the memory identifier.
Cards show the version transition and the proposed action from Muse Spark.
Badges highlight flag severity and settlement status.
Red badges mark breaking changes, while amber badges indicate review items.

The offline demo starts with a single flagged API contract memory.

![Single-flag offline demo review queue](/doyle/media/review-readme-single.png)

When multiple source changes occur, the queue presents flags across all four evidence sources.
Each card links directly to the detailed flag review page.

![Four-flag-kind review queue](/doyle/media/review-queue.png)

## Per-kind flag evidence

Visiting `/flag/<id>` opens the detail page for an individual flag.
The server tailors the evidence presentation to the flag kind.
Implementations in [`src/serve/evidence.rs`](https://github.com/nrynss/doyle/blob/main/src/serve/evidence.rs) and [`src/serve/mod.rs`](https://github.com/nrynss/doyle/blob/main/src/serve/mod.rs) handle four distinct evidence formats.

### 1. API contract flags

API contract flags carry `ve2_` element identifiers and `vc2_` change fact identifiers.
The review page displays the affected OpenAPI or Protobuf endpoint.
It shows Vimanam change facts, such as adding pagination constraints on `GET /patients`.
The page lists Muse Spark's proposed replacement note explaining the pagination format.

![API contract flag M1](/doyle/media/flag-contract-m1.png)

### 2. Legacy COBOL flags

Legacy source flags carry `le2_` element identifiers and `lc2_` change identifiers.
The page displays structural differences between COBOL programs.
For memory `L2`, it shows `PATIENT-RECORD` expanding from 34 bytes to 35 bytes.
The view highlights affected fields in copybooks alongside Muse Spark's proposal.

![Legacy COBOL flag L2](/doyle/media/flag-legacy-l2.png)

### 3. Console screen flags

Console screen flags carry `se2_` element identifiers and `sc2_` change identifiers.
The screen difference view compares terminal screen definitions read by `muse-wonder-1.0`.
For memory `S1`, it shows the decommission screen dropping the clinic-name confirmation prompt.
The view shows reconstructed text grids, changed elements, and proposed correction notes.

![Console screen flag S1](/doyle/media/flag-screens-s1.png)

### 4. Differential-run flags

Differential-run flags identify behavioral mismatches between legacy code and new rewrites.
The review page shows the compared programs, such as `BILLBATCH.cbl` and `wonder-bill.rs`.
It reports total mismatch counts, showing 535 mismatched records for memory `L1`.
GnuCOBOL execution trace analysis identifies blamed paragraphs, specifically `LOG-EXEMPT`.
A table shows sample account records comparing legacy zero fees with rewrite calculations.
The view also lists held `rewrite_artifacts` that guard hooks block from execution.

![Differential run flag L1](/doyle/media/flag-diffrun-l1.png)

## Reviewer links and authentication

The review server protects decisions with secret tokens generated at startup.
The server creates a random 64-hexadecimal character token in process memory.
It prints `Review at http://127.0.0.1:8080/?reviewer=<token>` to standard error.

When a reviewer visits that link, the server sets a session cookie:
`Set-Cookie: doyle_reviewer=<token>; HttpOnly; SameSite=Strict; Path=/`.
The server then issues an HTTP 303 redirect to `/`.

Logged-out visitors and automated agents see read-only evidence views without decision controls.
All screenshots on this page show the logged-out read-only view.
Submitting `POST /flag/<id>/decide` without a valid cookie returns an HTTP 403 response.
This security model prevents autonomous agents from approving their own memory changes.

## Decision actions

Authorized reviewers choose between four distinct actions:

- **approve:** Accepts Muse Spark's proposed correction text without edits (`verdict: approve`).
- **edit:** Allows the reviewer to modify the proposed text before saving (`verdict: edit_then_approve`).
- **retire:** Marks the stale memory retired across all memory systems (`verdict: retire`).
- **keep:** Settles the flag without modifying stored memory notes (`verdict: keep`).

The `keep` action settles false alarms where external changes do not invalidate the memory.

## Write-back with retry

When a reviewer settles a flag, [`src/core/decide.rs`](https://github.com/nrynss/doyle/blob/main/src/core/decide.rs) applies the choice.
The server writes updates or retirement markers to each configured adapter holding copies.
It records the decision audit trail in `.doyle/decisions.jsonl`.

External services like Lambo servers might become temporarily unreachable during a review session.
When an adapter write fails, the server records the error in `.doyle/decisions.jsonl`.
The flag remains marked with `decision_needs_writeback`.
The review page displays the pending state and retries write-back during subsequent review actions.

## The judges' overview page

The server includes a dedicated inspection page at `/judges`.
The implementation in [`src/serve/judges.rs`](https://github.com/nrynss/doyle/blob/main/src/serve/judges.rs) summarizes test results and measured metrics.
It presents side-by-side evidence comparing runs with Doyle against baseline runs without Doyle.
The view shows real-time flag counts and links directly to committed transcript logs.

![Judges overview page](/doyle/media/judges-page.png)
