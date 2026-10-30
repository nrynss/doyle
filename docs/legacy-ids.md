# Citing legacy COBOL: element IDs, changes, severities

How a Doyle memory cites COBOL, decided before any code (T9.5 builds the
seam, T9.6 the parser). The rule mirrors Vimanam: an element's ID stays
the same across versions when the element means the same thing, so a
citation written today still points at the same thing after an unrelated
edit. Line numbers, source order and comments are never in an ID.

## Elements

A memory may cite exactly these, nothing else:

| Kind | What | Keyed by |
|---|---|---|
| program | one `PROGRAM-ID` | the program name |
| paragraph | a named paragraph in `PROCEDURE DIVISION` | program + paragraph name |
| section | a named section in `PROCEDURE DIVISION` | program + section name |
| record | a level-01 record from a copybook | program + record name |
| field | a copybook data item | program + dot-joined level path from the record down |
| file | a `SELECT` file | program + `SELECT` name |

Working-storage items, `FD` entries, report-layout lines (`DETAIL-LINE`,
`TOTAL-LINE` and their subfields) and copybook comments are not elements.
A memory about a computed value cites the paragraph that produces it
(e.g. the totals line cites `WRITE-TOTALS`, not `T-BALANCE`); a memory
about nothing in this list is `rests_on_nothing`.

## The ID

- Prefix `le2_` for legacy elements, `lc2_` for legacy change facts.
  (`ve2_`/`vc2_` stay Vimanam's; `ElementId` and `ChangeId` keep their
  shape, per the evidence-source plan.)
- Canonical key, then `le2_` plus the 64 lowercase hex characters of the
  SHA-256 of the key's UTF-8 bytes:

```
legacy:<PROGRAM>                          program
legacy:<PROGRAM>:para:<PARAGRAPH>         paragraph
legacy:<PROGRAM>:section:<SECTION>        section
legacy:<PROGRAM>:record:<RECORD>          copybook record
legacy:<PROGRAM>:field:<PATH>             copybook field, PATH dot-joined
legacy:<PROGRAM>:file:<SELECT>           file
```

- Names are COBOL names as written, uppercased by the compiler's rules;
  the key uses the written form. The `PIC` clause, the `ASSIGN` literal,
  the copybook file name and the source line are attributes of the
  element, never parts of the key: a `PIC` change keeps the field's ID,
  which is what lets a flag name its subject.
- A rename is a remove plus an add: the new name keys a new ID, and
  citations of the old name flag as removed. There is no rename threading
  through aliases.

## Changes and severities

Severities are Doyle's own (`Breaking`, `Review`; non-breaking facts are
never written and flag nothing). Additive changes flag nothing, the way
Vimanam's do.

| Kind | Change | Severity |
|---|---|---|
| program | added | non-breaking |
| program | removed | breaking (citations of it and everything under it flag) |
| paragraph / section | any statement added, removed or modified, or a `PERFORM` target changed | breaking for citing memories |
| paragraph / section | added | non-breaking |
| paragraph / section | removed | breaking for citing memories |
| record | added | non-breaking |
| record | removed | breaking |
| record | layout changed: record length or any member offset changed (a field added anywhere, a member length changed) | breaking for memories citing the record |
| field | `PIC` or length changed | breaking for the field; also a layout change on the parent record |
| field | added | non-breaking for the field; still a layout change on the parent record |
| field | removed, renamed, or moved (level path changed) | breaking (rename/move = remove + add under a new ID) |
| file (`SELECT`) | added / removed | non-breaking / breaking |
| file | `ASSIGN` literal changed | review (the path moved; the layout may be identical) |
| file | organization or access mode changed | review |

Whitespace, comments and reformatting produce no change fact. A section
that only groups unchanged paragraphs is unchanged.

The `lc2_` change ID derives from the subject element's `le2_` ID plus
the change kind and the attribute that changed (e.g. the old and new
`PIC`); T9.6 fixes the exact derivation. It never contains a line number.

Native location (for the guard, formalized in T9.5/T9.6): program →
`PROGRAM-ID` + source file; paragraph/section → source file + name (a
line number may ride along as a hint, never as identity); field → copybook
file + level path + `PIC`; file → `SELECT` name + `ASSIGN` literal.

## Worked examples: BILLBATCH

