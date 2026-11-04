# Doyle: architecture

The product is in [`product.md`](../product.md). The work is in [`dev-diary/`](../dev-diary/README.md). This file says how Doyle is put together. Where it and product.md disagree, product.md wins.

## Shape

One Rust binary, `doyle`. One core that knows nothing about any particular memory system or model provider, and plug-ins on both sides of it.

```
            contracts (OpenAPI, protobuf)                 memory systems
                       │                                        │
                 ┌─────▼──────┐                        ┌────────▼────────┐
                 │  contract  │  Vimanam: entities     │    adapters     │  Muse Code built-in,
                 │            │  (ve2_ IDs) + changes  │ (MemoryAdapter) │  Lambo, next: Claude
                 └─────┬──────┘                        └───┬────────▲────┘  Code, Codex, mem0…
                       │ changes                     memories │        │ annotate / correct
                 ┌─────▼──────────────────────────────────────▼──┐     │
                 │                    core                       │     │
                 │  citation index · flagging · proposals ·      │─────┘
                 │  decisions · state store                      │
                 └──┬───────────────┬───────────────┬────────────┘
           curate   │      recheck  │               │ open flags
          ┌─────────▼──┐   ┌────────▼───┐   ┌───────▼───────┐   ┌─────────────┐
          │  curator   │   │  recheck   │   │    serve      │   │    guard    │
          │ (model)    │   │  (model)   │   │ review page,  │   │ Muse Code   │
          │ memory →   │   │ flag + new │   │ judges' page  │   │ hook: block │
          │ element IDs│   │ contract → │   │ approve /     │   │ actions on  │
          └────────────┘   │ proposal   │   │ retire / keep │   │ stale facts │
                           └────────────┘   └───────────────┘   └─────────────┘
```

## The core model

| Type | What it is |
|---|---|
| `ElementId` | A stable element ID from an evidence source: Vimanam's `ve2_…` (an operation, type, field, argument, result or enum value), legacy `le2_…`, or screens `se2_…`. Stable across versions and document order |
| `MemoryRef` | An adapter name plus that system's own memory ID. Doyle never copies memory text into its own store except as evidence on a flag |
| `Memory` | `MemoryRef`, text, last updated, scope. Read through an adapter |
| `Citation` | A memory rests on one or more elements: `MemoryRef`, `ElementId`s, the claim in one line, who recorded it (curator model or a person), confidence |
| `Flag` | A cited element changed: the memory, the change IDs (`vc2_…`, `lc2_…` or `sc2_…`), severity, the old and new versions |
| `Proposal` | For one flag: correct (with new text), retire, or keep; the rationale; the model that proposed it |
| `Decision` | A person's verdict on a proposal: approve, retire, keep, or edit then approve; who decided, and when |

Doyle's own state is the citation index plus the flag, proposal and decision log. It lives in one directory (`.doyle/` by default): JSON Lines, append-only, rebuilt into memory on start. There's no database. The volumes are small, and a log is the evidence trail the judges' page shows.

## Flagging

A change touches more than its subject. When Vimanam reports a change to entity E, Doyle flags every memory citing:

1. E itself;
2. any ancestor of E (a field change touches its type and the operations returning it);
3. any entity that references E's named type (shared `$ref`s).

