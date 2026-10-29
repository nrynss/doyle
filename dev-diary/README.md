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

**Current status:** P0, P1, P2 done. T3.1, T3.3, T4.1 done. In flight: T3.2, T4.2.

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
- **size:** Scope from XS to XL, then the class: the kind of work it is. The user assigns the agent and model.
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

The demo is built around one story, the Quiet Migration (product.md). Muse Wonder migrates a veterinary clinic chain's patient records, trusting an earlier session's memory that `GET /patients` returns everyone in one response. The API now pages. Without Doyle, every step of its plan passes its own check, the old system is decommissioned, and 4,712 of 4,812 records are gone for good. With Doyle, the guard stops the export, the correction lands in Muse Code and Lambo with the sessions that relied on it named, and the migration pages to the end and keeps the old system until the counts match. Protect everything that moment depends on before anything else.

---

## Status Board

| Phase | Tasks complete | Status |
|---|---|---|
| P0 | 7 / 7 | Done. T0.7 (second real upgrade) closed on the user's pick of Stripe Charges → PaymentIntents. |
| P1 | 5 / 5 | Done. Contracts frozen at T1.5. |
| P2 | 4 / 4 | Done: check, cite, flags, index. |
| P3 | 2 / 3 | T3.3 done. T3.2 in flight. |
| P4 | 1 / 4 | T4.1 done. T4.2 next (needs T1.4 — done; ureq wiring owner TBD). |
| P5 | 0 / 3 | Waits for P2–P4. |
| P6 | 0 / 2 | Waits for T2.2 (guard) and P5 (review page). |
| P7 | 0 / 5 | T7.1 can start once T0.3 lands. |
| P8 | 0 / 4 | Waits for P7. |

---

## Handoff Log

Newest first. One entry per closed task, finding, or split.

