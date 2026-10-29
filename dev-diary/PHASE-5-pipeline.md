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
status:     in-review
```
For each configured adapter, list memories, skip those already cited at their current text, run the curator against the current contract, and write citations. `--dry-run` prints what it would cite. A memory the curator ties to nothing is recorded as such, so it isn't asked again until its text changes.

**Done when:** on the fixtures, through the Muse Code and Lambo adapters on replayed transports and the replay model client, the citations match T4.3's expected set; a second run makes no model calls.

**Notes (T5.1)**
- Built: `src/curate.rs` implements `run` plus `curate`/`curate_refs`/`curate_all`/`curate_slice`/`run_curate` entry points. For each adapter it searches once per contract entity (query from native location: path, operation id, type/field), dedupes memories, skips those already cited at their current text (citation claim compared to memory text), runs the Baseten curator model, and writes citations. Rests-on-nothing memories are recorded as citations with empty element lists. `--dry-run` prints (memory, claim, element count) and writes nothing.
- Decided: citation claim stores the memory's text so skip-on-unchanged-text works across runs; the curator's restated claim is carried in the returned `CurateItem.claim` for reporting. Model id is the Baseten curator slug.
- Verified: Muse Code fixture (4 memories) and a scripted Lambo server (4 concepts) each produce 4 citations; a second run issues zero model calls and writes nothing; `--dry-run` leaves the store empty. `./tools/check.sh` passes (195 tests).

---

### T5.2: `doyle check` with proposals ★
```yaml
requires:   [T2.3, T4.4]
fixture-ok: yes
size:       S · mid
owns:       src/check.rs
status:     claimed:beam-free-t5.2
```
Extend T2.3: after flagging, run the re-check on each new flag, write proposals, and include them in both report formats. `--no-recheck` keeps T2.3's behaviour for CI without a key.

**Done when:** on the demo, with replayed replies, the stale memory's flag carries a correct proposal; `--no-recheck` gives T2.3's output unchanged.

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

- [ ] `doyle curate` cites memories from both systems, idempotently (T5.1).
- [ ] `doyle check` produces flags with proposals (T5.2).
- [ ] A decision lands in every system that holds the memory, and partial failure is visible (T5.3).
- [ ] The demo runs offline end to end: curate, check, decide, with fixtures and replays only.
