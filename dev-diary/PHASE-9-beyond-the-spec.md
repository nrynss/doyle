# P9: Beyond the spec

Doyle's evidence grows past API specs to the other things a migrating agent learns from: legacy COBOL code, the screens of the old system, and recordings of people using it. Muse Wonder does the migration, the COBOL rewrite included. Doyle keeps what Wonder believes true and stops it where a stale belief would do damage (product.md, Beyond the spec).

**Doyle's eyes are Wonder's.** Every reading of a screen or recording is a call to `muse-wonder-1.0` on the Meta Model API. Doyle stores what Wonder saw as evidence with stable IDs and never reads pixels itself.

**The rule this phase keeps:** a flag is still computed, never a model's word. Wonder's vision turns pixels into an element list once per version; the diff between two lists, and the flag that follows, is deterministic. A differential run is evidence by execution, not by opinion.

**It does not disturb P7.** T7.2 and T7.3 record the API-only Quiet Migration as planned. Whether the legacy and screen evidence also goes into the recorded runs and the video is the user's decision (T9.4), taken once the probes report.

**Module declarations (orchestrator decision):** each task here may add the one declaration line its own module needs (`mod legacy;` and `mod screens;` in `src/evidence.rs`, `mod vision;` in `src/models/mod.rs`, `pub mod evidence;` in `src/lib.rs`), and nothing else in a file it does not own.

Read first: [`../product.md`](../product.md) (Beyond the spec), [`../docs/architecture.md`](../docs/architecture.md) (Evidence sources), `demo/README.md`, and the multimodal probe findings in the handoff log.

---

### T9.1: Legacy runtime
```yaml
requires:   []
fixture-ok: no
size:       S · mid
owns:       docs/legacy.md, testdata/legacy/runtime/
status:     claimed:msc-t9.1
```
Make COBOL run on this machine and in the gate's world. GnuCOBOL is not installed and not in the main package repositories: try the AUR package, then a container, and record which works, with versions. Compile and run a small batch program that reads a fixed-width file through a copybook, writes a report, and uses `ROUNDED` arithmetic and a `PERFORM` of a named paragraph. Find out how to trace which paragraphs ran for a given input (`-ftrace`, `-ftraceall`, or a `DISPLAY` convention) and what the trace looks like.

The gate must not need COBOL: anything that runs the compiler is a live test or a recorded output.

**Done when:** `docs/legacy.md` says how to install and run GnuCOBOL here, with versions; the sample program, its input, its output and a paragraph trace are in `testdata/legacy/runtime/`; the gate passes without `cobc` installed.

---

### T9.2: Wonder's eyes, measured
```yaml
requires:   []
fixture-ok: no
size:       M · frontier
owns:       docs/wonder-vision.md, testdata/vision/
status:     claimed:msc-t9.2
```
The first probe ran each case once, on clean web pages and a 12-second clip of still screens. Measure what Doyle will rely on:
- a 3270-style green screen (80×24, monospace, field labels and protected fields), rendered as an image: element list against ground truth;
- a real screen recording with motion (typing, scrolling, a menu opening), 60–120 seconds: step list and timestamps against ground truth;
- the before/after diff on three pairs, including one pair with no real change (does Wonder invent one?);
- repeatability: each case three times, and how much the answers move;
- limits: largest video Wonder accepts as a `video_url` data URL, its token cost per second, and whether a file URL or upload exists;
- the request shapes that work and those that fail, with the error text.

Reasoning tokens count against `max_tokens` on the Meta API: set `reasoning_effort` and the cap deliberately, and record what you used.

Budget: $3. Run one case first, check its cost, and stop and report if the rest would go over.

**Done when:** `docs/wonder-vision.md` gives each case's score, spread across runs, cost and the shapes that work; the ground truth and recorded replies (no key) are in `testdata/vision/`; it says plainly which of screens, diffs and recordings Doyle can build on and which it cannot.

---