Additive, non-breaking changes flag nothing. `Review` severity (Vimanam's inferred cases, such as pagination) flags, marked as needing review.

## The plug-in seams

- **`MemoryAdapter`** (trait). Required: `list(query)` (search, not enumerate), `read`, `write` (fresh memories only). Optional, through capability flags: `annotate` (put a warning on the memory so it shows on recall), `supersede` (create the correction and mark the stale memory replaced by it), `watch` (notice new memories; the first call reports everything visible and the caller baselines). The core calls only the trait. Lambo has no special paths.
- **`ModelClient`** (trait). One call: a prompt and a JSON schema in, structured JSON out. Implementations: the Meta Model API (Muse Spark, the re-check), Baseten (Muse Spark Open, the curator), and a replay client that serves recorded responses, for tests and an offline demo.
- **Contract source.** Vimanam's `contract` module, used as a library with default features off. OpenAPI first; protobuf comes free with Vimanam 2.0.

## Evidence sources (P9, built)

A memory rests on evidence, and an API contract is one kind. PHASE-9 puts the contract source behind an **evidence source** seam (T9.5): index a version into elements (ID, kind, parent, references, native location), and diff two versions into change facts (ID, subject, kind, severity). Flagging, the guard and the review page read only the seam.

| Source | Elements | Change facts from | ID prefix |
|---|---|---|---|
| **Vimanam** (built) | operations, types, fields, results | Vimanam's diff | `ve2_` / `vc2_`, unchanged |
| **Legacy** (T9.6) | COBOL programs, paragraphs, copybook records and fields, files | Doyle's diff of two parsed versions | `le2_` / `lc2_` (`docs/legacy-ids.md`) |
| **Screens** (T9.7) | screens, elements, steps with time spans | Doyle's diff of two element lists that `muse-wonder-1.0` read, one call per version | `se2_` / `sc2_` |
| **Differential run** (T9.8) | the legacy source's paragraphs | mismatched outputs between the COBOL batch and a rewrite, traced to the paragraphs that ran | the legacy source's |

The model reads pixels once per version. Everything after that, the diff and the flag, is computed. Doyle never writes the rewrite it checks: Muse Wonder does.

## Commands

| Command | Does |
|---|---|
| `doyle curate` | Read memories through the adapters, have the curator cite each against the current contract, write citations |
| `doyle cite` | Record a citation by hand (for tests and for memories the curator misses) |
| `doyle check OLD NEW` | Diff two contract versions, flag cited memories, run the re-check, write proposals. The exit code says whether anything was flagged, for CI |
| `doyle diffrun PROGRAM RUN` | Read a differential run (the COBOL batch against a rewrite, from a manifest), trace each mismatched output to the paragraphs that ran, and flag memories resting on them |
| `doyle screens read` | Have `muse-wonder-1.0` read console screens into an element list with stable IDs, the input to a screens check |
| `doyle serve` | The review page and the judges' page. A decision writes the correction back through the adapters |
| `doyle guard` | The Muse Code hook. `PreToolUse`: read the hook JSON on stdin, deny an action that touches an element with an unsettled flag, with the reason. `SessionStart`: inject a short notice of open flags |

The guard matches an action against the native locations of flagged elements (method and path, operation ID, type name) found in the tool input. It's deterministic: no model call on the hot path.

## Dependencies

Each enters in the commit that first needs it, with its reason.

| Crate | Why |
|---|---|
| `vimanam` 2.x, no default features | The contract model, stable IDs and diff |
| `serde`, `serde_json` | The state log, hook JSON, model I/O. Already in Vimanam's tree |
| `ureq` (rustls) | HTTPS to the Meta Model API and Baseten, and HTTP to Lambo's MCP endpoint. Blocking keeps the binary simple; nothing needs async |
| None: a small HTTP server written over `std::net::TcpListener` | `doyle serve`, bound to 127.0.0.1 (`src/serve/mod.rs`) |

Keys come from the environment or the keyring, never a file (see AGENTS.md).

## Facts the design rests on

- **Muse Code built-in memory** (verified on Muse Code 1.4.14 with `muse-wonder-1.0`): Wonder recalls only what it wrote to its memory file. The default scope is `~/.local/share/muse/memory/projects/<path-slug>-<16-hex hash>/MEMORY.md`, and the shared one is `<repo>/.agents/memory/`. Notes carry `type` and `description` frontmatter and no source. A `[STALE … now X]` annotation changes Wonder's answer in a fresh session.
- **Lambo** is reached over MCP (streamable HTTP, `http://127.0.0.1:7700/mcp` locally). A correction is a derive with `against` naming the stale memory, so recall warns on the memory itself.
- **Vimanam 2.0** gives `contract::openapi::parse` → `Contract` (entities with `ve2_` IDs, parent links, references) and change facts with a `subject` entity ID and severity, including inferred pagination changes.
