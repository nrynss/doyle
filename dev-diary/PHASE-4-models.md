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
status:     not-started
```
A `ModelClient` for the Meta Model API (Muse Spark for the re-check): structured output against the request's JSON schema, reasoning effort, usage and cost read from the reply, retries on rate limits and 5xx with a cap, the key from the environment (T0.2). Never logs the key or the request headers.

Offline tests use recorded HTTP replies.

**Done when:** a recorded reply parses into the schema; a malformed reply fails with the model id and the first lines of the reply; a 429 retries and then gives up with a clear error.

**T4.1b (live):** one structured call on `muse-spark-1.3`; record it through T1.4's recording mode. Owns `tests/live_meta.rs`. Budget: five calls.

---

### T4.2: Baseten client
```yaml
requires:   [T1.4, T0.2]
fixture-ok: yes
size:       S · mid
owns:       src/models/baseten.rs
status:     not-started
```
A `ModelClient` for Muse Spark Open on Baseten (the curator, for the open-weight bonus), with the same guarantees as T4.1. The endpoint and model come from T0.2.

**Done when:** as T4.1, on recorded Baseten replies.

**T4.2b (live):** one structured call on the curator model; record it. Owns `tests/live_baseten.rs`. Budget: five calls.

---

### T4.3: Curator prompt ★
```yaml
requires:   [T4.1, T2.1, T0.3]
fixture-ok: yes
size:       M · frontier
owns:       src/prompts/mod.rs, src/prompts/curator.rs, testdata/replies/curator/
status:     not-started
```
Given one memory's text and the contract index (operations, types and fields with their native names and `ElementId`s), return the elements the memory rests on, the claim in one line, and a confidence; or "rests on nothing in this contract". The model chooses only from IDs it was given; any other ID is dropped in code, never trusted.

Measure on the demo's memories and on at least ten more written to vary phrasing (an endpoint named by path, by operation ID, by behaviour only; a memory about something outside the contract). Run on the Baseten curator model (T4.2) and on Muse Spark (T4.1), and record both.

**Done when:** the prompt and its schema are in `src/prompts/curator.rs`; recorded replies replay offline; the handoff text gives the measured precision and recall of citations against a hand-written expected set, per model, and the user has picked the curator's model.

---

### T4.4: Re-check prompt ★
```yaml
requires:   [T4.1, T2.2]
fixture-ok: yes
size:       M · frontier
owns:       src/prompts/recheck.rs, testdata/replies/recheck/
status:     not-started
```
Given a flagged memory, its citation, the changes behind the flag, and the old and new versions of each cited element, propose: **correct** (with replacement text that is true of the new contract, short and plain), **retire** (the memory no longer applies), or **keep** (the change doesn't touch what the memory claims), with a one-line rationale. Replacement text quotes nothing from the contract that could read as an instruction (README, guideline 7).

Measure on the demo's flag and on cases built to need each answer, including a flag the re-check should answer with keep.

**Done when:** the prompt and schema are in `src/prompts/recheck.rs`; recordings replay offline; each answer type is produced where expected, and the handoff text gives the measured results.

---

## Exit Criteria

- [ ] The Meta Model API client works offline and live (T4.1, T4.1b).
- [ ] The Baseten client works offline and live (T4.2, T4.2b).
- [ ] The curator's citations are measured on both models, and the user has chosen its model (T4.3).
- [ ] The re-check produces correct, retire and keep where each is right, measured (T4.4).
