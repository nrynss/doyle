# Lambo as a client: the calls Doyle needs

Status: every call Doyle needs is recorded against the running server (Lambo 0.8.0). Doyle has two sessions of its own on it: `doyle` for the product and the demo, and `doyle-probe` for throwaway writes like these. Transcripts `01`–`04` are the first, read-only probe of `lambo-dev`; `05`–`15` are the write half, all in `doyle-probe`. Nothing was ever written to `lambo-dev`.

Sources: Lambo's `docs/reference/mcp.mdx`, `config.mdx` and `cli.mdx`. The shapes below marked **recorded** come from the server; those marked **docs** come from the reference and have not been checked against the running server.

## Transport

- Streamable HTTP at `http://127.0.0.1:7700/mcp`. The default session is `lambo-dev`. Doyle's sessions are at `http://127.0.0.1:7700/mcp/s/doyle` and `/mcp/s/doyle-probe`.
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

### 4. `lambo_derive`: write a memory (recorded: `07-derive-memory.*`)

Request:
```json
{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"lambo_derive","arguments":{
  "agent_id":"doyle-probe",
  "concepts":[{"content":"GET /patients returns every patient in one response; one call exports the whole clinic","concept_type":"logic"}]}}}
```

Response: an acknowledgement, not the write. The write is applied in the background, so the ack carries a receipt id and no node ids:

```
accepted 1 concept(s) for background write; validated and ordered, not yet applied
receipt lwr1.<...>: pending — admitted, not yet applied; ask again
```

`structuredContent` holds `receipt`, `receipt_state` (`pending`), `agent_id` and `warnings`. `created` and `matched` are absent until the outcome is known.

### 5. `lambo_stats` with a receipt: learn the outcome (recorded: `08-stats-wait-memory.*`, `12-stats-wait-correction.*`)

Request: `{"name":"lambo_stats","arguments":{"agent_id":"doyle-probe","receipt":"<receipt>","wait_ms":5000}}`

**Recorded:** `structuredContent.receipt` carries `detail` (`applied — derived 1 concept(s): 1 created (1 embedded), 0 matched existing`), `created` (full node ids), `matched`, and counts. For a correction, `detail` ends `; 1 contradiction(s) recorded; 0 stale direct dependenc(ies)`. The full stats block comes back too, including the server's ledger path (replaced with `~` in the recording).

Use `wait_ms` when the next read must see the write. Otherwise the outcome arrives on the next tool response. The receipt states are `applied`, `pending`, `failed`, `dropped`, and others. Doyle must treat `pending` and `applied` differently.

### 6. `lambo_derive` with `against`: correct a memory (recorded: `11-derive-correction-against.*`)

Request:
```json
{"name":"lambo_derive","arguments":{"agent_id":"doyle-probe","concepts":[
  {"content":"GET /patients returns 100 patients per page and a next_cursor; follow next_cursor until it is null to export every patient","concept_type":"logic","against":["810c1cbf"]}]}}
```

`against` must name exactly one existing concept in the same session. A reference that matches none or several is refused before the ack, and nothing is written. Receipt text ends `; 1 contradiction(s) recorded` once applied. The contradiction adds no dependents and changes no score.

### 7. `lambo_recall`: read, and look for the warning (recorded: `09-…`, `13-recall-right-after-correction.*`, `14-recall-after-75s.*`)

Request: `{"name":"lambo_recall","arguments":{"agent_id":"doyle-probe","query":"GET /patients export every patient","top_k":5}}`

Response: `content[0].text` is the context block. `structuredContent` carries `context`, `hits[]` (each with `node_id`, `content`, `concept_type`, `score`, `is_canonical`, `blast_radius`), and `warnings[]`.

The contradiction warning, **recorded** on the first recall after the correction was applied:
```
GET /patients returns every patient in one response; one call exports the whole clinic [Logic] (score 0.94, id 810c1cbf)
⚠ Contradicted at <time> by doyle-probe: "GET /patients returns 100 patients per page and a next_cursor; follow next_curso…" (id 1be4e4a6)

GET /patients returns 100 patients per page and a next_cursor; follow next_cursor until it is null to export every patient [Logic] (score 0.88, id 1be4e4a6)
⚠ Contradicts "GET /patients returns every patient in one response; one call exports the whole …" (id 810c1cbf)
```
The stale memory carries `⚠ Contradicted … (id <correction>)` and the correction carries `⚠ Contradicts … (id <stale>)`. Both lines are in the context block and in `warnings`. The same two lines came back 75 seconds later. Doyle parses the `⚠ Contradicted` line and matches it to the hit by id.

### 8. `lambo_inspect` (recorded: `10-inspect-memory.*`)

Request: `{"name":"lambo_inspect","arguments":{"agent_id":"doyle-probe","focus":"<8-hex id or node id>","depth":2}}`

**Recorded:** `structuredContent` has `admission`, `focus`, `resolution`, `view` and `warnings`. The text shows the focus, its canonization status (`Candidate refused`, with the reasons), and the neighbourhood by hop.

### 9. `lambo_stale` (recorded: `15-stale.*`)

Lists direct dependents of contradicted concepts: `stale_report.dependents` and `total`. Empty here, because nothing in `doyle-probe` cites the stale memory through `depends_on`.

## Addressing a session other than `lambo-dev`

**Recorded:** `POST /mcp/s/<session>` reaches a pinned session. Before Doyle's sessions were added, `/mcp/s/doyle-probe` answered 404 and created nothing (`04-unpinned-session-initialize.*`): a serve started with `--session` flags serves those sessions and nothing else, and `scope` on a call can only name the session itself or one of its configured layers.

**Done, by the user's decision:** the shared writer (`lambo-dogfood.service`, started by `~/lambo-dogfood/bin/serve-http.sh`) now runs with `--session lambo-dev --session doyle --session doyle-probe`. `lambo-dev` is first, so it stays the default at `/mcp` and no existing client changes. After the restart all three answered `initialize` with 200, and `session_leases` shows the HTTP writer holding all three. The script's previous version is kept beside it as `.serve-http.sh.bak-before-doyle`.

## Surprises

- **Writes are asynchronous.** `lambo_derive` returns a receipt before the write is applied. A recall right after a derive can miss it unless the call waits on the receipt with `lambo_stats` and `wait_ms`.
- **The contradiction warning is immediate (recorded), not delayed as the docs suggested.** It showed on the first recall after the correction's receipt was applied, and was unchanged 75 seconds later. The 60-second `canonization_edge_min_age` in the docs governs canonization, not this warning.
- **Warnings are not on the hit itself.** The warning shows as a `⚠` line in the context block and in `warnings`; `hits[]` carries no flag. Doyle should read both.
- **The server has eight tools, not seven.** `tools/list` returns `lambo_stale` as well as the seven the reference lists, and no `lambo_derive_image`, which the reference documents. Doyle should read `tools/list` at startup rather than hardcode the set.
- **Every response is SSE**, including `tools/list`. A JSON-only client will fail silently.
- **The writer is single.** Two clients with the same `agent_id` share soft locks, so Doyle's probe id must differ from `lambo-dev`'s writer, `http-shared-writer` in the server's command line.
- **Reads on `lambo-dev`** are allowed by the task; they are recorded here and no memory was written.

## Not covered

- `lambo_stale` with a dependent: nothing in the probe cites the stale memory through `depends_on`, so the report is empty. P3 can add one if the adapter needs it.