The T9.1 sample: program `BILLBATCH` (`BILLBATCH.cbl`), seven paragraphs,
one copybook record (`patient.cpy`), two files. The program has no
sections. IDs below are the scheme applied literally
(`le2_` + SHA-256 hex of the key).

Program:

- key `legacy:BILLBATCH`
- `le2_be3caaf70f3125fa6cc719da40eb9b356e258afe83ba1527943e65867dead079`
- program BILLBATCH

Paragraphs (`PROCEDURE DIVISION`, each `PERFORM`ed by name):

| Paragraph | Key suffix | ID |
|---|---|---|
| `MAIN-PARA` | `para:MAIN-PARA` | `le2_2e1ccc42b46636ec00adf64f718d727d5110dbadf0df5085c98609c09590c488` |
| `READ-NEXT` | `para:READ-NEXT` | `le2_9e88e4bb2b151c9bbd4634a1601dfacc0a0f0f03281b35e0732e3404dbcddcd9` |
| `PROCESS-ALL` | `para:PROCESS-ALL` | `le2_ed52f2639dd1235812b5dd87ba91b73728636879883ad77f92b3f0b0afad9716` |
| `CALC-LATE-FEE` | `para:CALC-LATE-FEE` | `le2_3422f89619e902b31f7fe202a2fc9a05701bbc7ae83f09cf366989f5e95f749e` |
| `LOG-EXEMPT` | `para:LOG-EXEMPT` | `le2_af6c903a18d5e82a8429ca3d0b9c2ad6a5bdc11b876f8ae18bb2d926b47d83a3` |
| `WRITE-DETAIL` | `para:WRITE-DETAIL` | `le2_3e24136be0c79237bb6fd791ab71a3cb871e1d0e7cbb0c4e4e3a9fc41b6b867b` |
| `WRITE-TOTALS` | `para:WRITE-TOTALS` | `le2_72951287dc5071d4725e669b23300824422e343209bdadb81e9f61e3174b7d3d` |

Full key example: `legacy:BILLBATCH:para:CALC-LATE-FEE`. Its two
statements (`COMPUTE WS-LATE-FEE ROUNDED = BALANCE * 0.015`, `ADD
WS-LATE-FEE TO WS-TOTAL-FEE`) are the paragraph's content: changing the
rate to `0.02` is a breaking change on this same ID.

Record and fields (`01 PATIENT-RECORD`, 34 chars: 6 + 20 + 7 + 1):

- record key `legacy:BILLBATCH:record:PATIENT-RECORD`
- `le2_690243c98922cdcded58beed19f46cb8725f9690399b9f1c297cc465c4f21533`
- record PATIENT-RECORD (copybook patient.cpy)

| Field | Level path | `PIC` | ID |
|---|---|---|---|
| `ACCT-NO` | `PATIENT-RECORD.ACCT-NO` | `9(6)` | `le2_45b9463e291475e0882b9909f0f4e8762e5c39528785f7697b6023433315facc` |
| `ACCT-NAME` | `PATIENT-RECORD.ACCT-NAME` | `X(20)` | `le2_aa156746e052e0c3095b931dceea8aa24c04efdee6b8331d58d5e1801cd82f1e` |
| `BALANCE` | `PATIENT-RECORD.BALANCE` | `9(5)V99` | `le2_115161c1611668d048b209f67556c356ed639c6a74f05ecd39bf85c515909ce2` |
| `EXEMPT-FLAG` | `PATIENT-RECORD.EXEMPT-FLAG` | `X` | `le2_72c916279d8685a43a018ce196a4b790b00816441abfd7e4444ace1cc15cc0f9` |

Full key example: `legacy:BILLBATCH:field:PATIENT-RECORD.BALANCE`. The
`PIC 9(5)V99` is an attribute: widening it to `9(7)V99` keeps this ID and
raises a breaking field change plus a breaking layout change on
`PATIENT-RECORD` (the record grows past 34 chars). Inserting a new field
between `BALANCE` and `EXEMPT-FLAG` is non-breaking for the new field
itself and breaking for the record (every later offset moves).

Files:

- key `legacy:BILLBATCH:file:ACCT-FILE`
- `le2_c49db257411076d0f1b1ce83039459dd83e03476d4cca6ac5eb3f6a8ad5583d0`
- file ACCT-FILE ASSIGN "patients.dat"
- key `legacy:BILLBATCH:file:REPORT-FILE`
- `le2_672f4512a15a7b96608cb6f383cf33ac782d1a4acd2811519f508b57be839a2c`
- file REPORT-FILE ASSIGN "report.txt"

## The curator probe

The check: can the curator (Muse Spark Open,
`meta-models/muse-spark-open-1026` on Baseten) cite business-rule memories
against the 15-element list above, naming the right paragraph and field?

Prompt: the T4.3 curator prompt and schema mirrored for COBOL
(`src/prompts/curator.rs` is the original). Same reply fields, same
JSON schema, same budget (2048 output tokens), same ID-filtering rule
(unknown IDs dropped in code; nothing citable becomes
`rests_on_nothing`). Two wording substitutions: "API contract" →
"legacy COBOL program", "Contract elements" → "Program elements". The
`request` object in each recording holds exactly the sent envelope
(`model`, `system`, `user`, `schema`, `budget`); `reasoning_effort` is a
client setting, omitted (default) on all four calls.

Three memories, all business rules a migrating agent would plausibly hold:

- L1 (true): "BILLBATCH charges a 1.5% late fee on each account's
  balance, rounded to the cent, computed in CALC-LATE-FEE." Expected:
  the `CALC-LATE-FEE` paragraph; also accepted: the program, the
  `BALANCE` field.
- L2 (false of the code, the T9.4 trap shape): "Exempt accounts still
  accrue the 1.5% late fee; the exempt flag only marks the report line."
  Expected: the `LOG-EXEMPT` paragraph and the `EXEMPT-FLAG` field (a
  false claim is still cited to what would make it false); also accepted:
  `CALC-LATE-FEE`, `BALANCE`.
- L3 (layout): "Each patient account record is 34 characters: a 6-digit
  account number, a 20-character name, a 7-digit balance with an implied
  decimal point, and a 1-character exempt flag." Expected: the
  `PATIENT-RECORD` record; also accepted: any of its four fields.

Result: 3 of 3 cite correctly, with no unknown IDs dropped. L1 cited the
program, `CALC-LATE-FEE` and `BALANCE` (confidence 0.93). L2 cited
`CALC-LATE-FEE`, `EXEMPT-FLAG`, `BALANCE`, `LOG-EXEMPT` and
`WRITE-DETAIL` (0.88) — both expected elements present; the extra cites
are the surrounding rule and the report line the claim mentions, the same
ancestor/context generosity the API-curator scoring already accepts. L3
cited the record and all four fields (1.0).

Recordings: `testdata/legacy/ids/L1-baseten.json`,
`L2-baseten.json`, `L3-baseten.json` (request envelope plus reply value
and usage; no key in any file).

Calls and usage (Baseten returns no dollar cost, same as T4.3):

| Call | Input tokens | Output tokens | Outcome |
|---|---|---|---|
| L1 | 1135 | 1350 | cited program + CALC-LATE-FEE + BALANCE |
| L2 first attempt | (no usage returned) | null content | empty reply, retried |
| L2 retry | 1125 | 1694 | cited LOG-EXEMPT + EXEMPT-FLAG + context |
| L3 | 1143 | 1555 | cited record + all four fields |

The L2 first attempt came back `null` content with HTTP 200 — the same
starvation T4.3 measured when reasoning consumes the token budget. A
single retry at default effort succeeded, so no `reasoning_effort: low`
override was needed and the committed recording holds only the
successful reply.

No "it cannot" finding: the curator names the right paragraph and field
on all three memories, including the false one. What the probe does not
show: sections (the sample has none), renames, or multi-version
stability — those are T9.6's tests to run against real diffs.

Notes for T9.6: the parser subset is the T9.1 sample's constructs
(`IDENTIFICATION`/`ENVIRONMENT`/`DATA`/`PROCEDURE`, `COPY`, `SELECT`,
level-01/05 items with `PIC`, named paragraphs, `PERFORM`); anything
outside it (`EVALUATE`, nested programs, `REDEFINES`) is out of scope
until the story fixtures say otherwise. The fixed-width offset rule above
means the differ must recompute member offsets, not just compare `PIC`
strings.
