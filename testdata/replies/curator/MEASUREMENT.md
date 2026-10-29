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
- Baseten returned the ID list under `rests_on` instead of `element_ids`
  (it accepts the JSON schema but does not enforce it). The system prompt
  now names the exact fields, and the parser accepts the one observed alias.
  M1 with the alias still scored 2/2.
- Three cases needed `reasoning_effort: low` after three default-effort
  length failures each (M3/meta, M4/baseten, X12/baseten; recorded requests
  carry the effort that produced them). Default effort burned the whole
  2048-token budget on reasoning for these inputs; low effort answered in
  310–744 output tokens with full marks on M3 and M4.

## Files

- `<case>-<model>.json`: 32 recordings (16 cases × 2 models).
- `expected.json`: the hand-written expected set.
- This file: how the numbers were produced.
