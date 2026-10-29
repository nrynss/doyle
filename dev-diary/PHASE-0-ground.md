# P0: Ground

Everything later tasks stand on: the repository, the keys, the demo scenario and its fixtures, and three probes that turn assumptions about Muse Code, Lambo and Vimanam into recorded evidence. Nothing here is product code.

Read first: [`../product.md`](../product.md), [`../docs/architecture.md`](../docs/architecture.md), [`README.md`](README.md).

---

### T0.1: Repository on git footing ★
```yaml
requires:   []
fixture-ok: yes
size:       XS · light
owns:       .gitignore, LICENSE, README.md, Cargo.toml, rust-toolchain.toml, tools/check.sh, .github/workflows/ci.yml, src/main.rs
status:     done
```
The repository, the Apache-2.0 licence, the pinned toolchain (Rust 1.99.1, edition 2024), a stub CLI with `check` and `serve`, the gate (`tools/check.sh`: rustfmt, clippy with warnings denied, tests) and CI running the same script.

**Done when:** a fresh clone builds, the gate passes, CI runs the gate on push to `main` and on pull requests. Met by the first commits.

---

### T0.2: Keys and accounts ★
```yaml
requires:   []
fixture-ok: no
size:       XS · user
owns:       .env.example
status:     done
```
The user confirms access and where each key lives; the orchestrator writes `.env.example` with variable names only.

- **Meta Model API:** the key Muse Code and Doyle's re-check use. Settle the variable name (for example `META_API_KEY`) and confirm `muse-spark-1.3` answers on it.
- **Baseten:** `BASETEN_API_KEY` (or the keyring). Settle the Muse Spark Open model and its endpoint for the curator.
- **Muse Code:** installed and signed in (1.4.14 confirmed). Record the plan: pay-as-you-go, Wonder in Standard mode (handoff log, "Models available").
- **Lambo:** a running server reachable at `http://127.0.0.1:7700/mcp`, and the session Doyle writes to.

**Done when:** `.env.example` lists every variable with a one-line comment and no value; the handoff log records which models answered, with no key value anywhere.

**Closed.**
- **Meta Model API:** `MODEL_API_KEY`, or the keyring (`service meta-model-api`). The key OMP was already signed in with was copied into the keyring; `muse-spark-1.3` answered on `https://api.meta.ai/v1/chat/completions`.
- **Baseten:** `BASETEN_API_KEY`, or the keyring (`service baseten`). The curator model is `meta-models/muse-spark-open-1026` on `https://inference.baseten.co/v1`. The undated `meta-models/muse-spark-open` returns 403 "please check the api-key you provided" for the same key, though the key lists both models.
- **Muse Code:** 1.4.14, pay-as-you-go, Wonder in Standard mode.
- **Lambo:** reachable at `http://127.0.0.1:7700/mcp` (406 to a plain request, as MCP expects). T0.5 picks the scratch session.

---

### T0.3: Demo scenario and fixtures ★
```yaml
requires:   []
fixture-ok: yes
size:       S · frontier
owns:       testdata/contracts/, testdata/memories/, testdata/README.md
status:     in-review
```
The scenario every later task tests against. The user signs off the story; the agent writes the files.

- **The API:** a small records service as OpenAPI 3. `testdata/contracts/records-v1.yaml`: `GET /records` returns every record. `records-v2.yaml`: the same operation returns one page plus a `next_cursor`, with a `cursor` argument and a page-size default. Add at least one unrelated additive change in v2 (a new optional field elsewhere) so "additive flags nothing" has a case. Keep both small enough to read in one screen.
- **The memories, as each system would hold them:**
  - `testdata/memories/muse-code/`: a Muse Code memory folder in the verified format (`MEMORY.md` with frontmatter and inline notes, plus one per-topic file), holding the stale belief ("`GET /records` returns all records in one response"), one memory resting on an element that does not change, and one resting on the additively changed type.
  - `testdata/memories/lambo/`: the same three as Lambo concepts, as JSON matching what Lambo's MCP tools return (T0.5 confirms the shape).
- `testdata/README.md` says what each file is for, and which memory should be flagged by the v1 → v2 change and which should not.

**Done when:** both contracts parse with `vimanam` (T0.6 records it); the expected flag set is written down in `testdata/README.md` before any flagging code exists; the user has signed off the story.

**Notes (in review).**
- **Built.** Two contracts with three operations each (`listRecords`, `getRecord`, `getStats`); a Muse Code memory folder (`MEMORY.md` with two inline notes and an index line, plus `records-stats.md`); the same three memories as a Lambo `lambo_derive` call and a `lambo_recall` result; `testdata/README.md` with the five-line story, what each file is for, and the expected flag set `{M1}`.
- **Decided.** The additive change is on `Stats`, a type of its own, so the unchanged operation (`getRecord`) shares nothing that changes. `Record` stays identical in both versions and is the item type of `GET /records` in both, which makes the shared-`$ref` trap explicit: the change belongs to `listRecords`' response, not to `Record`. The stale note stays inline in `MEMORY.md`, the layout the annotation was verified on; the topic file holds a memory that is never flagged. M3 (`record_count` is the total) stays true, and the demo can use it to show the lost records.
- **Lambo shapes** follow the published MCP reference; ids and scores are made up. T0.5 confirms them against a live server. `lambo_inspect` is not modelled, because its structured result is documented only in part.
- **Vimanam.** The only binary on hand is 1.7.0 (`~/.local/bin`, `~/.cargo/bin`); the build in the Vimanam checkout reports 1.4.2. Both parse the contracts. 1.7.0's diff: one breaking (`GET /records` 200 `/type` `array` → `object`), one review (200 `/items` removed), six non-breaking (`cursor`, `limit`, `items`, `next_cursor`, `items` required, and `GET /stats` `oldest_created_at`); `GET /records/{id}` has no change. 1.7.0 has no inferred pagination change and uses `vc1_` IDs; the 2.x run is T0.6's.

