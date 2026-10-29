# Development Diary: Phase Handoffs

This document breaks down the work for **Doyle**, the Meta Global AI Developer Hackathon entry (Indie track).

Reference specification:
- [`../product.md`](../product.md): the product, the memory systems, the models, the hackathon rules, the cautions.
- [`../docs/architecture.md`](../docs/architecture.md): the core model, flagging, the plug-in seams, the commands, the dependencies, and the facts the design rests on.
- [`ORCHESTRATOR.md`](ORCHESTRATOR.md): how the orchestrator dispatches, validates, lands and closes, with the brief template.
- [`../AGENTS.md`](../AGENTS.md): the rules every agent follows.

Where a phase document and product.md conflict, product.md wins. Record the discrepancy in the handoff log.

Each phase document gives an engineer starting cold the complete context for its tasks.

**This diary is public.** The repository goes public for submission. Nothing private goes in here: no key, no account detail, no personal path beyond `~`, no private repository's name.

**Current status:** P0 in progress: T0.1 and T0.2 are done.

---

## Swarm Coordination Rule

> **Phase-level dependencies are advisory. Task-level dependencies are binding.**

Do not wait for a whole phase to finish. If a task requires only `T1.1`, start it the moment `T1.1` closes.

**Fixtures make most work offline.** P0 records the demo contracts, seed memories, Muse Code hook payloads and Lambo transcripts into `testdata/`, and P4 records model replies. Tasks marked `fixture-ok: yes` run against those files with no key, no running server and no spend. Only `b` tasks call a live service.

---

## Phase Graph

| Phase | Document | Requires | Runs parallel with | Blocks |
|---|---|---|---|---|
| **P0** Ground | [PHASE-0-ground.md](PHASE-0-ground.md) | none | none | everything |
| **P1** Contracts | [PHASE-1-contracts.md](PHASE-1-contracts.md) | T0.3, T0.4, T0.5, T0.6 | none | P2–P6 |
| **P2** Flagging | [PHASE-2-flagging.md](PHASE-2-flagging.md) | P1 | P3, P4 | P5, the guard |
| **P3** Adapters | [PHASE-3-adapters.md](PHASE-3-adapters.md) | P1 | P2, P4 | P5 |
| **P4** Models | [PHASE-4-models.md](PHASE-4-models.md) | P1, T0.2 | P2, P3 | P5 |
| **P5** Pipeline | [PHASE-5-pipeline.md](PHASE-5-pipeline.md) | P2, P3, P4 | P6 guard | P6 review page, P7 |
| **P6** Surfaces | [PHASE-6-surfaces.md](PHASE-6-surfaces.md) | T2.2, P5 for the review page | P5 | P7 |
| **P7** Evidence | [PHASE-7-evidence.md](PHASE-7-evidence.md) | P5, P6 | none | P8 |
| **P8** Ship | [PHASE-8-ship.md](PHASE-8-ship.md) | P7 | none | submission |

```text
  P0 ──▶ P1 ──┬──▶ P2 flagging ──┐
              ├──▶ P3 adapters ──┼──▶ P5 pipeline ──┐
              └──▶ P4 models ────┘                  ├──▶ P7 evidence ──▶ P8 ship
                   P2 ──▶ P6 guard ─────────────────┤
                   P5 ──▶ P6 review page ───────────┘
```

P0 and P1 are the serial bottleneck. Once the contracts freeze, three tracks run at once without touching each other's files.

---

## Parallel Tracks

| Track | Phase | Network required? | First task | Notes |
|---|---|---|---|---|
| **A: Flagging** | P2 | No | T2.1 | Vimanam in, flags out. The product's correctness lives here |
| **B: Adapters** | P3 | `b` tasks only | T3.1 | Muse Code built-in memory and Lambo, behind one trait |
| **C: Models** | P4 | `b` tasks only | T4.1 | The clients, then the curator and re-check prompts, verified by measurement |
| **D: Guard** | P6 | No | T6.1 | Starts when T2.2 lands; needs only flags and the recorded hook payloads |

If capacity is short, run A and B first. A makes the flags true and B makes the correction land. Both are on the critical path.

