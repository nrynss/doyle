# P5: Pipeline

The parts joined: memories in through the adapters, citations from the curator, flags from a contract change, proposals from the re-check, and a person's decision written back to every memory system holding the memory.

Read first: [`../docs/architecture.md`](../docs/architecture.md) (Commands), P2–P4's phase files and handoff entries.

---

### T5.1: `doyle curate` ★
```yaml
requires:   [T1.2, T3.1, T3.3, T4.3]
fixture-ok: yes
size:       S · mid
owns:       src/curate.rs
status:     done
```
For each configured adapter, list memories, skip those already cited at their current text, run the curator against the current contract, and write citations. `--dry-run` prints what it would cite. A memory the curator ties to nothing is recorded as such, so it isn't asked again until its text changes.

**Done when:** on the fixtures, through the Muse Code and Lambo adapters on replayed transports and the replay model client, the citations match T4.3's expected set; a second run makes no model calls.

**Notes (T5.1)**
- Built: `src/curate.rs` implements `run` plus `curate`/`curate_refs`/`curate_all`/`curate_slice`/`run_curate` entry points. For each adapter it searches once per contract entity (query from native location: path, operation id, type/field), dedupes memories, skips those already cited at their current text (citation source_text compared to memory text), runs the Baseten curator model, and writes citations. Each adapter also gets one unfiltered (empty-query) pass so token-disjoint memories are not silently skipped. Rests-on-nothing memories are recorded as citations with empty element lists. `--dry-run` prints (memory, claim, element count) and writes nothing.
- Decided: skip-on-unchanged-text now keys on `Citation.source_text` (the full memory text); citation `claim` stores the curator's restated claim on both cited and rests-on-nothing paths, and `CurateItem.claim` carries the same restated claim for reporting. Model id is the Baseten curator slug.
- Verified: Muse Code fixture (4 memories) and a scripted Lambo server (4 concepts) each produce 4 citations; a second run issues zero model calls and writes nothing; `--dry-run` leaves the store empty. `./tools/check.sh` passes (199 tests).
- Remediated (review): skip state now keys on `Citation.source_text` (full memory text) instead of `claim`; citation `claim` restored to the curator's restated claim on both cited and rests-on-nothing paths. Empty-query unfiltered pass added per adapter so token-disjoint memories are curated. Added restart-skip, text-change re-trigger, dry-run-empty-store, exact-set and rests-on-nothing tests. Binary fixture runs remain harness-only (hardcoded demo paths); dry-run guarantee pinned by unit test.

---

### T5.2: `doyle check` with proposals ★
```yaml
requires:   [T2.3, T4.4]
fixture-ok: yes
size:       S · mid
owns:       src/check.rs
status:     done
```
Extend T2.3: after flagging, run the re-check on each new flag, write proposals, and include them in both report formats. `--no-recheck` keeps T2.3's behaviour for CI without a key.

