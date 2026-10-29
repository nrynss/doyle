# testdata/vimanam

Vimanam 2.0.0's view of the Quiet Migration contracts (`testdata/contracts/patients-v1.yaml` → `patients-v2.yaml`). These files are generated, not written by hand. They are the evidence for the flagging work in P2: which entity IDs (`ve2_…`) and change IDs (`vc2_…`) Doyle will cite.

## Files

| File | What it is |
|---|---|
| `patients-v1-v2.json` | CLI JSON diff (`schema_version` 2): both sides' entity lists, the summary and all 11 change facts. |
| `patients-v1-v2.md` | CLI Markdown report, for people. It omits some facts (see "Where the Markdown differs" below). |
| `entities-v1.json` | Library parse of v1: `Contract` with `format`, `title`, `version` and 32 `entities` (id, kind, name, parent, services, native, attributes, references). |
| `entities-v2.json` | Library parse of v2: the same shape, 38 entities. |
| `changes-v1-v2.json` | Library diff of v1 → v2: the 11 change facts, the same objects and the same `vc2_` IDs as the CLI JSON. A `source` field names the call. |

Every entity ID that exists in v1 also exists in v2 with the same value (IDs are stable across versions). The new v2 entities are `cursor`, `limit`, `PatientPage`, its `items` and `next_cursor` fields, and `Stats.generated_at`.

## Vimanam version and commands

Vimanam **2.0.0** (crates.io), installed into a scratch root outside the repository, not globally:

```
cargo install vimanam --version 2.0.0 --locked --root <scratch>/vimanam
<scratch>/vimanam/bin/vimanam --version        # vimanam 2.0.0
```

`--locked` worked. `diff`'s `--contract` defaults to `2` in 2.0.0 (`diff --help`), so no flag was passed.

CLI (run from the repository root):

```
vimanam diff testdata/contracts/patients-v1.yaml testdata/contracts/patients-v2.yaml --format json -o testdata/vimanam/patients-v1-v2.json
vimanam diff testdata/contracts/patients-v1.yaml testdata/contracts/patients-v2.yaml -o testdata/vimanam/patients-v1-v2.md
```

Library: a scratch Cargo project outside the repository (not committed) with `vimanam = { version = "=2.0.0", default-features = false }`, `serde_json` and `anyhow`. It calls `vimanam::contract::openapi::parse(path)` for the `Contract` (entities), `vimanam::parse_openapi(path)` for the `ApiDocumentation`, and `vimanam::contract::openapi::diff(&old, &new)` for the `Vec<Change>`. The call names were read from the crate source in the cargo registry (`src/contract/mod.rs`, `src/contract/openapi.rs`).

The library diff and the CLI JSON agree: the same 11 change IDs in the same order.

## The change facts

Severity is Vimanam's verdict. "Subject" is the entity the fact is about; the name is the human name. Rows are in the order the CLI prints them.