---

## Task Pattern

```markdown
### T2.2: Flagging ★
requires:   T1.2, T2.1
fixture-ok: yes
size:       M · frontier
owns:       src/core/flag.rs
status:     not-started
```

- **requires:** Binding task ids.
- **fixture-ok:** Whether the task can be built and tested against `testdata/` alone.
- **size:** Scope from XS to XL, then the class to send.
- **owns:** The paths this task alone edits.
- **status:** `not-started`, `claimed:<agent>`, `in-review`, `done`, `blocked`, or `superseded` (the user replaced the task's plan; the task says by what).

| Class | Send it for |
|---|---|
| **frontier** | A decision others build on. Prompt and judgement work verified by measurement. A spec that needs interpreting. |
| **mid** | Well specified implementation, or a spec precise enough to follow literally. |
| **light** | Mechanical, low ambiguity work that its own test verifies. |
| **user** | Only the user can do it: an account, a key, a recording, a submission, a decision. |

A star (★) marks a task on the critical demo path.

A `b` task (T3.3b) is the live sibling of its parent. It owns only its live test, an `#[ignore]`d test named `live_*`. A live test never runs in CI and never gates a build. A live call is evidence: commit the test, and paste its transcript into the handoff log.

---

## Developer Guidelines

1. **Claim before starting.** The orchestrator sets `status: claimed:<agent>` in the phase document on `main`.
2. **Stay inside `owns`.** A change outside it is a new task or a handoff note.
3. **Contracts freeze at the end of P1.** A change to a type in `src/core/model.rs` or to either trait updates its fixtures in `testdata/` in the same commit, and is the orchestrator's call.
4. **The core never names a memory system.** Nothing under `src/core/` imports an adapter. Lambo gets no special path; a second adapter must be one new file and nothing else.
5. **Every flag traces to a Vimanam change.** No flag without a `vc2_` change ID and a citation behind it. The flag decision is computed, never a model's word.
6. **No model call on the guard's path.** The guard decides from Doyle's state alone, fast enough for a `PreToolUse` hook.
7. **What Doyle writes into a memory is short and plain.** Muse Code loads memory into context even in an untrusted workspace (product.md, Cautions). An annotation quotes nothing from a contract that could read as an instruction.
8. **New work only.** Write it here. Never copy code from Lambo, Vimanam or any earlier project; depend on the published crate or talk to the running server.
9. **Gaps in Vimanam or Lambo become issues.** Write the issue text in the handoff log. Don't work around it here. The user files it.
10. **Document handoffs.** A task without a handoff entry is unfinished. Record what exists, what surprised you, and what the next agent should not have to work out again.

---

## Scope

Every task here ships. A task that turns out to be blocked becomes a finding, not a cut. Record it, name every task that depends on it, and mark it `blocked`. If a task is too large, split it into `T3.1a` beside `T3.1` and record the split.

---

## Critical Path

```
keys (T0.2) + demo scenario and fixtures (T0.3) + Muse Code probes (T0.4) + Lambo probe (T0.5) + Vimanam probe (T0.6)
  -> core model (T1.1) -> store (T1.2) -> adapter trait (T1.3) -> model client trait (T1.4) -> dispatch (T1.5)
  -> contract index (T2.1) -> flagging (T2.2)
  -> Muse Code adapter (T3.1, T3.2) + Lambo adapter (T3.3)
  -> Meta client (T4.1) -> curator (T4.3) + re-check (T4.4)
  -> curate (T5.1) -> check with proposals (T5.2) -> write-back (T5.3)
  -> guard (T6.1) + review page (T6.2)
  -> demo workspace (T7.1) -> run without Doyle (T7.2) -> run with Doyle (T7.3)
  -> video (T8.2) -> public (T8.3) -> submit (T8.4)
```

The demo is built around one moment. Muse Wonder remembers that the records endpoint returns everything. The contract changes to return one page and a cursor. Without Doyle, the backup Wonder writes returns 200 and silently loses records. With Doyle, the guard stops it, the memory is corrected in Muse Code and Lambo, and the backup is right. Protect everything that moment depends on before anything else.

---

## Status Board

| Phase | Tasks complete | Status |
|---|---|---|
| P0 | 2 / 7 | T0.1 and T0.2 done. T0.3 landed, in review until the user signs off the story. T0.4 and T0.5 are with agents. T0.6 is ready once T0.3 closes; T0.7 is ready. |
| P1 | 0 / 5 | Waits for T0.3–T0.6. |
| P2 | 0 / 4 | Waits for P1. |
| P3 | 0 / 3 | Waits for P1. |
| P4 | 0 / 4 | Waits for P1. |
| P5 | 0 / 3 | Waits for P2–P4. |
| P6 | 0 / 2 | Waits for T2.2 (guard) and P5 (review page). |
| P7 | 0 / 5 | T7.1 can start once T0.3 lands. |
| P8 | 0 / 4 | Waits for P7. |

---

## Handoff Log

Newest first. One entry per closed task, finding, or split.

- **T0.3 landed, waiting on the user's sign-off of the story.** `testdata/contracts/records-v1.yaml` and `records-v2.yaml`: `listRecords` goes from a bare array to a `RecordPage` (`items`, `next_cursor`; `cursor` and `limit`, default 100); `getRecord` is unchanged; `Stats` gains an optional `oldest_created_at`. Three memories in Muse Code format (`testdata/memories/muse-code/`) and as Lambo derive and recall JSON (`testdata/memories/lambo/`, shapes from Lambo's published MCP reference; T0.5 confirms them). Expected flag set in `testdata/README.md`: M1 only, with two traps written down (no flag through the shared `Record` type, and no flag from the additive changes on `listRecords` alone). Validated by the orchestrator: the diff holds only T0.3's paths, the gate passed in the worktree and on `main` after the merge.
  - **Finding for T0.6:** no Vimanam 2.x binary is on this machine. `vimanam` is 1.7.0 in `~/.local/bin` and `~/.cargo/bin`, and the checkout's built binary reports 1.4.2. 1.7.0 parses both contracts and reports `GET /records` 200 `array` → `object` as breaking plus one review item, with no pagination kind and `vc1_` IDs. T0.6 must run Vimanam 2.0.0 (crates.io), installed into a scratch root, to confirm the inferred pagination change and the `ve2_`/`vc2_` IDs.
