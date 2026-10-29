# P2: Flagging

Vimanam in, flags out. This is where Doyle is right or wrong: a missed flag lets an agent act on a stale memory, and a false flag teaches people to ignore Doyle. Everything here is computed; no model is involved.

Read first: [`../docs/architecture.md`](../docs/architecture.md) (Flagging), `testdata/README.md` (the expected flag set), `testdata/vimanam/`.

---

### T2.1: Contract index ★
```yaml
requires:   [T1.1, T0.6]
fixture-ok: yes
size:       M · mid
owns:       src/contract.rs, Cargo.toml (the vimanam line), Cargo.lock
status:     done
```
Load a contract file through Vimanam's library (default features off) and build an index Doyle can query: entity by `ElementId`; parent chain; entities that reference a named type; native location (method and path, operation ID, type and field name) for display and for the guard. Diff two versions into Vimanam's change facts, kept as Vimanam gives them.

**Done when:** on the demo contracts, the index answers each query in a test, matching `testdata/vimanam/`; the diff gives the recorded change set; a malformed contract fails with the file name and Vimanam's reason.

Notes (T2.1, in-review): `src/contract.rs` holds `ContractIndex` (Vimanam's neutral surface plus the parsed document the diff reads, one `parse_openapi` then `adapt`, no double parse), `NativeLocation` (Doyle's own display/guard struct), and `ContractError` (file plus Vimanam's reason verbatim; `Display` names the file). `entity`, `parent_chain` (entity itself first, then ancestors, cycle-guarded), `referencers` (native order), `native_location` (operation ID and type name resolved through the parent chain; non-OpenAPI natives give `None` for method/path), `diff` (old.diff(new), facts untouched). Seven tests against the demo contracts: entity-ID lists equal `entities-v1/v2.json` (32/38), chain `[200, listPatients]`, v1 `Patient` referencers are the two 200s, v2 adds `PatientPage`, locations for the operation/result/type/field cases, diff IDs equal `changes-v1-v2.json` (11, severity 1/3/7) with M1's three facts on their subjects, malformed file fails naming the file. Surprises: hand-copied `ve2_`/`vc2_` IDs dropped single characters in three constants (caught by the tests, then every test ID checked against the fixtures by script); Vimanam's `ApiDocumentation` is `Debug`-only so the index is not `Clone`. Outside `owns`, approved: one `mod contract;` declaration line in `src/main.rs` with a scoped allow, mirroring T1.1's `mod core;` precedent. Gate passes in the worktree.

---

### T2.2: Flagging ★
```yaml
requires:   [T1.2, T2.1]
fixture-ok: yes
size:       M · frontier
owns:       src/core/flag.rs
status:     in-review
```
Given the citation index and a set of changes, produce flags (architecture.md, Flagging): the subject, its ancestors, and entities referencing its named type. `NonBreaking` flags nothing; `Breaking` flags; `Review` flags, marked for review. One flag per memory per check, listing every change behind it. A memory already flagged and not yet decided is not flagged twice.

Rules that need interpreting are this task's to decide and write down: where ancestor propagation stops (a field change flags the operation returning its type, but not every other operation in the service), and how a change to a shared type reaches memories citing operations that use it.

Vimanam 2.0 rates the optional field added to the `Stats` type as `review` at the type level (T0.6, `testdata/vimanam/README.md`, row 9) while the same field on `getStats`' response is non-breaking. M3 must not be flagged by it: decide here how a type-level `review` on an added optional field propagates, and write the rule down.

**Done when:** on the demo, exactly the memories `testdata/README.md` names are flagged, with the right change IDs; the additive change flags nothing; tests cover each propagation path and the no-duplicate rule; the propagation rules are written under this task.

Notes (T2.2, in-review): `src/core/flag.rs` holds `flag_citations(citations, changes, old, open_flags, old_version, new_version) -> Vec<Flag>`: one flag per memory in first-citation order, every matching change ID in Vimanam's order, severity the highest of its changes, evidence the citation's claim, `Flag.id` left empty for the store to assign. Non-breaking is filtered before it can flag (Doyle's `Severity` cannot represent it); unknown subjects still match direct citations; citations and changes are matched by `ve2_`/`vc2_` string equality. 11 tests; the demo gives exactly `{M1}` with `[constraint, breaking, pagination]` and severity breaking. Gate passes in the worktree.

Propagation rules, decided here:
- A change flags citers of its subject, of every ancestor of the subject, and — only for shared types — of every referencer of the subject's named type plus those referencers' ancestors. The subject's named type is itself when it is a type, else the nearest ancestor that is one; operation-scoped subjects (operations, arguments, results, inline response fields) have none.
- Ancestor propagation stops at the using operation: a field change flags citers of its type and of the operations returning that type (the operation is the parent of the response that references the type), but never any other operation in the service. The referencer hop is single: referencers of referencers are not followed.
- Operation-scoped changes never expand through `$ref`s: a change to `listPatients`' 200 does not flag citers of `getPatient` or `Patient` even though both responses reference it in v1 and `PatientPage` references it in v2. Propagation is upward-only; citing a descendant of the subject never flags.
- A type-level `review` on an added field flags nothing. The `Stats.generated_at` review at the type level is the same additive change Vimanam rates non-breaking on `getStats`' 200, seen through the shared schema; an optional field breaks no existing reader, so it propagates nowhere — not to referencers, not even to direct citers of the type. This is what keeps M3 unflagged. Limitation: should Vimanam ever rate a required field addition as `review`, this suppresses it too; no such fact exists in the fixtures.

Surprises: hand-copied `vc2_` IDs were wrong in two test constants (`M1_CONSTRAINT`, `M1_PAGINATION`, one character each) despite being read from the README; every ID in the file is now script-checked against `testdata/vimanam/*.json` (15 constants, all present). Vimanam's `Severity` and `ChangeKind` are `#[non_exhaustive]`, so matching on them needs a wildcard arm; unknown future severities flag nothing. Outside `owns`, the single `mod flag;` declaration line in `src/core/mod.rs` (alphabetical, before `model`) plus its scoped re-export allow, mirroring the T1.1/T2.1 precedents.

---

### T2.3: `doyle check` without proposals ★
```yaml
requires:   [T2.2, T1.5]
fixture-ok: yes
size:       S · mid
owns:       src/check.rs
status:     not-started
```
`doyle check OLD NEW`: load both contracts, diff, flag against the store, write the flags, print a report. `--format markdown|json`. Exit codes: 0 nothing flagged, 1 flags written, 2 error, so CI can fail a build on stale memory. Proposals are added in T5.2.

**Done when:** on the demo it prints the expected flags in both formats and exits 1; on two identical contracts it exits 0; the JSON output has a test pinning its shape.

---

### T2.4: `doyle cite`
```yaml
requires:   [T1.2, T2.1, T1.5]
fixture-ok: yes
size:       XS · light
owns:       src/cite.rs
status:     not-started
```
Record a citation by hand: a memory reference, the contract, and an element given by its native location (`GET /patients`) or its `ElementId`. Refuse an element the contract doesn't have, naming the closest matches.

**Done when:** citing the demo's memories by hand and then running `doyle check` gives the expected flags; an unknown element is refused with suggestions.

---

## Exit Criteria

- [ ] The contract index answers every query the flagging and the guard need (T2.1).
- [ ] The demo's expected flag set is produced exactly, and the additive change flags nothing (T2.2).
- [ ] `doyle check` runs end to end without a model, with CI exit codes (T2.3).
- [ ] Citations can be recorded by hand (T2.4).
