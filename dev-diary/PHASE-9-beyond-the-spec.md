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
status:     done
```

Notes (msc-t9.1): done, awaiting orchestrator validation.
- AUR first, as planned: `gnucobol 3.2-8` PKGBUILD fetched, upstream tarball
  verified, PGP key imported. System install needs sudo, which this machine
  does not grant, so the same source was built into
  `$HOME/.local/gnucobol-3.2` with `--without-db` (Berkeley DB headers
  absent here); json-c and libxml2 configured as upstream. `make` clean,
  `cobc (GnuCOBOL) 3.2.0` against GCC 16.2.1.
- Container second: `debian:stable-slim` +
  `apt-get install gnucobol` (metapackage v5 pulls `gnucobol3 3.2-3) gives
  `cobc (GnuCOBOL) 3.2.0` against C 14.2.0. Both compilers build and run
  the sample identically.
- Not GnuCOBOL, noted and skipped: `gcc-gcobol` 16.2.1 in the main repos
  is the GCC COBOL frontend, a different compiler.
- Sample: `BILLBATCH.cbl` + `patient.cpy` + 5-record `patients.dat`;
  `ROUNDED` rounds both ways (0.49995 up to 0.50, 14.99985 up to 15.00),
  exempt record carries 0.00; totals 5 / 1,393.33 / 17.15 in `report.txt`.
- Trace: `-ftrace` at compile plus `COB_SET_TRACE=1` at run; without the
  variable the binary is silent. One stderr line per paragraph with source
  line; `-ftraceall` adds statement lines (OPEN/READ/PERFORM/UNTIL).
  Recorded in `trace.txt`; the exempt record shows LOG-EXEMPT where the
  others show CALC-LATE-FEE.
- Gate: `./tools/check.sh` passes with no `cobc` on PATH, and no file under
  `src/`, `tests/`, `tools/` or `Cargo.toml` names cobc/gnucobol. All
  compiler runs were by hand; nothing committed runs the compiler.
- Surprise: `-ftrace` alone prints nothing — the runtime switch is
  undocumented in `cobc --help` and only in the runtime configuration;
  worth one line in T9.8's runner so a silent trace is not mistaken for a
  clean one.
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
status:     done
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

Notes (msc-t9.2): measured on `muse-wonder-1.0`, `reasoning_effort low`, 22 replies at 200 + 3 at 400, ~127k tokens, estimated under $2 of the $3 budget (no cost field; basis in the doc). Screens: recall 0.92-0.95, kinds/protection perfect, same two texts misread every run. Diffs: 18/18 caught, `[]` x3 on the identical pair. Recording (60 s): all 7 steps found x3, onsets within ~1 s, one run fragments static holds. Limits: 11 MB data URL and 120 s accepted (lower bounds); remote https URLs are fetched server-side; no upload endpoint (`GET /v1/files` 404). Working: `image_url`/`video_url` data URLs, json_schema with vision. Failing with error text: `input_video` and bare `video` part types, tiny caps (reasoning eats `max_tokens`: 3000 truncates a 37-element list). One generator bug along the way (a footer edit leaked into a message row) was caught by the model as a real fourth change; the runs were discarded and re-done clean. Verdict in the doc: screens and diffs buildable; recordings buildable with +-5 s span padding.

---

### T9.3: Citing legacy code
```yaml
requires:   [T9.1]
fixture-ok: yes
size:       M · frontier
owns:       docs/legacy-ids.md, testdata/legacy/ids/
status:     done
```

Notes (msc-t9.3): scheme decided and probe recorded, awaiting orchestrator validation.
- `docs/legacy-ids.md`: six citable kinds (program, paragraph, section,
  copybook record/field, file); `le2_`/`lc2_` prefixes; ID = SHA-256 hex
  of a canonical `legacy:<PROGRAM>:<kind>:<path>` key, never line numbers;
  `PIC`/`ASSIGN`/order are attributes. Change table with severities
  (statement change, removal, PIC change and record-layout change break;
  adds are non-breaking; ASSIGN/mode changes review; rename = remove+add).
  Worked examples with computed IDs for all 15 BILLBATCH elements.
- Probe: 3 business-rule memories (true L1, false-of-code L2, layout L3)
  against the 15-element list on Muse Spark Open via Baseten. 3/3 cite
  the right paragraph/record/field (L1 program+CALC-LATE-FEE+BALANCE 0.93;
  L2 LOG-EXEMPT+EXEMPT-FLAG plus context 0.88; L3 record+all four fields
  1.0); no unknown IDs. Recordings in `testdata/legacy/ids/`, no key.
- Spend: 4 calls (L1 ok; L2 first attempt null content, retry ok; L3 ok).
  Tokens in/out: 1135/1350, 1125/1694, 1143/1555; no cost_usd returned.
  L2's null-then-success matches the T4.3 starvation finding; default
  effort sufficed on retry.
- Surprise: the model needed no COBOL-specific prompting — the mirrored
  curator prompt with a program element list cited precisely, including
  the false memory (citation is not fact-checking).
- For T9.6: parser subset and the offset-recompute rule are in the doc's
  closing notes; the sample has no sections, so section IDs are
  decided-but-unprobed.
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
status:     done

