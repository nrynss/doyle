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

## Exit Criteria

- [x] Muse Code memory is read from both scopes with stable references (T3.1).
- [x] Muse Code notes are annotated and superseded safely, and a live Wonder session follows the annotation (T3.2, T3.2b). Live: fresh session answered the annotation's name, never the seeded one; scratch and global artifacts restored.
- [x] Lambo memories are read and corrected through MCP, and recall warns on the corrected memory (T3.3, T3.3b). Live confirmed on `doyle-probe`: retry is a no-op, `*` serves as match-all.
- [x] Both adapters pass the same conformance test, and nothing outside `src/adapters/` names either.
