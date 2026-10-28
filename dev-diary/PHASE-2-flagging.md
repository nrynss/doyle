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
status:     not-started
```
Load a contract file through Vimanam's library (default features off) and build an index Doyle can query: entity by `ElementId`; parent chain; entities that reference a named type; native location (method and path, operation ID, type and field name) for display and for the guard. Diff two versions into Vimanam's change facts, kept as Vimanam gives them.

**Done when:** on the demo contracts, the index answers each query in a test, matching `testdata/vimanam/`; the diff gives the recorded change set; a malformed contract fails with the file name and Vimanam's reason.

---

### T2.2: Flagging ★
```yaml
requires:   [T1.2, T2.1]
fixture-ok: yes
size:       M · frontier
owns:       src/core/flag.rs
status:     not-started
```
Given the citation index and a set of changes, produce flags (architecture.md, Flagging): the subject, its ancestors, and entities referencing its named type. `NonBreaking` flags nothing; `Breaking` flags; `Review` flags, marked for review. One flag per memory per check, listing every change behind it. A memory already flagged and not yet decided is not flagged twice.

Rules that need interpreting are this task's to decide and write down: where ancestor propagation stops (a field change flags the operation returning its type, but not every other operation in the service), and how a change to a shared type reaches memories citing operations that use it.

**Done when:** on the demo, exactly the memories `testdata/README.md` names are flagged, with the right change IDs; the additive change flags nothing; tests cover each propagation path and the no-duplicate rule; the propagation rules are written under this task.

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
Record a citation by hand: a memory reference, the contract, and an element given by its native location (`GET /records`) or its `ElementId`. Refuse an element the contract doesn't have, naming the closest matches.

**Done when:** citing the demo's memories by hand and then running `doyle check` gives the expected flags; an unknown element is refused with suggestions.

---

## Exit Criteria

- [ ] The contract index answers every query the flagging and the guard need (T2.1).
- [ ] The demo's expected flag set is produced exactly, and the additive change flags nothing (T2.2).
- [ ] `doyle check` runs end to end without a model, with CI exit codes (T2.3).
- [ ] Citations can be recorded by hand (T2.4).
