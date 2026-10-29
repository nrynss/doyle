# P7: Evidence

The demo, run for real and recorded, and the numbers that say whether Doyle works. The judges' page shows them.

Read first: [`../product.md`](../product.md) (The moment the demo is built around, For judges), [`README.md`](README.md) (Critical Path).

---

### T7.1: Demo workspace ★
```yaml
requires:   [T0.3]
fixture-ok: yes
size:       M · mid
owns:       demo/
status:     done
```
What Muse Wonder works in during the demo:
- the old patient records service, serving the v1 or v2 behaviour of `testdata/contracts/` on a local port with 4,812 fixed patients, and the decommission operation that really deletes them;
- the new platform it migrates into, which accepts an import and reports its count;
- a client repository with the migration plan as a task prompt (export, verify, import, decommission), its own `.agents/` and Muse Code configuration, and a script that resets everything to the starting state;
- a check that counts the patients on the new platform against the old system's.

**Done when:** `demo/README.md` says how to start both systems in either version, reset the workspace and count; a v1 migration moves every patient and a naive v2 migration provably moves 100.

Notes (T7.1):
- Built `demo/` only: `old_service.py` (v1/v2 on port 8091), `new_platform.py` (port 8092), scripts, `client/` repo, README.
- Old service generates 4,812 patients deterministically (seed 20240517); v2 pages 49 pages (48x100 + 12); decommission requires token `CONFIRM-DELETE-ALL-PATIENTS`.
- Verified: v1 migration imports 4,812 and check passes; naive v2 imports 100 and check fails (100 vs 4,812); paged v2 imports 4,812 and passes.
- Port assignments: old 8091, new 8092, Doyle serve reserved 8080.
- No Rust/Cargo changes; gate passes.

---

### T7.2: Run without Doyle ★
```yaml
requires:   [T7.1]
fixture-ok: no
size:       S · user
owns:       evidence/without-doyle/
status:     not-started
```
Seed Wonder's memory in the demo workspace with the v1 belief (in its own session, so it writes the memory itself), switch the service to v2, and give a fresh session the migration plan. Record the session (transcript, `muse export`, and a screen recording for the video), and the record count.

**Done when:** the recorded session shows every step of the plan passing its own check, the old system decommissioned, and 4,712 patients missing, with the counts in the handoff text.

---

### T7.3: Run with Doyle ★
```yaml
requires:   [T7.2, T5.1, T5.2, T5.3, T6.1, T6.2]
fixture-ok: no
size:       S · user
owns:       evidence/with-doyle/
status:     not-started
```
The same start, with Doyle: curate, check v1 → v2, the guard registered. Wonder is stopped, the correction is approved on the review page, it lands in Muse Code memory and Lambo, and Wonder pages through every patient and keeps the old system until the counts match. Record it all as in T7.2.

**Done when:** the recorded session shows the denial with its reason, the approval, the corrected memory in both systems, and every patient on the new platform.

---

### T7.4: Measures
```yaml
requires:   [T7.3]
fixture-ok: no
size:       M · frontier
owns:       evidence/measures.md, evidence/measures/
status:     not-started
```
The numbers the judges' page states: memories flagged, wrong actions prevented, false alarms, citations the curator got right, proposals accepted unedited. Measured on the demo and on the second upgrade if T0.7 chose one, each with how it was counted.

**Done when:** `evidence/measures.md` states each number with its method and the files behind it.

---

### T7.5: Judges' page
```yaml
requires:   [T6.2, T7.4]
fixture-ok: yes
size:       S · mid
owns:       src/serve/judges.rs
status:     not-started
```
Under `doyle serve`, "For judges: start here": a table linking the recorded runs with and without Doyle, the measures, the Muse Code session logs, and the review page itself (product.md, For judges). It says plainly what is new (Doyle) and what is an existing dependency (Vimanam, Lambo).

**Done when:** the page serves from a fresh clone with the evidence committed, and every link resolves.

---

## Exit Criteria

- [ ] The demo workspace resets and counts patients (T7.1).
- [ ] Both runs are recorded, and they differ the way product.md says (T7.2, T7.3).
- [ ] The measures are stated with their method (T7.4).
- [ ] The judges' page links all of it (T7.5).