**Orchestrator decision (user): the extension goes into the recorded runs and the video, not offline-only.** The story fixtures must support the extended runs (T9.10) and the video (T8.2).
```
Write the extension of the Quiet Migration as fixtures, the way T0.3 wrote the original:
- the COBOL billing batch (a few hundred lines), its copybook and data for the same 4,812 patients, in a v1 and a v2;
- the green-screen operator console in v1 and v2, as screens and a short training video for v1;
- Wonder's legacy memories, at least one per kind: a business rule that was never right (for example, a late fee Wonder remembers as charged on the whole balance, where the code exempts vaccinations), and a console procedure the v2 redesign changed (the decommission confirmation that v2 drops);
- the expected flags for each, with the traps that must not flag;
- the migration plan, grown to: move the records, rewrite the billing batch in Rust, prove the rewrite matches, retire the console.

**The user decides** the traps and whether this goes into the recorded runs and the video. Bring two or three options for each trap, with a recommendation, to the orchestrator before writing the fixtures.

**Done when:** `testdata/legacy/story/README.md` tells the extended story step by step with the expected flags; the COBOL compiles and runs under T9.1's runtime; the user has signed off the traps.

Notes (msc-t9.4-fixtures): fixtures landed per the signed-off options (A2, B1-as-only-diff, C1, D1-alone). All under testdata/legacy/story/, deterministic, no model calls.
- Batch: v1 217 lines / v2 243 lines, same 4,812 patients (record i = API p-%05d). v1 copybook 34 chars with VISIT-CODE (replaces T9.1's EXEMPT-FLAG mechanism); v2 35 chars, the ONLY diff being APPLY-PROMPT-DISCOUNT + DISCOUNT-FLAG (+ its CHARGE-ACCOUNT wiring, cited by nothing). Both compile and run under docs/legacy.md (GnuCOBOL 3.2.0 user-prefix build): v1 fee $63,904.43 with 535 LOG-EXEMPT / 4,277 CALC-LATE-FEE; v2 fee $63,732.15 over 107 discounts; CROSS-FOOT OK both.
- Rewrites: wonder-bill.rs (carries L1) mismatches exactly the 535 V-records ($8,308.80 overcharge); fixed-bill.rs is byte-identical to the COBOL detail + TOTALS (cmp clean). rustc -O, no crates, run against v1/patients.dat.
- Screens: 4 stills + ground truths; lookup pair differs by PF5 UPDATE->SAVE only (cited by nothing); decomm pair drops warning + prompt, CONFIRM->DECOMMISSION. training-v1.mp4 45 s / 121 KB (within T9.2 limits); S1 cites T5-T6, S2 cites T2-T3.
- Elements: elements-v1/v2.json with computed le2_ IDs (20/20 shared stable; CALC-LATE-FEE matches the doc). legacy-diff.json: 4 facts, only the layout change flags (L2).
- Memories: L1-L4 + S1-S2 in Muse Code format, lambo.json, provenance.json; plan.md beats B1-B5.
- Takes (outside repo): takes/2026-10-31-console-v1.mkv, 2026-10-31-console-v2.mkv, 2026-10-31-training-v1.mkv.
- Surprise: 7 of the 15 worked le2_ IDs printed in docs/legacy-ids.md do not equal SHA-256 of their own canonical keys (truncated/transposed hex: PROCESS-ALL, LOG-EXEMPT, WRITE-DETAIL, WRITE-TOTALS, BALANCE, ACCT-FILE, REPORT-FILE). Story fixtures use true hashes; T9.6 must compute, not copy. Doc fix is T9.3's file, left for the orchestrator.
- Gate passes (fixtures only; cobc/rustc/ffmpeg ran by hand, nothing committed runs them).
- Constraint recalled (orchestrator decision, recorded above): the extension goes into the recorded runs (T9.10) and the video, so every option below is judged on two bars — performable by Wonder in a T7-style run (a denial without Doyle, a correction with Doyle) and filmable in three minutes (each failure and each correction is one shot, not a montage). For each option the without-Doyle failure and the with-Doyle correction landing are stated.
- Shared scaffolding every option assumes. The batch is BILLBATCH grown to a few hundred lines: the T9.1 paragraphs plus account-ageing, a fee schedule and a summary section, reading one fixed-width record per patient (4,812 records, same patients as the API story) through an extended copybook, in a v1 and a v2. The console is the T9.2 3270 style: a lookup screen and a decommission screen in v1 and v2, plus a short training recording for v1 only (v2 is never trained, it is only re-watched). Wonder's legacy memories are L-rules (business rules citing paragraphs/fields) and S-procedures (console steps citing screens and video spans), alongside M1–M4, whose expected flag set stays exactly `{M1}` from the API diff. The grown plan: move the records (M1 beat, unchanged), rewrite the billing batch in Rust, prove the rewrite matches, retire the console.
- Beats the traps hang on. B1 export: single `GET /patients`, 100 rows, check passes (M1). B2 rewrite: Wonder ports BILLBATCH to Rust carrying its remembered rule. B3 prove: Wonder's self-check compares rewrite output against the batch on a sample it picks. B4 retire: Wonder follows its remembered decommission procedure on the v2 console. B5 consequence: wrong fees billed at scale plus an unconfirmed decommission. With Doyle: B1 is denied at the export as today; B2/B3 are stopped by the differential-run flag with mismatching records as evidence; B4 is stopped by the screen flag with old/new frames as evidence; each correction lands in Muse Code (supersede) and Lambo (against), and the review page names the relying sessions.

Trap A: the never-right business rule (required ingredient: at least one L-memory false of the code from the start). T9.1's sample exempts by `EXEMPT-FLAG = "Y"`; each option says how the story rule reuses or differs from that.
- Option A1 (reuse the flag). Story rule: BILLBATCH exempts `EXEMPT-FLAG = "Y"` accounts from the 1.5% fee, exactly as T9.1; Wonder remembers "the fee is charged on the whole balance, the flag only marks the report line" (the T9.3 L2 shape, promoted to a story memory). Without Doyle: Wonder's Rust rewrite charges every account, its self-check compares totals on a non-exempt sample and passes, and 4,812 statements go out with overcharged exempt accounts. With Doyle: the differential run mismatches on exempt records, the flag cites `CALC-LATE-FEE`/`LOG-EXEMPT` with the inputs and both outputs, the guard denies the ship-rewrite step, correction lands in both memories. Recommendation: cheapest and already probed (curator 3/3 on this shape), smallest fixture. Against: it replays T9.3's probe as the demo's centerpiece, and it ignores the phase sketch's own example (vaccinations). Capture: the T9.10 run segment showing the exempt-record mismatch table (batch vs rewrite output); raw take stays in demo-shoot `takes/`, the small recording commits under `testdata/legacy/story/`.
- Option A2 (differ: exempt by visit kind, recommended). The copybook gains one field, e.g. a visit/reason code, and the code exempts vaccination-visit accounts from the fee; Wonder remembers "1.5% on the whole balance, no exceptions". Same without/with-Doyle shape as A1, except the mismatching records are the vaccination visits — which rhymes with the original story's stakes (vaccination histories lost) and is one camera shot in the video (the mismatch table). Differing from T9.1 is the point: the fixture shows a second exemption mechanism, so the demo proves Doyle flags the rule, not the flag. Cost: one added copybook field, which is also Trap B's additive case if v2 adds it — see B1. Recommendation: pick this. It matches the phase sketch's example, it is still one record per patient (no line-item explosion in the 4,812-row data), and the failure reads on film. Capture: the T9.10 run segment showing the vaccination-visit mismatch table; raw take stays in demo-shoot `takes/`, the small recording commits under `testdata/legacy/story/`.
- Option A3 (differ: wrong base, not wrong exemption). Wonder remembers "1.5% of the balance"; the code computes 1.5% of the past-due amount only (a second amount field). Without Doyle: every current-balance account is overcharged; with Doyle: diffrun mismatches on all non-delinquent records. Against: the mismatch set is nearly every record, which is a weaker evidence shot than a clean exempt/non-exempt split, and the copybook needs a second money field with its own ageing semantics to explain. Not recommended except as fallback if the user wants the exemption idea reserved for v2's change. Capture: the T9.10 run segment showing the near-total mismatch table, if chosen; raw take stays in demo-shoot `takes/`.

Trap B: the legacy additive / shared-element trap (what the v1→v2 batch diff must NOT flag). v2 needs at least one real change or the legacy diff flags nothing at all; the trap is what stays silent beside it.
- Option B1 (v2 adds a paragraph + field, both silent). v2 adds e.g. `APPLY-PROMPT-DISCOUNT` and a `DISCOUNT-FLAG` field; per `docs/legacy-ids.md` both are non-breaking adds, and the record-layout change they cause is the subtle part: the layout change on `PATIENT-RECORD` is breaking for memories citing the record. So the trap as drawn: an L-memory citing the record ("each record is N characters…") DOES flag on the layout change, while an L-memory citing only `CALC-LATE-FEE` or `BALANCE` does not flag from the add. Without Doyle: nothing fails here — that is the trap's nature; it is a false-alarm guard, and the run shows Wonder's correct discount memory untouched. With Doyle: no flag, no denial, the review page shows the add as non-breaking context. Recommendation: pick this. It exercises the exact subtlety `docs/legacy-ids.md` discloses (field-add silent, layout-change loud) and mirrors the original story's additive trap (`generated_at`). Capture: no screen capture — the v1/v2 program diff and the silent flag table are text fixtures under `testdata/legacy/story/`; the T9.10 run segment shows the correct discount memory left untouched.
- Option B2 (v2 changes the fee rate 1.5% → 2%). A true breaking statement change on `CALC-LATE-FEE`'s ID: memories citing that paragraph flag, memories citing `WRITE-TOTALS`, `BALANCE`, or the record do not (statement change does not reach sideways). Without Doyle: Wonder's rewrite keeps 1.5% and underbills everyone after cutover — a second live failure competing with Trap A's for screen time. Against: two simultaneous fee failures muddy the three-minute cut and the flag table; the story only needs one wrong-rule failure. Not recommended unless the user wants v2 itself (not Wonder's memory) to be a failure source. Capture: the T9.10 run segment showing the underbilled rewrite output, if chosen; raw take stays in demo-shoot `takes/`.
- Option B3 (v2 changes only `ASSIGN` literals / file layout, review severity). Memories citing the files flag at review; all paragraph/field memories stay silent. Without Doyle: Wonder points the rewrite at the old filename and reads stale data — a real but visually dull failure (a path string on screen). Against: weakest film of the three, and review-severity plumbing is the least-probed part of the scheme. Fallback only. Capture: none beyond the text diff fixtures, if chosen.

Trap C: the console procedure v2 changed (required ingredient: the decommission confirmation v2 drops), plus what must NOT flag.
- Option C1 (reuse the T9.2 pair-B shape, recommended). v1 decommission screen: warning line + "type the clinic name to confirm" prompt + PF12 CONFIRM; v2: warning and prompt gone, PF12 DECOMMISSION acts at once. Wonder's S-memory cites the v1 training span (e.g. 47–53 s region, ±5 s padding per the T9.2 verdict). Without Doyle: Wonder types the clinic name into a prompt that is not there (or the run shows the v2 screen accepting a bare PF12), the decommission goes through unconfirmed while the count check is still open — the exact visual rhyme with M4 ("no undo"). With Doyle: the screen diff flags the S-memory with old/new frames side by side, the guard denies the retire step naming the dropped confirmation, correction lands in both memories. The must-not-flag trap: S-memories citing the lookup screen and its training spans stay silent (only the decommission screen changed), and a cosmetic footer relabel elsewhere (PF5 UPDATE→SAVE style) flags nothing any S-memory cites. Recommendation: pick this. The pair is already measured (18/18 caught, `[]`×3 on identical, zero inventions), the generator exists, and the dropped-confirmation shot is the most filmable single frame in the extension. Capture: at fixture stage, the v1/v2 decommission screens and the short v1 training video — raw takes stay in demo-shoot `takes/` (named `<date>-<what>.mkv` per that README), small fixture copies commit under `testdata/legacy/story/`; at run stage, the unconfirmed-decommission segment.
- Option C2 (differ: v2 keeps a confirmation but changes its form — typed name becomes Y/N). S-memory "type the clinic name" flags; without Doyle Wonder answers Y to a prompt it never verified against counts. Against: the failure is a subtler sin (confirmed but carelessly), harder to land in one shot, and it contradicts the required ingredient (v2 drops the confirmation — a changed confirmation is not a dropped one). Not recommended; recorded here so the user can see the rejected middle ground. Capture: same screens-and-video set as C1, if chosen.
- Option C3 (v2 drops the whole decommission screen; retirement is a menu item). Bigger redesign, bigger flag (screen removed = breaking for all citing memories). Without Doyle: Wonder hunts a screen that does not exist and improvises — good drama but unscriptable in a T7-style run (the denial point moves depending on what Wonder improvises). Against: unperformable reliably. Not recommended. Capture: the improvised run segment, if chosen — raw take only, no fixture value.

Trap D: the differential-run trap (what execution evidence must NOT flag).
- Option D1 (the clean control, recommended). Fixtures include a correct Rust rewrite beside Wonder's wrong one: on recorded traces/outputs the correct rewrite mismatches nothing and flags nothing, while Wonder's wrong-rule rewrite mismatches exactly the exempt/vaccination records. The trap that must not flag: L-memories citing paragraphs on clean paths (`WRITE-TOTALS`, `WRITE-DETAIL` layout) stay silent when only fee-path records mismatch, and the whole run stays silent for the correct rewrite. Without Doyle: no failure — it is the control that makes B3's failure legible (same inputs, two outputs, one table). With Doyle: the with-Doyle run can show the corrected rewrite converging to zero mismatches — the closing shot of the video. Recommendation: pick this; T9.8's Done-when already demands "a correct rewrite flags nothing", so this trap is the T9.8 test written in story form. Capture: the T9.10 run segment with both rewrites side by side (wrong flags, correct clean) — the candidate closing shot; raw take stays in demo-shoot `takes/`.
- Option D2 (threshold trap: flag only above N mismatches). A single rounding-edge record differs (e.g. `ROUNDED` half-cent on one account) and must NOT flag — execution noise below a threshold. Against: invents threshold semantics no task owns (T9.8 says "for each mismatch … flag"), and GnuCOBOL `ROUNDED` is deterministic, so there is no honest noise source; the fixture would have to plant a fake flake. Not recommended. Capture: none — no honest take exists for planted noise.
- Option D3 (trace-absence trap: mismatch with no paragraph trace flags nothing-or-review). Plant a mismatch on a path the `-ftrace` run does not cover unless `COB_SET_TRACE=1` is set (the exact footgun T9.1 recorded). The trap: a memory citing an untraced paragraph must not be silently skipped — it flags at review with "mismatch, trace absent" rather than staying silent. Without Doyle: Wonder's self-check passes because it never set the variable — a true-to-life failure. With Doyle: flag at review severity, guard denies with the reason. This is the most sophisticated option and the best H-story for engineers, but it punts severity design into T9.8 and films poorly (the evidence is an empty stderr). Second choice; recommend only if the user wants the T9.1 footgun memorialized in the story. Capture: the T9.10 run segment showing the silent-check pass plus the empty-stderr evidence, if chosen; raw take stays in demo-shoot `takes/`.

Open questions for sign-off (user decisions, no fixtures until answered): (1) Trap A: A2 (vaccination-visit exemption, differs from T9.1) as drawn, or A1 (reuse the flag)? (2) Trap B: B1 (additive paragraph + field, layout-change subtlety) — and if so, does v2 carry any other change, or is the add the only legacy diff? (3) Trap C: C1 (pair-B reuse) confirmed, with the lookup-screen memories as the silent trap? (4) Trap D: D1 (clean-rewrite control) plus D3 (trace-absence at review) both, or D1 alone?
- Surprises: none — every option above is composed from measured findings (T9.1 trace switch, T9.2 pair B + padding verdict, T9.3 L2 shape, T0.3's two traps). The one structural finding this round: the never-right rule (Trap A) cannot be flagged by the v1→v2 legacy diff at all — the code never changed, so there is no change fact. Its only flag path is the differential run, which makes Trap D load-bearing for the story, not optional.
- Capture convention (demo-shoot README at `/home/nryn/work/demo-shoot/doyle/`): raw captures land in `takes/` named `<date>-<what>.mkv` and are never committed; small fixture copies commit under `testdata/legacy/story/`; the take (date, what, task) is noted in the diary so a later session can find it. The per-option Capture lines above say which take each option needs at fixture stage vs run stage.
- Handoff for the fixture round: on sign-off, write `testdata/legacy/story/` per T9.4's Done-when (README with the step-by-step story and expected flags; v1/v2 batch + copybook + 4,812-row data compiling under T9.1's runtime; v1/v2 screens + v1 training recording within T9.2's measured limits; L/S memories; grown plan). Nothing in `src/`, `tests/`, `tools/` or `Cargo.toml` is touched by this task.
- Repair (T9.6 findings, status stays done): `legacy-diff.json` gains the fifth fact T9.6's code diff proves — v2's WRITE-SUMMARY gained four discount-report lines (v2/BILLBATCH.cbl 240-243), a breaking `paragraph-statements-changed` fact on the WRITE-SUMMARY `le2_` ID, parallel to the CHARGE-ACCOUNT wiring fact. The `not_changes` line that called the discount summary lines non-citable layout is corrected: the SUMMARY-LINE-4 working-storage item is non-citable, the four paragraph statements that move it are the fact. Nothing cites WRITE-SUMMARY (no memory, plan or provenance names it), so the flag story is untouched: still 3 breaking facts, L2 alone flags. README counts corrected alongside: 24 shared IDs (all v1 IDs persist; only the two B1 adds are v2-only), 5 change facts. Both recomputed, not copied: the fact's subject ID is SHA-256 of `legacy:BILLBATCH:para:WRITE-SUMMARY` and matches both elements files; set comparison of the two elements files gives 24 shared / 0 v1-only / 2 v2-only.

---

### T9.5: Evidence-source seam ★
```yaml
requires:   [T9.3]
fixture-ok: yes
size:       M · frontier
owns:       src/contract.rs, src/core/flag.rs, src/evidence.rs, and the one declaration line in src/lib.rs
status:     done
```

Notes (msc-t9.5): seam built, gate green, awaiting orchestrator validation.
- `src/evidence.rs` (new): `NativeLocation` (moved verbatim from
  contract.rs), `EvidenceElement(Kind)` (ID, kind, parent, references,
  native; only `Type` is a shared named type), `EvidenceChange(Kind)` (ID,
  subject, kind, Doyle severity; non-breaking facts are never written),
  `ChangeFact` (implemented for Vimanam's `Change` and for
  `EvidenceChange`), `EvidenceSource` (element(s), parent chain,
  referencers, native location, `diff_evidence` which drops non-breaking).
- `src/contract.rs`: re-exports `NativeLocation` so every existing path
  keeps working; `ContractIndex` implements `EvidenceSource` by delegating
  to shared private helpers (`evidence_native`, `to_evidence`) — one code
  path serves the inherent methods and the trait. No signature changed.
- `src/core/flag.rs`: `flag_citations` is generic over `impl ChangeFact`
  and `impl EvidenceSource`; the propagation, severity and suppression
  logic is untouched. Tests module unchanged except the two import lines
  production code no longer needs at top level (relocated into the module,
  no test altered).
- `src/lib.rs`: the one approved line, `pub mod evidence;`.
- New tests (9, all in `src/evidence.rs`): a fake legacy-flavoured source
  with real `le2_` IDs from docs/legacy-ids.md flags through the same
  `flag_citations` (subject, ancestor, shared-type reach, additive
  suppression, review severity); Vimanam-behind-the-trait flags the demo
  identically; `diff_evidence` keeps the 4 flaggable facts in order;
  non-Vimanam natives resolve through the trait with the guard-matchable
  fields. Full gate passes: 276 lib tests green (267 pre-existing, none
  altered, plus the 9 new); `testdata/` untouched; `ElementId`/`ChangeId`
  shape unchanged.
- Surprise: Vimanam's `EntityKind`/`ChangeKind`/`Severity` are all
  `#[non_exhaustive]`, so the seam maps them with wildcard arms into
  `Other(...)`: a Vimanam addition can never break the build, it just
  rides as a named other.
