# testdata

Fixtures every later task tests against. The gate reads only these files: no key, no network, no running server. Every name here is invented: the clinic chain, the vendor service, the repository and the sessions.

## The story: the Quiet Migration

1. **Setting.** Fernwhistle Veterinary Group keeps 4,812 patient records, each with its vaccination history and allergy notes, on a vendor's records service. Muse Wonder works in the group's ops repository, `clinic-ops`.
2. **Where M1 came from.** While the service was still on v1, a reporting session (`muse-wonder-reporting`) built the monthly patient-count report and saved four notes, in Muse Code and in Lambo. M1: `GET /patients` returns every patient in one response. M2: `GET /patients/{id}` returns 404 for an unknown id. M3: `GET /stats` `patient_count` is the total. M4: decommission is permanent. Two more monthly runs relied on M1 and M3, and were right.
3. **The change.** The vendor moves to v2. `GET /patients` now returns one page of 100 and a `next_cursor`. Nothing tells the memories.
4. **Export.** The migration session (`muse-wonder-migration`) exports with one `GET /patients`, because of M1. It gets a 200 and 100 patients, so its check (a 200 and a non-empty list) passes.
5. **Verify.** It compares the export with a second single `GET /patients`: 100 against 100, with the same ids. The check passes because it repeats the mistake.
6. **Import, then decommission.** After the import the new platform holds 100 patients, which matches the export, so that check passes too. The session confirms the decommission last, and gets a 200. Because of M4 it knows there is no undo, and it goes ahead because every check passed.
7. **Consequence.** 4,712 of 4,812 records are gone for good, with their vaccination histories and allergy notes, and nothing ever failed. The first sign is a client calling about their dog's medication allergy, which the new platform has no record of. The next monthly report would count 100.
8. **With Doyle.** `doyle check` on v1 → v2 flags M1 and nothing else. At the export, the guard denies the single `GET /patients` and says why: v2 returns 100 per page. The review page shows the change, the proposed correction, and the sessions that relied on M1: `report-2`, `report-3`, and `migration-1`'s export and verify. A person approves, and the correction lands in Muse Code (as an annotation on the note) and in Lambo (`against` M1). Wonder pages through all 49 pages (48 of 100 and one of 12) and checks the 4,812 it exported against `patient_count`, which M3 says is the total and which is still true. It imports, and keeps the old system until the new platform also counts 4,812.

## Files

| Path | What it is for |
|---|---|
| `contracts/patients-v1.yaml` | The vendor's patient records service before the change (OpenAPI 3.0). `listPatients` (`GET /patients`) returns a bare array of every `Patient`. |
| `contracts/patients-v2.yaml` | The same service after it. `listPatients` takes `cursor` and `limit` (default 100, maximum 100) and returns a `PatientPage` (`items`, `next_cursor`). `getPatient` (`GET /patients/{id}`, 404 for an unknown id), `decommission` (`POST /admin/decommission`) and `Patient` (with `vaccinations` and `allergies`) are identical in both versions. `Stats` gains an optional `generated_at`: the additive case. |
| `memories/muse-code/MEMORY.md` | Wonder's built-in memory for `clinic-ops`, in the verified format (`docs/muse-code.md`): `type` and `description` frontmatter, M1, M2 and M4 as inline notes, and an index line for the topic file. Terse and with no source, the way Wonder writes. |
| `memories/muse-code/patients-stats.md` | The topic file the index points at, with its own frontmatter, holding M3. |
| `memories/lambo/seed-derive.json` | The `lambo_derive` call the reporting session made, as `agent_id` `muse-wonder-reporting`: M1–M4 as `logic` concepts. |
| `memories/lambo/recall-patients.json` | A `lambo_recall` by the migration session, and the result it gets once M1–M4 are stored, before any correction: `content`, then `structuredContent` with `context`, `hits` and `warnings`. |
| `memories/provenance.json` | Which session wrote each memory and which later sessions relied on it, step by step. It feeds the review page's "who relied on it". |

The Lambo files follow the shapes in `docs/lambo.md`, which come from Lambo's published MCP reference and not from a live server. Their node ids and scores are invented, and so is everything in `provenance.json`; each file says so in its `_fixture` field. T0.5 confirms the Lambo shapes and may adjust these files.

Elsewhere in this folder: `muse/hooks/` holds the Muse Code hook payloads (T0.4), `lambo/` the recorded Lambo handshake (T0.5), and `vimanam/` (T0.6) Vimanam's view of these contracts.

## The memories

| Id | Text | Rests on | Lambo id |
|---|---|---|---|
| M1 | GET /patients returns every patient in one response; one call exports the whole clinic | `listPatients`, its 200 response | `5e1d0a73` |
| M2 | GET /patients/{id} returns 404 for an unknown patient id | `getPatient`, its 404 response | `a94c2f18` |
| M3 | GET /stats patient_count is the total number of patients | `getStats`, `Stats.patient_count` | `2d7b86e0` |
| M4 | POST /admin/decommission is permanent once confirmed; there is no undo | `decommission` | `f03a9c51` |

All four were written by `report-1`. Who relied on each is in `provenance.json`: M1 by `report-2`, `report-3`, and `migration-1` (export, verify); M2 by `report-2`; M3 by `report-2` and `report-3`; M4 by `migration-1` (decommission).

## Expected flags, v1 → v2

Written before any flagging code exists. `doyle check patients-v1.yaml patients-v2.yaml` must produce exactly this.

| Memory | Flagged? | Why |
|---|---|---|
| M1 | **Yes** | `listPatients`' 200 response changes from an array of every patient to one page and a `next_cursor` (breaking: `array` → `object`), and the operation now pages at 100 (review). The memory is false in v2. |
| M2 | No | `getPatient`, its parameter, its 200 and 404 responses, and `Patient` are identical in both versions. |
| M3 | No | `Stats` gains only an optional `generated_at`. Additive changes flag nothing, and `patient_count` means the same. |
| M4 | No | `decommission` and `DecommissionRequest` are identical in both versions. |

The flag set is `{M1}`: one flag and no false alarms.

Two traps for the flagging code:

- `Patient` is the item type of `GET /patients` in both versions, and it is unchanged. The change belongs to `listPatients`' response, not to `Patient`. A memory that cites `Patient` or `getPatient` must not be flagged through the shared `$ref`, even though v2's new `PatientPage` references it.
- `listPatients` also has additive changes: the `cursor` and `limit` arguments, and `items` and `next_cursor` on the new page type. None of these flags anything on its own. M1 is flagged by the response shape and the pagination.

M3 and M4 stay true and matter in the demo. M3's `patient_count` is how Doyle's run proves the export complete. M4 is why the loss without Doyle cannot be undone.