| # | Change id (`vc2_…`) | Subject id (`ve2_…`) | Subject | Kind | Severity | Native kind |
|---|---|---|---|---|---|---|
| 1 | `vc2_6b9ac1411b5ee9a1075ddfbb0c5bd42539fa90e3db1f5b8a56cde2f66c3ef0f5` | `ve2_e9e6eff5e7653c9030cf2ac908ba27473bd2011c6aa7abf41abd8298abf45df6` | `cursor` argument of `listPatients` | `argument_added` | non_breaking | `parameter_added` |
| 2 | `vc2_c51832b42d95b3bd08010fe6d45ec3cdc7fe8ff962297759210d436fedda79c5` | `ve2_70215df48c165084129a175bf46d68fab5bfbd1eb5cca30250892848c2fa044b` | `limit` argument of `listPatients` | `argument_added` | non_breaking | `parameter_added` |
| 3 | `vc2_d7e1b2acbebe45b089106951b949710e3112d1586ed74cfe5fab47d702987277` | `ve2_86d3cdb7ce3a7f0b1a2fefa696f0acbbcf2d5406c717bb1f1f1cb5f4f62667ba` | `200` response of `listPatients` | `constraint_changed` | review | `response_schema_changed` |
| 4 | `vc2_9bedbbb47825fa87ea41aa38342d0492948286a7dd4a5ecbb378310261df6841` | `ve2_86d3cdb7ce3a7f0b1a2fefa696f0acbbcf2d5406c717bb1f1f1cb5f4f62667ba` | `200` response of `listPatients` | `field_type_changed` | breaking | `response_schema_changed` |
| 5 | `vc2_14d63f17bdfb4467870f8a2cd2e5e918171ba29935cb75283f9abc8693441609` | `ve2_86d3cdb7ce3a7f0b1a2fefa696f0acbbcf2d5406c717bb1f1f1cb5f4f62667ba` | `200` response of `listPatients` | `field_added` | non_breaking | `response_schema_changed` |
| 6 | `vc2_0653a7b991e46fa63f3df84606327c461a5dceed99cafa93a2a229ab8dc8fd24` | `ve2_86d3cdb7ce3a7f0b1a2fefa696f0acbbcf2d5406c717bb1f1f1cb5f4f62667ba` | `200` response of `listPatients` | `field_added` | non_breaking | `response_schema_changed` |
| 7 | `vc2_bfadda2dfdd29ad547e6471ed1597cd0926bdc571bf5e34d558069ba7a43ef86` | `ve2_86d3cdb7ce3a7f0b1a2fefa696f0acbbcf2d5406c717bb1f1f1cb5f4f62667ba` | `200` response of `listPatients` | `field_required_changed` | non_breaking | `response_schema_changed` |
| 8 | `vc2_7fa66397deef0d8f9302af4c94fcccdfdd0b3909895ed54b008b2103808cc132` | `ve2_6c3962f4033a5832c062e5d2bce8458ec018d67d8ff15cdbf109543d63662c5c` | `200` response of `getStats` | `field_added` | non_breaking | `response_schema_changed` |
| 9 | `vc2_6cfe1d7e93fec49f1e8a37c39c64364e23dbcd0ae3384e882ee8850d533903e0` | `ve2_c09c07ec8983f09afc5dafed67ee0e3e0d271de3b8f8f1bd61daa1111f063809` | `Stats` (type) | `field_added` | review | `surface_changed` |
| 10 | `vc2_63939142fa4b5f773d88166a6342c46dc416777aea41b2ed9d40b03ad23fe2b3` | `ve2_4e6b19cef1c5b0524fb5c8b9f921fc6df3117360c29c2c2d22e58aa536c547a4` | `PatientPage` (type) | `type_added` | non_breaking | `surface_changed` |
| 11 | `vc2_d82adaee7e47799b8027d3c1b9f458e16be8a20df1696186f351369f07e011e0` | `ve2_c81e37b7c42c484a1859c09e95bf8cec89a360db6743a691afab818f177c7bc6` | `listPatients` (`GET /patients`) | `pagination_changed` | review | `pagination_inferred` |

## Answers

**Is the pagination change reported?** Yes. Row 11: kind `pagination_changed`, native kind `pagination_inferred`, confidence `inferred`, evidence rule `list+request_controls+continuation`. Its subject is `listPatients` (`ve2_c81e37b7c42c484a1859c09e95bf8cec89a360db6743a691afab818f177c7bc6`), the operation, not the 200 response. Severity is **review**, so it is inferred and needs a person's judgment; it is not breaking. Its details name the controls (`cursor`, and `limit` with default and maximum 100) and the continuation (`next_cursor` on the 200 response).

