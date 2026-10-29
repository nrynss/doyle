# P4: Models

Meta models, one job each (product.md): the curator records what each memory rests on, and the re-check proposes the fix for a flagged one. The clients are plumbing; the prompts are the work, and they are verified by measurement on recorded replies.

Read first: [`../product.md`](../product.md) (Meta models, one job each), [`../docs/architecture.md`](../docs/architecture.md) (The plug-in seams), the trait and replay client from T1.4.

---

### T4.1: Meta Model API client ★
```yaml
requires:   [T1.4, T0.2]
fixture-ok: yes
size:       S · mid
owns:       src/models/meta.rs
status:     done
```
A `ModelClient` for the Meta Model API (Muse Spark for the re-check): structured output against the request's JSON schema, reasoning effort, usage and cost read from the reply, retries on rate limits and 5xx with a cap, the key from the environment (T0.2). Never logs the key or the request headers.

Offline tests use recorded HTTP replies.

**Done when:** a recorded reply parses into the schema; a malformed reply fails with the model id and the first lines of the reply; a 429 retries and then gives up with a clear error.

**Notes (T4.1)**
- Built: `src/models/meta.rs` with `MetaModelClient` implementing `ModelClient`, plus a small `HttpTransport` seam (trait + `HttpResponse` + `HttpTransportError`). The client reads `MODEL_API_KEY` from the environment at call time, POSTs an OpenAI-style chat-completions body with a `json_schema` response format, passes `reasoning_effort` and `max_tokens` through, retries 429/5xx and transport errors with a cap, and parses `choices[0].message.content` into JSON plus usage (prompt/completion tokens and cost from several field-name variants). Errors include the model id and the first lines of the reply; no key or header is ever stored or displayed.
- Decided: the transport seam is a trait object (`Box<dyn HttpTransport>`) so the ureq-backed implementation can be swapped in by T3.3/T4.2 without touching this module or `Cargo.toml`; `HttpTransport::post` returns status + body, leaving retry/parsing here. The key comes only from the environment (variable name overridable for tests), not the keyring, per the task's "from the environment (T0.2's variable)". Reasoning effort is configured on the client since `ModelRequest` (frozen in T1.4) carries no such field.
- Tests: 10 offline tests with a fake/capturing transport (recorded-reply parse, malformed body, 429 retry-and-give-up, 5xx retry, 400 immediate, missing key, reasoning-effort pass-through, malformed content string). `./tools/check.sh` passes (77 tests).
- Surprised me: edition 2024 makes `std::env::set_var` unsafe, so the test helper wraps it; `Ordering` had to be imported explicitly in the test module.

**T4.1b (live):** one structured call on `muse-spark-1.3`; record it through T1.4's recording mode. Owns `tests/live_meta.rs`. Budget: five calls.

---

### T4.2: Baseten client
```yaml
requires:   [T1.4, T0.2]
fixture-ok: yes
size:       S · mid
owns:       src/models/baseten.rs
status:     done
```
A `ModelClient` for Muse Spark Open on Baseten (the curator, for the open-weight bonus), with the same guarantees as T4.1. The endpoint and model come from T0.2.

**Done when:** as T4.1, on recorded Baseten replies.

**Notes (T4.2)**
- Built: `src/models/baseten.rs` with `BasetenModelClient` implementing `ModelClient`, plus a duplicated small `HttpTransport` seam (`HttpResponse`, `HttpTransportError`, trait) mirroring T4.1 (no shared seam exists in `models/mod.rs`, and only this file may be touched). Reads `BASETEN_API_KEY` from the environment at call time, POSTs an OpenAI-style chat-completions body with a `json_schema` response format, passes `reasoning_effort` and `max_tokens` through, retries 429/5xx and transport errors with a cap, and parses `choices[0].message.content` into JSON plus usage (prompt/completion tokens and cost from several field-name variants). Errors include the model id and the first lines of the reply; no key or header is ever stored or displayed. Fixed constants `DEFAULT_BASE_URL = "https://inference.baseten.co/v1"`, `DEFAULT_MODEL = "meta-models/muse-spark-open-1026"` (plus aliases), `DEFAULT_ENV_VAR = "BASETEN_API_KEY"` cite T0.2.
- Decided: duplicate the transport seam rather than reuse a shared one, because `models/mod.rs` has none and file ownership forbids editing it; the ureq-backed transport lands with T3.3's dependency line. Reasoning effort is configured on the client since `ModelRequest` carries no such field. Strict is kept `false` to mirror the Meta client; Muse Spark Open accepts `json_schema` but does not enforce it.
- Tests: 23 offline tests with a fake/capturing transport (recorded-reply parse, malformed body, 429 retry-and-give-up, 5xx retry, 400 immediate, missing key, reasoning-effort pass-through, malformed content string, pinned body/headers, bounded error, transport-error retry, usage spellings, constants, Debug no-leak). `./tools/check.sh` passes in the worktree.