- **T3.1 closed: Muse Code locate and read.** `src/adapters/muse_code/{mod,locate,parse}.rs`: FNV-1a-64 folder location, shared scope, frontmatter + inline notes + index lines, stable `{file}#{ordinal}` refs, list/read (+in-memory write stub, unsupported optionals). Adversarial review forced a ref-scheme change before landing: content-hash refs rehash on in-place annotation and collide on duplicate text, unimplementable against the trait contract — positional refs survive both appends and annotation. Also pinned: verbatim texts/frontmatter, genuine hash→folder locate (symlink + shared-scope cases), HOME never leaves temp dirs in tests. Re-review clean. Advisory for T3.2: ordinal refs shift if an earlier bullet is deleted or an index line is inserted mid-file (user-side edits via edit_memory are a real-world event) — plan a fallback (e.g. text-match on read miss); T3.2's own in-place annotation is safe. Gate passed on `main` after the merge.
- **T4.1 closed: the Meta Model API client.** `src/models/meta.rs` holds `MetaModelClient` over an `HttpTransport` seam (fake for tests; ureq-backed impl deferred — T3.3 owns the dependency line): env key at call time, `json_schema` response format, reasoning-effort/max_tokens pass-through, 429/5xx/transport retry with cap, usage+cost parsing across field spellings, errors carry model id + bounded first lines (500 chars), never the key/headers. Adversarial review found the Done-when clauses met but three gaps: outgoing body/auth header unpinned, error text byte-unbounded, transport retry and alternate usage spellings untested. Remediation pinned all three with tests only; re-review verified each kills its mutant, diff confined to meta.rs, 85 green — clean. Nits recorded (asymmetric top-level cost spellings, unasserted messages[]/headers/URL, silent truncation, transport message preservation). Open coordination: name ONE owner for the ureq transport wiring (candidates T4.2 or T4.1b); T4.1b must confirm the wire shape (base URL, field names) live. Gate passed on `main` after the merge.
- **T1.5 closed: command dispatch, contracts frozen.** `src/lib.rs` declares every module as `pub mod`; thin `src/main.rs` dispatches hand-rolled `src/cli.rs` parsing (all five commands, shared state-dir/adapters/verbosity, help/version); each command module exposes `run(&cli::Cli) -> ExitCode`, all exiting 2 as "not implemented yet". Stubs created for every P2–P6 module; `contract.rs` and `core/flag.rs` kept real and untouched. Review (first round, no remediation needed): every help text and exit code reproduced by running the binary, all 14 declarations verified, freeze complete — every file a later task owns is declared, undeclared names are new files their owners will create with orchestrator approval. Nits recorded, not fixed (frozen files): no dispatch tests, dead `STUB_EXIT_CODE`, undocumented future check 0/1/2 contract, stale store allow, pub-vs-private mod inconsistency, trimmed help detail, unproven external lib reachability. Contracts freeze at this landing: no edits to `src/main.rs`, `src/cli.rs`, or a `mod.rs` declaration list without orchestrator approval.
- **T1.4 closed: the model client trait.** `src/models/mod.rs` holds `ModelClient` (`complete` + `call` alias), `ModelRequest`/`Budget`/`Usage`/`ModelResponse`/`ModelError`, and `request_key`; `src/models/replay.rs` holds `ReplayClient` and `RecordingClient`. Adversarial review found the Done-when clauses met but the key function defective 4 ways: `None` vs `Some(0.0)` budget collision (silent wrong-serve), order-sensitive schema hashing vs a "canonical" claim, `DefaultHasher` toolchain instability misdescribed in the notes, and no key-sensitivity tests. Remediation: discriminant tag on the cost dimension, recursive key-sorting canonicalization, hand-rolled 64-bit FNV-1a with a golden-value test, per-field flip tests (16 tests, 69 green). Re-review verified the FNV-1a constants and order, both budget dimensions, and tests-only scope — clean. Residual nits recorded (objects-in-arrays pin, doc overclaim on cross-toolchain stability, unpadded hex, missing shared conformance harness suggestion for P4). Outside `owns`, approved: `mod models;` in `src/main.rs` (T1.5 relocates). Gate passed on `main` after the merge.
- **T1.3 closed: the memory adapter trait.** `src/adapters/mod.rs` holds `MemoryAdapter` (required `name`/`capabilities`/`list(query)`/`read`/`write`; optional `annotate`/`supersede`/`watch` with typed-`Unsupported` defaults), `Capabilities`, `AdapterError`, and the shared capability-aware `check_conformance`; `src/adapters/fake.rs` holds the full and reduced fakes. Adversarial review forced a contract change before landing: parameterless `list()` is unsatisfiable by Lambo (no list-all primitive), so `list(query)` is a search with an incompleteness contract; write-then-supersede is unimplementable for Lambo (no post-hoc link tool), so `supersede` creates the correction and returns its ref while `write` is fresh-only; `watch` reports everything on first call with caller baselining and persisted cursors. Remediation also strengthened conformance (no-op-supersede catcher) and fixed the `lambo_stale` misdescription. Re-review clean. T3.3's phase text already describes the creating-supersede division; the Lambo retry path is explicitly deferred to live verification in T3.3b. Outside `owns`, approved: `mod adapters;` in `src/main.rs` (T1.5 relocates). Watch for T3.3/P2: Lambo `watch`'s broad query is unnamed and `watch` still promises completeness — recommend a watch incompleteness note mirroring `list`'s. Gate passed on `main` after the merge.
- **T3.3 closed: the Lambo adapter.** `src/mcp.rs` (minimal streamable-HTTP MCP client: initialize/notify/tools-call, SSE parsing, session-id capture, finite timeouts, `Transport` seam with `UreqTransport`/scripted `FakeTransport`) and `src/adapters/lambo.rs` (`list` recall-verbatim top_k 50, `read` with batch-scoped warning pairing, `write` derive+receipt-wait, `supersede` inspect-first retry no-op else derive-with-`against`, `annotate` Unsupported, `watch` broad `*`). Adversarial review (6 LBs) forced the evidence up: real shared conformance over a scripted transcript-shaped fake, request body/header capture with wire asserts, both retry branches executed, `wait_ms` 4000 vs 5s timeout pinned. Two more rounds followed: missing `*` live-verification marking + untested recall-warnings path, then an over-match in the relaxed predicate fixed by batch-scoped L1+L2 pairing (unique-or-silent) with a two-contradiction regression test. All three re-reviews verified mutant-kills and scope; final gate 81 green. Adjudications landed in the phase notes (annotate-Unsupported compliant, restart loss known, `*` deferred to an extended T3.3b that now also covers the retry path). ureq 3.5.0 on `main` — T4.2's wiring note stands resolved. Outside `owns`, approved: two doc bullets in `src/adapters/mod.rs` (comment-only, no signature change). Gate passed on `main` after the merge.
- **T2.4 closed: `doyle cite`.** `src/cite.rs` reads citation specs from stdin, resolves via `ContractIndex::entities()` (minimal read-only accessor, dead second parse removed), refuses unknown elements with Levenshtein suggestions (bogus qualifiers refused, not defaulted), records through `Store::open` + `record_citation`. Permanent tests: real-binary cite→store→flag_citations yields exactly `{M1}`, JSONL/key=value/qualifier refusal paths, store round-trip identity, index agreement. Adversarial review found 6 LBs (headline test removed, unreachable JSONL branch, unvalidated key=value, silent wrong-element default, store bypass, index bypass); remediation fixed all with re-review clean. Scope exception acknowledged: the `pub use store::Store;` line (same one T2.3 landed — rebase merged it to a single copy) and the `entities()` accessor, both minimal and approved.
- **Finding against T2.4: main went red on land.** The merge passed the worktree gate but failed on `main`: two cite tests shell out to `target/debug/doyle`, which `cargo test` never rebuilds (test-harness binaries land in `deps/`, not `target/debug/`) — the main checkout's binary was from Oct 27, so the tests ran a pre-T1.5 binary with no `cite` subcommand. Real defect in test hygiene, not a flaky environment: the gate did not guarantee the artifact the tests invoke. Fixed before anything else landed: `tools/check.sh` now runs `cargo build --locked` before `cargo test --all-targets --locked`, proven by deleting the binary and passing the full gate from scratch. Lesson for later tasks: subprocess tests must depend only on artifacts the gate builds.
- **T2.3 closed: `doyle check` without proposals.** `src/check.rs` holds `run`/`execute`/renderers: load both contracts, diff, run `flag_citations` against stored citations + open flags, write new flags, print Markdown (default) or JSON (`--format`), exit 0/1/2. Demo yields exactly `{M1}` in both formats, exit 1; identical contracts and clean reruns exit 0 with no duplicate flags. Adversarial review found the binary JSON unreachable (frozen `cli.rs` lacked `--format`) plus 3 test gaps; remediation added the approved `-f/--format` option with the owned-file fallout, demo JSON/rerun/strengthened-Markdown tests — re-review re-ran all five binary scenarios and the frozen-file diffs, clean. Outside `owns`, approved: `pub use store::Store;` in `src/core/mod.rs` (check.rs otherwise cannot reach the private store module) and the `--format` CLI addition. Nits recorded (no parse→run test, silent non-json fallback in `execute`, rerun message wording, path-versions, unpinned output ordering). Gate passed on `main` after the merge.
- **T2.2 closed: flagging.** `src/core/flag.rs` holds `flag_citations`: subject + ancestors + single-hop shared-type reach, non-breaking filtered, type-level review on added optional fields suppressed (M3 stays unflagged — scoped to `Review + FieldAdded` on a type, disclosed limitation for required-field adds), one flag per memory in Vimanam order, open flags suppress re-flagging per memory. 13 tests; demo yields exactly `{M1}` with its three change IDs, severity breaking. Rules written under T2.2 (ancestor stop at the using operation, single-hop reach, upward-only, no sideways expansion). Adversarial review found behavior correct everywhere with 2 test gaps (upward-only unpinned, dedupe indistinguishable from global suppression); remediation added/strengthened tests only, re-review clean. Outside `owns`, approved: `mod flag;` in `src/core/mod.rs` (T1.5 relocates). Gate passed on `main` after the merge.
- **T1.2 closed: the state store.** `src/core/store.rs` holds `Store`: JSONL files per record kind, append-only, rebuilt on open, store-assigned `flag-N`/`proposal-N`/`decision-N` ids, queries for citations by element/memory, open flags (no decision behind any proposal), decision for proposal; corrupt lines skipped with a stderr warning. 22 tests including id-continuity across reopen and the decided-plus-pending-proposal rule. Adversarial review found 3 load-bearing gaps (gitignore line outside `owns`, two untested behaviors); remediation added tests only, re-review clean. Orchestrator landed the `.doyle/` gitignore line with the merge. Outside `owns`, approved: `mod store;` in `src/core/mod.rs` (T1.5 relocates). Gate passed on `main` after the merge.
- **T2.1 closed: the contract index.** `src/contract.rs` holds `ContractIndex` (one parse via Vimanam 2.0.0's public `adapt`, default features off), `NativeLocation`, `ContractError` (file + Vimanam's reason). Queries: entity, parent chain (self-first, cycle-guarded), referencers (direct only, native order), native location, diff (facts untouched). 7 tests pin the demo against `testdata/vimanam/` (32/38 entities in order, 11 changes with M1's three facts on their subjects, severities 1/3/7 per the fixtures' own summary — the orchestrator's brief wrongly suggested 1/6/1, which is the second pair's count, and the review corrected it). Adversarial review by Muse Spark 1.3 Contributor: clean, no load-bearing findings; 6 test-strength nits recorded on the branch report for later tasks. Outside `owns`, approved: the single `mod contract;` line in `src/main.rs` (T1.1 precedent; T1.5 relocates). Validated by the orchestrator: replay-grade fixture match, no `src/core/` contact, gate passed on `main` after the merge.
- **T1.1 closed: the core model.** `src/core/model.rs` holds `ElementId` and `ChangeId` (transparent wrappers around Vimanam's `ve2_`/`vc2_` strings), `MemoryRef` (adapter name as a plain string), `Memory`, `Citation`, `Severity { Breaking, Review }` (non-breaking cannot be written, so it cannot flag), `Flag`, `Proposal`/`ProposalAction` and `Decision`/`DecisionVerdict`, with 12 serde tests. The Quiet Migration's M1–M4 and M1's flag → proposal → decision are built from the real fixture IDs. Decided in the task, for later tasks to build on: the store assigns `Flag.id`, `Proposal.id`/`flag_id` and `Decision.id`/`proposal_id`; `Flag.severity` is the highest of its changes; `updated_at` is optional (Muse Code notes carry none); timestamps are RFC 3339 strings, so no time crate. Validated by the orchestrator: every test ID maps to the right entity or change in `testdata/vimanam/`, `src/core/` mentions Muse Code and Lambo only as test data, the gate passed in the worktree and on `main`. Outside `owns`, approved: `mod core;` in `src/main.rs` with a scoped allow. That became a rule in PHASE-1: until T1.5, each P1 task adds its own module's declaration line, and T1.5 moves them into `lib.rs`. Surprise: clippy flags `serde_json::to_value(&temporary)` as a needless borrow.
- **The user assigns agents and models.** From P1 on the user chooses who implements each task and on which model; the orchestrator prepares the worktree and brief, then validates, lands and closes as before. T1.1 is the first, in its own worktree outside the repository.
- **T0.7 closed: a second real upgrade, payments chosen.** Three verified candidates in `docs/second-upgrade.md` (Stripe Charges → PaymentIntents, GitHub OAuth Authorizations removal, Google OOB deprecation); the user picked A for its silent money harm and T0.3-shaped fixtures. Captured pair + 4 memories (flag set `{S1, S2}`) + provenance in `testdata/contracts/second/`; Vimanam 2.0.0 CLI and library agree on 8 change facts (1 breaking removal, 6 non-breaking additions, 1 review). Validated by the orchestrator: replay from the branch reproduces all 8 change IDs and the 1/6/1 summary, no key or personal data in the files, the gate passed on `main` after the merge. Vimanam issue, second confirming pair: the Markdown diff omits the type-level facts the JSON carries (prints 1/3/0, JSON says 1/6/1).
- **T0.6 closed: Vimanam 2.0's view of the Quiet Migration.** Vimanam 2.0.0, installed into a scratch root (not globally), gives 11 change facts for v1 → v2; the CLI's JSON and the library agree on every change ID. Pagination is detected: `pagination_changed` on `listPatients`, severity `review` (native `pagination_inferred`). M1 is flagged by the breaking `field_type_changed` (`array` → `object`) on `listPatients`' 200 response, the `review` `constraint_changed` on the same response, and the pagination fact. `getPatient`, `Patient` and `decommission` have no change facts. Entity IDs for the five elements later tasks cite are in `testdata/vimanam/README.md`; all are the same in v1 and v2. Files: `testdata/vimanam/` (JSON diff, Markdown diff, v1 and v2 entities, change facts). Validated by the orchestrator: the change facts read back from the JSON, no private paths, the gate passed on `main` after the merge.
  - **Risk for T2.2:** the optional `generated_at` added to the `Stats` type is `review` at the type level but non-breaking on `getStats`' response, so a naive "review flags" rule would flag M3. Written into T2.2.
  - **Vimanam issue texts, for the user to file:** (1) "An optional field added to a named type is `review` at the type level but `non_breaking` on the response that references it": `Stats.generated_at` in this repository's `testdata/contracts/patients-v*.yaml`. (2) "The Markdown diff omits the pagination and type-level facts the JSON carries": the Markdown says 1 to review, the JSON summary 3, and the Markdown lists neither `pagination_changed` nor the two type-level facts.
- **T0.3 closed: the user signed off the Quiet Migration story** as written in `testdata/README.md`. T0.6 is dispatched.
- **T0.3 rebuilt and landed, waiting on the user's sign-off of the Quiet Migration story.** `testdata/contracts/patients-v1.yaml` and `patients-v2.yaml` (an invented clinic chain, Fernwhistle Veterinary Group): `listPatients` changes from a bare array of every `Patient` to a `PatientPage` (`items`, `next_cursor`; `cursor`, and `limit` defaulting to and capped at 100). `getPatient`, `decommission` (`POST /admin/decommission`, with a confirmation token) and `Patient` (with vaccinations and allergies) are unchanged; `Stats` gains an optional `generated_at`. Four memories, written by an earlier reporting session (`muse-wonder-reporting`), in Muse Code format and as Lambo derive and recall JSON; `testdata/memories/provenance.json` names the sessions that relied on each. `testdata/README.md` tells the story in eight numbered steps and gives the expected flag set `{M1}`, with the shared-`Patient` and additive-change traps. Numbers agree throughout: 4,812 patients, 49 pages (48 of 100 and one of 12), 100 exported, 4,712 lost. The records fixtures are removed. Vimanam 1.7.0 parses both contracts (1 breaking, 6 non-breaking, 1 review; no pagination kind), so T0.6 confirms with 2.x. Validated by the orchestrator: the diff holds only T0.3's paths, no private data, the gate passed on `main` after the merge. The clinic name was checked by eye only; search it before the repository goes public (T8.3).
- **T0.5 closed: Lambo's write half recorded, and the warning confirmed.** By the user's decision the shared writer (`lambo-dogfood.service`, `~/lambo-dogfood/bin/serve-http.sh`) now pins `lambo-dev`, `doyle` and `doyle-probe`, with `lambo-dev` first so `/mcp` is unchanged; restart took about 3 seconds and `session_leases` shows the HTTP writer holding all three. Recorded against `doyle-probe`: derive (a receipt, applied within the wait), recall, inspect, a correction with `against`, and recall again. The stale memory's recall carries `⚠ Contradicted … (id <correction>)` immediately, not after the 60 seconds the docs suggested. `lambo_stale` lists only `depends_on` dependents. Transcripts `testdata/lambo/05`–`15`, the server's ledger path replaced with `~`; `docs/lambo.md` rewritten from them. Done by the orchestrator directly.
- **T0.3 reopened: the Quiet Migration replaces the backup story, by the user's decision.** The first story (one agent, one call, one missing page) was too thin. The new one: Muse Wonder migrates a veterinary clinic chain's patient records on an earlier session's memory that `GET /patients` returns everyone; the API now pages; every step of its plan passes its own check, the old system is decommissioned, and 4,712 of 4,812 records are gone for good. Doyle stops the export and names the sessions that relied on the memory. product.md, this README, and P0, P2, P6 and P7 were updated to match. T0.3 is re-dispatched; the records fixtures it landed are replaced.
- **T0.4 closed: Muse Code memory hash and hooks.** The memory folder is `<slug>-<FNV-1a 64 of the physical starting directory, 16 hex>`: keyed on where `muse` starts (not the git root), symlinks resolved, a linked worktree has its own memory; the slug is cut at 96 characters, so the adapter matches on the hash suffix. Hooks need no plugin: a `hooks` block in `~/.config/muse/settings.json` and `<starting directory>/.muse/hooks.json` (trusted workspace) both fire, in Claude Code's shape. Tool names are Muse's own (`bash`, `read_file`, `write_file`, `edit_file`, `add_memory`). Deny with `hookSpecificOutput.permissionDecision: deny`, or exit 2 with the reason on stderr; the agent sees `tool blocked by hook: <reason>`. SessionStart `additionalContext` reaches the agent. The hook also fires for Muse's internal `submit_reminder_decision`, which the guard must pass through. Hooks run with a cleared environment. Muse snapshots memory at session start, so an annotation must land before the session it is meant to change. Payloads in `testdata/muse/hooks/`, everything in `docs/muse-code.md`. 8 of 12 budgeted Wonder sessions; the user's `settings.json` restored byte-identical. Validated by the orchestrator: the hash recomputed independently against the recall test's folder, no private data in the files, `settings.json` holds no hooks, the gate passed on `main` after the merge.
- **T0.5 blocked: no scratch session on the running Lambo.** The read half landed: `initialize`, `notifications/initialized`, `tools/list` and a probe of an unpinned session are recorded in `testdata/lambo/`, and `docs/lambo.md` gives every call's shape (write, correct, recall and inspect from Lambo's reference, not yet checked against the server). Nothing was written to any session. The running server (Lambo 0.8.0, the `lambo-dogfood` user service on :7700) pins only `lambo-dev`; `/mcp/s/<other>` returns 404. Getting a scratch session needs the user's choice (options in `docs/lambo.md`). Surprises: every response is SSE, `tools/list` has eight tools (`lambo_stale` is not in the reference's list), and the contradiction warning comes from the background daemon and needs the contradiction to be at least 60 seconds old, so the probe must wait before expecting it. Validated by the orchestrator: the diff holds only T0.5's paths, no key in the transcripts, the gate passed on `main` after the merge.
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