### T9.3: Citing legacy code
```yaml
requires:   [T9.1]
fixture-ok: yes
size:       M · frontier
owns:       docs/legacy-ids.md, testdata/legacy/ids/
status:     not-started
```
Decide how a memory cites COBOL, before any code. An element is a program, a paragraph or section, a copybook record or field (with its level path and `PIC`), or a file. Its ID must stay the same across versions when the element means the same thing, the way Vimanam's do: derive it from the program ID and the element's name path, never from line numbers. Say what counts as a change to each kind (a field's `PIC` changes, a paragraph's statements change, a paragraph is removed) and its severity.

Then check the curator can cite: give Muse Spark Open three business-rule memories and the sample program's element list, and see whether it names the right paragraph and field. Recorded replies only in the gate.

**Done when:** `docs/legacy-ids.md` states the ID scheme, the change kinds and severities, with worked examples on the T9.1 program; a recorded curator run cites at least two of three memories correctly, or the doc says why it cannot.

---

### T9.4: The story, extended
```yaml
requires:   [T9.1, T9.2]
fixture-ok: yes
size:       M · frontier
owns:       testdata/legacy/story/
status:     not-started
```
Write the extension of the Quiet Migration as fixtures, the way T0.3 wrote the original:
- the COBOL billing batch (a few hundred lines), its copybook and data for the same 4,812 patients, in a v1 and a v2;
- the green-screen operator console in v1 and v2, as screens and a short training video for v1;
- Wonder's legacy memories, at least one per kind: a business rule that was never right (for example, a late fee Wonder remembers as charged on the whole balance, where the code exempts vaccinations), and a console procedure the v2 redesign changed (the decommission confirmation that v2 drops);
- the expected flags for each, with the traps that must not flag;
- the migration plan, grown to: move the records, rewrite the billing batch in Rust, prove the rewrite matches, retire the console.

**The user decides** the traps and whether this goes into the recorded runs and the video. Bring two or three options for each trap, with a recommendation, to the orchestrator before writing the fixtures.

**Done when:** `testdata/legacy/story/README.md` tells the extended story step by step with the expected flags; the COBOL compiles and runs under T9.1's runtime; the user has signed off the traps.

---

### T9.5: Evidence-source seam ★
```yaml
requires:   [T9.3]
fixture-ok: yes
size:       M · frontier
owns:       src/contract.rs, src/core/flag.rs, src/evidence.rs, and the one declaration line in src/lib.rs
status:     not-started
```
Today flagging reads Vimanam's index directly. Put an evidence source behind a trait: index a version into elements (ID, kind, parent, references, native location) and diff two versions into change facts (ID, subject, kind, severity). Vimanam becomes the first implementation with no change in behaviour; the legacy and screen sources plug in beside it. Element and change IDs carry their source in a prefix (`ve2_`/`vc2_` stay Vimanam's), so `ElementId` and `ChangeId` keep their shape and nothing in `testdata/` changes.

This touches frozen code, so it is the orchestrator's call, made here. Every existing test passes unchanged; that is the proof.

**Done when:** flagging runs over the trait; the Vimanam source passes every existing test, untouched; a fake second source in a test flags a memory through the same path; the guard still matches native locations from any source.

---

### T9.6: Legacy source ★
```yaml
requires:   [T9.5, T9.4]
fixture-ok: yes
size:       L · frontier
owns:       src/evidence/legacy/
status:     not-started
```
The evidence source for COBOL, on T9.3's scheme: parse copybooks and the procedure division into elements (no compiler needed for parsing), and diff two versions into change facts. A parser for the subset the fixtures use is enough; say what it does not handle.

**Done when:** on T9.4's v1 and v2, the diff gives the changes the story expects and no others; the elements and IDs match `docs/legacy-ids.md`; tests cover each change kind.

---