- Guard boundary (for T9.6): `src/guard.rs` is untouched (outside owns).
  All 30 guard tests pass unchanged, and the seam test proves a
  non-Vimanam source serves the fields the guard matches on. A
  decide-level test with a non-Vimanam-backed flag needs the guard's
  version loader to pick a source per flag version — that loader and the
  legacy version shape are T9.6's design space, so this task invents no
  file convention for them.
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
status:     done
```

Notes (msc-t9.6): implemented, gate green, validated at close (T9.8 landed on it; board counts P9 9/10).
- `src/evidence/legacy/` (new): `ids.rs` (canonical keys + `le2_`/`lc2_` SHA-256 derivation; change key `legacy-change:<subject>:<kind>:<old>:<new>` fixed here, T9.6's call), `parse.rs` (subset parser: IDENTIFICATION/ENVIRONMENT/DATA/PROCEDURE, COPY, SELECT, 01/05 PIC, paragraphs/sections; bodies opaque normalized text; REDEFINES/OCCURS/nested/66-77-88/COPY REPLACING/groups rejected by name), `mod.rs` (`LegacyIndex` + `EvidenceSource` impl + `diff`/`diff_evidence`; offsets recomputed from running lengths; sections map to Operation, record to Type, file to operation-id slot).
- Touched outside owns, both P9-header approved: the one `pub mod legacy;` line in `src/evidence.rs` (pub, not plain mod, so T9.8's diffrun and future consumers can reach `LegacyIndex` without a second line later) and `sha2 = "0.11"` in Cargo.toml/lock (already in the lock as a transitive dep; reason: ID derivation requires SHA-256).
- Story pair: 24/26 elements, every ID recomputes from its key (all 50 fixture IDs verified true hashes; CALC-LATE-FEE matches the doc's worked example). All 24 v1 IDs persist in v2 (the story README's "20 shared" is a miscount; only the two B1 adds are v2-only).
- True diff is FIVE facts, not the four `legacy-diff.json` lists: v2's WRITE-SUMMARY gained four discount-report lines (v2/BILLBATCH.cbl 240-243), a breaking statement change per the scheme. `legacy-diff.json`'s not_changes overclaims (the layout item is non-citable, the paragraph statements are not). No story memory cites WRITE-SUMMARY, so the flag story is untouched: seam keeps 3 breaking facts, L2 alone flags.
- Change-kind coverage in tests: statement change (same-ID), removal (para/record/file), PIC change (field + layout), same-length PIC (field only, no layout), add-silent (seam-empty), rename (remove + non-breaking add + layout), ASSIGN/org (review), reformat (no facts), section add/remove/change, program rename (single removal).
- Guard boundary: loader NOT changed. Reason: version shape needs a producer convention no task owns yet (check names contract files), and the same question recurs for screens/diffrun — one design later beats a legacy-only hack now. Fail-open stands. Pinned by two binary tests: legacy versions allow + warning naming the file; `le2_` elements under loadable contracts allow + warning.
- Surprises: v1→v2 WRITE-SUMMARY change (above); README "20 shared" miscount (above); sha2 already locked and cached so the dep needed no network.
- Gate: `./tools/check.sh` green, 298 lib tests (276 pre-existing untouched + 22 new).
The evidence source for COBOL, on T9.3's scheme: parse copybooks and the procedure division into elements (no compiler needed for parsing), and diff two versions into change facts. A parser for the subset the fixtures use is enough; say what it does not handle.

**Done when:** on T9.4's v1 and v2, the diff gives the changes the story expects and no others; the elements and IDs match `docs/legacy-ids.md`; tests cover each change kind.

---

### T9.7: Screen source ★
```yaml
requires:   [T9.5, T9.2, T9.4]
fixture-ok: yes
size:       L · frontier
owns:       src/evidence/screens/, src/models/vision.rs, src/prompts/vision/, and the media parts of ModelRequest in src/models/mod.rs
status:     done
```

Notes (msc-t9.7): built and gate-green (310 lib tests, all pre-existing green), awaiting orchestrator validation.
- `ModelRequest` gains `media: Vec<MediaPart>` (`ImagePng`/`VideoMp4` only, the two T9.2-measured shapes; base64 data URLs hand-rolled, no new dependency). `request_key` extends the key with kind, length and bytes only when media is present, so every existing recording replays with the same key (the replay golden-value test passes unchanged). Mechanical one-line `media: Vec::new()` fallout in 9 struct literals across `curate.rs`, `meta.rs`, `baseten.rs`, `replay.rs`, `prompts/curator.rs`, `prompts/recheck.rs`, `tests/live_meta.rs`, `tests/live_baseten.rs` (same commit as the field). New: media key tests (bytes/kind/order move the key; empty media keeps the old key) and wire-shape tests (`content_blocks`: bare string without media, text + `image_url`/`video_url` data-URL parts with media, per T9.2 §6).
- `src/prompts/vision/`: one request per version (frames then recording, named in order), `muse-wonder-1.0`, cap 8000 (T9.2's ~2x rule), closed 11-kind vocabulary, strict parser; unknown step-screen names drop to no reference with a warning (curator rule). `stated_cost` states input tokens (~1500/still, ~240/video-second) beside the cap.
- `src/models/vision.rs`: `read_version` runs the one call, returns the index, the reply's usage, warnings and the request key.
- `src/evidence/screens/`: `se2_`/`sc2_` IDs from content (SHA-256 hand-rolled, FIPS vectors green), never reply order. Screens are the shared-type analogue; steps reference the screen they show. The differ matches elements positionally (row ±1, col ±2, the T9.2 tolerances): footer-hint-only churn is cosmetic and writes no fact; anything else is a breaking procedure fact on the screen, with element detail for display. Spans pad ±5 s clamped to the recording.
- On T9.4's console, from recorded replies through a media-keyed record→replay round trip: lookup v1→v2 is one cosmetic text change (PF5 UPDATE→SAVE, drifted hints matched, no fact); decomm v1→v2 is one breaking fact (3 removals + CONFIRM→DECOMMISSION); S1 (screen and span-only cites) flags, S2 stays silent; identical versions diff to nothing.
- Stated cost of the recorded run: v1 ~12,200 in / 3,900 out tokens, v2 ~1,900 in / 2,100 out (magnitudes mirror T9.2 §5/§7; no dollar cost exists on the API, ~$15/MTok basis puts both calls near $0.30). Live values land with T9.7b.
- No live calls: `MODEL_API_KEY` is unset in this environment (Baseten only), and this task carries no live budget, so the wire path is tested as `content_blocks` JSON, not against the API. `src/models/meta.rs` is untouched (outside owns): the live send needs `messages[1].content = request.content_blocks()` there — one line for the orchestrator to approve, needed by T9.7b. `src/prompts/mod.rs` gained the `pub mod vision;` line inherent to the granted `src/prompts/vision/` dir, in the prompt commit; `mod screens;` went in as its own one-line commit per the task note.
- Surprise: the story lookup v1→v2 is not a bare relabel — the footer reflows (PF9 MENU/PF12 DECOMMISSION shift 2 columns left), so the matcher must tolerate column drift; exact-position matching would have invented a removal plus an addition.
Adding image and video parts to `ModelRequest` changes a frozen contract. It is approved here, on one condition: every existing recording still replays with the same key.

The evidence source for screens and recordings. One Wonder call per version turns frames or a recording into elements and steps (screen, element, action, time span), each given a stable ID derived from what it is, never from its order. The diff between two versions is computed, not asked of the model. Image and video parts go to the Meta client as T9.2 found works; the replay client serves them offline, keyed on the media's hash.

Cost is stated before a call is made, and each call is recorded with its usage.

**Done when:** on T9.4's console v1 and v2, from recorded replies, the diff gives the changed procedure and no invented change; a memory citing the training video's span is flagged; the replay key test covers media; the stated cost of the recorded run is in the handoff text.

**T9.7b (live):** run the screen source on the fixtures against `muse-wonder-1.0` and compare with the recorded replies. Owns `tests/live_screens.rs`. Budget: $2.

**T9.7b notes (live):** five live runs, v2-then-v1 each, `reasoning_effort low`, all through `read_version` (fuzzy test in `tests/live_screens.rs`, `#[ignore]`d, `live_*`). Input tokens deterministic every run (v2 1871, v1 11752); outputs vary (v2 2642-3666, v1 3335-4379 out). No reply carries `cost_usd`, as in T9.2; on the $15/MTok basis runs cost $0.30-0.32 each, total ~$1.54 of the $2 budget — the T9.7 ~$0.30/run estimate lands. Measured live values for the T9.7 stated-cost line: v2 ~1.9k in / ~2.6-3.7k out, v1 ~11.8k in / ~3.3-4.4k out; the 8000 cap never binds. Live reply shapes vs T9.2: element counts match ground truth exactly when no wobble (37/18 v2, 37/21 v1); the `FW-004812` V-drop from T9.2 §1 appears in every live read on both sides and cancels in the diff; live steps number 5-6 with onsets matching ground truth, but step-screen links under-link (results/scroll steps often name no screen; S2's live citation rested on 1 lookup step, S1's span on 1-2 decommission steps).
- The changed procedure holds 5/5: decommission is `ProcedureChanged` every run with the recorded shape (warning + prompt + input removed, PF12 CONFIRM relabelled), once byte-equal to it (run 4: exactly 4 changes); the PF12 relabel reads as `TextChanged` or remove+add as columns drift (36->40), and PF5 likewise (25->28) — the test accepts the relabel shape, not the match kind.
- Lookup procedure text is never invented (only the PF5 relabel plus wobble), but raw lookup silence held 1/5: two runs add a dashes separator fact (the row-2 rule the PNGs render and ground truth omits — T9.2 §1 excludes it from scoring, the differ has no such concept), and two runs flip one field's kind (`FW-004812` `input` col 15 vs `value` col 16; ground truth is `value` col 16 both sides). The test treats dashes-only footer churn as chrome per the T9.2 precedent and stays strict on kind, so kind-flip runs fail it.
- Flag path, observed live in the passing run: S1 and the span-only citation flag Breaking from the decommission fact alone; S2 flags Breaking from the wobble-only lookup fact — the S2-silence the recorded run shows is not guard-reliable on single live reads. Product implication, not a story error: the differ needs a separator concept and kind-tolerant matching (or multi-read voting) before a screen fact can deny a step.
- Surprise: the three wobble classes (separator presence, column drift past ±2, kind flip) all cancel or are bounded except through the facts they write — the reads are good, the matching is brittle.

