# P1: Contracts

The types and the two traits every other task codes against, the state store, and the command dispatch. These freeze at the end of P1: after that, changing one is the orchestrator's call and updates `testdata/` in the same commit.

P1 runs in order, one task at a time. It is the serial bottleneck: keep it small and exact.

**Module declarations before T1.5 (orchestrator decision):** until T1.5 makes the crate a library, a P1 task may add the one declaration line its own module needs (`mod core;` in `src/main.rs`, `mod store;` in `src/core/mod.rs`, `mod adapters;` or `mod models;` in `src/main.rs`), with a scoped `#[allow(dead_code)]` if the gate needs it. T1.5 moves every such line into `src/lib.rs` and drops the allows.

Read first: [`../docs/architecture.md`](../docs/architecture.md) (The core model, The plug-in seams, Commands), [`PHASE-0-ground.md`](PHASE-0-ground.md) and what P0 recorded in `testdata/` and `docs/`.

---

### T1.1: Core model ★
```yaml
requires:   [T0.3, T0.6]
fixture-ok: yes
size:       M · frontier
owns:       src/core/mod.rs, src/core/model.rs, Cargo.toml, Cargo.lock
status:     done
```
The types in architecture.md's core model table: `ElementId`, `MemoryRef`, `Memory`, `Citation`, `Flag`, `Proposal`, `Decision`, with serde round-trips. `ElementId` and the change IDs wrap Vimanam's strings without re-deriving them. `MemoryRef` names its adapter by a string the adapter declares, never an enum of known systems, so a new adapter needs no change here.

Adds `serde` and `serde_json`; the commit message says why.

**Done when:** every type round-trips through JSON in a test; the demo's four memories and their expected citations from `testdata/README.md` can be written as values of these types in a test; nothing in `src/core/` names Muse Code or Lambo.