- **T0.2 closed: keys and models confirmed.** Meta Model API: `MODEL_API_KEY` or keyring `service meta-model-api`; `muse-spark-1.3` answered. Baseten: `BASETEN_API_KEY` or keyring `service baseten`; the curator model is `meta-models/muse-spark-open-1026`, because the undated `meta-models/muse-spark-open` returns 403 for the same key with a message blaming the key. Lambo answers at `http://127.0.0.1:7700/mcp`. `.env.example` lists the names, with no values.
- **Plan rebuilt in the house structure.** The single plan file became this diary, with the architecture in `docs/architecture.md`. Process, by the user's decision: one agent per task, validated and landed by the orchestrator; no review loops.
- **Finding: Muse Wonder's memory is its memory file.** Recall test on Muse Code 1.4.14 with `muse-wonder-1.0`, in two scratch repositories:
  - A fact Wonder was told not to save came back UNKNOWN in fresh sessions, in the same repository and in another. Only session logs held it.
  - Told to remember, Wonder wrote it inline into `~/.local/share/muse/memory/projects/<path-slug>-<16-hex hash>/MEMORY.md` under `type: reference` and `description:` frontmatter, with no source. A fresh session in that repository recalled it; one in another repository didn't.
  - After `[STALE per Doyle …: the codename is now SAFFRON-7]` was appended to the note, a fresh session answered SAFFRON-7.
  - So the lead adapter targets that file, and annotation in place is the override. Still open for T0.4: how the hash is derived, and how hooks register on 1.4.x.
- **Models available on this account.** `muse-wonder-1.0` and `muse-spark-1.3-contributor` answer; `muse-wonder-full` and `muse-wonder-full-1.0` do not exist or lack access. `muse usage` reports no subscription allowance, so Wonder runs pay-as-you-go in Standard mode; `--contributor-mode on` is accepted but nothing shows it applied.
