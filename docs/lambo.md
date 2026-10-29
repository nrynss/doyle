# Lambo as a client: the calls Doyle needs

Status: the read half is recorded; the write half is blocked. The running Lambo server (version 0.8.0) serves only its pinned session, `lambo-dev`, and the docs show no way to address a scratch session on it without changing its configuration and restarting it. No write was sent, so no memory was written to any session. The recorded transcripts are in `testdata/lambo/`.

Sources: Lambo's `docs/reference/mcp.mdx`, `config.mdx` and `cli.mdx`. The shapes below marked **recorded** come from the server; those marked **docs** come from the reference and have not been checked against the running server.

## Transport

- Streamable HTTP at `http://127.0.0.1:7700/mcp`. The default session is `lambo-dev`.
- **Recorded:** every response that carries a body is `text/event-stream`, even `tools/list`. The body is one SSE event, `data: <JSON-RPC message>`, preceded by an `id: 0` and `retry: 3000` line. Doyle's client must parse SSE, reading the `data:` lines and decoding each as JSON-RPC.
- **Recorded:** a notification (`notifications/initialized`) answers `202 Accepted` with no body.
- **Recorded:** `initialize` returns an `Mcp-Session-Id` response header. The recorded transcripts replace its value with `<mcp-session-id>`. Send it back on every later request.
- A plain GET to `/mcp` returns 406, as the task notes say.

## Calls

Each call is an HTTP POST with `Content-Type: application/json`, `Accept: application/json, text/event-stream`, and `Mcp-Session-Id` after initialize.

### 1. `initialize` (recorded: `01-initialize.*`)

Request: `{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"doyle-probe","version":"0.0.1"}}}`

Response: `result.serverInfo` is `{"name":"lambo","version":"0.8.0"}`, `result.capabilities` is `{"tools":{}}`, and `result.instructions` names the session (`Lambo agentic graph memory for session 'lambo-dev'. ...`). The instructions text is what a model sees, so Doyle should not depend on its wording.

### 2. `notifications/initialized` (recorded: `02-notifications-initialized.*`)

Request: `{"jsonrpc":"2.0","method":"notifications/initialized"}`, with the session header. Response: 202, no body.

### 3. `tools/list` (recorded: `03-tools-list.*`)

Returns eight tools. Required arguments, from the published schemas:

| Tool | Required | Use |
|---|---|---|
| `lambo_recall` | `agent_id` | Recall; `query` is required in practice unless `image` or `query_vector` is sent (the schema does not express this) |
| `lambo_derive` | `agent_id`, `concepts` | Write concepts. Optional `against`, `depends_on`, `parent_of`, `event_time`, `scope`, `repo_scope` on each concept or call |
| `lambo_record_action` | `agent_id`, `action` | Record an activity, not a fact |
| `lambo_inspect` | `agent_id`, `focus` | One concept's neighbourhood |
| `lambo_stats` | `agent_id` | Session health; `receipt` and `wait_ms` ask about one write |
| `lambo_stale` | `agent_id` | Direct dependents of contradicted concepts |
| `lambo_saints` | `agent_id` | Canonical concepts |
| `lambo_reserve` | `agent_id`, `node_id` | Soft lock on a concept |

Every tool rejects unknown arguments. `concept_type` values are lowercase on the wire: `entity`, `logic`, `constraint`, `resource`, `observation` (**docs**; the schema confirms it).

### 4. `lambo_derive`: write a memory (docs; not yet recorded)

Request:
```json
{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"lambo_derive","arguments":{
  "agent_id":"doyle-probe",
  "concepts":[{"content":"Records API GET /records returns all records in one response","concept_type":"logic"}]}}}
```

Response: an acknowledgement, not the write. The write is applied in the background, so the ack carries a receipt id and no node ids:

```
accepted 1 concept(s) for background write; validated and ordered, not yet applied
receipt lwr1.<...>: pending — admitted, not yet applied; ask again
```

`structuredContent` holds `receipt`, `receipt_state` (`pending`), `agent_id` and `warnings`. `created` and `matched` are absent until the outcome is known.

### 5. `lambo_stats` with a receipt: learn the outcome (docs)

Request: `{"name":"lambo_stats","arguments":{"agent_id":"doyle-probe","receipt":"<receipt>","wait_ms":5000}}`

Use `wait_ms` when the next read must see the write. Otherwise the outcome arrives on the next tool response. The receipt states are `applied`, `pending`, `failed`, `dropped`, and others. Doyle must treat `pending` and `applied` differently.

### 6. `lambo_derive` with `against`: correct a memory (docs)