### T9.7: Screen source ★
```yaml
requires:   [T9.5, T9.2, T9.4]
fixture-ok: yes
size:       L · frontier
owns:       src/evidence/screens/, src/models/vision.rs, src/prompts/vision/, and the media parts of ModelRequest in src/models/mod.rs
status:     not-started
```
Adding image and video parts to `ModelRequest` changes a frozen contract. It is approved here, on one condition: every existing recording still replays with the same key.

The evidence source for screens and recordings. One Wonder call per version turns frames or a recording into elements and steps (screen, element, action, time span), each given a stable ID derived from what it is, never from its order. The diff between two versions is computed, not asked of the model. Image and video parts go to the Meta client as T9.2 found works; the replay client serves them offline, keyed on the media's hash.

Cost is stated before a call is made, and each call is recorded with its usage.

**Done when:** on T9.4's console v1 and v2, from recorded replies, the diff gives the changed procedure and no invented change; a memory citing the training video's span is flagged; the replay key test covers media; the stated cost of the recorded run is in the handoff text.

**T9.7b (live):** run the screen source on the fixtures against `muse-wonder-1.0` and compare with the recorded replies. Owns `tests/live_screens.rs`. Budget: $2.

---

### T9.8: Differential run ★
```yaml
requires:   [T9.6, T9.1]
fixture-ok: yes
size:       M · frontier
owns:       src/evidence/diffrun.rs, testdata/legacy/diffrun/
status:     not-started
```
Evidence by execution. Given the COBOL batch, a rewrite and a set of inputs, run both, compare outputs record by record, and for each mismatch read the paragraph trace (T9.1) to find which paragraphs ran. A memory citing a paragraph on a mismatched path is flagged, with the inputs and both outputs as its evidence. Running COBOL is live; the gate runs on recorded traces and outputs.

Doyle does not write the rewrite. In tests the rewrite is a fixture; in the demo it is Wonder's.

**Done when:** on recorded runs, the planted wrong rule is flagged with its mismatching records as evidence, and a correct rewrite flags nothing.

---

### T9.9: Review page, extended
```yaml
requires:   [T9.7, T9.8]
fixture-ok: yes
size:       M · frontier
owns:       src/serve/evidence.rs, and the routes it adds in src/serve/
status:     not-started
```
The review page shows each flag's evidence in its own terms: for a screen, the old and new frames side by side with the changed element marked and the cited span playable; for legacy code, the paragraph before and after; for a differential run, the inputs and both outputs. Same approve / edit / retire / keep.

**Done when:** each evidence kind renders on the demo fixtures; a person settles a screen flag and a legacy flag from the page; the page has been looked at in a browser at phone and desktop width, light and dark.

---

### T9.10: Runs, extended ★
```yaml
requires:   [T9.4, T9.7, T9.8, T9.9, T7.3]
fixture-ok: no
size:       S · user
owns:       evidence/legacy/
status:     not-started
```
If the user put the extension into the runs (T9.4): the extended migration without and with Doyle, recorded as in T7.2 and T7.3. Without Doyle, Wonder's rewrite carries the wrong rule and the console decommission goes through unconfirmed. With Doyle, the differential run and the re-watched console flag both, the guard stops the step, and the corrections land in Muse Code and Lambo.

**Done when:** both recordings show what the story in `testdata/legacy/story/` says, with the counts and mismatches in the handoff text.

---

## Exit Criteria

- [ ] COBOL runs here and the gate does not need it (T9.1).
- [ ] Wonder's vision is measured, with what Doyle can and cannot build on (T9.2).
- [ ] A memory can cite legacy code by a stable ID (T9.3).
- [ ] The extended story and its traps are signed off by the user (T9.4).
- [ ] Flagging runs over evidence sources, with Vimanam's behaviour unchanged (T9.5).
- [ ] Legacy code, screens and differential runs each flag the planted memory and nothing else (T9.6, T9.7, T9.8).
- [ ] A person settles each kind of flag from the review page (T9.9).
- [ ] The extended runs are recorded, if the user chose them (T9.10).
