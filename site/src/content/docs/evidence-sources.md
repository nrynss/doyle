---
title: Evidence sources
description: How Doyle extracts stable element IDs and change facts from API contracts, COBOL programs, console screens, and differential runs.
---

Coding agents memorize facts from documentation, code, terminal screens, and test runs. When upstream sources change, those memories rot silently.

Doyle unifies evidence extraction across four heterogeneous sources. Each source produces deterministic element identifiers and structured change facts.

## API contracts via Vimanam

Doyle integrates the Vimanam library in [`src/contract.rs`](https://github.com/nrynss/doyle/blob/main/src/contract.rs) to inspect OpenAPI and Protobuf definitions.

### What a memory cites

Agent memories cite stable API contract elements. These include operations, object schemas, input parameters, and response fields.

Vimanam computes stable identifiers with the `ve2_` prefix. Identifiers remain identical across minor edits and document reordering.

Change facts receive deterministic identifiers with the `vc2_` prefix.

### What makes a memory stale

A memory becomes stale when Vimanam discovers breaking or review changes between contract versions.

Breaking changes include removed endpoints, narrowed parameters, and altered data types.

Review changes include inferred behavioral shifts. For example, adding pagination with `maxItems: 100` triggers a review flag.

### Flag evidence

When a contract change touches a cited element, Doyle raises a flag. The flag bundles explicit proof:

- Unique `vc2_` change fact identifiers.
- Change severity categorized as `breaking` or `review`.
- Native locations such as `GET /patients` or specific parameter names.
- Human-readable descriptions explaining how the interface shifted.

## Legacy COBOL

Doyle parses COBOL programs and copybooks in [`src/evidence/legacy/mod.rs`](https://github.com/nrynss/doyle/blob/main/src/evidence/legacy/mod.rs). Details appear in [`docs/legacy-ids.md`](https://github.com/nrynss/doyle/blob/main/docs/legacy-ids.md).

### What a memory cites

During mainframe modernization, agents hold beliefs about batch processing rules and data layouts.

Memories cite six structural element kinds:

- Programs (`PROGRAM-ID`)
- Paragraphs in the `PROCEDURE DIVISION`
- Sections in the `PROCEDURE DIVISION`
- Level-01 records from copybooks
- Member fields with dot-joined record paths
- External files defined in `SELECT` statements

Element identifiers use the prefix `le2_`. Change facts use the prefix `lc2_`.

Identities hash canonical element keys with SHA-256 as implemented in [`src/evidence/legacy/ids.rs`](https://github.com/nrynss/doyle/blob/main/src/evidence/legacy/ids.rs). Keys ignore line numbers and comments.

### What makes a memory stale

Modifying program logic or data layouts invalidates legacy citations:

- Changing a record byte length, such as `PATIENT-RECORD` growing from 34 to 35 characters.
- Moving field offsets or modifying `PIC` definitions.
- Removing paragraphs or sections.
- Changing `PERFORM` or `CALL` control targets.

### Flag evidence

A legacy flag presents clear structural diffs:

- All triggered `lc2_` change identifiers.
- Exact byte offset and length shifts for affected record layouts.
- Native COBOL names like `CALC-LATE-FEE` or `PATIENT-RECORD.BALANCE`.
- Old and new versions of the parsed source files.

## Console screens and training video

Operators interact with legacy systems through terminal consoles and video workflows. Doyle processes visual evidence in [`src/evidence/screens/mod.rs`](https://github.com/nrynss/doyle/blob/main/src/evidence/screens/mod.rs). Experimental results appear in [`docs/wonder-vision.md`](https://github.com/nrynss/doyle/blob/main/docs/wonder-vision.md).

### What a memory cites

Doyle invokes `muse-wonder-1.0` through `doyle screens read` to parse console screenshots and video recordings.

Memories cite five visual element kinds:

- Whole screens identified by console name and screen title
- Text fields with normalized labels and grid positions
- Action targets including function keys like `PF12`
- Operator status and error messages
- Training video steps with measured time spans

Screen element identifiers use the prefix `se2_`. Change facts use the prefix `sc2_`.

Both derive from SHA-256 hashes defined in [`src/evidence/screens/ids.rs`](https://github.com/nrynss/doyle/blob/main/src/evidence/screens/ids.rs).

### What makes a memory stale

Screen memories become stale when the user interface alters operational constraints:

- Dropping mandatory input prompts, such as removing the confirmation field on a decommission screen.
- Changing function key bindings, such as remapping `PF12` from confirm to exit.
- Modifying critical warning banners or status indicators.

### Flag evidence

Screen flags provide visual diff context:

- Specific `sc2_` change identifiers.
- Side-by-side comparisons of before and after element values.
- Grid coordinates and native function key tokens.

## Differential runs

When rewriting batch jobs into modern languages, agents frequently introduce subtle calculation bugs. Doyle isolates behavioral divergence in [`src/evidence/diffrun.rs`](https://github.com/nrynss/doyle/blob/main/src/evidence/diffrun.rs) and [`src/diffrun_cmd.rs`](https://github.com/nrynss/doyle/blob/main/src/diffrun_cmd.rs).

### What a memory cites

Differential runs cite legacy COBOL paragraphs using `le2_` element identifiers. These citations anchor the original business logic.

### What makes a memory stale

Doyle executes the legacy COBOL batch program and the modern rewrite against identical test inputs.

A memory becomes stale when record-level outputs diverge. For example, a rewrite bug might overcharge 535 patient accounts by \$8,308.80.

### How blame works

Doyle traces execution to pinpoint the exact routine responsible for output discrepancies:

1. GnuCOBOL runs with `-ftrace` enabled, emitting paragraph execution events to stderr for each record.
2. Doyle isolates records that produced mismatched outputs.
3. It computes the intersection of all paragraphs executed during those failing records.
4. It subtracts paragraphs that also ran during completely clean records.
5. If an exclusive paragraph remains, Doyle blames that routine directly (`behavioral_mismatch_blamed`).
6. If no single routine is exclusive, Doyle attributes the flag to shared candidate paragraphs (`behavioral_mismatch_shared`).

In our test suite, this subtraction isolates the `LOG-EXEMPT` paragraph instantly.

### Flag evidence

A differential-run flag carries forensic proof:

- Assigned `lc2_` change fact identifiers.
- Total count of mismatching records.
- Names and `le2_` identifiers of blamed paragraphs.
- Sample mismatching record lines comparing legacy output against rewrite output.
- The `rewrite_artifacts` paths passed to `doyle guard` to hold premature shipping.
