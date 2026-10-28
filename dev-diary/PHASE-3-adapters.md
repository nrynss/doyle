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
status:     not-started
```
Find a repository's memory scopes: the private per-project folder under `~/.local/share/muse/memory/projects/` (by the derivation T0.4 settled, or its recorded fallback) and the shared `<repo>/.agents/memory/`. Parse `MEMORY.md` and topic files: frontmatter, inline notes, index lines pointing at files. Each note becomes a `Memory` with a stable `MemoryRef` (scope, file, and the note's position or name) that survives an unrelated edit to the same file.

Tests run on copies of `testdata/memories/muse-code/` in temporary directories, never on the real memory folder.

**Done when:** the trait's conformance test passes for list and read; every note in the fixture is read with its frontmatter; a hand-edited `MEMORY.md` with an extra note keeps the other notes' references stable.

---

### T3.2: Muse Code adapter: annotate and supersede ★
```yaml
requires:   [T3.1]
fixture-ok: yes
size:       S · mid
owns:       src/adapters/muse_code/write.rs
status:     not-started
```
Write a new note; annotate a note in place (`[STALE per Doyle: …]`, the form the recall test proved); supersede a note (the correction replaces it, with a one-line trace of what it replaced). Every write keeps the frontmatter valid, keeps the index in step, writes atomically (temporary file and rename), and respects Muse Code's `.muse-memory.lock`. Annotations are short and plain (README, guideline 7).

**Done when:** the full conformance test passes; a write interrupted between temp file and rename leaves the original intact; the Muse Code index limit (T0.4) is respected; the handoff text includes a before-and-after of the fixture's stale note.

**T3.2b (live):** annotate a note in a scratch repository's real Muse Code memory folder, then ask a fresh `muse exec` session on `muse-wonder-1.0` the question the note answers. Owns `tests/live_muse_code.rs`. Budget: three sessions.

---

### T3.3: Lambo adapter ★
```yaml
requires:   [T1.3, T0.5]
fixture-ok: yes
size:       M · mid
owns:       src/adapters/lambo.rs, src/mcp.rs, Cargo.toml (the ureq line), Cargo.lock
status:     not-started
```
A minimal MCP client over streamable HTTP in `src/mcp.rs` (initialize, call a tool, read the result), used by the adapter. The adapter lists and reads memories in a configured Lambo session, writes new ones, and corrects one by deriving the correction with `against` naming the stale memory (supersede). Annotate maps to the same contradiction, so recall warns. The agent id Doyle writes under is stable and configured, never per call.

Offline tests replay `testdata/lambo/` through a fake transport.

**Done when:** the conformance test passes against the recorded transcripts; an unreachable server fails with the URL and a clear message, not a hang.

**T3.3b (live):** against a running Lambo, in a scratch session: write a memory, correct it through the adapter, recall it, and see the warning. Owns `tests/live_lambo.rs`.

---

## Exit Criteria

- [ ] Muse Code memory is read from both scopes with stable references (T3.1).
- [ ] Muse Code notes are annotated and superseded safely, and a live Wonder session follows the annotation (T3.2, T3.2b).
- [ ] Lambo memories are read and corrected through MCP, and recall warns on the corrected memory (T3.3, T3.3b).
- [ ] Both adapters pass the same conformance test, and nothing outside `src/adapters/` names either.
