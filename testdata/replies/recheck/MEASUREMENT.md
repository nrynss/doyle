# T4.4 measurement: the re-check on Muse Spark

Model: `muse-spark-1.3` on the Meta Model API (`https://api.meta.ai/v1/chat/completions`),
via T4.1's `MetaModelClient`. Output-token cap 2000 (the probe at 500 failed:
the model spent ~500 tokens reasoning and returned `content: null` with
`finish_reason: length`). Prompt and schema are `src/prompts/recheck.rs`
(`system_prompt`, `response_schema`, `measurement_cases`); each recording is
the `ModelResponse` for its request under this directory, named
`<request_key>.json`, so `ReplayClient` replays them offline with no network.
Request keys cover model, system, user, schema and budget.

## Results (4 cases, live, 2026-10-30)

| Case | Request key | Expected | Actual | Rationale (verbatim) |
|---|---|---|---|---|
| `m1_correct` (demo M1: `GET /patients` now pages) | `743fb1aeb5adf40e` | correct | correct | GET /patients changed from bare array of every patient to paginated pages of up to 100 with next_cursor. |
| `s1_retire` (removed `POST /charges`, Bellflower second-upgrade shape) | `4e20334d1683311b` | retire | retire | POST /charges (createCharge) was removed in v2 so the claim no longer applies. |
| `m3_keep` (type-level review: optional `generated_at` on `Stats`) | `9b11e1023796c648` | keep | keep | patient_count remains required integer; v2 only adds optional generated_at |
| `m4_keep_unchanged` (flag on `listPatients` pagination; memory cites unchanged `decommission`) | `bb2ac3f876aad226` | keep | keep | POST /admin/decommission is identical in v2; flag is unrelated GET /patients pagination. |

Provenance note: `s1_retire`'s change/element IDs are constructed (shape mirrors the real S1 removal, IDs synthetic); all other cases' IDs come from `testdata/vimanam/`.

4 / 4 match. Retire/keep replies carry `new_text: null`; the correct reply's
`new_text` (192 chars, single line) passes `check_replacement_text` and states
the paging truth (page object, up to 100 in `items`, `next_cursor`,
cursor/limit defaulting to and capped at 100). Offline tests in
`src/prompts/recheck.rs` replay these files and assert each action plus the
guideline-7 check on the correct text.

## Spend

6 model calls total (cap 50): 1 failed probe at the 500-token cap, 4 scoring
calls, 1 re-run of `m1_correct` after fixing a driver-side match-string bug
(the model answer was already correct; the driver's comparison was wrong).
No 429/5xx retries. Usage per call (input / output tokens; the API reports
no cost field, so `cost_usd` is null throughout):

- `m1_correct`: 843 / 1861 (first attempt 843 / 500, content null)
- `s1_retire`: 544 / 1247
- `m3_keep`: 594 / 754
- `m4_keep_unchanged`: 610 / 755

Total scored usage: 2591 input / 4617 output tokens (output includes
reasoning tokens). No dollar cost observed.