---

### T9.8: Differential run ★
```yaml
requires:   [T9.6, T9.1]
fixture-ok: yes
size:       M · frontier
owns:       src/evidence/diffrun.rs, testdata/legacy/diffrun/
status:     done
```

Notes (msc-t9.8): built, gate green (345 lib tests: 332 pre-existing + 13 new), awaiting orchestrator validation.
- `src/evidence/diffrun.rs` (new): evidence by execution over recordings. Parsers for batch detail lines + TOTALS (`parse_report`, `parse_totals`, money in cents), `-ftrace` stderr (`parse_trace`), per-record split (`split_trace`, `PROCESS-ALL` .. `WRITE-DETAIL` for BILLBATCH, taken as parameters; prologue, inter-record reads and totals epilogue land in `outside`), the committed per-record-paths form (`parse_record_paths`), comparison with drift-as-error (`compare_reports`, `run_diffrun`, `run_diffrun_with_paths`). Blame is exclusivity: a paragraph on a mismatched path and no clean path (`exclusive_paragraphs`) becomes one breaking `diffrun-mismatch` fact on the paragraph's stable `le2_` ID (`diffrun_changes` via the T9.6 index); `flag_diffrun` runs the same `flag_citations` every source uses and returns each flag with its paragraphs and mismatching records (`DiffrunFlag`, `mismatch_evidence`, `total_overcharge_cents`). Empty trace fails loudly naming `COB_SET_TRACE=1` (the T9.1 footgun). No model calls, no compiler invocation in the gate.
- `testdata/legacy/diffrun/` (new): README with record + reproduce commands; mini run (first 10 story records: real 78-event trace, batch/wrong/fixed reports — wrong mismatches exactly 000003+000009, TOTALS 62.02 vs 68.96; fixed identical); full run (`full-wrong-report.txt` 4,812 lines TOTALS $72,213.23, `full-record-paths.txt` per-record paths from the real 33,692-event trace by the README recipe). Full trace itself (2.9 MB) not committed.
- Tests: mini wrong flags L1 alone with 2 record-evidences blamed on LOG-EXEMPT; mini fixed flags nothing; full 535 mismatches == `mismatches.txt` accounts, each a V-record with $0.00 charged 1.5% rounded, overcharge $8,308.80, L1 alone flags with all 535; full fixed rests on the recorded `cmp`-clean identity + the covered empty path (stated in the test name); L2/L3/L4 silent on diffrun in all runs.
- Hand verification (by hand in /tmp, nothing committed runs it): v1 COBOL rebuilt and ran byte-identical to the story `report.txt` (paragraph counts 4,277/535 exact); both rewrites rebuilt and rerun (wrong TOTALS $72,213.23 with 535 accts matching `mismatches.txt`, fixed detail+TOTALS identical).
- Touched outside owns, both P9-header approved in the brief: the one `pub mod diffrun;` line in `src/evidence.rs`.
- Surprises: (1) the story `trace-summary.txt` prose says 33,895 trace lines; fresh runs give 33,695 total / 33,692 paragraph events with every paragraph count and the report byte-identical — prose drift, not a rerun difference. (2) The first full-run segment carries the prologue (MAIN-PARA/OPEN-FILES/WRITE-HEADER) because splitting on WRITE-DETAIL accumulates from the start — hence the PROCESS-ALL..WRITE-DETAIL path convention, so run scaffolding can never blame a memory.
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
status:     done
```
The review page shows each flag's evidence in its own terms: for a screen, the old and new frames side by side with the changed element marked and the cited span playable; for legacy code, the paragraph before and after; for a differential run, the inputs and both outputs. Same approve / edit / retire / keep.

**Done when:** each evidence kind renders on the demo fixtures; a person settles a screen flag and a legacy flag from the page; the page has been looked at in a browser at phone and desktop width, light and dark.

Notes (wonder-t9.9): built, gate green, awaiting orchestrator validation.
- `src/serve/evidence.rs` (new): kind detection by ID prefix (`sc2_`/`se2_` screens, `lc2_`/`le2_` legacy or — when `new_version` is a `.json` run manifest — diffrun, else the contract view unchanged), three loaders, three renderers, and the one streaming route. `src/serve/mod.rs` gains only: the `mod evidence;` line, one route arm (`GET /flag/{id}/evidence/video`), one dispatch-line swap in `flag_page`, and a `media` payload on `Response` so the recording streams as bytes (T5.4's adapter-construction region untouched; no collision).
- Version locators (documented in the module): legacy flags name `.cbl` paths on both sides; screen flags name version JSON files in the vision reply shape plus `console`/`recording` display keys (frames are rebuilt from the element lists, no still file needed); a diffrun flag names the batch `.cbl` as old and a run manifest JSON (`inputs`/`cobol_report`/`rewrite_report` + `trace` or `record_paths`) as new — T9.8's recorded form, never a live compile. Unreadable evidence is named on the page with the change IDs, never silent.
- Rendering: screens show both 80×24 frames with changed elements `<mark>`ed, the verdict table (decommission procedure-changed, lookup cosmetic), the element-change table, and the cited T5–T6 steps with ±5 s cited spans and a `<video>` cued `#t=23,45` to the union span; legacy shows the record layout 34→35 with member offsets plus the copybook before/after (a paragraph change shows the raw paragraph before/after, covered by the CHARGE-ACCOUNT test); diffrun shows counts, totals, $ overcharge, the blamed paragraph, and each mismatch's input + both outputs (first 25).
- Tests (9 new, 354 lib total, all through `ReviewServer::handle` — seed story fixtures → serve → HTTP): each kind seeded through its real flagging path (`flag_citations` on the legacy and screen diffs, `flag_diffrun` on the mini recordings; only the flagged memory flags in each), contract flags still render the T6.2 section, unreadable evidence is fail-visible, and the decide path is reused read-only. Settle tests: screen flag approved and legacy flag edited through POST `/flag/{id}/decide` — decision recorded by the operator in the store, the stale note gains `[Superseded by MEMORY.md#…]`, and the correction lands in `MEMORY.md` (the Muse Code adapter writes corrections to the index file, not the topic file — asserted as found). Proposals are seeded records: no recheck producer exists for non-contract flags yet, which is a producer gap for a later task, not a page gap.
- Browser evidence (not committed; method for the handoff log): the three seeded flag pages served over a real socket and captured with headless Chrome (`--headless=new`, `--force-dark-mode` for dark; sampled page backgrounds match the CSS variables exactly, light `#f7f5f0` / dark `#171512`) at 1280 px and 390 px in light and dark, plus legacy at both and diffrun at desktop. An operator settle was also done over real HTTP (303 → settled view; `decisions.jsonl` and both memory files inspected). The `<video>` element loads the fixture recording with its controls and duration; its first frame renders in captures.
- Surprise: Firefox headless ignored the `prefers-color-scheme` profile override here in both directions, so the theme captures moved to Chrome's `--force-dark-mode`, verified by pixel sampling rather than assumed.

