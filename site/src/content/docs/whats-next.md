---
title: What's next
description: Planned work after the hackathon, stated clearly as not built.
---

This page outlines future engineering work identified during project evaluation.
None of the features or experiments described on this page are built.
They represent planned milestones beyond the hackathon submission.

## Concurrent server mode and HTTP guard endpoint (not built)

Today, `doyle serve` runs on a single thread using standard synchronous sockets.
An idle HTTP client holding a connection open blocks other incoming requests until the socket times out.
Furthermore, `doyle guard` runs as a local process accessing files on local disk.

Future plans include upgrading the server into a concurrent multi-threaded service.
This service will expose an HTTP guard endpoint alongside the review UI.
Agents distributed across remote virtual machines will query a centralized Doyle server.
The shared service will evaluate flags across distributed environments without file contention.

## Additional memory adapters (not built)

Doyle currently provides production adapters for Muse Code and [Lambo](https://github.com/nrynss/lambo).
Because the core engine interacts solely through the adapter trait, adding new systems requires no core modifications.

Planned adapters include:

- Claude Code: reading and updating project instructions stored in `CLAUDE.md`.
- OpenAI Codex: managing agent constraints defined in `AGENTS.md`.
- Cursor: updating local IDE instructions inside `.cursorrules`.
- Hosted memory systems: integrating external memory backends like `mem0`, Letta, and Zep.

## Screens differ tolerance pass (not built)

The screen evidence pipeline uses `muse-wonder-1.0` to convert terminal screenshots into structured element inventories.
Single-read model inference occasionally introduces slight jitter on column coordinates or character OCR.
For example, reading an identifier as `FW-004812` instead of `FWV-004812` raised false alarm `S2`.

A planned tolerance pass in [`src/evidence/screens/mod.rs`](https://github.com/nrynss/doyle/blob/main/src/evidence/screens/mod.rs) will address this jitter.
The differ will normalize whitespace, group minor column shifts, and compute string edit distances.
These heuristics will prevent visual noise from triggering false-alarm flags.

## Measuring the curator and running a second upgrade (not built)

Current benchmarks measure curation accuracy and migration outcomes on the patient records service and billing batch.
Two future evaluation experiments remain planned:

1. Statistical curator evaluation: executing repeated model draws to quantify citation variance across diverse prompting techniques.
2. Second contract upgrade: testing the Stripe Charges to PaymentIntents migration designed in [`docs/second-upgrade.md`](https://github.com/nrynss/doyle/blob/main/docs/second-upgrade.md).

This second scenario involves financial payments where stale assumptions cause silent losses.
Running it will measure downstream sessions reading corrected versus stale memories across complex workflows.