---

### T0.4: Muse Code probes ★
```yaml
requires:   []
fixture-ok: no
size:       S · mid
owns:       testdata/muse/, docs/muse-code.md
status:     claimed:sonnet-t0.4
```
Three questions the adapter and the guard depend on, answered on the installed Muse Code with evidence. Part of this is already known (README handoff log, "Muse Wonder's memory is its memory file"); restate it in `docs/muse-code.md` and close the rest.

1. **The memory folder hash.** How `<path-slug>-<16-hex hash>` under `~/.local/share/muse/memory/projects/` is derived from a repository path (a hash of the canonical path, the git root, the remote?). Test against several scratch repositories, including a linked worktree. If it can't be derived, record the fallback: match the slug and pick the newest, and what that costs.
2. **Hook registration on 1.4.x.** Whether a `hooks` block in `~/.config/muse/settings.json` fires, whether a project hook loads in a trusted workspace, and whether a plugin is needed. Register a `PreToolUse` and a `SessionStart` hook that only log their stdin to a file, and run one `muse exec` session against a scratch repository.
3. **The hook wire format.** Record the exact stdin JSON for `PreToolUse` (a shell `curl` and a file edit) and `SessionStart` into `testdata/muse/hooks/`, and confirm what output denies an action with a reason, and what output injects context at session start.

Use scratch repositories under the session scratchpad. Never change the user's real settings permanently: back up `settings.json` first and restore it, and say so in the report.

**Done when:** `docs/muse-code.md` answers all three with the commands run and their output; the recorded payloads are in `testdata/muse/hooks/`; a deny and an injection were each seen to work, or the report says exactly what failed.

---

### T0.5: Lambo probe ★
```yaml
requires:   []
fixture-ok: no
size:       S · light
owns:       testdata/lambo/, docs/lambo.md
status:     blocked
```
Turn "Lambo is reached as a client" into recorded calls Doyle can be built against without a running server.

- Speak MCP over streamable HTTP to a running Lambo (`initialize`, `tools/list`, then `lambo_recall`, `lambo_inspect`, `lambo_derive`) in a scratch session, not the user's working session.
- Write a memory, then a correction with `against` naming it, and record whether recall then warns on the memory itself.
- Record each request and response as JSON in `testdata/lambo/` (no key, no personal data), and the session handshake.

**Notes (blocked, not in review).** The read half is done: `initialize`, `notifications/initialized` and `tools/list` are recorded in `testdata/lambo/`, and `docs/lambo.md` lists the calls with their shapes. The write half is not done. The running server (Lambo 0.8.0) pins only `lambo-dev`; `/mcp/s/<scratch>` answers 404, and the docs show no scratch session reachable without a configuration change and a restart of that server. No write was sent. Options and the choice are in `docs/lambo.md`, "Addressing a session other than `lambo-dev`". The warning on a contradicted memory is therefore not observed. Unblock by choosing one of the three options there, then the write steps run.

**Done when:** `docs/lambo.md` lists the calls Doyle needs with their request and response shapes; the transcripts are in `testdata/lambo/`; the warning on recall was seen, or the gap is written as issue text in the handoff log.

---

### T0.6: Vimanam probe ★
```yaml
requires:   [T0.3]
fixture-ok: yes
size:       XS · light
owns:       testdata/vimanam/
status:     not-started
```
Run Vimanam 2.x on the demo contracts, through its CLI and a scratch program using the library, and record what Doyle will get: the entity IDs for every operation, type and field touched, and the change facts (`subject`, `kind`, `severity`) for v1 → v2. Confirm the pagination change is reported, and with which severity. The scratch program is not committed; its output is.

**Done when:** `testdata/vimanam/records-v1-v2.json` holds the v2 JSON diff and `testdata/vimanam/entities-v1.json` the v1 entity list; the handoff text says whether the pagination change is detected and how it is classified.

---

### T0.7: A second real upgrade
```yaml
requires:   []
fixture-ok: no
size:       M · frontier
owns:       testdata/contracts/second/, docs/second-upgrade.md
status:     not-started
```
product.md's open question: a real public API upgrade with visible stakes (payments, auth, personal data) where a memory written against the old version would cause harm. Find candidates, capture both versions from their public sources, run Vimanam on them, and write three to five memories an agent plausibly would have saved. The user picks one.

**Done when:** `docs/second-upgrade.md` lists the candidates with their source links and diffs; the chosen pair and its memories are in `testdata/contracts/second/`; the user has chosen.

---

## Exit Criteria

- [x] The repository builds and the gate passes in CI (T0.1).
- [ ] Every key Doyle needs is named in `.env.example`, and access is confirmed (T0.2).
- [ ] The demo contracts and seed memories exist, with the expected flag set written down (T0.3).
- [ ] The memory folder hash, hook registration and hook payloads are settled with evidence (T0.4).
- [ ] Lambo's calls are recorded, and the warning on a contradicted memory is confirmed (T0.5).
- [ ] Vimanam's view of the demo change is recorded (T0.6).