**Notes (T1.1)**
- Built: `src/core/model.rs` with the seven contract types plus `ChangeId` (the task's "the change IDs wrap Vimanam's strings" needs a home). `ElementId`/`ChangeId` are transparent `String` wrappers holding `ve2_`/`vc2_` IDs verbatim. `MemoryRef { adapter, id }` keeps the adapter as a plain string. `Memory { reference, text, updated_at: Option<String>, scope }`. `Citation { memory, elements, claim, recorded_by, confidence: f32 }`. `Severity { Breaking, Review }`: non-breaking is unrepresentable, so additive changes cannot flag. `Flag { id, memory, changes, severity, old_version, new_version, evidence }`. `Proposal { id, flag_id, action, rationale, proposed_by }` with `ProposalAction { Correct { new_text }, Retire, Keep }`. `Decision { id, proposal_id, verdict, decided_by, decided_at }` with `DecisionVerdict { Approve, Retire, Keep, EditThenApprove { new_text } }`. `src/core/mod.rs` re-exports them. 12 unit tests: a JSON round-trip per type, exact-shape checks, the severity vocabulary (including that `"non_breaking"` is rejected), and the demo's four memories with their expected citations built from the real `ve2_` IDs, closing the loop with M1's flag (3 changes), proposal and decision.
- Decided: the ids and links the architecture table implies but does not spell out. `Flag.id`, `Proposal.flag_id` and `Decision.proposal_id` (plus `Proposal.id`, `Decision.id`), store-assigned, so T1.2 can answer "open flags (no decision yet)". `Flag.severity` is the highest severity among its changes. `updated_at` is `Option` because Muse Code notes carry no timestamp. Timestamps are RFC 3339 strings, keeping a time crate out. The demo test uses `"muse-code"`/`"lambo"` string literals as adapter-name data, proving the demo's refs fit `MemoryRef`; no identifier, enum, module or import in `src/core/` names either system.
- Outside `owns`, one line: `mod core;` in `src/main.rs` (plus a scoped `#[allow(dead_code)]`), without which the new modules never compile and their tests never run. T1.5 relocates it to `lib.rs` and drops both allows (the twin is on the re-export in `src/core/mod.rs`).
- Heads-up for T1.2: it owns `src/core/store.rs` but not the declaration list in `src/core/mod.rs`, which T1.5 owns; the store needs a `mod store;` line from someone.
- Discrepancy: the task says "the demo's three memories" but `testdata/README.md` has four (M1-M4) since the Quiet Migration rebuild; the test covers all four.
- Surprised me: `serde_json::to_value` takes its argument by value, so clippy flags `to_value(&temporary)` as a needless borrow.

---

### T1.2: State store ★
```yaml
requires:   [T1.1]
fixture-ok: yes
size:       S · mid
owns:       src/core/store.rs
status:     done
```
Doyle's own state in one directory (`.doyle/` by default, overridable): JSON Lines, append-only, one file per record kind (citations, flags, proposals, decisions), loaded into memory on open. Queries the later tasks need: citations by element, citations by memory, open flags (no decision yet), the decision for a proposal. A corrupt last line (a crash mid-write) is skipped with a warning, never a failure.

**Done when:** tests cover append and reload, each query, a truncated last line, and two stores in two directories not seeing each other; `.doyle/` is in `.gitignore`.

**Notes (T1.2)**
- Built: `src/core/store.rs` with `Store`: `open(dir)` creates/loads a directory of JSON Lines files (`citations.jsonl`, `flags.jsonl`, `proposals.jsonl`, `decisions.jsonl`), append-only writes with newline-terminated lines, rebuilt into memory on open. Store assigns ids to flags, proposals and decisions (`flag-N`, `proposal-N`, `decision-N`, derived from existing records on reload). Queries: `citations()`/`citations_for_element()`/`citations_for_memory()`, `open_flags()` (flags with no decision behind any of their proposals), `decision_for_proposal()`. A line that fails to parse (including a truncated last line from a crash mid-write) is skipped with a warning on stderr; open never fails on it. `open_default()`/`DEFAULT_DIR` cover the `.doyle` default; the path is otherwise overridable via `open`.
- Decided: filenames use the `.jsonl` extension; record methods are named `record_*` with `add/append/push/insert/create` aliases; queries offer `for`/`by`/`find` aliases; single-record getters return owned values (cloned) for easy assertion, with `_ref` variants for borrows. `StoreError` is a small enum over io/json with `Display`/`Error` impls, no new dependency. `open` creates the directory if missing.
- Outside `owns`, one line: `mod store;` in `src/core/mod.rs`. No change to `Cargo.toml`/`Cargo.lock`. `.gitignore` was not edited per the ownership rule; the exact line to land is `.doyle/` (see report).
- Surprised me: clippy's `collapsible_if` rejected the nested parse checks in `next_id`, fixed with a let-chain. rustfmt wanted the `load_jsonl` call on one line.

---

### T1.3: Memory adapter trait ★
```yaml
requires:   [T1.1, T0.4, T0.5]
fixture-ok: yes
size:       S · frontier
owns:       src/adapters/mod.rs, src/adapters/fake.rs
status:     done
```
The `MemoryAdapter` trait (architecture.md, The plug-in seams). Required: `name`, `list`, `read`, `write`. Optional, declared through a capabilities value: `annotate`, `supersede`, `watch`; calling one an adapter lacks returns a typed "unsupported", never a panic. Errors say which adapter and which memory.

Design it against both real systems before either adapter exists: the Muse Code note format (T0.4, T0.3) and Lambo's calls (T0.5). Then write an in-memory fake that implements every capability, for the core's tests.

**Done when:** the fake passes a shared conformance test (list, read, write, annotate, supersede) that P3's adapters will run too; the trait's docs say, for each method, what Muse Code and Lambo will do with it.

**Notes (T1.3)**
- Built: `src/adapters/mod.rs` holds `MemoryAdapter` (required `name`/`capabilities`/`list(query)`/`read`/`write`; optional `annotate`/`supersede`/`watch` with default bodies returning `AdapterError::Unsupported`), `Capabilities { annotate, supersede, watch }` with `NONE`/`ALL` and `supports(op)`, `AdapterError { NotFound, Unsupported, Backend }` whose `Display` names the adapter and, where one is involved, the memory. `src/adapters/fake.rs` holds `FakeAdapter` (in-memory, every capability, deterministic `mem-N` IDs, `with_capabilities` for a reduced set). 12 tests: the shared `check_conformance` run against the full and the reduced fake (list, read, write, annotate, supersede, watch report-once), a no-op-supersede catcher, plus fake-specific visibility, retry, filter and error-message tests.
- Decided, after adversarial review changed the contract: `list(query)` is a search, not an enumeration — Lambo has no list-all primitive, so an enumerate method would be unsatisfiable; empty query means no filter for enumerable adapters and `Backend` for recall-backed ones, with an incompleteness contract on recall results. `write` is fresh-memories-only and the adapter mints the ID; `supersede(stale_id, correction_text)` is the sole creator of corrections (exactly one per call; retry with identical text is a no-op returning the existing ref), because Lambo can only link via `against`-at-derive and a write-then-link flow would double-write or orphan. `watch` reports everything visible on the first call — the adapter never silently baselines; the caller baselines by discarding, and real adapters persist the cursor (Muse: path set; Lambo: node-ID set). The conformance runner is capability-aware: it exercises an optional op when claimed and asserts the typed `Unsupported` when not. The Lambo `supersede` retry path (inspect-first/derive-nothing) is explicitly unverified and deferred to live verification in T3.3b, not assumed. Each trait method's docs say what the Muse Code adapter (note files, `[STALE ...]` append, folder rescan) and the Lambo adapter (`lambo_recall`/`lambo_derive`+receipt wait/`against`, recall polling) will do, grounded in `docs/muse-code.md` and `docs/lambo.md`.
- Outside `owns`, one line: `mod adapters;` in `src/main.rs`, first to keep the declaration list alphabetical (`adapters` sorts before `contract`), with a scoped `#[allow(dead_code)]` and a scoped `#[allow(unused_imports)]` on the `FakeAdapter` re-export, mirroring the T1.1/T2.1 precedent; T1.5 relocates both. No change to `Cargo.toml`/`Cargo.lock`: std only (`RefCell`, `HashMap`), no new dependency.
- Surprised me: clippy's `collapsible_if` rejects the nested borrow-safe form of the `watch` drain and wants the let-chain back once the order list is cloned; and a `pub use` in a private binary module trips `unused_imports`, not `dead_code`, so it needs its own scoped allow.

---

### T1.4: Model client trait ★
```yaml
requires:   [T1.1]
fixture-ok: yes
size:       S · mid
owns:       src/models/mod.rs, src/models/replay.rs
status:     done
```
The `ModelClient` trait: a request (model id, system prompt, user content, a JSON schema for the reply, a budget) in, parsed JSON plus usage (tokens, cost if known) out. A replay client keyed by a hash of the request serves recorded replies from a directory and fails loudly on a miss. A recording mode wraps any client and writes what it gets.

**Done when:** the replay client serves a hand-written recording in a test, a miss fails with the request's key in the message, and recording then replaying gives the same reply.

**Notes (T1.4)**
- Built: `src/models/mod.rs` holds `ModelClient` (one method, `complete`, with a `call` alias), `ModelRequest`, `Budget`, `Usage`, `ModelResponse`, `ModelError` (with `ReplayMiss { key }`, `Backend`, `InvalidRequest`), `request_key`, and a hand-rolled 64-bit FNV-1a hasher. `src/models/replay.rs` holds `ReplayClient` (reads `<dir>/<key>.json`, `ReplayMiss` on a missing file with the key in the message) and `RecordingClient<C>` (wraps any client, writes each reply to `<dir>/<key>.json`). 16 tests in `replay.rs`: golden key value, hand-written recording served, miss carries the key, record-then-replay equality, nested-dir creation, schema key-order invariance, per-field key sensitivity (model, system, user, schema, both budget dimensions including `None`-vs-`Some(0.0)`).
- Decided: hashing uses a hand-rolled 64-bit FNV-1a hasher (std only, no dependency) over the model id, system prompt, user content, the recursively key-sorted canonical JSON rendering of the schema, and the budget (max_output_tokens as `Option<u32>`; max_cost_usd as a discriminant tag plus `f64::to_bits`, so `None` and `Some(0.0)` cannot collide). FNV-1a is stable across toolchains and Rust versions; the offset basis, prime and field sequence are a versioned contract of this crate, so recordings survive a toolchain bump. (The earlier note's "SipHash-based, seeded per process" description of `DefaultHasher` was wrong and is withdrawn.) No new dependency. Recordings are serialized `ModelResponse` (`value` + `usage`), so a hand-written one needs both fields. `Budget` is a struct with `max_output_tokens`/`max_cost_usd` plus `UNLIMITED` and `tokens(n)`; `Usage::NONE` and `ModelError::backend` are convenience helpers. The `mod models;` line was added to `src/main.rs` (scoped `#[allow(dead_code)]`), mirroring the T1.1/T1.3 precedent; T1.5 relocates it.
- Outside `owns`: only that single declaration line in `src/main.rs`. No change to `Cargo.toml`/`Cargo.lock`.
- Gate: `./tools/check.sh` passed (rustfmt, clippy `-D warnings`, 69 tests including the 16 new).
- Surprised me: `pub use replay::{...}` of items defined in `mod.rs` itself is not allowed (E0603), so the re-export lists only `RecordingClient` and `ReplayClient`; the rest are native to `mod.rs`. Also `DefaultHasher::finish` needs `Hasher` in scope.

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

- [x] The core types round-trip, and the demo's memories and citations are expressible in them (T1.1).
- [ ] The store reloads, answers its queries, and survives a truncated line (T1.2).
- [ ] The adapter trait has a conformance test that the fake passes (T1.3).
- [ ] The model client replays and records (T1.4).
- [ ] Every command dispatches to its own module (T1.5).
- [ ] Contracts frozen: the orchestrator records it in the handoff log.
