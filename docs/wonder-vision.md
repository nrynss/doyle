# Wonder's vision, measured (T9.2)

What Doyle can rely on when it uses `muse-wonder-1.0` on the Meta Model API
(`POST https://api.meta.ai/v1/chat/completions`) as its eyes: how well Wonder
reads a 3270 green screen, diffs two versions, and timestamps a recording;
how much the answers move run to run; what it costs; which request shapes work.

Every number below traces to a committed recording in `testdata/vision/runs/`
(`<case>-run<N>.request.json` holds the prompt, cap, effort and media hashes;
`<case>-run<N>.response.json` holds the full reply with usage). Ground truth
is the `*.json` beside the media; `generate.py` renders both from one model,
`score.py`/`score2.py` compute the element/step/diff tables into
`runs/element-scores.json`, `runs/step-scores.json`, `runs/diff-scores.json`.
The gate replays nothing live: there is no Rust code in this task, only files.

Model: `muse-wonder-1.0`. `reasoning_effort: "low"` on every model call (only
this value was tried). `max_tokens` is a cap on reasoning + output together.
No reply ever carried a cost field, so spend is tracked in tokens; the dollar
estimate under Cost states its basis.

Media (all committed): `lookup-v1.png` (42,703 B, the 3270 patient-lookup
screen), `lookup-v1-copy.png` (byte-identical), `lookup-v2.png`,
`decomm-v1.png`, `decomm-v2.png` (all 960x648 PNG), `recording-60s.mp4`
(149,057 B, 60.0 s, 5 fps, 300 frames: menu, typing, results, scrolling, menu
overlay, decommission confirm, typing to confirm),
`limit-big-60s.mp4` (969,221 B, 60 s, 1920x1296),
`limit-huge-120s.mp4` (2,219,214 B, 120 s, 1920x1296),
`limit-noisy-5s.mp4` (11,105,023 B, 5 s, noisy),
`tiny-2s.mp4` (20,335 B, 2 s, shape probes).

## 1. Screen element list

Prompt (system): "You read IBM 3270 green-screen operator consoles. Reply with
exactly the JSON the schema asks for: one entry per text element visible on
screen. Positions are 1-indexed: row 1 is the top line, column 1 is the
leftmost column. kinds: title, screen-id, heading, datetime, label, value,
input, message, status, footer, pfkey." User: "List every text element on this
80x24 green screen: titles, headings, labels, values, input fields, messages,
the status line and function-key hints. For each give its exact text,
1-indexed row and column, kind, and whether the field is protected (true) or
an editable input (false)." Schema: `elements[{text,row/col/kind/protected}]`.
Ground truth: 37 elements in `lookup-v1.json`.

Scoring: text match after upper-casing and collapsing whitespace; a reported
separator rule (only run 1 reported the dashes line) is ignored, not scored.
Position counts when row is within 1 and column within 2.

| Run | Cap | Finish | Recall | Precision | Positioned | Kind | Protected | In / Out (reasoning) |
|---|---|---|---|---|---|---|---|---|
| 1 | 3000 | stop | 0.9189 | 0.8947 | 1.00 | 1.00 | 1.00 | 943 / 2635 (1365) |
| 2 | 3000 | **length, content truncated** | — | — | — | — | — | 943 / 3000 (1897) |
| 3 | 5000 | stop | 0.9459 | 0.9459 | 0.80 | 1.00 | 1.00 | 943 / 2126 (911) |
| 4 | 5000 | stop | 0.9459 | 0.9459 | 1.00 | 1.00 | 1.00 | 943 / 2495 (1282) |

Spread across the three complete runs: recall 0.919–0.946, precision
0.895–0.946. Kind and protected/unprotected right every time (35/35, 35/35,
34/34). Stable errors, identical in all three runs: `FWV-01` read as `FW-01`
and `FWV-004812` read as `FW-004812` (a `V` dropped in two small dim strings);
the row-20 `____` input field is found every time; value columns wander
(16/17/19 against 16/18). Run 1 alone split `DATE 06/14/26` into two elements.
Run 2 is the cap finding: at `low` effort the model spent 1,897 reasoning
tokens of a 3,000 cap and the JSON was cut off mid-list. A 37-element screen
needs a cap around 5,000 at `low` effort.

## 2. Recording: steps and timestamps

