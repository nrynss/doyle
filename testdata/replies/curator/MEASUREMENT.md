# T4.3 curator measurement

Live runs of the curator prompt on the demo memories (M1–M4) plus twelve
extra memories (X1–X12), on both models. Each case file holds the case id,
the memory text, the full request (model, system, user, schema, budget) and
the model's reply with usage. `expected.json` is the hand-written expected
citation set each reply is scored against.

## Models

- `meta`: Muse Spark (`muse-spark-1.3`) via the T4.1 client on
  `https://api.meta.ai/v1/chat/completions`.
- `baseten`: Muse Spark Open (`meta-models/muse-spark-open-1026`) via the
  T4.2 client on `https://inference.baseten.co/v1/chat/completions`.

## Results (against `expected.json`)

| Model | Micro precision | Micro recall | Exact-set cases |
|---|---|---|---|
| meta (Muse Spark) | 27/28 = 0.964 | 27/28 = 0.964 | 14/16 |
| baseten (Muse Spark Open) | 31/35 = 0.886 | 31/32 = 0.969 | 13/16 |

Scoring: a predicted ID counts as right when it is in `expected` or in
`also_accepted` (ancestors, arguments, types the claim names); anything else
is a false positive; a missing `expected` ID is a false negative. An empty
prediction on a `rests_on_nothing` case scores 1.0/1.0.

Per-case deviations:

- M2/meta cited only the 404 result, not the `getPatient` operation
  (recall 1/2). M2/baseten cited both.
- X1/baseten cited only the 404 result, not the operation (recall 1/2).
  X1/meta cited both.
- X10 (generated_at, v2-only; expected: nothing): meta correctly cited
  nothing; baseten cited the stats operation, response and type (3 false
  positives).
- X12 (cursor paging, v2-only; expected: nothing): both models cited the
  `listPatients` operation (1 false positive each). Judgment call, recorded
  here: the claim is false of v1 and no cursor element exists, so the strict
  reading is "rests on nothing"; the operation is the nearest element.

Everything else, including all three unanimous rests-on-nothing cases (X4
report time, X8 Lambo warning, X11 nonexistent POST), matched exactly.

## Cost and usage observed

Neither provider returned a `cost_usd` value on any call, so cost is
reported in tokens (prompt + reply from each reply's `usage`):

| Model | Calls recorded | Avg input tokens | Avg output tokens |
|---|---|---|---|
| meta | 16 | 1830 | 1334 |
| baseten | 16 | 1807 | 1201 |

Live spend: 46 model calls total (2 failed probes at budget 512, 2 ok
probes, 32 full-run calls with 4 length failures, 7 single-case retries, 3
low-effort recoveries). No 429/5xx retries were needed; all failures were
empty or truncated replies at the token cap. No dollar cost was reported by
either API.

## Prompt adjustments during measurement

- Budget 512 starved both models (empty replies, `finish_reason: length` on
  meta). Raised to 2048 in `build_curator_request`; committed.
- The parser accepts `rests_on` as a purely defensive alias for the
  `element_ids` field. It was never observed live: all 32 committed replies
  carry the ID list under `element_ids` (grep for `"rests_on"` over this
  directory finds zero hits outside `rests_on_nothing`). The system prompt
  names the exact reply fields, and the alias path is covered only by a
  synthetic unit test in `src/prompts/curator.rs`. Scores are unaffected
  either way, since every recorded reply parsed through the primary field.
- Three cases needed `reasoning_effort: low` after three default-effort
  length failures each (M3/meta, M4/baseten, X12/baseten). Default effort
  burned the whole 2048-token budget on reasoning for these inputs; low
  effort answered with full marks on M3 and M4 (per-file output-token
  counts are in the effort ledger below). Note: `reasoning_effort` is a
  setting on the model client, not part of the recorded request envelope,
  so the effort that produced each file is listed in the ledger, not read
  from the recordings.

## Effort ledger

`reasoning_effort` lives on the model client (`with_reasoning_effort`), not
in `ModelRequest`, so it is NOT in any committed recording: every
`request` object in this directory holds exactly the keys `budget`,
`model`, `schema`, `system`, `user` (verified over all 32 files). The
effort setting that produced each file is therefore listed here. Output
tokens are each reply's `usage.output_tokens`, verified from the
recordings.

Low effort (`reasoning_effort: "low"`):

- `M3-meta.json` — low — 744 output tokens.
- `M4-baseten.json` — low — 466 output tokens.
- `X12-baseten.json` — low — 310 output tokens.

Default effort (all other 29 files):

- `M1-meta.json` — default — 1408 output tokens.
- `M1-baseten.json` — default — 634 output tokens.
- `M2-meta.json` — default — 1896 output tokens.
- `M2-baseten.json` — default — 1975 output tokens.
- `M3-baseten.json` — default — 1685 output tokens.
- `M4-meta.json` — default — 1504 output tokens.
- `X1-meta.json` — default — 1591 output tokens.
- `X1-baseten.json` — default — 1183 output tokens.
- `X2-meta.json` — default — 1276 output tokens.
- `X2-baseten.json` — default — 985 output tokens.
- `X3-meta.json` — default — 1224 output tokens.
- `X3-baseten.json` — default — 1199 output tokens.
- `X4-meta.json` — default — 1228 output tokens.
- `X4-baseten.json` — default — 1005 output tokens.
- `X5-meta.json` — default — 1993 output tokens.
- `X5-baseten.json` — default — 1240 output tokens.
- `X6-meta.json` — default — 1329 output tokens.
- `X6-baseten.json` — default — 1717 output tokens.
- `X7-meta.json` — default — 1181 output tokens.
- `X7-baseten.json` — default — 1514 output tokens.
- `X8-meta.json` — default — 362 output tokens.
- `X8-baseten.json` — default — 538 output tokens.
- `X9-meta.json` — default — 1397 output tokens.
- `X9-baseten.json` — default — 1418 output tokens.
- `X10-meta.json` — default — 1651 output tokens.
- `X10-baseten.json` — default — 1755 output tokens.
- `X11-meta.json` — default — 698 output tokens.
- `X11-baseten.json` — default — 1588 output tokens.
- `X12-meta.json` — default — 1866 output tokens.

## Single-case retry ledger

The 46-call total breaks down as 2 failed probes at budget 512, 2 ok
probes, 32 full-run first attempts (4 of which length-failed), 7
single-case retries, and 3 low-effort recoveries. The per-case breakdown
of the 7 retries cannot be reconstructed from the committed evidence:
each recording holds only its final successful reply (no attempt counter,
timestamp, or failure record), and no driver log was committed. One line
per retry, as far as the evidence goes:

1. Re-run of a length-failed full-run first attempt — case undetermined from committed evidence.
2. Re-run of a length-failed full-run first attempt — case undetermined from committed evidence.
3. Re-run of a length-failed full-run first attempt — case undetermined from committed evidence.
4. Re-run of a length-failed full-run first attempt — case undetermined from committed evidence.
5. Further single-case retry after an empty or truncated reply — case and trigger undetermined from committed evidence.
6. Further single-case retry after an empty or truncated reply — case and trigger undetermined from committed evidence.
7. Further single-case retry after an empty or truncated reply — case and trigger undetermined from committed evidence.

(Lines 1–4 must include the re-runs that completed the 32-file set after
the 4 full-run length failures; the 3 low-effort recoveries listed above
are counted separately as the final attempts for their cases.)

## Files

- `<case>-<model>.json`: 32 recordings (16 cases × 2 models).
- `expected.json`: the hand-written expected set.
- This file: how the numbers were produced.