Request:
```json
{"name":"lambo_derive","arguments":{"agent_id":"doyle-probe","concepts":[
  {"content":"GET /records returns one page and a next_cursor","concept_type":"logic","against":["<8-hex id of the first memory>"]}]}}
```

`against` must name exactly one existing concept in the same session. A reference that matches none or several is refused before the ack, and nothing is written. Receipt text ends `; 1 contradiction(s) recorded` once applied. The contradiction adds no dependents and changes no score.

### 7. `lambo_recall`: read, and look for the warning (docs)

Request: `{"name":"lambo_recall","arguments":{"agent_id":"doyle-probe","query":"GET /records","top_k":5}}`

Response: `content[0].text` is the context block. `structuredContent` carries `context`, `hits[]` (each with `node_id`, `content`, `concept_type`, `score`, `is_canonical`, `blast_radius`), and `warnings[]`.

The contradiction warning (docs):
```
user schema [Entity, canonical] (score 1.95, id f0000000, blast radius 14)
⚠ Contradicted at 2026-08-10T10:28:00Z by agent-c: "accounts replaced the users table" (id 8c41d0e2)
```
It appears in the hit's block and in `warnings`. Doyle parses `⚠ Contradicted` lines and matches the hit by its id.

### 8. `lambo_inspect` (docs)

Request: `{"name":"lambo_inspect","arguments":{"agent_id":"doyle-probe","focus":"<8-hex id or node id>","depth":2}}`

Response: the neighbourhood as text, with `dependents (N):`, and structured `dependents`, `dependents_truncated` and `admission`.

## Addressing a session other than `lambo-dev`

**Recorded:** `POST /mcp/s/lambo-dev` answers 200, so the pinned session has its own URL. `POST /mcp/s/doyle-probe` answers 404 and no session is created (`04-unpinned-session-initialize.*`). `GET /admin/sessions` answers 404: no admin credential is configured.

**From the docs and the running server's configuration (keys only, no values read):**

- The server was started with one `--session lambo-dev`, HTTP, loopback. Its config has `[store]`, `[embedder]` and `[daemon]` sections and no `[serve]` section, so it pins nothing else and has no credentials.
- The docs say a serve "configured with nothing more than `--session` flags ... serves its pinned sessions and nothing else" (`cli.mdx`, "Pinned sessions and their URLs"). A session that is not pinned is unknown to `/mcp/s/<name>` and `/mcp` never attaches one on demand.
- `scope` on a tool call names only the session itself or one of its configured layers (`config.mdx`, `[serve] layers`). The running server has no layers, so `scope` cannot reach another session.

The ways to get a scratch session, none of them available on the running server as it stands:

1. Restart the server with an extra `--session doyle-probe`, or add it to `[serve] sessions`. The URL then becomes `/mcp/s/doyle-probe`. This changes the user's running service.
2. Add a `[[serve.credential]]` with `sessions` or `session_prefix` and `create = true`, which attaches the session on demand. This needs a token and a restart, and loopback then requires a bearer on every request.
3. Start a separate stdio `lambo serve --session doyle-probe` process. Docs say a stdio serve owns one session and reads its own config. It is a second writer on the same store, and it creates a new session there. It is not "this running server".

Option 1 is the least invasive, and it still changes the user's running service. The user must choose it. Until then, the write steps stay blocked.

## Surprises

- **Writes are asynchronous.** `lambo_derive` returns a receipt before the write is applied. A recall right after a derive can miss it unless the call waits on the receipt with `lambo_stats` and `wait_ms`.
- **The contradiction warning is not immediate (docs).** It is found by the background daemon and appears from the daemon's next cycle, not on the write's own next recall. A contradiction counts only once it is at least `canonization_edge_min_age` (60 seconds by default) old. Doyle's probe must wait before it expects the warning.
- **Warnings are not on the hit itself (docs).** The warning shows as a `⚠` line in the context block and in `warnings`. Doyle should read both.
- **The server has eight tools, not seven.** `tools/list` returns `lambo_stale` as well as the seven the reference lists, and no `lambo_derive_image`, which the reference documents. Doyle should read `tools/list` at startup rather than hardcode the set.
- **Every response is SSE**, including `tools/list`. A JSON-only client will fail silently.
- **The writer is single.** Two clients with the same `agent_id` share soft locks, so Doyle's probe id must differ from `lambo-dev`'s writer, `http-shared-writer` in the server's command line.
- **Reads on `lambo-dev`** are allowed by the task; they are recorded here and no memory was written.

## Not yet done

- Steps 2 to 4 of the task (derive, recall, inspect, the correction and the second recall): blocked on addressing a scratch session.
- The warning on the contradicted memory: not observed, because no correction was written. No issue was filed. If the gap shows up once a scratch session is available, its issue text belongs in the T0.5 report.