Ground truth (`recording-60s-steps.json`): S1 menu 0–7, S2 type patient ID
7–20, S3 results appear 20–24, S4 scroll 24–40, S5 action menu 40–47, S6
decommission screen 47–53, S7 type clinic name 53–60. Same prompt shape as
above with `steps[{description,start_s,end_s}]`, cap 6000. Tolerance: start
and end each within 5 s. The mapping of reported steps to S1–S7 is the one
hand judgment, recorded in `score2.py`; deviations are computed.

| GT step | Run 1 (9 steps) | Run 2 (9 steps) | Run 3 (10 steps) |
|---|---|---|---|
| S1 0–7 | 0–7 ✓ | 0–7 ✓ | 0–7 ✓ |
| S2 7–20 | 7–20 ✓ | 7–19 ✓ | 7–19 ✓ |
| S3 20–24 | 20–28 ✓ | 20–27 ✓ | 20–28 ✓ |
| S4 24–40 | 28–40.2 ✓ | 27–32 fragments ✗ | 28–40.2 ✓ |
| S5 40–47 | 40.2–47.2 ✓ | 39–40.2, hold missing ✗ | 40.2–47 ✓ |
| S6 47–53 | 47.2–53.2 ✓ | 46.2–47.2, hold missing ✗ | 47–53 ✓ |
| S7 53–60 | 53.2–59.2 ✓ | 52.2–59.2 ✓ | 53–59.2 ✓ |

All three runs find all seven steps. Run 1: 7/7 within tolerance. Run 3: 7/7.
Run 2: 4/7 — it reports the transition moments (menu opens, decommission
opens) but not the static holds, so three ends fall 6–8 s early. Step counts
move (9, 9, 10 reported for 7 true) because typing gets split off the menu
hold. Onsets are stable to about a second; the S3/S4 boundary sits at 28 in
all three runs against a ground truth of 24 — the list visibly moves at 28,
nothing moves 24–28, so the model is arguably righter than the fixture. Usage
is identical all three runs: 13,531 in / ~1,400–1,550 out (reasoning ~500–615).
Two runs misread the first typed character once each (`WV-004812`); the ID is
right everywhere else including all results rows.

## 3. Before/after diffs

Same two-image prompt for all pairs (A first, B second): report each change
with before, after and location; return an empty list for identical versions.
Cap 4000. Ground truth: pair A (`lookup-v1/v2`) 3 changes — PHONE→MOBILE row
12, $42.50→$44.00 row 16, PF5 UPDATE→PF5 SAVE footer; pair B
(`decomm-v1/v2`) 3 changes — warning removed row 9, confirmation prompt
removed row 10, PF12 CONFIRM→DECOMMISSION footer; pair C (byte-identical
copies) no changes.

| Pair | Runs | Caught | Missed | Invented |
|---|---|---|---|---|
| A | 1–3 | C1, C2, C3 every run | none | none |
| B | 1–3 | C1, C2, C3 every run | none | none |
| C | 1–3 | — | — | none (`[]` all three) |

Before/after text is exact in all nine runs. Locations name the right row
except pair B run 3, which says rows 10/11 for rows 9/10. The no-change pair
is the point: three runs, zero invented differences. Usage per call ~1,693 in
/ ~360–750 out (reasoning ~350–640); no-change calls are cheapest (~400 out).

Incident, kept honest: the first diff-A triple measured a four-change pair.
The generator's footer edit had matched the substring `PF5 UPDATE` inside the
row-19 message too, corrupting it in `lookup-v2.png`. The model caught all
four real differences including the unintended one. Those three replies were
deleted (their media hash no longer matches the fixed file), the generator
was fixed to build the footer from a parameter, and the triple was re-run
clean: exactly the three intended rows differ, verified by script.

## 4. Repeatability

Element lists: recall range 0.027, precision range 0.051; kinds and
protected flags perfect and identical; the same two texts misread every time.
Diffs: byte-stable verdicts — 3/3, 3/3, 3/3 caught and `[]` ×3. Steps: all
onsets within ~1 s; hold ends move on static screens (one run in three
fragments holds). Nothing flips between runs: no element, change or step that
one run finds and another denies, apart from run 2's hold durations.

## 5. Limits

