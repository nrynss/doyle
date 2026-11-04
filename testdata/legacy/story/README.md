# The story, extended: fixtures (T9.4)

The Quiet Migration grows past the API. Fernwhistle's old system is not only
a records service: it is a COBOL billing batch, a copybook, the 4,812 patient
accounts it bills, and a green-screen operator console with a training video.
Muse Wonder's job grows with it: move the records (the original story,
unchanged), **rewrite the billing batch in Rust, prove the rewrite matches,
retire the console** (`plan.md`). Doyle keeps what Wonder believes true, and
stops it at the step where a stale belief would do damage.

User sign-off (options memo, PHASE-9): Trap A as **A2**, Trap B as **B1**
with the add as the **only** legacy diff in v2, Trap C as **C1**, Trap D as
**D1 alone**. No D3. What is built below is exactly that, nothing else.

Hand-built and deterministic: three generator scripts, no model calls, no
network. Re-running them reproduces every byte except the take filenames.

## The cast

The API memories M1-M4 and their expected flag set `{M1}` are unchanged
(`testdata/README.md`). The legacy memories join them:

| Id | Text | True? | Cites | Written by |
|---|---|---|---|---|
| L1 | 1.5% late fee on the whole balance, no exceptions; every account pays | **No** (Trap A2) | CALC-LATE-FEE, BALANCE, LOG-EXEMPT, VISIT-CODE | ops-1 |
| L2 | Each record is 34 chars: 6-digit number, 20-char name, 7-digit balance, 1-char visit code | Yes of v1 | PATIENT-RECORD + fields | ops-1 |
| L3 | CALC-LATE-FEE computes 1.5% of BALANCE, rounded to the cent | Yes | CALC-LATE-FEE, BALANCE | ops-1 |
| L4 | V2 prompt-payment accounts (DISCOUNT-FLAG Y) keep 10% of the fee; APPLY-PROMPT-DISCOUNT applies it | Yes of v2 | APPLY-PROMPT-DISCOUNT, DISCOUNT-FLAG | ops-2 |
| S1 | Retire: open DECOMMISSION, type the clinic name, press PF12 CONFIRM; the warning means it is permanent | Yes of v1 | decomm-v1 screen, training T5-T6 | ops-1 |
| S2 | Look up: enter the ID on the lookup screen; results list the matches; PF5 edits | Yes | lookup screen, training T2-T3 | ops-1 |

L1's shape is the T9.3 probe's false memory promoted to a story memory: the
code exempts vaccination visits (VISIT-CODE `V` -> LOG-EXEMPT, no fee) and
Wonder remembers the fee on everything. A false claim is still cited to what
would make it false: citation is not fact-checking.

## The story, step by step