**Done when:** on the demo, with replayed replies, the stale memory's flag carries a correct proposal; `--no-recheck` gives T2.3's output unchanged (markdown byte-identical; JSON keeps T2.3's flags with `null` proposal slots).

**Notes (T5.2)**
- Built: `src/check.rs` extended. `execute`/`run` now take `--no-recheck` from `Command::Check`. When off, after flagging each new flag is re-checked: citation lookup, `RecheckInput` build (reusing a committed measurement case when its change-id set matches, so the replay client serves the recorded reply; generic fallback otherwise), `build_request`, `ModelClient::complete`, strict `parse_response`, guideline-7 enforced by the parser, proposal written via `Store::record_proposal`. Proposals rendered in both markdown and json (per-flag `proposal` + `proposals` array when non-empty).
- Approved cli.rs change: added `no_recheck: bool` to `Command::Check`, parsed `--no-recheck`, updated `CHECK_HELP`. No other cli.rs changes.
- `--no-recheck` keeps T2.3 behaviour exactly: no model calls, no proposals in output, markdown byte-identical, JSON keeps T2.3's flags with `null` proposal slots, identical exit codes.
- Default model client is a `ReplayClient` over `testdata/replies/recheck` (override via `DOYLE_RECHECK_REPLAY_DIR`); no key lookups. `execute_with_model` allows injection for tests.
- Retry policy (L1): an open flag with no proposal is still needing work, so a rerun re-attempts the re-check for it; the flag itself is never duplicated (open-flag dedup by (adapter, memory id)), and once a proposal is recorded the flag is skipped. A flag that gets no proposal (replay miss or parse failure) emits a per-flag diagnostic on stderr naming the flag id and the cause, and stays proposal-less so the next run retries. The diagnostic repeats on each failing rerun.
- Gate passes: 199 tests.

---

### T5.3: Decisions and write-back ★
```yaml
requires:   [T1.2, T3.2, T3.3]
fixture-ok: yes
size:       M · frontier
owns:       src/core/decide.rs
status:     done
```
Apply a person's decision to a proposal: approve (write the correction), edit then approve, retire, or keep. Write it through every adapter holding the memory, by each one's capabilities: supersede where supported, else annotate, else write a new memory and report that the old one stays. Record the decision and, per adapter, what was done. A partial failure (Lambo down, Muse Code fine) leaves the decision recorded with the failed adapter marked for retry, never half-silent.

**Done when:** tests through the fake and both adapters (replayed) cover each decision and the partial failure; after an approve on the demo, the Muse Code fixture note and the Lambo transcript show the correction.

**Notes (T5.3)**
- Built: `src/core/decide.rs` holds `apply_decision` (record decision FIRST, then write each target adapter; one mutating call per adapter), `retry_failed` (no store write; identical text so retries land on the adapter no-ops), `Target { adapter, memory_id }` (the caller resolves which copies hold the logical memory — e.g. M1 under one id in Muse Code, another in Lambo), `DecisionReport { decision_id, proposal_id, results }` with one `PerAdapterResult` per target, and `AdapterOutcome { Superseded, Annotated, WrittenNew { stale_remains: true }, Kept, Failed { message, retryable } }`. The verdict wins over the proposal: `Approve` follows the proposal's action, `Retire` retires, `Keep` keeps, `EditThenApprove` corrects with the person's text. `DecideError` is store-failure only; adapter failures are report entries, never errors.
- Routing table (decision x capability -> calls): Correct -> supersede where supported, else annotate with `[STALE correction: <text>]` (the correction rides in the warning, since no correction memory can be made), else write in the stale memory's scope (read first for the scope) and report the old one stays. Retire -> supersede with `RETIREMENT_TEXT`, else annotate `[RETIRED: no longer applies]`, else write the notice and report the old one stays. Keep -> no adapter call, decision still recorded.
- Retry-convergence argument, with the stated assumption: every retry re-issues the identical text, and each adapter's `supersede` is idempotent for identical (stale, text) — Muse Code reuses the identical-text correction, Lambo checks cache then inspects for the existing against-link, the fake checks pointer plus text (all pinned by their own retry tests). Tested: same-adapter retry is a cache no-op, fresh-adapter retry (empty caches, as after a restart) inspects first and derives nothing, Muse Code re-apply returns the same ref with exactly one correction bullet. Limits disclosed in the module docs: `annotate` is NOT idempotent, so only `Failed` targets are retried; the write-fallback mints per call, so a timed-out-but-landed write could duplicate (no write-only adapter exists today — both real adapters support `supersede`).
- Stop-gate outcomes (no STOP needed): the store HAS `record_decision`; it has NO per-adapter outcome record, and none was added — outcomes live in the returned `DecisionReport`, retry state with the caller until the review page owns it (T6.2). Recording the decision settles the flag in store terms, so surfacing pending retries is the caller's job. The trait already had every hook needed (`supports`, `supersede`/`annotate`/`write`/`read`, typed errors); `NotFound`/`Unsupported` map to non-retryable `Failed`, `Backend` to retryable.
- `src/core/decide.rs` imports the `MemoryAdapter` seam trait: the first `crate::adapters` use under `src/core/`. Compliant with guideline 4 in substance (no system named, uniform capability routing, no Lambo path), noted here.
- Wording note for the orchestrator (no file touched): `testdata/README.md` says the correction lands in Muse Code "as an annotation on the note", but T5.3's specified routing is supersede-first and Muse Code supports `supersede` — the demo approve yields `[Superseded by MEMORY.md#N]` plus a new correction note, not `[STALE ...]`. The stale-marking asserted is the superseded-by pointer; `[STALE correction: ...]` appears only on the annotate-fallback path (covered through a reduced fake).
- Verified: 14 tests — each verdict/proposal combination through the full fake, annotate-only and write-only fakes, the real Muse Code adapter on a temp fixture copy (M1 approve asserts the superseded-by pointer and correction content), the real Lambo adapter over a transcript-shaped scripted server (approve asserts the Contradicted warning, correction content, and the against-linked derive naming M1's short id), and the partial failure (failing Lambo + working Muse Code -> decision recorded, Lambo retryable-Failed; healed retry -> exactly one correction; cache, inspect-first and Muse re-apply converge). Offline throughout: temp dirs, temp store, scripted transports; no HOME mutation, no network, no live server. `./tools/check.sh` passes (248 tests).

---

## Exit Criteria

- [x] `doyle curate` cites memories from both systems, idempotently (T5.1).
- [x] `doyle check` produces flags with proposals (T5.2).
- [x] A decision lands in every system that holds the memory, and partial failure is visible (T5.3).
- [ ] The demo runs offline end to end: curate, check, decide, with fixtures and replays only.
