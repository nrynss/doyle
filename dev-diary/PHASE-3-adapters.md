# P3: Adapters

The memory systems Doyle corrects, each behind the `MemoryAdapter` trait. The lead is Muse Code's built-in memory, the memory Muse Wonder writes and recalls. The second is Lambo. Each adapter is one module; the core never names either.

Read first: [`../docs/architecture.md`](../docs/architecture.md) (The plug-in seams, Facts the design rests on), `docs/muse-code.md` and `docs/lambo.md` from P0, the trait and its conformance test from T1.3.

---

### T3.1: Muse Code adapter: locate and read ★
```yaml
requires:   [T1.3, T0.4]
fixture-ok: yes
size:       M · mid
owns:       src/adapters/muse_code/mod.rs, src/adapters/muse_code/locate.rs, src/adapters/muse_code/parse.rs
status:     done
```
Find a repository's memory scopes: the private per-project folder under `~/.local/share/muse/memory/projects/` (by the derivation T0.4 settled, or its recorded fallback) and the shared `<repo>/.agents/memory/`. Parse `MEMORY.md` and topic files: frontmatter, inline notes, index lines pointing at files. Each note becomes a `Memory` with a stable `MemoryRef` (scope, file, and the note's position or name) that survives an unrelated edit to the same file.

Tests run on copies of `testdata/memories/muse-code/` in temporary directories, never on the real memory folder.

**Done when:** the trait's conformance test passes for list and read; every note in the fixture is read with its frontmatter; a hand-edited `MEMORY.md` with an extra note keeps the other notes' references stable.

---

**Notes (T3.1)**
- Built: `src/adapters/muse_code/{mod,locate,parse}.rs`. `locate` computes the FNV-1a-64 hash of the canonicalised starting directory, matches the private per-project folder under `~/.local/share/muse/memory/projects/` on the hash suffix alone, and adds the shared `<repo>/.agents/memory/` folder. `parse` splits frontmatter (`type`/`description`), collects inline `- ` notes, and follows `[label](file.md)` index lines into topic files. Each note becomes a `Memory` with a stable id `{file}#{fnv1a64(text)}` that survives an unrelated edit to the same file. `MuseCodeAdapter` implements `name`/`capabilities`/`list`/`read`/`write` (write is a minimal in-memory stub; full file write/annotate/supersede/watch land in T3.2 and currently report `Unsupported`).
- Decided: capabilities report `NONE`, so annotate/supersede/watch return typed `Unsupported` and the shared conformance passes for list and read. `list("")` returns every note across all scopes; non-empty query is a case-insensitive substring filter. `new(path)` locates scopes and falls back to treating the path as a single memory folder, so tests can point it at a copied fixture. Frontmatter is parsed and attached per note but not surfaced on `Memory` (no field exists); it is available via `parse::Note`/`Frontmatter`.
- Surprised me: the slug cut at 96 characters makes the slug alone ambiguous, so matching on the hash suffix is required. Tests use nanosecond temp dir names to avoid parallel-test collisions.

### T3.2: Muse Code adapter: annotate and supersede ★
```yaml
requires:   [T3.1]
fixture-ok: yes
size:       S · mid
owns:       src/adapters/muse_code/write.rs
status:     done
```
Write a new note; annotate a note in place (`[STALE per Doyle: …]`, the form the recall test proved); supersede a note (the correction replaces it, with a one-line trace of what it replaced). Every write keeps the frontmatter valid, keeps the index in step, writes atomically (temporary file and rename), and respects Muse Code's `.muse-memory.lock`. Annotations are short and plain (README, guideline 7).

**Done when:** the full conformance test passes; a write interrupted between temp file and rename leaves the original intact; Doyle's writes stay small plain deltas by construction (short in-place annotations, one marker per note, corrections as single notes) — no index limit is enforced because T0.4 recorded none (clause amended at land time; the original text cited a limit that was never established); the handoff text includes a before-and-after of the fixture's stale note.

**T3.2b (live):** annotate a note in a scratch repository's real Muse Code memory folder, then ask a fresh `muse exec` session on `muse-wonder-1.0` the question the note answers. Owns `tests/live_muse_code.rs`. Budget: three sessions.