1. **Setting.** The same 4,812 patients as the API story (record *i* is API
   patient `p-%05d`, name `Patient %05d`). The batch reads one fixed-width
   record per patient through the copybook: v1 is 34 chars
   (number, name, balance, **visit code** — Trap A2 replaces the T9.1
   sample's exempt-flag mechanism with exemption by visit kind), v2 is 35
   chars (plus DISCOUNT-FLAG). 535 records are vaccination visits
   (every 9th record, plus record 3); 107 non-vaccination records carry the
   v2 discount flag.
2. **The code change.** v2 adds exactly one thing (Trap B1, the only legacy
   diff): paragraph APPLY-PROMPT-DISCOUNT + field DISCOUNT-FLAG. Adds are
   non-breaking. The one-char-longer record IS a layout change on
   PATIENT-RECORD. Fee, exemption, bands, files: identical. The wiring
   (`IF DISCOUNT-FLAG PERFORM APPLY-PROMPT-DISCOUNT` in CHARGE-ACCOUNT) is
   part of the add and is cited by no story memory.
3. **The console change.** The lookup screen changes by one cosmetic footer
   relabel only (PF5 UPDATE -> SAVE), which no S-memory cites. The
   decommission screen drops the warning line, the type-the-name prompt, and
   PF12 CONFIRM becomes PF12 DECOMMISSION (Trap C1).
4. **Rewrite.** Wonder ports the batch to Rust carrying L1 (`wonder-bill.rs`:
   1.5% on every account, no visit-code check) with L2's 34-char image and
   L4's discount correct. It compiles; the fee math spot-checks against L3.
5. **Prove.** Wonder's self-check runs batch and rewrite on ten non-exempt
   accounts: every line matches, totals agree. The check passes because the
   sample never visits the exempt path.
6. **Retire.** Wonder follows S1 on the v2 console: the screen takes the
   keys without complaint, because there is no prompt and PF12 acts at once.
7. **Consequence.** 4,812 statements go out with 535 overcharged vaccination
   visits ($8,308.80 too much), and the old system is decommissioned
   unconfirmed. The first sign is a client calling about a vaccination bill.
8. **With Doyle.** The differential run compares all 4,812 records: 535
   detail lines mismatch, every one a vaccination visit (`mismatches.txt`),
   with the inputs and both outputs as evidence; the flag cites L1 through
   LOG-EXEMPT / VISIT-CODE and the guard denies the ship-rewrite step. The
   legacy diff flags L2 (record 34 -> 35) and nothing else. The screen diff
   (old warning + prompt + CONFIRM against blank + DECOMMISSION) flags S1
   with both frames; the guard denies the retire step. The corrected rewrite
   (`fixed-bill.rs`) converges to zero mismatches. Every correction lands in
   Muse Code (supersede) and Lambo (against); the review page names the
   relying steps (`memories/provenance.json`).

## Expected flags

| Memory | Legacy diff v1->v2 | Screen diff v1->v2 | Differential run | Why |
|---|---|---|---|---|
| M1 | n/a (API diff flags) | n/a | n/a | unchanged from `testdata/README.md` |
| M2, M3, M4 | silent | silent | silent | nothing they cite changed or mismatched |
| L1 | **silent** | n/a | **FLAGS** | the code never changed under L1, so no legacy fact exists; the 535 mismatching V-records ran LOG-EXEMPT, which L1 cites |
| L2 | **FLAGS** (breaking layout 34->35) | n/a | silent | layouts are not paragraphs; no record cites a run path |
| L3 | **silent** | n/a | **silent** | CALC-LATE-FEE unchanged; mismatches never ran it (the control) |
| L4 | **silent** | n/a | silent | cites the added elements; adds flag nothing |
| S1 | n/a | **FLAGS** | n/a | warning, prompt and CONFIRM all dropped |
| S2 | n/a | **silent** | n/a | lookup changed by one footer relabel no S-memory cites |

Traps that must not flag: L3 and L4 on the legacy diff (additive/wiring
silence); L2 and L3 on the differential run (clean paths); S2 on the screen
diff (cosmetic silence); the correct rewrite on everything.

## The COBOL batch, and how to run it

v1 is 217 lines, v2 243: the T9.1 paragraphs grown with validation, the fee
schedule, banded summary, cross-foot self-check and a 72-column report. v2
differs only by the Trap B1 add (see `legacy-diff.json`). Both compile and
run under the T9.1 runtime (`docs/legacy.md`): GnuCOBOL 3.2.0, no `-O`
needed, filenames are `ASSIGN`ed literally so run in the version directory.

```
export PATH=$HOME/.local/gnucobol-3.2/bin:$PATH
export LD_LIBRARY_PATH=$HOME/.local/gnucobol-3.2/lib
cd testdata/legacy/story/v1
cobc -x -ftrace -o /tmp/billbatch-v1 BILLBATCH.cbl
COB_SET_TRACE=1 /tmp/billbatch-v1 2> trace.txt
cd ../v2
cobc -x -ftrace -o /tmp/billbatch-v2 BILLBATCH.cbl
COB_SET_TRACE=1 /tmp/billbatch-v2 2> trace.txt
```

Recorded outputs are committed (`report.txt`, `trace-summary.txt`): the full
`-ftrace` trace is 33,895 lines, so the summary holds the paragraph counts
plus the regeneration recipe. Recorded totals:

- v1: 4,812 records, balance $4,814,213.63, fee $63,904.43; 535 exempt
  visits, 0 errors; bands 215 / 993 / 3,604; CROSS-FOOT OK. Trace: 4,277
  CALC-LATE-FEE against 535 LOG-EXEMPT.
- v2: same balance; fee $63,732.15 over 107 discount records ($172.28 off);
  trace adds 107 APPLY-PROMPT-DISCOUNT; CROSS-FOOT OK.

## Files

| Path | What |
|---|---|
| `v1/BILLBATCH.cbl`, `v1/patient.cpy`, `v1/patients.dat` | batch, 34-char copybook, 4,812 records |
| `v1/report.txt`, `v1/trace-summary.txt` | recorded run + paragraph counts |
| `v2/` (same names) | the add only: discount paragraph + 35-char copybook + records |
| `v2/report.txt` | recorded run: fee $63,732.15, 107 discounts |
| `screens/lookup-v1.png` etc. + `*.json` | four stills with ground-truth element lists |
| `screens/lookup-changes.json`, `screens/decomm-changes.json` | expected screen diffs (1 cosmetic; 3 cited by S1) |
| `training-v1.mp4` (45 s, 5 fps, 121 KB) + `training-v1-steps.json` | v1 training video: menu, lookup, type ID, results, scroll, decomm, type name; S1 cites T5-T6, S2 cites T2-T3 |
| `memories/billing-rules.md`, `memories/console-procedures.md` | L1-L4, S1-S2 in Muse Code format |
| `memories/lambo.json`, `memories/provenance.json` | Lambo derive shape + who relied on what |
| `elements-v1.json`, `elements-v2.json` | 24 / 26 elements with `le2_` IDs (all 24 shared IDs stable across versions; CALC-LATE-FEE matches the worked example in `docs/legacy-ids.md`) |
| `legacy-diff.json` | the 5 expected change facts (1 layout breaking -> L2; 2 wiring breaking with no starring memory; 2 non-breaking adds) |
| `rewrite/wonder-bill.rs`, `rewrite/fixed-bill.rs` | Wonder's wrong rewrite (carries L1) + the correct control; `rustc -O`, no crates |
| `rewrite/mismatches.txt`, `rewrite/evidence.txt` | 535 mismatching V-records with both fees + recorded totals (wrong $72,213.23; fixed byte-identical to the batch) |
| `generate-data.py`, `generate-screens.py`, `generate-elements.py` | the deterministic generators |
| `plan.md` | the grown migration plan (beats B1-B5) |

Raw captures live outside the repo in
`~/work/demo-shoot/doyle/takes/` (`2026-10-31-console-v1.mkv`,
`2026-10-31-console-v2.mkv`, `2026-10-31-training-v1.mkv`); the small fixture
copies above are what the gate and later tasks read. The video sits within
the measured limits (`docs/wonder-vision.md` §5/8): 45 s of 120 s, 121 KB
of 11 MB, S-spans citable with the ±5 s padding.