| Media | Duration | Bytes | Accepted | Prompt tokens | Tokens/s |
|---|---|---|---|---|---|
| `recording-60s.mp4` | 60 s | 149,057 | yes | 13,531 | 225.5 |
| `limit-big-60s.mp4` | 60 s | 969,221 | yes | 15,336 | 255.6 |
| `limit-huge-120s.mp4` | 120 s | 2,219,214 | yes | 30,636 | 255.3 |
| `limit-noisy-5s.mp4` | 5 s | 11,105,023 | yes | 932 | 186.4 |

Largest verified: an 11 MB `video_url` data URL is accepted, and a 120 s
recording is accepted. No upper bound was found; the 11 MB probe was stopped
there to protect the budget, so 11 MB is a lower bound, not the maximum.
Tokens scale with duration and resolution (225/s at 960x648, 255/s at
1920x1296), barely with bytes (6.5x bytes added 13% tokens). A remote URL is
fetched server-side: `video_url` with `https://example.com/x.mp4` fails only
because the origin 404s (`media_url_origin_error`, see Shapes). No upload
endpoint exists: `GET /v1/files` is 404, and `GET /v1/models` lists nine
models with no file object among them (both replies committed as
`runs/get_v1_files.json`, `runs/get_models.json`).

## 6. Shapes that work and fail

Work: `image_url` with a `data:image/png;base64` URL; `video_url` with a
`data:video/mp4;base64` URL; `response_format` `json_schema` (`strict: false`)
on vision calls; `reasoning_effort: "low"`; `max_tokens` as the combined
reasoning+output cap. Fail, all HTTP 400 with no usage billed:

- `{"type": "input_video", "input_video": {"url": ...}}` →
  `invalid_request_error`: `` `messages[1].content[1]` did not match any
  supported type `` (`shape-input-video-run1`).
- `{"type": "video", "video": {"url": ...}}` → same error text
  (`shape-type-video-run1`).
- `{"type": "video_url", "video_url": {"url": "https://example.com/x.mp4"}}`
  → `invalid_request_error` code `media_url_origin_error`: `failed to
  download media at messages[1].content[1]: origin returned HTTP 404`
  (`shape-file-url-run1`). Validation passed; the fetch was attempted.
- Caps at or below reasoning need: `max_tokens: 300` at `low` effort returns
  `finish_reason: length` with null content (`limit-big-run1`); same for 3000
  on the 37-element screen (`screen-run2`). Budget at least ~2x the expected
  JSON size for reasoning at `low` effort.

## 7. Cost

No reply carries a cost field (`cost`, `cost_usd`, `total_cost` all absent;
top-level and `usage` alike), consistent with the T4 probes. Spend in tokens:

- screens: 4 calls, 3,772 in / 10,256 out (reasoning 5,455)
- recording: 3 calls, 40,593 in / 4,365 out (reasoning 1,711)
- diffs: 9 calls, 15,237 in / 5,303 out (reasoning 5,043)
- limits: 3 calls, 46,904 in / 1,077 out (reasoning 954)
- shape failures: 3 calls, HTTP 400, no usage
- live total: 22 replies at 200 (19 kept), 3 at 400; kept usage 106,506 in /
  20,946 out, 127,452 tokens overall, 12,696 of them reasoning.

Dollar basis: the P9 probe's ~$0.17 for roughly 11k tokens implies on the
order of $15 per million tokens, which puts this task at under $2 — inside
the $3 budget. The API itself gives no price, so this is an estimate, not a
bill.

Prompt caching is visible but inconsistent: repeated identical images show
`cached_tokens` up to 1,652; the 60 s video shows 0 all three runs.

## 8. Verdict

- **Screens: Doyle can build on them.** Recall 0.92–0.95 with zero movement
  on kinds and protection, stable across three runs. The residual OCR wobble
  (a dropped `V`, column drift of a few cells) is why element IDs must come
  from the diff of two reads, never from one read's raw text.
- **Diffs: Doyle can build on them.** 18/18 changes caught across nine runs,
  exact before/after text, and three empty replies on the identical pair. The
  generator incident is the strongest evidence: the model reported a real
  fourth change the fixture did not intend.
- **Recordings: Doyle can build on them, with span padding.** Every step
  found in every run, onsets stable to ~1 s; cite spans with ±5 s padding
  because static-hold ends move and one run in three fragments holds. 60 s is
  measured; 120 s and 11 MB are accepted; cost is ~225–255 input tokens per
  second, so a cited span of seconds costs little and a full training video
  costs knowingly.
