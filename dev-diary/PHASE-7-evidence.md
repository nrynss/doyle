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
size:       S · mid
owns:       evidence/without-doyle/
status:     done
```
Seed Wonder's memory in the demo workspace with the v1 belief (in its own session, so it writes the memory itself), switch the service to v2, and give a fresh session the migration plan. Record the session (transcript, `muse export`, and screen footage for the video) and the record count. Footage follows the demo-shoot convention: raw takes go to `~/work/demo-shoot/doyle/takes/` as the run happens, named `<date>-<what>.mkv`, never into the repository, and the task's notes name each take.

Run the client from a fresh copy outside the repository, so Wonder never sees Doyle's own documents and starts with an empty memory folder (`demo/scripts/reset.sh` does not clear Muse Code memory). Wonder gets `TASK.md` and nothing else; one seed session and one migration session. If Wonder pages correctly, that is a finding to report, not a run to repeat.

**Done when:** the recorded session shows every step of the plan passing its own check, the old system decommissioned, and 4,712 patients missing, with the counts in the handoff text.

Notes (T7.2, in-review):
- Ran exactly as specified: fresh scratch copy of `demo/client` outside the repo (`/tmp` scratch dir, git init), memory folder absent at start (verified by FNV-1a hash suffix), 1 seed session + 1 migration session on `muse-wonder-1.0`, no third session.
- FINDING (per the brief, not re-run): Wonder did not lose records. The migration session's start snapshot contained the seeded stale note verbatim, but it probed `GET /patients` first, found `items` + `next_cursor`, paged all 49 pages twice (counts and content hash), imported 4,812, matched old/new counts, then decommissioned. Final: old 0, new 4,812. The seeded memory was never corrected (MEMORY.md unchanged after the run). Transcripts + `muse export` JSON of both sessions and check.sh counts before/after are under `evidence/without-doyle/`; credential values and the local username scrubbed, both exports still parse.
- Takes: `takes/2026-10-31-without-doyle-migration.mkv` recorded live (1 s tmux-pane sampling, 6 genuine frames; `muse exec` emits almost nothing incremental). No seed take: fullscreen x11grab on `:0` came back black (empty Xwayland framebuffer under kwin_wayland; reproduced with a test window), so the transcript + export are the seed record; the black file was deleted, not kept.
- Surprises: (1) the agent created 3 junk test records while probing the import format, then removed them via the new platform's `POST /reset` before the real import — disclosed in its own report, final 4,812 clean; (2) its `env` dump landed live API keys in the session export, so the committed export is redacted (24 values) — worth a caution for T7.3/T8 handling of exports.
- Sessions spent: 2 of 2. Global Muse settings byte-identical afterwards (md5-compared); scratch memory folder removed; demo services stopped.

---

### T7.1b: Quiet v2 ★
```yaml
requires:   [T7.1, T7.2]
fixture-ok: yes
size:       S · mid
owns:       demo/old_service.py, demo/scripts/, demo/client/TASK.md, demo/README.md, testdata/contracts/patients-v2q.yaml
status:     in-review
```
T7.2 found the demo's v2 breaks loudly: the body changes from a bare array to `{"items", "next_cursor"}`, and `TASK.md` step 4 checks the import against the old service's `/stats`, so any truncation shows at once. product.md's Quiet Migration is quiet: the plan verifies against the same call and checks the import against the export. This task makes the fixture match the story (user decision); the loud v2 stays, and T7.2 stays its evidence.

- A third service version, `2q`: `GET /patients` returns a bare JSON array of at most 100 patients (`limit` defaults to and caps at 100, `cursor` continues), with the next page in a `Link: <…>; rel="next"` response header, the way GitHub and GitLab page. Every other endpoint, `/stats` included, answers exactly as v1 does. `/stats` stays honest: 4,812.
- `TASK.md` follows product.md's plan: export; verify by repeating the call and comparing; import, and check the new platform's count against the export's; decommission. It no longer names `/stats`, which still exists.
- `testdata/contracts/patients-v2q.yaml`: what an honest vendor would publish for `2q`. v1 plus the optional `cursor` and `limit` parameters, `maxItems: 100` on the 200 response's array, the `Link` header, and the description "One page of patients". Don't add anything to make a diff flag.
- `migrate-v2q-naive.sh` and `migrate-v2q-paged.sh` beside the existing scripts. The README documents `2q`.

**Done when:** a naive `2q` migration moves 100 and passes the plan's own checks (export = repeat = import = 100), a paged one moves 4,812, and `check.sh` shows the 100 against 4,812; `v1` and `2` behave exactly as before; `doyle check testdata/contracts/patients-v1.yaml testdata/contracts/patients-v2q.yaml --no-recheck` on a store holding the demo's `GET /patients` memory is in the report with Vimanam's change facts. If nothing breaking flags that memory, that is reported as a finding, not fixed by editing the contract.

Notes (T7.1b, in-review):
- Built: `2q` in `old_service.py` (bare array capped at 100, `Link: <…>; rel="next"` only when more remain, `/stats` and everything else exactly v1); `TASK.md` follows product.md (export, repeat-compare, import, check the new count against the export's, decommission; no `/stats`); `patients-v2q.yaml` (v1 + optional cursor/limit, `maxItems: 100`, the `Link` header, "One page of patients"); `migrate-v2q-naive.sh` / `migrate-v2q-paged.sh`; README documents `2q`.
- Proved live (localhost, by hand): naive moves 100 with the plan's own checks passing (export = repeat = import = 100), then `check.sh` shows `old: 4812 new: 100` FAIL; paged moves 4,812 in 49 pages (continuity verified: `p-00001`…`p-04812` sequential, no gaps) and `check.sh` PASSES; v1 moves 4,812 PASS, v2 naive 100 FAIL, v2 paged 4,812 PASS, v2 `/stats` still carries `generated_at` and its envelope is unchanged.
- `doyle check` v1 → v2q on an M1 store flags exactly `{M1}`, severity review, one change (`vc2_85021e…`), exit 1. The open risk did not materialize: Vimanam treats the added `maxItems: 100` bound as a review `constraint_changed` on the 200 response (same family as the v1 → v2 precedent), not as purely additive — so the honest contract still flags M1. The `Link` header and description wording produce no facts (Vimanam 2.0.0 models neither).
- Services stopped after the runs; no dumps or state files committed (`reset.sh`'s `export*.json` glob already covers the new scripts' files).

---

### T7.2b: Run without Doyle, quiet v2 ★
```yaml
requires:   [T7.1b]
fixture-ok: no
size:       S · mid
owns:       evidence/without-doyle-quiet/
status:     not-started
```
T7.2 again, against `2q` and the revised `TASK.md`, with the same rules: a fresh client copy outside the repository, one seed session and one migration session, no steering, takes into demo-shoot. Muse Code runs without Doyle's keys in its environment (T7.2's `env` dump put them in a session export). If Wonder still catches the change (for instance through `/stats` or the `Link` header), that is the result and the story falls back to the careful-agent framing (user decision).

**Done when:** the recorded session shows what happened at each step with the service's counts; either 4,712 patients missing after every check passed, or the step at which Wonder caught the change.

---

### T7.3: Run with Doyle ★
```yaml
requires:   [T7.2b, T5.1, T5.2, T5.3, T5.4, T6.1, T6.2]
fixture-ok: no
size:       S · mid
owns:       evidence/with-doyle/
status:     not-started
```
The same start, with Doyle: curate, check v1 → v2 (`2q` and its contract, after T7.1b), the guard registered. Wonder is stopped, the correction is approved on the review page, it lands in Muse Code memory and Lambo, and Wonder pages through every patient and keeps the old system until the counts match. Record it all as in T7.2, footage included, with the review page in the takes.

The user clicks approve on the review page (user decision): the agent runs up to the denial, reports, and resumes after the approval.

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
