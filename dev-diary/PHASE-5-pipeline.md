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
status:     not-started
```
Apply a person's decision to a proposal: approve (write the correction), edit then approve, retire, or keep. Write it through every adapter holding the memory, by each one's capabilities: supersede where supported, else annotate, else write a new memory and report that the old one stays. Record the decision and, per adapter, what was done. A partial failure (Lambo down, Muse Code fine) leaves the decision recorded with the failed adapter marked for retry, never half-silent.

**Done when:** tests through the fake and both adapters (replayed) cover each decision and the partial failure; after an approve on the demo, the Muse Code fixture note and the Lambo transcript show the correction.

---

## Exit Criteria

- [x] `doyle curate` cites memories from both systems, idempotently (T5.1).
- [x] `doyle check` produces flags with proposals (T5.2).
- [ ] A decision lands in every system that holds the memory, and partial failure is visible (T5.3).
- [ ] The demo runs offline end to end: curate, check, decide, with fixtures and replays only.
