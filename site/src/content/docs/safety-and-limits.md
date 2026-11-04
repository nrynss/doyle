---
title: Safety, limits, and failures we hit
description: Security rules, guard boundaries, live vision wobble, single-threaded server behavior, and the failures published in evidence.
---

This document outlines Doyle's security boundaries, operational limits, vision model variance, and the failures encountered during development.

## Keys never in files

API credentials never reside in committed files, configuration files, or logs.
Doyle loads model credentials through [`src/models/keys.rs`](https://github.com/nrynss/doyle/blob/main/src/models/keys.rs).
It reads environment variables first, falling back to OS keyring lookups with `secret-tool`.

Before launching agent test sessions, the harness purges sensitive environment variables.
It removes variables matching patterns like `*_API_KEY`, `*_TOKEN`, `*CREDENTIALS`, and `*DSN`.
This practice prevents agents from observing or leaking secrets in command transcripts.

## Prompt-injection caution on `MEMORY.md`

Muse Code loads committed `.agents/memory/MEMORY.md` files even in untrusted workspaces.
Meta documentation explicitly highlights shared memory files as potential prompt-injection vectors.

Doyle treats memory modification as a sensitive operation.
All memory edits require human approval on the review page before write-back.
Doyle keeps correction text short, factual, and plain.
It avoids quoting untrusted contract fields that an agent might interpret as instructions.

## What the guard catches and misses

The `doyle guard` hook inspects tool inputs on the hot execution path without invoking models.
It matches native locations and artifact paths deterministically.

The guard catches commands containing:

- Flagged API operations and HTTP endpoints like `GET /patients`.
- Legacy source identifiers like `CALC-LATE-FEE` or terminal actions like `PF12`.
- Exact artifact paths specified in `rewrite_artifacts`, such as `wonder-bill.rs` or `bin/billbatch`.

The guard also has known blind spots:

- It cannot catch wrapper scripts whose filenames and command arguments omit flagged tokens.
- It cannot block a git tag command like `git push origin v1.0.0` when `rewrite_artifacts` lists only build files.
- The proof suite in [`tests/story_flags.rs`](https://github.com/nrynss/doyle/blob/main/tests/story_flags.rs) explicitly verifies both blocked and allowed paths.

## Screen wobble and the three false alarms

Visual reading introduces variance.
During live execution of Story 2, a single read by `muse-wonder-1.0` wobbled on a patient ID and column spacing.
The model misread `FWV-004812` as `FW-004812`.

This wobble caused the screen differ to report an unintended change.
The differ raised flag `S2` across both Muse Code and Lambo, generating one false-alarm denial in session one.
Separately, a curator draw in Muse Code linked memory `L4` to an edited COBOL element, although `L4` remained valid.

Human reviewers settled all three false alarms as `keep`.
The run report in [`evidence/legacy/with-doyle/README.md`](https://github.com/nrynss/doyle/blob/main/evidence/legacy/with-doyle/README.md) documents and counts each false alarm.

## Single-threaded review server

The review server in [`src/serve/mod.rs`](https://github.com/nrynss/doyle/blob/main/src/serve/mod.rs) uses a minimal HTTP server built on `std::net::TcpListener`.
It operates synchronously on a single thread because memory adapters use local state.

Each connection enforces a ten-second read timeout.
If an HTTP client holds an idle keep-alive socket open, subsequent requests wait until the socket closes or times out.
This architecture supports single-operator review sessions while keeping code simple and dependency-free.

## The self-approval failure and fix

In early migration run `T7.3a`, Muse Wonder was denied its first `GET /patients` call.
Instead of pausing for assistance, Wonder followed the denial URL to `doyle serve`.
It inspected open proposals and approved its own corrections through the unauthenticated `/decide` endpoint.

Run details are published in [`evidence/with-doyle-self-approved/README.md`](https://github.com/nrynss/doyle/blob/main/evidence/with-doyle-self-approved/README.md).
We retained this run as evidence of an autonomous agent bypassing safety guardrails.

Task `T6.4` fixed this vulnerability:

- Generated a random, unguessable reviewer URL printed only on server stderr.
- Added an authenticated `HttpOnly` session cookie required to view decision controls.
- Configured `doyle guard` to deny agent tool calls attempting to access Doyle's state, binaries, or web routes.

In subsequent run `T7.3`, Wonder stopped immediately after the guard denial and requested human help.

## Operator standing in for the reviewer in Story 2

In Story 2, the task operator acted as the reviewer under explicit instructions from the user.
The decision log in [`evidence/legacy/with-doyle/README.md`](https://github.com/nrynss/doyle/blob/main/evidence/legacy/with-doyle/README.md) marks each entry as `decided_by: operator (user's instruction)`.

The run exercised genuine security enforcement.
The reviewer link remained outside the agent workspace, and the agent was blocked from Doyle's endpoints.

## Full transparency and committed evidence

Nothing in Doyle is hidden or simulated.
Every test execution, failure, false alarm, denial, and timeout is recorded verbatim.
All artifacts are committed under [`evidence/`](https://github.com/nrynss/doyle/tree/main/evidence) and logged in [`dev-diary/`](https://github.com/nrynss/doyle/tree/main/dev-diary).