**Notes (T3.2)**
- Built: `src/adapters/muse_code/write.rs` with `write`, `annotate`, `supersede`, plus `set_rename_hook` test seam. `mod.rs` gains the approved `mod write;` line, flips capabilities to annotate+supersede (watch stays unsupported), and delegates the three methods; list/read apply a recorded-scope map so a write's scope round-trips. Atomic writes use a temp file + rename seam; `.muse-memory.lock` is respected by refusing to write while a live pid holds it.
- Annotate edits the note's bullet in place (`- text [STALE ...]`), so the ordinal ref is unchanged. Supersede creates the correction as a new bullet in `MEMORY.md`, appends `[Superseded by {id}]` to the stale bullet, and is a no-op on retry with identical text. Interrupted rename leaves the original file byte-identical (tested via seam).
- Decided: notes are appended inline to `MEMORY.md` (topic-file promotion only when needed). No index limit is enforced: T0.4 recorded none (it settled the folder hash, hook registration, and wire format), and the earlier "512-line snapshot" note in this diary was unsourced — withdrawn. What bounds Doyle's writes is construction, not enforcement: short in-place annotations, one marker per note, corrections as single notes. Scope is recorded in adapter state because Muse Code stores none on disk.
- Surprised me: `adapter.state` cannot be borrowed mutably and immutably at once (RefCell) — fixed by reading first. The `RENAME_HOOK` mutex needed poison-resistance across parallel tests. Conformance passed on first run after wiring.

**T3.2b notes (live):** `tests/live_muse_code.rs` (`live_muse_code_annotate_guides_fresh_session`, ignored, 2 sessions per run). Seed session saved "the office cactus is named Zebulon" with scope `personal_project` into the real per-project folder; the adapter annotated it in place (`MEMORY.md#1`); a fresh probe session ("What is the office cactus named?") answered "Saffron" with no mention of Zebulon — the correction name exists only in the annotation, so the answer proves the session followed it. Before/after of the note is in the test output. Decided: fixed distinctive fact (office cactus) instead of reusing the plant from the docs, so a concurrent session's memory cannot answer the probe; scope is named explicitly in the seed prompt because an unscoped prompt landed in the global `memory/personal/` folder in a manual probe (the per-project folder is what this task must exercise). Surprised me: the model echoed the scope back ("scope: `personal_project`") when instructed, and the seed's `.muse-memory.lock` names a dead pid, which the adapter's fail-open rule correctly ignores. Scratch repo and project folder are removed by a cleanup guard; the manual probe's `personal/` note was removed afterwards, leaving `memory/` as found.

---

### T3.3: Lambo adapter ★
```yaml
requires:   [T1.3, T0.5]
fixture-ok: yes
size:       M · mid
owns:       src/adapters/lambo.rs, src/mcp.rs, Cargo.toml (the ureq line), Cargo.lock
status:     done
```
A minimal MCP client over streamable HTTP in `src/mcp.rs` (initialize, call a tool, read the result), used by the adapter. The adapter lists and reads memories in a configured Lambo session, writes new ones, and corrects one by deriving the correction with `against` naming the stale memory (supersede). Annotate maps to the same contradiction, so recall warns. The agent id Doyle writes under is stable and configured, never per call.

Offline tests replay `testdata/lambo/` through a fake transport.

**Done when:** the conformance test passes against the recorded transcripts; an unreachable server fails with the URL and a clear message, not a hang.

**T3.3b (live):** against a running Lambo, in a scratch session: write a memory, correct it through the adapter, recall it, and see the warning. Additionally: exercise the supersede retry path (correct the same stale memory twice — the second must be a no-op returning the existing ref, with no duplicate derive), and probe the broad-watch `*` query (record what the server returns for `*` and whether it can serve as a match-all). Owns `tests/live_lambo.rs`.

