# testdata

Fixtures every later task tests against. The gate reads only these files: no key, no network, no running server.

## The demo story

1. Muse Wonder remembers that `GET /records` returns every record in one response, so a backup can fetch it once.
2. The records service moves to v2: `GET /records` now returns one page of 100 records and a `next_cursor` for the rest.
3. Without Doyle, Wonder writes the backup from memory: one call, a 200, and every record past the first page silently missing.
4. With Doyle, the v1 → v2 diff flags that memory (and only that one), Muse Spark proposes a correction, and a person approves it.
5. The guard stops the one-call backup, the memory is corrected in Muse Code and Lambo, and Wonder writes a backup that follows `next_cursor` to the end.

## Files

| Path | What it is for |
|---|---|
| `contracts/records-v1.yaml` | The records service before the change (OpenAPI 3.0). `listRecords` (`GET /records`) returns a bare array of every `Record`. |
| `contracts/records-v2.yaml` | The same service after it. `listRecords` takes `cursor` and `limit` (default 100, maximum 500) and returns a `RecordPage`: `items` plus `next_cursor`. `getRecord` is unchanged. `Stats` gains an optional `oldest_created_at`, an additive change no memory depends on. |
| `memories/muse-code/MEMORY.md` | A Muse Code memory folder in the verified on-disk format: `type` and `description` frontmatter, two inline notes (M1, M2) and an index line pointing at the topic file. Written the way Wonder writes: terse, no source. |
| `memories/muse-code/records-stats.md` | The per-topic file the index points at, with its own frontmatter, holding M3. |
| `memories/lambo/seed-derive.json` | The `lambo_derive` call that puts M1–M3 into a Lambo session as `logic` concepts. |
| `memories/lambo/recall-records.json` | A `lambo_recall` request and the result it returns once M1–M3 are stored, before any correction: `content`, then `structuredContent` with `context`, `hits` and `warnings`. |

The Lambo shapes follow Lambo's published MCP reference (`docs/reference/mcp.mdx`), not a live server. The node ids and scores are made up. T0.5 records real calls into `testdata/lambo/` and may adjust these two files to match what a running server returns. `lambo_inspect` is not modelled here: the reference documents its structured fields only in part, and T0.5 records a real one.

`testdata/vimanam/` (T0.6) holds Vimanam's view of these contracts: the entity IDs and the change facts.

## The memories

| Id | Text | Rests on |
|---|---|---|
| M1 | GET /records returns all records in one response; a backup can fetch it once | `listRecords` (`GET /records`), its 200 response |
| M2 | GET /records/{id} returns 404 when the record does not exist | `getRecord` (`GET /records/{id}`), its 404 response |
| M3 | GET /stats record_count is the total number of records | `getStats` (`GET /stats`), `Stats.record_count` |

In Muse Code, M1 and M2 are inline notes in `MEMORY.md` and M3 is in `records-stats.md`. In Lambo they are the three concepts in `seed-derive.json`, ids `3b7e9a1c`, `8f20c6d4` and `c41a57e2` in `recall-records.json`.

## Expected flags, v1 → v2

Written before any flagging code exists. `doyle check records-v1.yaml records-v2.yaml` must produce exactly this.

| Memory | Flagged? | Why |
|---|---|---|
| M1 | **Yes** | The 200 response of `listRecords` changes shape from an array of every record to one page plus `next_cursor` (breaking: `array` → `object`), and the operation is now paginated with a default page size of 100 (review). The memory is false in v2. |
| M2 | No | `getRecord`, its parameters, its responses and `Record` are identical in both versions. |
| M3 | No | `Stats` gains only an optional field, `oldest_created_at`; additive changes flag nothing, and `record_count` means the same. |

The flag set is `{M1}`. One flag, no false alarms.

Two traps for the flagging code:

- `Record` is the item type of `GET /records` in both versions and is itself unchanged. The change belongs to `listRecords`' response, not to `Record`, so a memory citing `Record` or `getRecord` must not be flagged through a shared `$ref`.
- `listRecords` also has additive changes (`cursor`, `limit`, `next_cursor`). They flag nothing on their own; M1 is flagged by the response shape and the pagination.

M3 stays true and is useful in the demo: comparing the backup's record count with `record_count` is how the lost records show.