**Which changes would flag M1** (`listPatients`' 200 response and pagination)? Under the rules in `docs/architecture.md`:

- Row 4, `field_type_changed`, **breaking**, subject the 200 response of `listPatients` (`/type` `array` → `object`). This is the breaking fact.
- Row 11, `pagination_changed`, **review**, subject `listPatients` itself. Flagged through the operation M1 cites.
- Row 3, `constraint_changed`, **review**, subject the same 200 response (`/items` removed). Also flags, as review.
- Rows 5, 6 and 7 are non-breaking additions to the same response. They flag nothing on their own.

So M1 is flagged by rows 3, 4 and 11.

**Do `getPatient`, `Patient` and `decommission` have change facts?** No, none of them. None of the 11 subjects is `getPatient` (`ve2_c69ae2bf6960e6f28e01bf913a93aa0a325f6d3ee49d445a68b1f580f27af2b5`), its arguments or responses, `Patient` (`ve2_ca77e31b900362a36a579ba59cdebfd8302cbfad2ec4b2d544857c0fcd30167a`) or its fields, `decommission` (`ve2_9e9663295485ddf808651f117c8ca42ab58e33fbe49a27309e0547f77c866846`), its body, its responses, or `DecommissionRequest`. `Patient` is referenced by v2's `PatientPage` (row 10) and by the `listPatients` response, but no change is about `Patient` itself, so a shared `$ref` does not reach `getPatient` or `Patient` from these facts.

**Which changes are additive (NonBreaking) and must flag nothing?** Rows 1, 2, 5, 6, 7, 8 and 10: the `cursor` and `limit` arguments, the `items` and `next_cursor` fields, `items` becoming required in the response, `Stats.generated_at` on the `getStats` response, and the new `PatientPage` type. These are the seven `non_breaking` facts in the summary.

**One fact is a problem for the trap in `testdata/README.md`.** Row 9 is a `field_added` for `Stats.generated_at` at the type level (subject `Stats`, `ve2_c09c07ec8983f09afc5dafed67ee0e3e0d271de3b8f8f1bd61daa1111f063809`) with severity **review**. The same field on the `getStats` response (row 8) is non-breaking. Vimanam calls an optional additive field on a shared named type "review", and `docs/architecture.md` says review facts flag. `getStats`' 200 response references `Stats`, so under rule 3 (entities referencing the changed named type) a review fact on `Stats` can reach M3, which the expected flag set `{M1}` says must not flag. Whether it does depends on how M3's citations are recorded: M3 cites `getStats` and `Stats.patient_count`, and the operation is the parent of the response, not the entity that references `Stats`. The flagging task must settle this. See the issue text below.

**The ve2_ IDs the later tasks will cite** (stable across v1 and v2):

| Name | Kind | ID |
|---|---|---|
| `listPatients` | operation, `GET /patients` | `ve2_c81e37b7c42c484a1859c09e95bf8cec89a360db6743a691afab818f177c7bc6` |
| `getPatient` | operation, `GET /patients/{id}` | `ve2_c69ae2bf6960e6f28e01bf913a93aa0a325f6d3ee49d445a68b1f580f27af2b5` |
| `getStats` | operation, `GET /stats` | `ve2_88f1296b143cdad99952319ae783095b3386438f0c8932538d9bfbe1b7fcfb0c` |
| `decommission` | operation, `POST /admin/decommission` | `ve2_9e9663295485ddf808651f117c8ca42ab58e33fbe49a27309e0547f77c866846` |
| `Patient` | type | `ve2_ca77e31b900362a36a579ba59cdebfd8302cbfad2ec4b2d544857c0fcd30167a` |
| `Stats` | type (for row 9) | `ve2_c09c07ec8983f09afc5dafed67ee0e3e0d271de3b8f8f1bd61daa1111f063809` |
| `PatientPage` | type, new in v2 (for row 10) | `ve2_4e6b19cef1c5b0524fb5c8b9f921fc6df3117360c29c2c2d22e58aa536c547a4` |

## Where the Markdown differs from the JSON

`patients-v1-v2.md` prints "1 to review" and lists only the `/items` removal. The JSON summary says `review: 3`. The Markdown report leaves out the pagination fact (row 11), the `Stats` type fact (row 9), and the `PatientPage` type fact (row 10, shown in the JSON summary as non-breaking). For the flagging code, the JSON is the source of truth.

## Issues for Vimanam (text for the handoff log)

1. **Optional additive field on a named schema is `review`.** `Stats.generated_at` is reported as a type-level `field_added` with severity `review` (row 9, `surface_changed`), while the same field on the `getStats` response is `non_breaking` (row 8). An optional field that is not required should be non-breaking at the type level too.
2. **The Markdown report hides facts that the JSON carries.** The pagination fact (row 11) and the type-level facts (rows 9 and 10) are missing from the Markdown tables, and its summary says "1 to review" where the JSON summary says `review: 3`.