---

### T9.11: Legacy and screen flags from the binary ★
```yaml
requires:   [T9.6, T9.7, T9.8, T9.9]
fixture-ok: yes
size:       M · mid
owns:       src/check.rs (the evidence-kind dispatch), src/diffrun_cmd.rs (new), src/cli.rs (the options below only, orchestrator-approved), src/main.rs (one dispatch arm for diffrun, orchestrator-approved), src/lib.rs (one module line), src/guard.rs (only if a legacy or screen flag cannot be matched to its step), tests/
status:     done
```

Notes (msc-t9.11): built, gate green except one superseded T9.6 pin (below), awaiting orchestrator validation.
- Commands. `doyle check OLD NEW` picks the source from the file extensions: two `.cbl` programs diff as legacy COBOL (LegacyIndex::load, copybooks beside), two `.json` files with a screens list diff as console screens (the review page's version shape: console/screens/steps/recording, loaded by a new `check::load_screen_version` shared with the guard), anything else diffs as contracts, unchanged. No `--kind` option: adding a field to `Command::Check` would break the two literal constructors in `src/serve/mod.rs` (outside owns), and extension detection is unambiguous for every fixture in the repo; the choice is documented in `doyle check --help`. `doyle diffrun PROGRAM RUN` is new (`src/diffrun_cmd.rs`): the run manifest names inputs/both reports plus `record_paths` (or a raw `trace` + bounds), the same convention the review page reads; the flag's old version is the program, its new version the manifest. Flags go to the same store through `flag_citations` in both commands.
- Guard (`src/guard.rs` only). Versions load per path: `.cbl` through `LegacyIndex`, screens-shaped `.json` through the shared loader, anything else as a contract; a diffrun manifest loads as nothing, so its flag matches through the program side alone. Fail-open stands: unloadable sides are skipped with the existing warning. Type names now match ASCII-case-insensitively (`decommission` matches a `DECOMMISSION` console command); operation IDs stay case-sensitive. Every T6.1/T6.3 test passes unchanged (35 guard tests green).
- Ship vs retire matching. The ship-rewrite step is a `bash` deploy naming the blamed paragraph (`LOG-EXEMPT`, L1's diffrun-blamed operation): denied with Doyle's reason naming L1 and the paragraph. The console-decommission step is a command containing `DECOMMISSION` (T9.12's commands name the screen): denied naming S1 and `type decommission`. Traps allowed: an `APPLY-PROMPT-DISCOUNT` command (L4 silent), a `lookup` command (S2 silent), a `WRITE-TOTALS` command (cited by nothing), and `ls -la`.
- Re-check. Contract path untouched. Legacy/screen/diffrun flags get a one-line stderr skip note on a default run (no model calls, keyless) and silence under `--no-recheck`; the producer is the later task's.
- Fixed a real bug the manual run caught: diffrun rendered pre-write flags, so flag IDs printed empty; the store-assigned IDs are now carried onto the rendered flags (pinned by an assertion in the e2e test).
- Pin rewrite (orchestrator-granted exception): the T9.6 fail-open test is now `legacy_versions_match_and_fail_open_only_when_unloadable` — real `.cbl` versions load and a cited-paragraph tool call denies; only a genuinely unloadable version warns and allows.
- E2e (`tests/story_flags.rs`, 6 tests, all through the built binary): legacy diff flags L2 alone; screen diff S1 alone; diffrun flags L1 alone with exactly the 535 `mismatches.txt` accounts, $8,308.80 overcharge, blame `LOG-EXEMPT`; the byte-identical rewrite flags nothing; all reruns exit 0; guard denies/allows as above; `--no-recheck` markdown per kind; default-run skip notes; exit-2 loud failures; CLI shape/parse tests.
- Surprise / needs-orchestrator. The T9.6 pin `legacy_versions_allow_with_a_warning` (in `src/evidence/legacy/mod.rs`, outside owns) now fails: it pins fail-open for real `.cbl` versions, which directly contradicts this task's Done-when (the guard must deny through legacy versions). The T9.6 notes anticipated this loader as the later task that owns all three sources — this is that task. The sibling pin (`unknown_prefix_elements_allow_with_a_warning`) still passes. `cargo test --lib` is 419 green + that 1 pin; everything else in `./tools/check.sh` (fmt, clippy, build) is green. The pin needs an orchestrator edit on landing; the gate cannot go fully green until then.

Found while briefing T9.10: the legacy, screen and differential-run sources exist (`src/evidence/`) and their flags render on the review page (T9.9), but no command produces them. `doyle check` diffs API contracts only (`ContractIndex`), and nothing outside tests calls `LegacyIndex::diff_evidence`, `screens::diff_versions` or `flag_diffrun`. A live run with Doyle could flag M1 and nothing else.

Build:
- `doyle check OLD NEW` picks the source from what it is given: an API contract as today, a legacy elements file (`elements-v1.json`/`elements-v2.json` or the COBOL sources, whichever T9.6 indexes from), or a screen index (`decomm-v1.json`/`decomm-v2.json` and the training steps). One explicit `--kind contract|legacy|screens` option if detection would be ambiguous. Flags go to the same store through `flag_citations`.
- `doyle diffrun`: compares the COBOL batch's report with a rewrite's report over the same inputs, with the per-record paths, and writes the flags `flag_diffrun` computes, with their mismatches as evidence.
- The guard denies the step each new flag guards: shipping or running the rewrite while L1's diffrun flag is open, and the console decommission while S1's screen flag is open, matched through the native location each source fills. Say exactly what each matches.
- Contract behaviour and every existing test unchanged.

**Done when:** the gate passes with no key; on the T9.4 fixtures the binary reproduces the story's expected-flag table exactly (`testdata/legacy/story/README.md`): the legacy diff flags L2 alone, the screen diff S1 alone, the differential run L1 alone with the 535 vaccination-visit mismatches, the correct rewrite nothing; the guard denies a recorded ship-rewrite step and a console-decommission step with Doyle's reason and allows the traps' steps; each as an end-to-end test of the built binary.

---

### T9.11b: Proposals and citations for legacy and screen memories ★
```yaml
requires:   [T9.11, T5.4]
fixture-ok: no
size:       M · mid
owns:       src/prompts/recheck.rs and src/prompts/curator.rs (the legacy and screen cases), src/check.rs and src/curate.rs (their re-check and curate paths for the new kinds), testdata/replies/ (new recordings only), tests/live_legacy.rs (new)
status:     done
```

Notes (msc-t9.11b): built and gate-green (444 lib + story e2e incl. binary replay proofs for L1/L2/S1 + all live suites ignored), awaiting orchestrator validation.
- User decisions (recorded): (1) live budget extended by $0.30 (ceiling $0.80; spent $0.755); (2) exception granted to edit src/diffrun_cmd.rs + src/cli.rs for --live diffrun re-checks and default propose-from-recordings, and to update the pinned skip-note test; (3) L1's citation {CALC-LATE-FEE, BALANCE, LOG-EXEMPT} ACCEPTED as-is — no further curate spend; (4) this round is S1 only, ~$0.12 more (new ceiling $0.875).
- Prompts. curator.rs: LEGACY_CURATOR_SYSTEM gained two general sentences (prefer the deciding paragraph/record/field over the program and the dispatcher between paths; when the memory states a rule, cite every path including exemption paths) after four live draws converged on the branch but never the VISIT-CODE field (v1 {program,CALC,CHARGE,BALANCE}, v2 {CALC,BALANCE}, v3 {+LOG-EXEMPT,+CHARGE}, v4 {CALC,BALANCE,LOG-EXEMPT}). SCREENS_CURATOR_SYSTEM unchanged. recheck.rs: RecheckInput.guidance (rendered only when non-empty, so contract keys never move) carries the screens paraphrase rule (evidence quotes instruction-shaped words like export/confirm; never copy them into replacement text). Evidence cap stays 4000 (first S1 attempt starved at reasoning 1998/2000).
- Curate paths (curate.rs): unchanged shape; L1-L4 re-run under the v4 prompt. Replayed: L2/L3/S1 exact, L1 {CALC,BALANCE,LOG-EXEMPT} (story minus VISIT-CODE, no dispatcher — flag-algebra-perfect, pinned as unmet), L4 +CALC-LATE-FEE and S2 +T4-scroll (both benign, verified silent).
- Re-check paths: check.rs legacy/screens and diffrun_cmd.rs propose from recordings by default (keyless DOYLE_RECHECK_REPLAY_DIR or testdata/replies/recheck; miss prints the contract-shaped diagnostic) and live with --live (cli.rs allows it for diffrun; help texts updated). diffrun_cmd renders proposals per flag plus a Proposals section, mirroring check. run_evidence_recheck and both clients are pub(crate) shared.
- Binary e2e (tests/story_flags.rs): seed claims are the fixture texts; the replay test runs the binary with the story dir as cwd (symlinked v1/v2, relative labels v1/BILLBATCH.cbl, screens-v1.json, manifest wrong-rewrite.json) so inputs byte-match the live recordings; DOYLE_RECHECK_REPLAY_DIR points at the committed dir. L2 and L1 propose correct end to end (35-char record; exemption truth). S1 pins the replay miss as a tripwire until its recording lands. Reruns exit 0. The --no-recheck silence tests are untouched.
- Live (8 calls this round, keys via T5.4 loader through a scratchpad wrapper, never committed): L1 v3 {CALC,BAL,CHARGE,LOG} $0.037, L1 v4 {CALC,BAL,LOG} $0.038, L2/L3/L4 v4 $0.047/$0.030/$0.037, S1 retry starved at 4000 (3998 reasoning, null; +$0.081). Task total $0.755 of $0.80 (per-call ledger in the handoff). S1 run-4 reply (correct substance, checker-rejected on export) and the starved run-5 are in the diary transcript, not committed.
- New recordings: testdata/replies/curator-legacy/ (L1-L4 v4), curator-screens/ (S1/S2), recheck/ (L1, L2 keys) — sibling dirs so the contract replay test keeps its exact glob.
- Unmet (reported, not glossed). (1) Curate L1: four draws, best is story-minus-VISIT-CODE — ACCEPTED as-is per user decision; no further curate spend. The committed v4 recording and its replayed test pin exactly what the model cites. (2) Nothing else open: recheck S1 landed this round; diffrun and default-check wiring landed with replayed binary proof (L1+L2+S1).
- S1 round (this session): per the diagnosis the lever is cap, not wording. `EVIDENCE_SCREENS_MAX_OUTPUT_TOKENS = 8000` (new const + `build_screens_evidence_request`; legacy/diffrun/contract stay at 4000/2000, so their keys never move) and a lighter guidance (information + prohibition — "never quote screen text" — no paraphrase task). One retry: 1409 in / 3118 out ($0.0679): correct, guideline-7 clean — "without the permanence notice or the clinic name entry field seen before, and the PF12 key has different wording". Committed as `testdata/replies/recheck/ddc4152b9eed8c68.json`; replayed lib test and binary e2e now cover the S1 proposal end to end. Round spend $0.068; task total $0.823 of $0.875.
- Surprises. (1) The rule-paths sentence fixed the branch but re-included the dispatcher; naming the dispatcher fixed that but the field stays invisible — each prompt repair moves exactly one element. (2) The S1 retry starved HARDER with the guidance (3998 vs 3401 reasoning) — the paraphrase task adds reasoning load; a bigger cap, not a better sentence, is the next lever. (3) Binary replay keys include the cwd-relative version labels, so the e2e test must run the binary from the story dir — the env-var replay dir is what makes that hermetic. (4) The lighter no-quotation rule plus 8000 cap landed S1 first try with 3118 out — the model had the substance all along; it needed room, not instructions.
- Handoff log: T9.11b is done except L1's last field (accepted as-is): legacy/screens/diffrun curate + re-check with replayed proofs (curate L1–L4/S1/S2 per the notes above; recheck L1/L2/S1), binary e2e proposing L1+L2+S1 corrections from recordings, $0.823/$0.875 live spend. For T9.10: nothing open on this task's wiring; the demo runs inherit working producers. No key, token, or private path in any committed file; live transcripts (no key) in the report.

T9.9 found no re-check producer for non-contract flags: its tests seeded proposals by hand. Curate cites memories against an API contract only, so in a live run the L and S memories would never be cited.

Build:
- The re-check proposes a correction, retirement or keep for a legacy, screen or differential-run flag, given the flag's own evidence (the changed elements, the old and new frames' ground truth, or the mismatching records with both outputs). Muse Spark, as for contracts.
- Curate cites L memories against the legacy elements and S memories against the screens and training spans, through the Baseten curator (T9.3 probed the legacy shape).
- Replay recordings for every new prompt, so the gate stays keyless.

**Done when:** the gate passes with no key; replayed, curate cites L1–L4 and S1–S2 as the story's citations and the re-check proposes a correction for L1, L2 and S1 matching the story's corrected memories in substance; a `#[ignore]`d live test runs one curate and one re-check per kind on the fixtures under a $0.50 budget, transcript in the report.

---

### T9.12: The operator console ★
```yaml
requires:   [T9.4]
fixture-ok: yes
size:       S · mid
owns:       demo/console/ (new), demo/scripts/ (start and stop lines for the console only), demo/README.md (a console section)
status:     done
```

Notes (msc-t9.12): built, gate green, awaiting orchestrator validation.
- `demo/console/fern-console` (new, executable, stdlib only): one shell
  command per operation, subcommand names the screen (`lookup`,
  `decommission`) with a `SCREEN: LOOKUP/DECOMMISSION (fern console
  vN)` header so a guard can match it. Prints the 24-line screen plus
  a `RESULT:` line; the screen bytes equal the fixture ground truth
  (proven by `test_console.py`, not assumed: the text is embedded from
  the fixtures and the test diffs it).
- v1 decommission requires `--clinic "MAPLE ST CLINIC"` + `--key PF12`
  (missing/wrong -> exit 2 REFUSED, no HTTP, service count untouched);
  v2 ignores `--clinic`, `--key PF12` (default) acts at once. Success
  POSTs the documented confirmation token to the old service's
  `/admin/decommission`, so the old count falls to 0 exactly as the
  curl path does. Lookup renders its screen in both versions (exit 0
  for the recorded account, exit 2 otherwise).
- `demo/console/test_console.py`: 9 tests, all green — 4 rendering
  (one per screen and version, server-free via the refusal paths) + 5
  behaviour (old service on a test port, bounded waits/timeouts:
  v1 refuse missing/wrong clinic with count unchanged at 4812,
  v1 success with count 0, v2 at-once success with count 0, lookup in
  both). Live e2e on ports 8093/8094 repeated the same outcomes.
- `demo/scripts/start-console.sh` (new): starts the old service via
  `start-old.sh` unless /health answers with a real patient_count;
  `demo/scripts/stop.sh`: two lines clearing the console's
  last-result note (the console is one-shot, no process to stop).
  `demo/README.md`: console section (commands, refusal/success
  shapes, check.sh effect, test command).
- Surprises: port 8091 on this machine is occupied by an unrelated
  local app that answers 200 with HTML on every path, so a bare
  `curl -sf .../health` health check lies; `start-console.sh` parses
  the health body for `patient_count` instead. The pre-existing
  `start-old.sh` has the same weak check, but that file is outside
  this task's owns, so it is noted here, not fixed.

The extended story's retire step has Wonder follow S1 on the v2 console, but the console exists only as four still frames and a training video (`testdata/legacy/story/screens/`). A run needs a console Wonder can drive.

Build: a small terminal console, v1 and v2, rendering exactly the lookup and decommission screens of the T9.4 fixtures (their ground-truth JSON is the spec), driven from a shell so Wonder can operate it: look up an account; open DECOMMISSION; in v1 the warning, the type-the-clinic-name prompt and PF12 CONFIRM; in v2 no warning, no prompt, PF12 DECOMMISSION acting at once. Decommission marks the old records service retired (the same effect as today's decommission endpoint), so the counts show it. Each operation is one shell command whose text names the screen, so a guard can match it. Standard library only, new code.

**Done when:** the gate passes; each screen as the console renders it equals the fixture's ground truth (a test per screen and version); a v1 decommission without the clinic name is refused and with it succeeds; a v2 decommission succeeds at once; `demo/README.md` documents it.

---

### T9.13: Capture rig ★
```yaml
requires:   [T9.12]
fixture-ok: no
size:       S · mid
owns:       ~/work/demo-shoot/doyle/capture-rig.md and ~/work/demo-shoot/doyle/tools/ (outside the repository); nothing in the repository
status:     done
```

Notes (msc-t9.13): rig built; review take recorded; session take blocked on
person-only OBS steps, reported to the orchestrator.
- `~/work/demo-shoot/doyle/capture-rig.md` + `tools/` (new): `obs.py`
  (obs-websocket v5, password from a person-named file under `~/.config/`,
  never printed), `strip-env.sh` (the T7.2b key-stripping rule, count-only
  output, refuses to run with credentials present), `take-session.sh`
  (record-start, launch the key-stripped TUI with the prompt verbatim as its
  initial argument, pane-stability wait, record-stop, move the take, drop
  the tmux session; sends nothing after launch), `seed-review-state.py`
  (copies fixtures to scratch, writes the story citations, runs the binary
  keyless and asserts L2-alone / S1-alone / L1-alone-535-$8,308.80 before
  anything records), `record-review.mjs` (Playwright 1920x1080, fresh
  cookie-less context, list then each flag page), `scan-take.sh` (values via
  `grep -l` from a 0600 patterns file only; structural patterns report
  counts; fails on any value hit).
- Findings, all verified by running, not assumed. obs-websocket is enabled
  on 127.0.0.1:4455 with auth required (settings path in the rig doc);
  canvas already 1920x1080@30, MKV. `send-keys -l ... Enter` sends the word
  Enter literally — the key needs its own send-keys. A bare TUI in a fresh
  dir raises the workspace-trust prompt, so the rig passes
  `--trust-workspace` (run-only, saves nothing). Inside the TUI Enter is a
  newline; the rig passes the prompt as the initial argument, which the TUI
  submits itself (verified on 1.4.14 with the echo provider: prompt
  submitted, reply streamed, pane stable, wait loop exited; no memory folder
  written, global settings byte-identical). `obs.py` status/record/scenes
  and the missing-password exit were exercised against a fake
  obs-websocket server asserting the challenge-response; the one real
  `scenes` parse bug this caught (missing-key assumption) is fixed.
- Takes: `takes/2026-11-03-t9.13-review.mkv` (1920x1080, ~26 s, VP8): the
  flag list (L2, S1, L1 open), the S1 screen flag's before/after frames with
  the decommission procedure change, the L1 diffrun flag's 535-row mismatch
  table; read-only view throughout, reviewer link never shown. The session
  take is unmet: OBS is not running and the rig's password file
  (`~/.config/doyle-shoot/obs.env`) does not exist, and both plus the scene
  and the approval answers are person-only. Exact asks are in the report.
- Scan: the take, all six tools and the rig doc carry no key value, no
  reviewer token, no `auth.json` content and no private path; the only scan
  hits are the doc's own reviewed rule text. The scan caught one real leak
  first (a hardcoded `/home/` path in the recorder, rebuilt from `$HOME`).
- Surprises: the earlier takes in `takes/` are 1280-wide pane-sampling
  assemblies, not OBS captures — the rig doc records that method as the
  fallback the T7 takes used, while the takes it prescribes are OBS's.
  OBS auto-splits recordings at 15 minutes by profile setting; test takes
  stay short.

Closed by the orchestrator (the session take, with the user's go-ahead to drive OBS):
- `takes/2026-11-03-t9.13-session-test.mkv`: 1920x1080, 31 s, video only. One harmless prompt to `muse-wonder-1.0` in a `/tmp` scratch repository; frames show the prompt submitted, the model thinking, the answer streaming in and the finished state, each sampled frame different. Muse settings byte-identical, no memory folder written, leak scan clean.
- What it took on this machine (KDE Wayland, NVIDIA): PipeWire capture of a native-Wayland terminal, X11 window capture and `x11grab` all gave black frames, and a share restored after an OBS restart froze on its first frame. What works: OBS and the rig terminal both as X11 clients, and a fresh portal share of the terminal window after each OBS start (the person's click). Recorded in `capture-rig.md` with a `Doyle rig` scene (`tools/obs-setup.py`); the Receipts scenes are untouched.
- Superseded after the close: window capture froze whenever the rig window was covered, so the terminal is now captured through `tools/termcast.py` (the tmux pane streamed to a local page that OBS shows as a browser source, rendered offscreen like the Receipts sources): no visible window, no share click. Re-verified with a stand-in Muse (prompt with backticks, `$(…)`, quotes and newlines arrived byte-identical; no secret reached it; the take kept recording through a 30 s approval wait) and one real Wonder take. Also fixed: an unanswered approval prompt no longer ends a take; `--no-session-log` dropped (T9.10 exports sessions); the rig refuses any OBS scene but its own; `strip-env.sh` reads names from bash and never matches values; the OBS file is remuxed only once it has settled.
- Rig fixes before any real take: the launch line no longer types a home path or shell-expands the prompt (paths reach the pane as variables, the prompt is read in the pane); the TUI starts on Wonder, not its default model; tmux runs with no user config and no status bar (it showed the host name); takes are remuxed to video only (the profile records the mic); `obs.py record-dir` and `raw` are read-only.

Every take so far is a still frame: `muse exec` prints nothing until it finishes. The extended runs need the session recorded as it happens.

Build, per `~/work/demo-shoot/README.md`'s layout and the receipts and before-yes rigs:
- Muse Code run interactively (not `muse exec`) in a visible terminal at 1920×1080, driven by `tmux send-keys`, so its output streams on screen. The prompt is typed verbatim; nothing steers the session after it.
- OBS records that terminal over obs-websocket (connection settings from a file under `~/.config/`, never committed or printed), one take per session into `takes/<date>-<what>.mkv`.
- The review page recorded with Playwright at 1920×1080 (the `playwright` package in `~/work/lambo/scripts/recording/`), a fresh context, and the reviewer link never shown on screen.
- Keys stripped from every Muse Code environment, as in T7.2b. How an approval prompt Muse Code raises interactively is handled: nobody but the person answers it, and the rig never sends keys to one.

**Done when:** a short test session (one harmless prompt in a scratch folder) is captured live: its take shows the output appearing as it streams, not a single frame; a review-page take from the offline demo state; `capture-rig.md` lets a later agent run the rig cold; no key, token, reviewer link or private path in any take or file.

---

### T9.14: Scrub the scaffolding from what Wonder reads ★
```yaml
requires:   [T9.11, T9.11b]
fixture-ok: no
size:       S · mid
owns:       testdata/legacy/story/v1/, testdata/legacy/story/v2/, testdata/legacy/story/rewrite/wonder-bill.rs (comments only), testdata/legacy/story/generate-*.py (only where they write these files), testdata/replies/ (re-recordings only, if a replay key changes), tests/ (only assertions on the changed text)
status:     done
```

Found validating T9.13's review take: the story's COBOL sources label their own traps and name the build's tasks. `v1/patient.cpy` says the vaccination-visit exemption is "Trap A2" and "replaces the T9.1 sample's EXEMPT-FLAG mechanism"; `v1/BILLBATCH.cbl` calls its fee schedule "the rule Trap A turns on"; the v2 files mark "the ONLY legacy change in v2 (Trap B1)"; both name "T9.4 fixtures"; the rewrites' headers say "story fixture, Trap D". In T9.10 Wonder reads these files to rewrite the batch, so the labels tell it where the traps are, and they render on the review page's before/after evidence in the takes.

Build:
- Remove every trap label, task number, "story"/"fixture"/"planted" word, every mention of memories or flags, and any other build-process wording from the comments of the files Wonder can read in T9.10: the v1 and v2 programs and copybooks, and `rewrite/wonder-bill.rs` (the rewrite handed to Wonder as its earlier work). Keep the comments a real shop's code would carry (what a field holds, what a paragraph does), in the same voice. Change comment text only, never code, and keep each file's line count so line references hold (blank `*>` / `//` lines where text goes).
- **No comment states the vaccination-visit exemption (user decision).** The rule lives only in the code (the `VISIT-CODE` check in the fee paragraph), so Wonder finds it by reading the code or not at all. That covers the copybook's exemption lines, both fee schedules' vaccination-visit entries, v2's "same exemption" and its V-record note, and in `wonder-bill.rs` the header's account of its own wrong rule and the `BUG (L1)` marker. `VISIT-CODE` may still be described as a field ("V" for a vaccination visit), not as a fee rule.
- `rewrite/fixed-bill.rs` is Doyle's control and never goes into Wonder's folder; leave it as it is.
- Prove nothing moved: the legacy parser strips `*>` comments, so the regenerated `elements-v1.json`/`elements-v2.json` and `legacy-diff.json` must be byte-identical, every stable ID unchanged, and v1/v2 must still compile and produce byte-identical reports. If a replay key changes because a prompt quotes the source, say which, and re-record it live within $0.25.
- The Doyle-side notes (`README.md`, `legacy-diff.json` notes, the generators' docstrings, `memories/`) are Doyle's own record and stay as they are.

**Done when:** `grep -rniE 'trap|T9\.|fixture|story|planted|memor|flag|exempt|BUG'` over the v1, v2 sources and `wonder-bill.rs` finds nothing outside code; element files, IDs and batch reports byte-identical; `tests/story_flags.rs` and the gate pass with no key; no comment in those files states the exemption or the rewrite's bug.

Notes (msc-t9.14): scrubbed, gate green, awaiting orchestrator validation.
- Comment text only, line counts held: v1 217, v2 243, both copybooks 11, wonder-bill.rs 69. Every paragraph's first line still matches generate-elements.py's line_hints (v1 MAIN-PARA 96 through WRITE-SUMMARY 203; v2 MAIN-PARA 106 through WRITE-SUMMARY 225), so the committed elements files regenerate byte-identical (proven by regen + cmp, then restored). legacy-diff.json restored untouched: the script writes 4 facts, the committed file carries the hand-maintained 5th WRITE-SUMMARY fact, so it is compare-only by design.
- v1 header: "Fernwhistle billing batch." plus the record/fee/report shape; the schedule keeps the standard 1.5% entry, vaccination-visit entries and the v2 mention are blank `*>` lines. v2 header: "with prompt-payment discount", new paragraph + field, "the record grows by one char; the other fields keep their order"; schedule keeps standard + DISCOUNT-FLAG entries; the memories/flags sentences are blank lines. v1 copybook keeps the VISIT-CODE field line ("V" for a vaccination visit, blank otherwise), exemption lines blanked. v2 copybook keeps the DISCOUNT-FLAG + layout note, Trap B1 label dropped. LOG-EXEMPT (both): "Zeroes the fee and counts the record. The count feeds the summary page." VALIDATE-RECORD (both): "story data" -> "input data". v2 APPLY-PROMPT-DISCOUNT: "v2 only" dropped, "keeps 10%" corrected to "takes 10% off the fee". v2 VERIFY-TOTALS: "v2 discount" -> "the discount". wonder-bill.rs header is build/run instructions only; the BUG (L1) line is a blank `//`.
- Proofs: grep leaves only code-identifier and literal matches (classified in the report); COBOL v1/v2 rebuilt by hand under docs/legacy.md (user-prefix GnuCOBOL 3.2.0) produce byte-identical reports (cmp); wonder-bill.rs rebuilt with rustc -O, TOTALS $72,213.23 as recorded in evidence.txt; tests/story_flags.rs 6/6 and ./tools/check.sh green with no key in the environment (444 lib + 6 story e2e, live suites ignored).
- No replay key moved: legacy/screens/diffrun recheck and curator prompts render from the parsed index (comments stripped), so every golden key replays unchanged — no live re-recording, $0 of the $0.25 budget spent.
- fixed-bill.rs, README.md, legacy-diff.json notes, generator docstrings, memories/ untouched. No generator writes these files' comments (generate-data.py writes only patients.dat; generate-elements.py hardcodes paras/fields), so no generator edit was needed.
- Surprise: one edit round-tripped a blank comment line (v1 -1, v2 +1 with the total unchanged), caught by the line-count check and repaired with blank `*>` lines; paragraph positions re-verified against the generator hints before proving anything else.

---

### T9.15: The guard holds the ship step by the rewrite's files ★
```yaml
requires:   [T9.11]
fixture-ok: yes
size:       S · mid
owns:       src/guard.rs (the legacy and diffrun matching), src/diffrun_cmd.rs (the manifest's rewrite artifacts, carried onto the flag), src/evidence/diffrun.rs (only if the flag record needs the field), tests/story_flags.rs (the ship-step cases), docs/guard.md (what a diffrun flag guards)
status:     not-started
```

Found by T9.10's guard pre-check, before any Wonder session: on the story's flags the guard denied the console decommission as it should, but allowed every natural ship step (`rustc -O wonder-bill.rs -o bin/billbatch`, `./deploy/ship-billing.sh`, `cp bin/billbatch /srv/billing/billbatch`, `git tag … && git push`, `./wonder-bill > report.txt`) and denied `grep -n LOG-EXEMPT BILLBATCH.cbl`, a read of the batch's code. L1's flag matched on the blamed paragraph's name in the command text. T9.11's end-to-end test passed only because its ship command was written with the name in it (`deploy wonder-bill to production --fee-path LOG-EXEMPT`).

Build (user decision):
- A differential-run flag rests on the rewrite, so it guards the rewrite's own files. The run manifest names them (`rewrite_artifacts`: the source, the built binary, the ship target, as paths or file names); `doyle diffrun` carries them onto the flag. While the flag is open, the guard denies a tool call whose command or file argument names one of them, with Doyle's reason naming L1, the blamed paragraph and the mismatch count.
- Legacy and diffrun flags stop matching COBOL paragraph and field names in free command text, so reading or grepping the batch is allowed. Screen flags keep their screen-type match (the decommission deny is right). Contract flags unchanged.
- Fail visible: a diffrun flag with no artifacts named warns at `doyle diffrun` time that the guard cannot hold any step for it.

**Done when:** the gate passes with no key; through the built binary, on the story's flags, each natural ship step above is denied naming L1, `grep -n LOG-EXEMPT BILLBATCH.cbl` and `sed -n 1,40p BILLBATCH.cbl` are allowed, the console decommission is still denied naming S1, the traps T9.11 allowed are still allowed, and the contrived ship command is gone from the tests; every T6.x guard test passes unchanged.

---

### T9.10: Runs, extended ★
```yaml
requires:   [T9.4, T9.7, T9.8, T9.9, T7.3, T9.11, T9.11b, T9.12, T9.13, T9.14, T9.15]
fixture-ok: no
size:       S · user
owns:       evidence/legacy/
status:     claimed:orchestrator (paused for T9.15)
```
If the user put the extension into the runs (T9.4): the extended migration without and with Doyle, recorded as in T7.2 and T7.3. Without Doyle, Wonder's rewrite carries the wrong rule and the console decommission goes through unconfirmed. With Doyle, the differential run and the re-watched console flag both, the guard stops the step, and the corrections land in Muse Code and Lambo.

The wrong rule in Wonder's rewrite and the dropped confirmation are planted in T9.4's fixtures, not observed from Wonder. Run by an agent with T9.13's rig (user decision): interactive Muse Code recorded live, the planted rewrite handed to Wonder as its earlier session's work and labelled so in the README (`wonder-bill.rs` only; `fixed-bill.rs` and the story's Doyle-side files never enter Wonder's folder), the console from T9.12, the flags from the binary (T9.11, T9.11b). The person's only part is the approval on the review page. The recordings, their README and the judges' page say so. What these runs show is detection, by execution and by Wonder's eyes, traced to stable IDs.

Filmed for the demo video (user decision): every beat is a live OBS take with the rig, as it happens, from the planted setup through Wonder's sessions, Doyle's flags and guard, the review page with the person's approval (a browser window with no address bar, so the reviewer link never shows), the corrected memory and the final counts. `evidence/legacy/SHOTS.md` lists each shot and its take.

**Done when:** both recordings show what the story in `testdata/legacy/story/` says, or their READMEs say plainly where they did not, with the counts and mismatches in the handoff text; every shot in `SHOTS.md` is a live 1920×1080 take, video only, scan clean, or marked missing with the reason.

---

## Exit Criteria

- [x] COBOL runs here and the gate does not need it (T9.1). Two routes, both GnuCOBOL 3.2.0; gate verified COBOL-free.
- [x] Wonder's vision is measured, with what Doyle can and cannot build on (T9.2). Screens 0.92–0.95 recall, diffs 18/18 with zero inventions, recordings with ±5 s padding; ~127k tokens, under $2 of $3.
- [x] A memory can cite legacy code by a stable ID (T9.3). Six kinds, `le2_`/`lc2_`, name-path-keyed SHA-256 IDs; curator 3/3 on the sample.
- [x] The extended story and its traps are signed off by the user (T9.4). Built to the sign-off (A2, B1-as-only-diff, C1, D1-alone); v1/v2 compile and run, recompiled v1 byte-identical.
- [x] Flagging runs over evidence sources, with Vimanam's behaviour unchanged (T9.5). 267 existing tests pass unaltered; 9 new trait tests; guard byte-identical.
- [x] Legacy code, screens and differential runs each flag the planted memory and nothing else (T9.6, T9.7, T9.8). L2 on the legacy diff, S1+span on screens (S2 silent), L1 on the diffrun (fixed rewrite clean).
- [x] A person settles each kind of flag from the review page (T9.9). Screen + legacy settled via the page through untouched T5.3; browser-verified 1280/390 light/dark.
- [ ] The extended runs are recorded, if the user chose them (T9.10).