**T4.2b (live):** one structured call on the curator model; record it. Owns `tests/live_baseten.rs`. Budget: five calls.

---

### T4.3: Curator prompt ★
```yaml
requires:   [T4.1, T2.1, T0.3]
fixture-ok: yes
size:       M · frontier
owns:       src/prompts/mod.rs, src/prompts/curator.rs, testdata/replies/curator/
status:     done
```
Given one memory's text and the contract index (operations, types and fields with their native names and `ElementId`s), return the elements the memory rests on, the claim in one line, and a confidence; or "rests on nothing in this contract". The model chooses only from IDs it was given; any other ID is dropped in code, never trusted.

**Notes (T4.3)**
- Built: `src/prompts/curator.rs` holds `CURATOR_SYSTEM`, `curator_schema()` (element_ids, claim, confidence 0–1, rests_on_nothing), `render_contract_elements()` (one line per entity with native names), `build_curator_request()`, `allowed_ids()`, and `parse_curator_value()` which drops unknown IDs in code (unknown-only replies become RestsOnNothing) plus a `CuratorVerdict` enum. `src/prompts/mod.rs` declares the module (noting T4.4's `mod recheck;` lands with T4.4). 12 offline tests: schema shape, prompt contents, forged-ID dropped, forged-only → nothing, rests-on-nothing, malformed-by-field, confidence clamp, defensive `rests_on` alias (never observed live; synthetic-test-only), and three replay tests over the committed recordings (byte-identical replay via ReplayClient, prompt pinned, unanimous nothing-cases, exact P/R scores).
- Measured: 16 memories (M1–M4 plus X1–X12: path/op-ID/behaviour phrasings, three outside-contract, three v1-absent traps) × both models = 32 recordings in `testdata/replies/curator/` with `expected.json` (hand-written) and `MEASUREMENT.md`. Meta: P 0.964 / R 0.964 (14/16 exact). Baseten: P 0.886 / R 0.969 (13/16 exact). Neither API returned cost; ~1.8k in / ~1.2–1.3k out tokens per call. Spend: 46 calls total, no 429/5xx.
- Decided: budget 2048 (512 starved both models); parser accepts `rests_on` as a purely defensive alias for `element_ids` (never observed live; synthetic-test-only); three length-failure cases recovered with `reasoning_effort: low` (recorded in MEASUREMENT.md, whose per-file ledger lists the effort per recording).
- Surprised me: both models cite `listPatients` for cursor paging that exists only in v2 (strict expected: nothing, both flagged); Baseten hallucinates stats citations for v2-only `generated_at` on X10 while meta correctly cites nothing.
- Recommendation for the user (pick pending): Muse Spark is the safer curator — higher precision (0.964 vs 0.886), no hallucinations on the v1-absent traps, schema-conformant replies. Baseten/Muse Spark Open is viable for the open-weight bonus (recall 0.969) but needs the alias/ID-filtering the code already does, and it invented citations on X10.

Measure on the demo's memories and on at least ten more written to vary phrasing (an endpoint named by path, by operation ID, by behaviour only; a memory about something outside the contract). Run on the Baseten curator model (T4.2) and on Muse Spark (T4.1), and record both.

**Done when:** the prompt and its schema are in `src/prompts/curator.rs`; recorded replies replay offline; the handoff text gives the measured precision and recall of citations against a hand-written expected set, per model, and the user has picked the curator's model.

---

### T4.4: Re-check prompt ★
```yaml
requires:   [T4.1, T2.2]
fixture-ok: yes
size:       M · frontier
owns:       src/prompts/recheck.rs, testdata/replies/recheck/
status:     claimed:muse-spark-t4.4
```
Given a flagged memory, its citation, the changes behind the flag, and the old and new versions of each cited element, propose: **correct** (with replacement text that is true of the new contract, short and plain), **retire** (the memory no longer applies), or **keep** (the change doesn't touch what the memory claims), with a one-line rationale. Replacement text quotes nothing from the contract that could read as an instruction (README, guideline 7).

Measure on the demo's flag and on cases built to need each answer, including a flag the re-check should answer with keep.

**Done when:** the prompt and schema are in `src/prompts/recheck.rs`; recordings replay offline; each answer type is produced where expected, and the handoff text gives the measured results.

---

## Exit Criteria

- [ ] The Meta Model API client works offline and live (T4.1, T4.1b).
- [ ] The Baseten client works offline and live (T4.2, T4.2b).
- [x] The curator's citations are measured on both models, and the user has chosen its model (T4.3). Chosen: Muse Spark Open on Baseten (open-weight bonus over Spark's precision edge).
- [ ] The re-check produces correct, retire and keep where each is right, measured (T4.4).
