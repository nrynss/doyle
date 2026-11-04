---
title: Memory adapters
description: How Doyle reads, annotates, corrects, and retires memories in Muse Code and Lambo, and how to add a new adapter.
---

Doyle uses memory adapters to decouple its core verification engine from external memory implementations.
The core interacts only with the [`MemoryAdapter`](https://github.com/nrynss/doyle/blob/main/src/adapters/mod.rs) trait.

## The `MemoryAdapter` trait

The adapter trait defines how Doyle inspects, updates, and observes agent memories:

- `name(&self) -> &str`: returns the unique system identifier stored on citation references.
- `capabilities(&self) -> Capabilities`: declares support for optional operations such as `annotate`, `supersede`, and `watch`.
- `list(&self, query: &str) -> Result<Vec<Memory>, AdapterError>`: searches memories matching a text query.
- `read(&self, id: &str) -> Result<Memory, AdapterError>`: retrieves a single memory by its identifier.
- `write(&self, text: &str, scope: &str) -> Result<MemoryRef, AdapterError>`: records a new memory in the specified scope.
- `annotate(&self, id: &str, warning: &str) -> Result<(), AdapterError>`: writes an in-place warning without creating a replacement record.
- `supersede(&self, stale_id: &str, correction_text: &str) -> Result<MemoryRef, AdapterError>`: creates a correction and marks the original memory superseded.
- `watch(&self) -> Result<Vec<Memory>, AdapterError>`: returns memories created since the previous poll.

Adapters run synchronously.
They use internal mutability to manage caches and cursors.

## Muse Code adapter

The Muse Code adapter lives in [`src/adapters/muse_code/mod.rs`](https://github.com/nrynss/doyle/blob/main/src/adapters/muse_code/mod.rs).
It interfaces directly with the file-based memory system built into Muse Code.
Full integration findings are documented in [`docs/muse-code.md`](https://github.com/nrynss/doyle/blob/main/docs/muse-code.md).

### Scopes and storage paths

Muse Code stores memories in two distinct markdown scopes:

1. Private project scope: located under `$HOME/.local/share/muse/memory/projects/<path-slug>-<16-hex hash>/MEMORY.md`. The folder name suffix uses a 64-bit FNV-1a hash of the physical directory path.
2. Shared repository scope: stored inside `<project>/.agents/memory/MEMORY.md`. This file is committed to version control and shared across agent sessions.

The adapter also indexes standalone topic files ending in `.md` inside those folders.
This unindexed scanning was introduced after run T3.4, when Muse Wonder saved its belief into `old-records-service.md` without adding an index entry in `MEMORY.md`.

### Addressing and in-place updates

The adapter splits markdown files by top-level bullet items.
Each item receives a synthetic identifier matching `<filename>#<section-index>`.

When human reviewers approve decisions, the adapter rewrites files using atomic renames:

- `annotate`: appends a warning tag like `[STALE per Doyle: ...]` directly to the bullet text.
- `supersede`: marks the old item as `[Superseded by MEMORY.md#N]` and appends the replacement belief to `MEMORY.md`.
- `retire`: marks the target item with `[RETIRED ...]`, preventing future runs from adopting it.

Fresh Muse Code sessions read these annotations on startup.
The annotations prevent the model from repeating discredited facts.

## Lambo adapter

The Lambo adapter lives in [`src/adapters/lambo.rs`](https://github.com/nrynss/doyle/blob/main/src/adapters/lambo.rs).
Lambo provides graph-based agent memory over Model Context Protocol (MCP).
Detailed wire logs and schema analysis appear in [`docs/lambo.md`](https://github.com/nrynss/doyle/blob/main/docs/lambo.md).

### Client transport

Doyle never links Lambo as a static library.
Instead, it communicates over streamable HTTP MCP using `ureq`.
The endpoint defaults to `http://127.0.0.1:7700/mcp/s/doyle` and can be overridden via `LAMBO_URL`.

All non-trivial responses use Server-Sent Events (`text/event-stream`).
Doyle parses incoming SSE lines and extracts standard JSON-RPC payloads.

### Inspection and derive workflows

Lambo methods map to MCP tools:

- `list`: calls `lambo_recall` or `lambo_inspect` to retrieve candidate concepts matching the target query.
- `write`: executes `lambo_derive` without dependencies, then polls `lambo_stats` with `wait_ms` until the background receipt applies.
- `supersede`: calls `lambo_derive` with `against: [<stale_node_id>]`. This operation creates a first-class `Contradicts` relationship in the Lambo graph.

Subsequent `lambo_recall` queries automatically output staleness warnings on the contradicted node:

```text
⚠ Contradicted by doyle: "New pagination and cursor rules apply"
```

The coding agent encounters this warning during recall, preventing reliance on obsolete graph nodes.

## Writing a new adapter

To support a third-party memory system, follow these steps:

1. Create a module under `src/adapters/`.
2. Implement the `MemoryAdapter` trait. Provide required methods `name`, `capabilities`, `list`, `read`, and `write`.
3. If the backend supports linking or warnings, implement `annotate` or `supersede`.
4. Register the adapter name in the CLI adapter list parser.
5. Add unit and conformance tests using `check_conformance` from [`src/adapters/mod.rs`](https://github.com/nrynss/doyle/blob/main/src/adapters/mod.rs).

The core engine requires no modifications when adding new memory integrations.
