# P1: Contracts

The types and the two traits every other task codes against, the state store, and the command dispatch. These freeze at the end of P1: after that, changing one is the orchestrator's call and updates `testdata/` in the same commit.

P1 runs in order, one task at a time. It is the serial bottleneck: keep it small and exact.

Read first: [`../docs/architecture.md`](../docs/architecture.md) (The core model, The plug-in seams, Commands), [`PHASE-0-ground.md`](PHASE-0-ground.md) and what P0 recorded in `testdata/` and `docs/`.

---

### T1.1: Core model ★
```yaml
requires:   [T0.3, T0.6]
fixture-ok: yes
size:       M · frontier
owns:       src/core/mod.rs, src/core/model.rs, Cargo.toml, Cargo.lock
status:     not-started
```
The types in architecture.md's core model table: `ElementId`, `MemoryRef`, `Memory`, `Citation`, `Flag`, `Proposal`, `Decision`, with serde round-trips. `ElementId` and the change IDs wrap Vimanam's strings without re-deriving them. `MemoryRef` names its adapter by a string the adapter declares, never an enum of known systems, so a new adapter needs no change here.

Adds `serde` and `serde_json`; the commit message says why.

**Done when:** every type round-trips through JSON in a test; the demo's three memories and their expected citations from `testdata/README.md` can be written as values of these types in a test; nothing in `src/core/` names Muse Code or Lambo.

---

### T1.2: State store ★
```yaml
requires:   [T1.1]
fixture-ok: yes
size:       S · mid
owns:       src/core/store.rs
status:     not-started
```
Doyle's own state in one directory (`.doyle/` by default, overridable): JSON Lines, append-only, one file per record kind (citations, flags, proposals, decisions), loaded into memory on open. Queries the later tasks need: citations by element, citations by memory, open flags (no decision yet), the decision for a proposal. A corrupt last line (a crash mid-write) is skipped with a warning, never a failure.

**Done when:** tests cover append and reload, each query, a truncated last line, and two stores in two directories not seeing each other; `.doyle/` is in `.gitignore`.

---

### T1.3: Memory adapter trait ★
```yaml
requires:   [T1.1, T0.4, T0.5]
fixture-ok: yes
size:       S · frontier
owns:       src/adapters/mod.rs, src/adapters/fake.rs
status:     not-started
```
The `MemoryAdapter` trait (architecture.md, The plug-in seams). Required: `name`, `list`, `read`, `write`. Optional, declared through a capabilities value: `annotate`, `supersede`, `watch`; calling one an adapter lacks returns a typed "unsupported", never a panic. Errors say which adapter and which memory.

Design it against both real systems before either adapter exists: the Muse Code note format (T0.4, T0.3) and Lambo's calls (T0.5). Then write an in-memory fake that implements every capability, for the core's tests.

**Done when:** the fake passes a shared conformance test (list, read, write, annotate, supersede) that P3's adapters will run too; the trait's docs say, for each method, what Muse Code and Lambo will do with it.

---

### T1.4: Model client trait ★
```yaml
requires:   [T1.1]
fixture-ok: yes
size:       S · mid
owns:       src/models/mod.rs, src/models/replay.rs
status:     not-started
```
The `ModelClient` trait: a request (model id, system prompt, user content, a JSON schema for the reply, a budget) in, parsed JSON plus usage (tokens, cost if known) out. A replay client keyed by a hash of the request serves recorded replies from a directory and fails loudly on a miss. A recording mode wraps any client and writes what it gets.

**Done when:** the replay client serves a hand-written recording in a test, a miss fails with the request's key in the message, and recording then replaying gives the same reply.

---

### T1.5: Command dispatch ★
```yaml
requires:   [T1.1]
fixture-ok: yes
size:       S · mid
owns:       src/main.rs, src/lib.rs, src/cli.rs, the module declarations in src/core/mod.rs, src/adapters/mod.rs and src/models/mod.rs, and an empty stub of every module file named in P2–P6
status:     not-started
```
Split the crate into a library (`src/lib.rs`, declaring every module) and a thin binary (`src/main.rs`), so integration and live tests under `tests/` can reach the code. `doyle curate | cite | check | serve | guard` parsed in `src/cli.rs` and dispatched from `src/main.rs`, each to a stub function in its own module that returns "not implemented yet" with exit code 2. Shared options: the state directory, the adapters to use, a verbosity flag. Help text matches architecture.md's command table.

Also declare every module the later phases fill, each as a stub file with a one-line doc comment: `src/contract.rs`, `src/core/flag.rs`, `src/core/decide.rs`, `src/check.rs`, `src/cite.rs`, `src/curate.rs`, `src/adapters/muse_code/mod.rs`, `src/adapters/lambo.rs`, `src/mcp.rs`, `src/models/meta.rs`, `src/models/baseten.rs`, `src/prompts/mod.rs`, `src/serve/mod.rs`, `src/guard.rs`.

After this task no one edits `src/main.rs`, `src/cli.rs` or a `mod.rs` declaration list except with the orchestrator's approval; each later task owns its stub file and fills it.

**Done when:** `doyle --help` and each `doyle <command> --help` print; each stub exits 2; every module listed above exists and is declared; the gate passes.

---

## Exit Criteria

- [ ] The core types round-trip, and the demo's memories and citations are expressible in them (T1.1).
- [ ] The store reloads, answers its queries, and survives a truncated line (T1.2).
- [ ] The adapter trait has a conformance test that the fake passes (T1.3).
- [ ] The model client replays and records (T1.4).
- [ ] Every command dispatches to its own module (T1.5).
- [ ] Contracts frozen: the orchestrator records it in the handoff log.