**Notes (T3.3)**
- Built: `src/mcp.rs` minimal MCP client over streamable HTTP (ureq 3.5 with rustls, finite timeouts via `timeout_global`). `Transport` trait seam (`post`) plus `UreqTransport` and `FakeTransport` (queued/closure-scripted); `recorded_to_http` converts a recorded transcript JSON into an SSE `HttpResponse`. `McpClient` does initialize + notifications/initialized, session-id capture, and `tools/call` with SSE parsing and JSON-RPC error handling. `src/adapters/lambo.rs` `LamboAdapter` (name `"lambo"`, capabilities annotate false / supersede true / watch true): `list` recalls verbatim with `top_k` 50 (empty query → `Backend`); `read` inspects by id and appends the cached contradiction warning; `write` derives a concept and waits on the receipt via `lambo_stats` + `wait_ms`; `supersede` inspects first for an existing contradiction (retry no-op) and otherwise derives with `against` naming the stale concept's short id, then waits on the receipt; `annotate` returns `Unsupported`; `watch` uses the broad query `"*"` (named strategy `BROAD_WATCH_QUERY`) with locally-cached fallback and an in-memory seen set.
- Broad-watch query strategy named: `lambo_recall` with query `"*"` and `top_k` 50, node ids tracked in an in-memory `HashSet` so each concept reports once; recall failures are tolerated and locally cached writes are merged in, so a write Doyle made is never silently missed; caller baselines by discarding the first result. (Real adapters should persist this set; this one keeps it process-local, matching the fake's test-only behaviour.)
- ureq: requested "3.3"; cargo resolved to 3.5.0, which built cleanly on toolchain 1.99.1. No other dependency added.
- Adjudications (orchestrator rulings, landed with the code): annotate→Unsupported is the compliant reading — the frozen T1.3 trait governs over the loose "annotate maps to the same contradiction" phase prose; correction goes through supersede (derive with `against`). Restart loss is a known limitation: the supersede/scope/seen caches are process-local, so after a restart reads lose recorded warnings/scopes until the next in-process supersede. The broad-watch `*` match-all behavior is unrecorded (no transcript shows a server response to `*`); live verification is deferred to T3.3b as extended above. Read-warning pairing is batch-scoped (stale-side `Contradicts`-naming-id + same-batch `Contradicted`, unique-or-silent), never the first flat match — misattribution is a silent error, so ambiguity resolves to silence.
- Tests: `recorded_read_path` and `recorded_full_scenario` replay `testdata/lambo/` through `FakeTransport`; `unreachable_server_fails_with_url` hits a closed local port with a 200 ms timeout and asserts the URL appears in the error. Gate passes.
- Surprised me: recorded contradiction warnings use the 8-hex short id, not the full node id, so inspect-first retry resolves short → full via a second inspect before reading content. Also `lambo_inspect` carries no warnings; the contradiction text comes from recall, so read appends from the supersede cache instead.

**T3.3b notes (live):**
- Built: `tests/live_lambo.rs`, one `#[ignore]`d `live_*` test driving the real adapter against `http://127.0.0.1:7700/mcp/s/doyle-probe` with a pass-through counting transport (wraps `UreqTransport`, counts `lambo_derive` bodies) so the retry no-op can prove zero new derives. Unique per-run content (`t33b-<pid>-<nanos>` token) keeps the shared scratch session collision-free.
- Result: write returned a new node id; supersede derived the correction with `against` naming the stale short id and returned a different node id; recall returned both warnings (`⚠ Contradicted … (id <correction>)` on the stale memory, `⚠ Contradicts … (id <stale>)` on the correction); read of the stale id carries `⚠ Contradicted (id <correction full id>)`. Second supersede with identical text returned the existing correction ref with the derive count unchanged (2 → 2) — the in-process supersede-cache no-op, no duplicate derive.
- Broad-watch `*` verdict: the server answers `list("*")` with hits, not an error — the probe returned every concept in the session (both probe memories plus the earlier recorded pair), so `*` serves as a match-all on this server. `watch()`'s `BROAD_WATCH_QUERY` strategy is confirmed live.
- Surprised me: recall for the narrow token query returned all 4 session concepts, and the warnings array still carries the earlier recorded contradiction pair alongside the new one — warnings are session-global, not query-scoped, which is exactly why the adapter's batch-scoped pairing (never the first flat match) matters.

---

### T3.4: Muse Code adapter: topic files without an index ★
```yaml
requires:   [T3.1, T3.2]
fixture-ok: yes
size:       S · mid
owns:       src/adapters/muse_code/parse.rs, src/adapters/muse_code/write.rs, src/adapters/muse_code/mod.rs (tests, and wiring only if needed)
status:     done
```

Found by the T7.3 re-run: Wonder's seed session saved its note as a topic file (`old-records-service.md`, with `type` and `description` frontmatter) and wrote no `MEMORY.md`. `read_folder` returns nothing when `MEMORY.md` is absent, so `doyle curate` could not see the note. product.md names both layouts: a `MEMORY.md` index with short notes inline, or one file per topic.

Build:
- Read every top-level `*.md` topic file in the folder that carries frontmatter and is not reached from `MEMORY.md`, whether or not `MEMORY.md` exists. Notes reached through the index keep their current ids. A topic file's id is stable and derived from its file name, as indexed notes' ids derive from their source.
- Write-back for a topic-file note: annotate and supersede in place in that topic file, keeping its frontmatter and id, under the same lock and atomic-rename rules as today. Where the correction is written when there is no `MEMORY.md` is the task's call: follow what Muse Code itself would read (a new topic file with frontmatter, or a `MEMORY.md` index that links every existing topic file so none drops out of recall); say which and why.
- Ignore Muse's own files (`.muse-memory.lock`, dotfiles, temp files) and files without frontmatter.

**Done when:** the gate passes; tests cover a folder with only a topic file (read, annotate, supersede, retry no-op), a folder with `MEMORY.md` plus an unindexed topic file (both read, ids of indexed notes unchanged), and the existing fixtures unchanged in ids and behaviour.

**Notes (T3.4)**
- Built: `read_folder` now reads `MEMORY.md` (if present) and then every top-level `*.md` topic file with a complete frontmatter block that the index did not reach, sorted by name. A topic file with bullets keeps `<file>#<n>` bullet ids, the same as when indexed; a topic file with no bullets is one note, its whole body, id `<file>#0` (bullets count from 1, so `#0` cannot collide). The id depends on the file name only, so it is the same before and after an index links the file. Dotfiles (lock, Doyle's `.x.doyle-tmp`), non-`.md` names, directories, `MEMORY.md` itself and files without frontmatter are skipped. Existing fixture ids are unchanged (pinned by a test).
- Annotate and supersede on `#0` append the marker after the last body text (`mark_note`), leaving frontmatter and the rest of the file byte-identical, through the same lock check and temp-file rename. The three duplicated bullet loops in `write.rs` became one `mark_note`; bullet behaviour is unchanged.
- Decided, where a correction goes with no `MEMORY.md`: Doyle creates `MEMORY.md`, with an index line `- [stem](file.md) — description` for every existing topic file, then the correction as a bullet. Why: Muse Code's session snapshot injects `MEMORY.md` only (docs/muse-code.md 1.1: `MEMORY.md:` under the scope; topic files are reached through `read_memory`), and a stale topic file is not in the snapshot, so a correction in a new topic file would sit where a session does not look first. A correction in `MEMORY.md` is in the snapshot, and the links keep every topic file reachable. The same creation path serves `write` into a folder with no `MEMORY.md`. Not verified against a live Muse session; no live call was made.
- Lock finding: Muse writes `pid=<n>` and a newline, which is exactly what `check_lock` parses (checked on the real folders; all five lock files on this machine name a pid that is no longer running). A lock naming a dead pid does not block (fail-open), so Muse leaving the file behind after a session does not block write-back. Left unchanged. Residual risk: a reused pid belonging to an unrelated process would block until it exits.
- Surprised me: the index Doyle creates would make a bullet-less topic file vanish (an indexed file with no bullets yielded no notes), so bullet-less indexed topic files with frontmatter are now read as a whole-body note too.

---

### T3.5: MCP client: a new session when Lambo forgets the old one ★
```yaml
requires:   [T3.3]
fixture-ok: yes
size:       S · mid
owns:       src/mcp.rs
status:     done
```

Found by the T7.3 re-run: `doyle serve` initialized its Lambo MCP session at start, and about 25 minutes later, when the person approved, Lambo answered `404` for that `Mcp-Session-Id`. The client sets `initialized` once and never re-initializes, so both Lambo write-backs failed (`http status: 404`) while Muse Code memory was corrected. The MCP Streamable HTTP transport says a client that gets `404` for a request carrying a session id must start a new session with a fresh `initialize`.

Build: on a `404` to a request that carried a session id, drop the id, re-run `initialize` and `notifications/initialized`, and retry the request once. A second `404`, or a `404` with no session id, is an error as today. No change to any adapter.

**Done when:** the gate passes; tests with the fake transport show a call that gets `404` re-initializes and succeeds on the retry with the new session id, a repeated `404` errors after one retry, and the existing recordings replay unchanged.


**Notes (T3.5).** `McpClient::call_tool` now remembers whether its request carried a session id. On `404` with one, it clears the id and `initialized`, runs `initialize` and `notifications/initialized` (which capture the new id), and re-sends the same body once. Any other status, a `404` with no session id, and a second `404` return the error as before. No adapter changed. Four fake-transport tests in `src/mcp.rs` cover the retry, the repeated `404`, a `404` with no id, and a `500`. Decision: the retry reuses the original JSON-RPC id, since the first attempt got no result. Surprise: `src/mcp.rs` had no tests module of its own; the transport was only exercised through adapters.

---

### T3.6: Lambo adapter: a correction Lambo already holds ★
```yaml
requires:   [T3.3, T3.5]
fixture-ok: yes
size:       S · mid
owns:       src/adapters/lambo.rs (supersede and its tests), testdata/lambo/ (new recordings only, from the doyle-probe session)
status:     done
```

Found by the T7.3 re-run. After T6.4, the person retried both flags' Lambo write-backs. flag-2's landed: its correction is recorded `against` the stale concept and recall warns. flag-1's failed with `lambo_stats returned no created node id`, yet its correction text is in Lambo (`df979a15`) and is not recorded against the stale concept. Serve logged two `os error 11` request failures, so the retry was likely sent twice: the first derive created the node, the second was matched to it by Lambo's de-duplication and the receipt listed nothing as created. The adapter treats an empty `created` as failure, so every later retry fails the same way.

Build:
- When the derive's receipt creates nothing but matches an existing concept, find that concept's id (from the receipt or a recall of the correction text) and treat it as the correction.
- Make sure the correction ends up recorded `against` the stale concept. Whether a matched re-derive records the contradiction is Lambo's behaviour to probe, not assume: record it against the `doyle-probe` session only (docs/lambo.md), never `doyle`, `doyle-t73` or `lambo-dev`. If a matched derive does not record it, find the call that does.
- A retry when the correction is already linked stays a no-op.

**Done when:** the gate passes with no key and no server; replay tests from new `doyle-probe` recordings show a supersede whose derive matches an existing concept returns that concept's id and leaves the correction `against` the stale concept, and a repeated supersede is a no-op; existing recordings replay unchanged; the probe transcript is in the report.


**Notes (T3.6).** Probe on `doyle-probe` (recordings 16-31). A derive with `against` on a correction that already exists creates nothing and lists the existing concept in the receipt's `matched`. If that concept was not yet linked, the same receipt reads `1 contradiction(s) recorded`, so the contradiction is recorded by the matched re-derive itself. If it was already linked it reads `0 created, 1 matched existing` with no contradiction and no duplicate edge. So `supersede` now takes `created[0]`, else `matched[0]`, and no extra call is needed; the receipt names the id, so no recall fallback was built. Surprise: `lambo_inspect` on the stale concept carries no `Contradicted` line, only a `Contradicts` edge from the correction in `focus.levels`, so the old early check never fired live. It now reads that edge too (matching its label to the correction text), which makes a repeated supersede a true no-op with no derive. Errors and the other paths are unchanged. No README in `testdata/lambo/`, so the recordings are listed in the report only.
---

## Exit Criteria

- [x] Muse Code memory is read from both scopes with stable references (T3.1).
- [x] Muse Code notes are annotated and superseded safely, and a live Wonder session follows the annotation (T3.2, T3.2b). Live: fresh session answered the annotation's name, never the seeded one; scratch and global artifacts restored.
- [x] Lambo memories are read and corrected through MCP, and recall warns on the corrected memory (T3.3, T3.3b). Live confirmed on `doyle-probe`: retry is a no-op, `*` serves as match-all.
- [x] Both adapters pass the same conformance test, and nothing outside `src/adapters/` names either.
