//! The evidence source for COBOL, on `docs/legacy-ids.md`'s scheme.
//!
//! A memory cites a program, a paragraph or section, a copybook record or
//! field, or a `SELECT` file, each by an `le2_` ID computed from its
//! canonical key ([`ids`]). [`LegacyIndex`] parses one version (a `.cbl`
//! program plus its `COPY`ed copybooks, no compiler needed) into elements,
//! and [`LegacyIndex::diff`] compares two versions into change facts with
//! `lc2_` IDs. [`LegacyIndex::diff_evidence`] serves the flag-worthy facts
//! through the [`EvidenceSource`] seam: non-breaking adds are never written,
//! the way Vimanam's are dropped.
//!
//! ## The parser subset
//!
//! The parser ([`parse`]) reads structure, never semantics, and only the
//! constructs the story fixtures use: the `IDENTIFICATION`, `ENVIRONMENT`,
//! `DATA` and `PROCEDURE` divisions, `COPY` names, `SELECT` files with
//! `ASSIGN` and organization, level-01 records with level-05 items carrying
//! `PIC`, and named paragraphs (optionally grouped under named sections).
//! Paragraph bodies are opaque normalized text (uppercased, comments cut,
//! whitespace collapsed): reformatting never counts as a change, and the
//! rate `0.015` versus `0.02` does. The differ recomputes member offsets
//! from the running field lengths, so a layout change is a length or offset
//! change, never a `PIC`-string comparison.
//!
//! What the parser does not handle, and how it says so:
//!
//! - No control-flow or data-flow analysis at all. `EVALUATE`, `IF`
//!   branches and every `PERFORM` form are plain body text: compared, never
//!   interpreted. A `PERFORM` target change counts through the body text;
//!   there is no call graph and no branch coverage.
//! - Rejected outright with a [`LegacyError`] naming the construct:
//!   `REDEFINES`, `OCCURS`, nested programs (a second `PROGRAM-ID` or `END
//!   PROGRAM`), level 66/77/88 items, `COPY REPLACING`, group items without
//!   `PIC` in a copybook, an elementary level-01 with its own `PIC`, and
//!   statements outside any paragraph. A paragraph name colliding with a
//!   scope terminator (`END-IF.`) or a no-op verb (`EXIT.`, `CONTINUE.`)
//!   alone on a line is outside the subset: the header heuristic would
//!   misread it, so it is documented, not guessed.
//! - Not elements, per `docs/legacy-ids.md`: `FD` entries, working-storage
//!   items, report-layout lines and copybook comments. Inline level-01
//!   records in the program file are ignored: records come from `COPY`ed
//!   copybooks. `FILLER` items count toward the record length and member
//!   offsets but never become elements.
//!
//! ## Native locations and the guard
//!
//! The seam's [`NativeLocation`](crate::evidence::NativeLocation) has no
//! COBOL slots, so the source maps onto the shared ones the way the T9.5
//! seam test sketched it: a program names its type slot, a paragraph or
//! section names the operation-ID slot, a record names the type slot, a
//! field names its record and itself, and a file names the operation-ID
//! slot with its `SELECT` name. The guard matches operation IDs and type
//! names, never field names, whatever source served them.
//!
//! Legacy flags hold no step at the guard (T9.15, user decision): a COBOL
//! paragraph or field name in free command text is a read of the code as
//! often as an action resting on the memory, so the guard no longer
//! matches it. Reading, grepping and compiling the batch are allowed. The
//! differential run is what guards the rewrite, by the rewrite's own files
//! (`rewrite_artifacts` in its run manifest). The guard-boundary tests
//! below pin it: legacy versions, loadable or not, allow silently.

use std::collections::HashMap;
use std::path::Path;

use super::{
    EvidenceChange, EvidenceChangeKind, EvidenceElement, EvidenceElementKind, EvidenceSource,
    NativeLocation,
};
use crate::contract::ContractError;
use crate::core::{ChangeId, ElementId, Severity};

pub mod ids;
pub mod parse;

use parse::{ParsedBlock, ParsedProgram};

/// Loading or parsing a COBOL version failed: which file, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyError {
    /// The program or copybook that failed to parse.
    pub file: String,
    /// What was wrong, naming the construct when one is unsupported.
    pub reason: String,
}

impl std::fmt::Display for LegacyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.file, self.reason)
    }
}

impl std::error::Error for LegacyError {}

/// What a legacy element is. Paragraphs and sections read like operations
/// (a name the guard can match); the copybook record is the shared named
/// type its fields belong to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyKind {
    Program,
    Paragraph,
    Section,
    Record,
    Field,
    File,
}

/// One indexed legacy element: its stable ID, its place in the version,
/// and the attributes the differ compares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyElement {
    /// The source's stable ID (`le2_…`).
    pub id: ElementId,
    /// What the element is.
    pub kind: LegacyKind,
    /// Display name: the paragraph, section, record, field or file name.
    pub name: String,
    /// The containing element, absent for the program.
    pub parent: Option<ElementId>,
    /// For fields: the dot-joined level path (`PATIENT-RECORD.BALANCE`).
    pub path: String,
    /// Normalized body text (paragraphs and sections), for the differ.
    pub body: String,
    /// The `PIC` clause (fields), uppercased.
    pub pic: String,
    /// Display length: a field's `PIC` length, a record's total.
    pub length: usize,
    /// Offset from the record start (fields).
    pub offset: usize,
    /// Member layout (records): `(level path, offset, length)` in source
    /// order, so the differ recomputes offsets instead of comparing `PIC`
    /// strings. A member set, offset or total change is a layout change.
    pub members: Vec<(String, usize, usize)>,
    /// The `ASSIGN` literal with its quotes (files).
    pub assign: String,
    /// The organization phrase (files), empty when absent.
    pub organization: String,
    /// The native source file's basename.
    pub source: String,
}

/// Doyle's severity plus the non-breaking case the seam drops: additive
/// changes flag nothing, so the raw diff carries what `diff_evidence`
/// never writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacySeverity {
    Breaking,
    Review,
    NonBreaking,
}

/// What changed between two versions, in `legacy-diff.json`'s vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegacyChangeKind {
    ProgramRemoved,
    ParagraphAdded,
    ParagraphRemoved,
    ParagraphStatementsChanged,
    SectionAdded,
    SectionRemoved,
    SectionStatementsChanged,
    RecordAdded,
    RecordRemoved,
    RecordLayoutChanged,
    FieldAdded,
    FieldRemoved,
    FieldPicChanged,
    FileAdded,
    FileRemoved,
    FileAssignChanged,
    FileOrganizationChanged,
}

impl LegacyChangeKind {
    /// The kind string in change keys and `legacy-diff.json`.
    pub fn kind_str(&self) -> &'static str {
        match self {
            Self::ProgramRemoved => "program-removed",
            Self::ParagraphAdded => "paragraph-added",
            Self::ParagraphRemoved => "paragraph-removed",
            Self::ParagraphStatementsChanged => "paragraph-statements-changed",
            Self::SectionAdded => "section-added",
            Self::SectionRemoved => "section-removed",
            Self::SectionStatementsChanged => "section-statements-changed",
            Self::RecordAdded => "record-added",
            Self::RecordRemoved => "record-removed",
            Self::RecordLayoutChanged => "record-layout-changed",
            Self::FieldAdded => "field-added",
            Self::FieldRemoved => "field-removed",
            Self::FieldPicChanged => "field-pic-changed",
            Self::FileAdded => "file-added",
            Self::FileRemoved => "file-removed",
            Self::FileAssignChanged => "file-assign-changed",
            Self::FileOrganizationChanged => "file-organization-changed",
        }
    }

    /// Doyle's severity for the kind. Adds are non-breaking; `ASSIGN` and
    /// organization moves need review; everything else breaks citers.
    pub fn severity(&self) -> LegacySeverity {
        match self {
            Self::ParagraphAdded
            | Self::SectionAdded
            | Self::RecordAdded
            | Self::FieldAdded
            | Self::FileAdded => LegacySeverity::NonBreaking,
            Self::FileAssignChanged | Self::FileOrganizationChanged => LegacySeverity::Review,
            _ => LegacySeverity::Breaking,
        }
    }
}

/// One change fact between two versions: the subject, the kind, and the
/// attribute on either side when the kind carries one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyChange {
    /// The source's change ID (`lc2_…`).
    pub id: ChangeId,
    /// The changed element.
    pub subject: ElementId,
    /// What changed.
    pub kind: LegacyChangeKind,
    /// The attribute before the change (bodies, `PIC`s, lengths, literals).
    pub old_attr: String,
    /// The attribute after the change.
    pub new_attr: String,
    /// Doyle's severity, including the non-breaking the seam drops.
    pub severity: LegacySeverity,
}

/// One parsed COBOL version, indexed for flagging and the guard.
#[derive(Debug, Clone)]
pub struct LegacyIndex {
    program: String,
    elements: Vec<LegacyElement>,
    by_id: HashMap<String, usize>,
}

impl LegacyIndex {
    /// Parse a version from source text: the program plus each `COPY`ed
    /// copybook as `(filename, source)`.
    pub fn parse(
        program_src: &str,
        program_file: &str,
        copybooks: &[(&str, &str)],
    ) -> Result<Self, LegacyError> {
        Self::from_parsed(parse::parse_program(program_src, program_file, copybooks)?)
    }

    /// Load a version from a `.cbl` path: `COPY` names resolve to siblings.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, LegacyError> {
        Self::from_parsed(parse::load_program(path.as_ref())?)
    }

    fn from_parsed(parsed: ParsedProgram) -> Result<Self, LegacyError> {
        let mut elements = Vec::new();
        let mut by_id = HashMap::new();
        let mut push = |element: LegacyElement| {
            by_id.insert(element.id.0.clone(), elements.len());
            elements.push(element);
        };
        let program_id = ids::element_id(&ids::program_key(&parsed.program));
        push(LegacyElement {
            id: program_id.clone(),
            kind: LegacyKind::Program,
            name: parsed.program.clone(),
            parent: None,
            path: String::new(),
            body: String::new(),
            pic: String::new(),
            length: 0,
            offset: 0,
            assign: String::new(),
            organization: String::new(),
            source: parsed.program_file.clone(),
            members: Vec::new(),
        });
        for block in &parsed.blocks {
            match block {
                ParsedBlock::Section(section) => push(LegacyElement {
                    id: ids::element_id(&ids::section_key(&parsed.program, &section.name)),
                    kind: LegacyKind::Section,
                    name: section.name.clone(),
                    parent: Some(program_id.clone()),
                    path: String::new(),
                    body: section.body_normalized.clone(),
                    pic: String::new(),
                    length: 0,
                    offset: 0,
                    assign: String::new(),
                    organization: String::new(),
                    source: parsed.program_file.clone(),
                    members: Vec::new(),
                }),
                ParsedBlock::Paragraph(para) => push(LegacyElement {
                    id: ids::element_id(&ids::paragraph_key(&parsed.program, &para.name)),
                    kind: LegacyKind::Paragraph,
                    name: para.name.clone(),
                    parent: Some(program_id.clone()),
                    path: String::new(),
                    body: para.body_normalized.clone(),
                    pic: String::new(),
                    length: 0,
                    offset: 0,
                    assign: String::new(),
                    organization: String::new(),
                    source: parsed.program_file.clone(),
                    members: Vec::new(),
                }),
            }
        }
        for book in &parsed.copybooks {
            for record in &book.records {
                let record_id = ids::element_id(&ids::record_key(&parsed.program, &record.name));
                let total: usize = record.fields.iter().map(|f| f.length).sum();
                let members: Vec<(String, usize, usize)> = record
                    .fields
                    .iter()
                    .map(|f| (format!("{}.{}", record.name, f.name), f.offset, f.length))
                    .collect();
                push(LegacyElement {
                    id: record_id.clone(),
                    kind: LegacyKind::Record,
                    name: record.name.clone(),
                    parent: Some(program_id.clone()),
                    path: String::new(),
                    body: String::new(),
                    pic: String::new(),
                    length: total,
                    offset: 0,
                    assign: String::new(),
                    organization: String::new(),
                    source: book.filename.clone(),
                    members: members.clone(),
                });
                for field in &record.fields {
                    if field.filler {
                        continue;
                    }
                    let path = format!("{}.{}", record.name, field.name);
                    push(LegacyElement {
                        id: ids::element_id(&ids::field_key(&parsed.program, &path)),
                        kind: LegacyKind::Field,
                        name: field.name.clone(),
                        parent: Some(record_id.clone()),
                        path,
                        body: String::new(),
                        pic: field.pic.clone(),
                        length: field.length,
                        offset: field.offset,
                        assign: String::new(),
                        organization: String::new(),
                        source: book.filename.clone(),
                        members: Vec::new(),
                    });
                }
            }
        }
        for file in &parsed.files {
            push(LegacyElement {
                id: ids::element_id(&ids::file_key(&parsed.program, &file.name)),
                kind: LegacyKind::File,
                name: file.name.clone(),
                parent: Some(program_id.clone()),
                path: String::new(),
                body: String::new(),
                pic: String::new(),
                length: 0,
                offset: 0,
                assign: file.assign.clone(),
                organization: file.organization.clone(),
                source: parsed.program_file.clone(),
                members: Vec::new(),
            });
        }
        Ok(Self {
            program: parsed.program,
            elements,
            by_id,
        })
    }

    /// The program this version indexes.
    pub fn program(&self) -> &str {
        &self.program
    }

    /// The indexed element with this ID, if this version holds one.
    pub fn legacy_element(&self, id: &ElementId) -> Option<&LegacyElement> {
        self.by_id.get(&id.0).map(|&i| &self.elements[i])
    }

    /// Diff this index (old) against another (new): every change fact,
    /// including the non-breaking adds the seam drops. Changed and removed
    /// elements come first in old-index order, then additions in new-index
    /// order. A program rename is degenerate: only the old program's
    /// removal is reported, since every new-side element is trivially new.
    pub fn diff(&self, new: &Self) -> Vec<LegacyChange> {
        if self.program != new.program {
            let subject = ids::element_id(&ids::program_key(&self.program));
            return vec![Self::fact(
                subject,
                LegacyChangeKind::ProgramRemoved,
                "",
                "",
            )];
        }
        let mut facts = Vec::new();
        for old in &self.elements {
            let Some(current) = new.legacy_element(&old.id) else {
                facts.push(self.removed(old));
                continue;
            };
            facts.extend(self.changed(old, current));
        }
        for current in &new.elements {
            if self.legacy_element(&current.id).is_none() {
                facts.push(self.added(current));
            }
        }
        facts
    }

    /// Diff through the seam: only the flag-worthy facts, in diff order.
    /// Additive changes flag nothing, so the seam drops them here.
    pub fn diff_evidence(&self, new: &Self) -> Result<Vec<EvidenceChange>, ContractError> {
        Ok(self
            .diff(new)
            .into_iter()
            .filter(|c| c.severity != LegacySeverity::NonBreaking)
            .map(|c| c.as_evidence())
            .collect())
    }

    fn fact(
        subject: ElementId,
        kind: LegacyChangeKind,
        old_attr: &str,
        new_attr: &str,
    ) -> LegacyChange {
        LegacyChange {
            id: ids::change_id(&subject, kind.kind_str(), old_attr, new_attr),
            subject,
            severity: kind.severity(),
            kind,
            old_attr: old_attr.to_string(),
            new_attr: new_attr.to_string(),
        }
    }

    fn removed(&self, old: &LegacyElement) -> LegacyChange {
        let kind = match old.kind {
            LegacyKind::Program => LegacyChangeKind::ProgramRemoved,
            LegacyKind::Paragraph => LegacyChangeKind::ParagraphRemoved,
            LegacyKind::Section => LegacyChangeKind::SectionRemoved,
            LegacyKind::Record => LegacyChangeKind::RecordRemoved,
            LegacyKind::Field => LegacyChangeKind::FieldRemoved,
            LegacyKind::File => LegacyChangeKind::FileRemoved,
        };
        Self::fact(old.id.clone(), kind, "", "")
    }

    /// The addition fact for an element present only in the new version.
    /// The program case never reaches here: a renamed program returns
    /// early from [`LegacyIndex::diff`] as a removal.
    fn added(&self, current: &LegacyElement) -> LegacyChange {
        let kind = match current.kind {
            LegacyKind::Program => LegacyChangeKind::ProgramRemoved,
            LegacyKind::Paragraph => LegacyChangeKind::ParagraphAdded,
            LegacyKind::Section => LegacyChangeKind::SectionAdded,
            LegacyKind::Record => LegacyChangeKind::RecordAdded,
            LegacyKind::Field => LegacyChangeKind::FieldAdded,
            LegacyKind::File => LegacyChangeKind::FileAdded,
        };
        Self::fact(current.id.clone(), kind, "", "")
    }

    /// The changed-element facts between two present elements of one ID.
    fn changed(&self, old: &LegacyElement, current: &LegacyElement) -> Vec<LegacyChange> {
        match old.kind {
            LegacyKind::Paragraph | LegacyKind::Section => {
                if old.body != current.body {
                    let kind = match old.kind {
                        LegacyKind::Section => LegacyChangeKind::SectionStatementsChanged,
                        _ => LegacyChangeKind::ParagraphStatementsChanged,
                    };
                    vec![Self::fact(old.id.clone(), kind, &old.body, &current.body)]
                } else {
                    Vec::new()
                }
            }
            LegacyKind::Record => {
                if old.length != current.length || old.members != current.members {
                    vec![Self::fact(
                        old.id.clone(),
                        LegacyChangeKind::RecordLayoutChanged,
                        &old.length.to_string(),
                        &current.length.to_string(),
                    )]
                } else {
                    Vec::new()
                }
            }
            LegacyKind::Field => {
                if old.pic != current.pic {
                    vec![Self::fact(
                        old.id.clone(),
                        LegacyChangeKind::FieldPicChanged,
                        &old.pic,
                        &current.pic,
                    )]
                } else {
                    Vec::new()
                }
            }
            LegacyKind::File => {
                let mut facts = Vec::new();
                if old.assign != current.assign {
                    facts.push(Self::fact(
                        old.id.clone(),
                        LegacyChangeKind::FileAssignChanged,
                        &old.assign,
                        &current.assign,
                    ));
                }
                if old.organization != current.organization {
                    facts.push(Self::fact(
                        old.id.clone(),
                        LegacyChangeKind::FileOrganizationChanged,
                        &old.organization,
                        &current.organization,
                    ));
                }
                facts
            }
            LegacyKind::Program => Vec::new(),
        }
    }

    fn to_evidence(&self, element: &LegacyElement) -> EvidenceElement {
        let kind = match element.kind {
            LegacyKind::Program => EvidenceElementKind::Other("program".to_string()),
            LegacyKind::Paragraph | LegacyKind::Section => EvidenceElementKind::Operation,
            LegacyKind::Record => EvidenceElementKind::Type,
            LegacyKind::Field => EvidenceElementKind::Field,
            LegacyKind::File => EvidenceElementKind::Other("file".to_string()),
        };
        EvidenceElement {
            id: element.id.clone(),
            kind,
            parent: element.parent.clone(),
            references: Vec::new(),
            native: self.evidence_native(element),
        }
    }

    fn evidence_native(&self, element: &LegacyElement) -> NativeLocation {
        match element.kind {
            LegacyKind::Program => NativeLocation {
                method: None,
                path: None,
                operation_id: None,
                type_name: Some(element.name.clone()),
                field_name: None,
            },
            LegacyKind::Paragraph | LegacyKind::Section => NativeLocation {
                method: None,
                path: None,
                operation_id: Some(element.name.clone()),
                type_name: None,
                field_name: None,
            },
            LegacyKind::Record => NativeLocation {
                method: None,
                path: None,
                operation_id: None,
                type_name: Some(element.name.clone()),
                field_name: None,
            },
            LegacyKind::Field => {
                let record = element
                    .parent
                    .as_ref()
                    .and_then(|p| self.legacy_element(p))
                    .map(|r| r.name.clone())
                    .unwrap_or_default();
                NativeLocation {
                    method: None,
                    path: None,
                    operation_id: None,
                    type_name: Some(record),
                    field_name: Some(element.name.clone()),
                }
            }
            LegacyKind::File => NativeLocation {
                method: None,
                path: None,
                operation_id: Some(element.name.clone()),
                type_name: None,
                field_name: None,
            },
        }
    }
}

impl LegacyChange {
    /// The seam's view of this fact. Non-breaking facts never reach here:
    /// [`LegacyIndex::diff_evidence`] drops them first.
    fn as_evidence(&self) -> EvidenceChange {
        let kind = match self.kind {
            LegacyChangeKind::FieldAdded => EvidenceChangeKind::FieldAdded,
            ref other => EvidenceChangeKind::Other(other.kind_str().to_string()),
        };
        let severity = match self.severity {
            LegacySeverity::Breaking => Severity::Breaking,
            LegacySeverity::Review => Severity::Review,
            LegacySeverity::NonBreaking => {
                unreachable!("non-breaking facts never reach the seam")
            }
        };
        EvidenceChange {
            id: self.id.clone(),
            subject: self.subject.clone(),
            kind,
            severity,
        }
    }
}

impl EvidenceSource for LegacyIndex {
    fn element(&self, id: &ElementId) -> Option<EvidenceElement> {
        self.legacy_element(id).map(|e| self.to_evidence(e))
    }

    fn elements(&self) -> Vec<EvidenceElement> {
        self.elements.iter().map(|e| self.to_evidence(e)).collect()
    }

    fn parent_chain(&self, id: &ElementId) -> Vec<EvidenceElement> {
        let mut chain = Vec::new();
        let mut next = self.legacy_element(id);
        while let Some(current) = next {
            if chain.len() > self.elements.len() {
                break;
            }
            next = current.parent.as_ref().and_then(|p| self.legacy_element(p));
            chain.push(self.to_evidence(current));
        }
        chain
    }

    /// The legacy source cites no cross-element references: no shared
    /// named type is referenced from another element, so propagation runs
    /// on subjects and ancestors only.
    fn referencers(&self, _id: &ElementId) -> Vec<EvidenceElement> {
        Vec::new()
    }

    fn native_location(&self, id: &ElementId) -> Option<NativeLocation> {
        self.legacy_element(id).map(|e| self.evidence_native(e))
    }

    fn diff_evidence(&self, new: &Self) -> Result<Vec<EvidenceChange>, ContractError> {
        self.diff_evidence(new)
    }
}

#[cfg(test)]
mod tests {
    use super::super::ChangeFact;
    use super::*;
    use crate::core::{Citation, MemoryRef, Severity, flag_citations};

    fn fixture(name: &str) -> String {
        format!("{}/testdata/{name}", env!("CARGO_MANIFEST_DIR"))
    }

    fn story_v1() -> LegacyIndex {
        LegacyIndex::load(fixture("legacy/story/v1/BILLBATCH.cbl")).expect("v1 loads")
    }

    fn story_v2() -> LegacyIndex {
        LegacyIndex::load(fixture("legacy/story/v2/BILLBATCH.cbl")).expect("v2 loads")
    }

    /// Every element ID in the story fixtures recomputes from its canonical
    /// key: the scheme is honored, nothing is copied — including
    /// `CALC-LATE-FEE`, whose ID matches the worked example in
    /// `docs/legacy-ids.md` across both samples.
    #[test]
    fn story_elements_and_ids_match_the_scheme() {
        for (version, index) in [("v1", story_v1()), ("v2", story_v2())] {
            let recorded: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(fixture(&format!("legacy/story/elements-{version}.json")))
                    .expect("fixture reads"),
            )
            .expect("fixture parses");
            let expected = recorded["elements"].as_array().expect("array");
            let ours = index.elements();
            assert_eq!(
                ours.len(),
                expected.len(),
                "{version}: element count (ours {ours:?})"
            );
            for (element, want) in ours.iter().zip(expected.iter()) {
                let key = want["key"].as_str().expect("key");
                let want_id = want["id"].as_str().expect("id");
                let computed = ids::element_id(key);
                assert_eq!(computed.0, want_id, "{version}: key {key}");
                assert_eq!(element.id.0, want_id, "{version}: index id for {key}");
            }
        }
        assert_eq!(story_v1().elements().len(), 24);
        assert_eq!(story_v2().elements().len(), 26);
        // The shared elements keep stable IDs across versions.
        let v1_ids: Vec<String> = story_v1()
            .elements()
            .iter()
            .map(|e| e.id.0.clone())
            .collect();
        let v2_ids: Vec<String> = story_v2()
            .elements()
            .iter()
            .map(|e| e.id.0.clone())
            .collect();
        let shared = v1_ids.iter().filter(|id| v2_ids.contains(id)).count();
        // All 24 v1 IDs persist in v2 (the story README says 20, a miscount:
        // every v1 key recomputes to a v2 ID above, and the only v2-only keys
        // are the two Trap B1 adds). Stability across versions is the point
        // of the name-path-keyed scheme.
        assert_eq!(shared, 24, "every v1 ID stays stable");
    }

    /// The full diff on the story pair is the story's four changes and no
    /// others; the seam keeps the two breaking facts.
    #[test]
    fn story_diff_gives_the_expected_changes_and_no_others() {
        let old = story_v1();
        let new = story_v2();
        let facts = old.diff(&new);
        let summary: Vec<(&str, &str, LegacySeverity)> = facts
            .iter()
            .map(|f| (f.subject.0.as_str(), f.kind.kind_str(), f.severity))
            .collect();
        let added_para = ids::element_id(&ids::paragraph_key("BILLBATCH", "APPLY-PROMPT-DISCOUNT"));
        let added_field =
            ids::element_id(&ids::field_key("BILLBATCH", "PATIENT-RECORD.DISCOUNT-FLAG"));
        let record = ids::element_id(&ids::record_key("BILLBATCH", "PATIENT-RECORD"));
        let charge = ids::element_id(&ids::paragraph_key("BILLBATCH", "CHARGE-ACCOUNT"));
        // v2's WRITE-SUMMARY also changed: four added lines moving the
        // discount totals (visible at testdata/legacy/story/v2/BILLBATCH.cbl
        // lines 240-243). Per docs/legacy-ids.md any added statement is a
        // breaking change on the paragraph's stable ID, so the true diff is
        // five facts, not the four legacy-diff.json lists: T9.4 counted the
        // discount report lines as non-citable layout (true of the
        // SUMMARY-LINE-4 item) but missed the paragraph statements that move
        // them. No story memory cites WRITE-SUMMARY, so the flag story is
        // unaffected; the finding goes in the task report.
        let write_summary = ids::element_id(&ids::paragraph_key("BILLBATCH", "WRITE-SUMMARY"));
        assert_eq!(
            summary,
            [
                (
                    charge.0.as_str(),
                    "paragraph-statements-changed",
                    LegacySeverity::Breaking
                ),
                (
                    write_summary.0.as_str(),
                    "paragraph-statements-changed",
                    LegacySeverity::Breaking
                ),
                (
                    record.0.as_str(),
                    "record-layout-changed",
                    LegacySeverity::Breaking
                ),
                (
                    added_para.0.as_str(),
                    "paragraph-added",
                    LegacySeverity::NonBreaking
                ),
                (
                    added_field.0.as_str(),
                    "field-added",
                    LegacySeverity::NonBreaking
                ),
            ],
            "old-order changes then new-order adds: {summary:?}"
        );
        // The layout fact carries the record lengths that recomputed it.
        let layout = facts
            .iter()
            .find(|f| f.kind == LegacyChangeKind::RecordLayoutChanged)
            .expect("layout fact");
        assert_eq!(
            (layout.old_attr.as_str(), layout.new_attr.as_str()),
            ("34", "35")
        );
        // The wiring fact carries the bodies that differ.
        let wiring = facts
            .iter()
            .find(|f| f.kind == LegacyChangeKind::ParagraphStatementsChanged)
            .expect("wiring fact");
        assert_eq!(wiring.subject, charge);
        assert!(wiring.new_attr.contains("APPLY-PROMPT-DISCOUNT"));
        assert!(!wiring.old_attr.contains("APPLY-PROMPT-DISCOUNT"));

        let seam = old.diff_evidence(&new).expect("seam diff works");
        assert_eq!(seam.len(), 3, "adds never reach the seam: {seam:?}");
        assert!(
            seam.iter()
                .all(|f| f.change_severity() == Some(Severity::Breaking)),
            "all three facts break"
        );
        // Change IDs are lc2_ fingerprints of the fixed derivation.
        for fact in &seam {
            assert!(fact.change_id().starts_with("lc2_"));
            assert_eq!(fact.change_id().len(), 4 + 64);
        }
        let layout_seam = seam
            .iter()
            .find(|f| f.change_subject() == record.0)
            .expect("layout fact in seam");
        let expected_layout = ids::change_id(&record, "record-layout-changed", "34", "35");
        assert_eq!(layout_seam.change_id(), expected_layout.0.as_str());
    }

    fn story_citation(memory: &str, claim: &str, elements: &[ElementId]) -> Citation {
        Citation {
            memory: MemoryRef {
                adapter: "muse-code".to_string(),
                id: memory.to_string(),
            },
            elements: elements.to_vec(),
            claim: claim.to_string(),
            recorded_by: "curator".to_string(),
            confidence: 0.9,
            source_text: claim.to_string(),
        }
    }

    /// The story's expected flag set through the seam: L2 flags on the
    /// layout change; L1, L3 and L4 stay silent.
    #[test]
    fn story_flags_only_the_record_memory() {
        let old = story_v1();
        let new = story_v2();
        let facts = old.diff_evidence(&new).expect("seam diff works");
        let el = |key: &str| ids::element_id(key);
        let citations = vec![
            story_citation(
                "L1",
                "1.5% late fee on the whole balance",
                &[
                    el("legacy:BILLBATCH:para:CALC-LATE-FEE"),
                    el("legacy:BILLBATCH:field:PATIENT-RECORD.BALANCE"),
                    el("legacy:BILLBATCH:para:LOG-EXEMPT"),
                    el("legacy:BILLBATCH:field:PATIENT-RECORD.VISIT-CODE"),
                ],
            ),
            story_citation(
                "L2",
                "each record is 34 chars",
                &[
                    el("legacy:BILLBATCH:record:PATIENT-RECORD"),
                    el("legacy:BILLBATCH:field:PATIENT-RECORD.ACCT-NO"),
                    el("legacy:BILLBATCH:field:PATIENT-RECORD.ACCT-NAME"),
                    el("legacy:BILLBATCH:field:PATIENT-RECORD.BALANCE"),
                    el("legacy:BILLBATCH:field:PATIENT-RECORD.VISIT-CODE"),
                ],
            ),
            story_citation(
                "L3",
                "CALC-LATE-FEE computes 1.5% of BALANCE",
                &[
                    el("legacy:BILLBATCH:para:CALC-LATE-FEE"),
                    el("legacy:BILLBATCH:field:PATIENT-RECORD.BALANCE"),
                ],
            ),
            story_citation(
                "L4",
                "prompt-payment accounts keep 10% of the fee",
                &[
                    el("legacy:BILLBATCH:para:APPLY-PROMPT-DISCOUNT"),
                    el("legacy:BILLBATCH:field:PATIENT-RECORD.DISCOUNT-FLAG"),
                ],
            ),
        ];
        let flags = flag_citations(&citations, &facts, &old, &[], "v1", "v2");
        assert_eq!(flags.len(), 1, "only L2 flags: {flags:?}");
        assert_eq!(flags[0].memory.id, "L2");
        assert_eq!(flags[0].severity, Severity::Breaking);
        assert_eq!(flags[0].changes.len(), 1, "only the layout fact");
    }

    // -- Synthetic change-kind coverage ------------------------------------
    //
    // A two-field program plus one file, small enough to vary by hand.

    const MINI_COPYBOOK: &str = "01 TEST-REC.\n    05 F1 PIC 9(6).\n    05 F2 PIC X(20).\n";
    const MINI_PROGRAM: &str = "IDENTIFICATION DIVISION.\nPROGRAM-ID. TESTPGM.\nENVIRONMENT DIVISION.\nINPUT-OUTPUT SECTION.\nFILE-CONTROL.\n    SELECT IN-FILE ASSIGN TO \"in.dat\"\n        ORGANIZATION IS LINE SEQUENTIAL.\nDATA DIVISION.\nFILE SECTION.\nFD IN-FILE.\nCOPY \"test.cpy\".\nPROCEDURE DIVISION.\nMAIN-PARA.\n    PERFORM WORK-PARA\n    STOP RUN.\nWORK-PARA.\n    COMPUTE X = Y * 0.015\n    ADD X TO TOTAL.\n";

    fn mini(program: &str, copybook: &str) -> LegacyIndex {
        LegacyIndex::parse(program, "TESTPGM.cbl", &[("test.cpy", copybook)]).expect("mini parses")
    }

    fn base_mini() -> LegacyIndex {
        mini(MINI_PROGRAM, MINI_COPYBOOK)
    }

    #[test]
    fn statement_change_keeps_the_subject_id() {
        let old = base_mini();
        let changed = MINI_PROGRAM.replace("0.015", "0.02");
        let new = mini(&changed, MINI_COPYBOOK);
        let facts = old.diff(&new);
        assert_eq!(facts.len(), 1, "one fact: {facts:?}");
        assert_eq!(facts[0].kind, LegacyChangeKind::ParagraphStatementsChanged);
        assert_eq!(facts[0].severity, LegacySeverity::Breaking);
        let work = ids::element_id(&ids::paragraph_key("TESTPGM", "WORK-PARA"));
        assert_eq!(facts[0].subject, work, "same ID after the edit");
        assert!(facts[0].old_attr.contains("0.015"));
        assert!(facts[0].new_attr.contains("0.02"));
        assert_eq!(old.diff_evidence(&new).expect("seam").len(), 1);
    }

    #[test]
    fn paragraph_removal_breaks_citers() {
        let old = base_mini();
        let pruned = MINI_PROGRAM.replace(
            "WORK-PARA.\n    COMPUTE X = Y * 0.015\n    ADD X TO TOTAL.\n",
            "",
        );
        let new = mini(&pruned, MINI_COPYBOOK);
        let facts = old.diff(&new);
        assert_eq!(facts.len(), 1, "one fact: {facts:?}");
        assert_eq!(facts[0].kind, LegacyChangeKind::ParagraphRemoved);
        assert_eq!(facts[0].severity, LegacySeverity::Breaking);
        let work = ids::element_id(&ids::paragraph_key("TESTPGM", "WORK-PARA"));
        assert_eq!(facts[0].subject, work);
    }

    #[test]
    fn pic_change_breaks_the_field_and_recomputes_the_layout() {
        let old = base_mini();
        // Widen F2: the record grows 26 -> 27 and F2's own fact fires.
        let wide = MINI_COPYBOOK.replace("05 F2 PIC X(20).", "05 F2 PIC X(21).");
        let new = mini(MINI_PROGRAM, &wide);
        let facts = old.diff(&new);
        assert_eq!(facts.len(), 2, "field fact plus layout fact: {facts:?}");
        let field = ids::element_id(&ids::field_key("TESTPGM", "TEST-REC.F2"));
        let record = ids::element_id(&ids::record_key("TESTPGM", "TEST-REC"));
        let pic = facts
            .iter()
            .find(|f| f.kind == LegacyChangeKind::FieldPicChanged)
            .expect("pic fact");
        assert_eq!(pic.subject, field);
        assert_eq!(
            (pic.old_attr.as_str(), pic.new_attr.as_str()),
            ("X(20)", "X(21)")
        );
        let layout = facts
            .iter()
            .find(|f| f.kind == LegacyChangeKind::RecordLayoutChanged)
            .expect("layout fact");
        assert_eq!(layout.subject, record);
        assert_eq!(
            (layout.old_attr.as_str(), layout.new_attr.as_str()),
            ("26", "27")
        );
    }

    #[test]
    fn same_length_pic_change_fires_no_layout_fact() {
        let old = base_mini();
        // 9(6) -> X(6): the same six characters, no offset moves.
        let same = MINI_COPYBOOK.replace("05 F1 PIC 9(6).", "05 F1 PIC X(6).");
        let new = mini(MINI_PROGRAM, &same);
        let facts = old.diff(&new);
        assert_eq!(facts.len(), 1, "only the field fact: {facts:?}");
        assert_eq!(facts[0].kind, LegacyChangeKind::FieldPicChanged);
    }

    #[test]
    fn add_silent_paragraph_flags_nothing() {
        let old = base_mini();
        let grown = format!("{MINI_PROGRAM}NEW-PARA.\n    DISPLAY \"NEW\".\n");
        let new = mini(&grown, MINI_COPYBOOK);
        let facts = old.diff(&new);
        assert_eq!(facts.len(), 1, "one fact: {facts:?}");
        assert_eq!(facts[0].kind, LegacyChangeKind::ParagraphAdded);
        assert_eq!(facts[0].severity, LegacySeverity::NonBreaking);
        assert!(
            old.diff_evidence(&new).expect("seam").is_empty(),
            "adds never reach the seam"
        );
    }

    #[test]
    fn rename_is_remove_plus_add_with_a_layout_change() {
        let old = base_mini();
        let renamed = MINI_COPYBOOK.replace("05 F1 PIC 9(6).", "05 F1X PIC 9(6).");
        let new = mini(MINI_PROGRAM, &renamed);
        let facts = old.diff(&new);
        assert_eq!(facts.len(), 3, "remove, add, layout: {facts:?}");
        let gone = ids::element_id(&ids::field_key("TESTPGM", "TEST-REC.F1"));
        let added = ids::element_id(&ids::field_key("TESTPGM", "TEST-REC.F1X"));
        assert_ne!(gone, added, "no rename threading through aliases");
        let kinds: Vec<LegacyChangeKind> = facts.iter().map(|f| f.kind.clone()).collect();
        assert!(kinds.contains(&LegacyChangeKind::FieldRemoved));
        assert!(kinds.contains(&LegacyChangeKind::FieldAdded));
        assert!(kinds.contains(&LegacyChangeKind::RecordLayoutChanged));
        assert_eq!(
            facts
                .iter()
                .find(|f| f.kind == LegacyChangeKind::FieldAdded)
                .expect("add")
                .severity,
            LegacySeverity::NonBreaking
        );
        let seam = old.diff_evidence(&new).expect("seam");
        assert_eq!(seam.len(), 2, "remove plus layout; the add is dropped");
    }

    #[test]
    fn file_assign_and_mode_changes_need_review() {
        let old = base_mini();
        let moved = MINI_PROGRAM.replace("ASSIGN TO \"in.dat\"", "ASSIGN TO \"other.dat\"");
        let new = mini(&moved, MINI_COPYBOOK);
        let facts = old.diff(&new);
        assert_eq!(facts.len(), 1, "one fact: {facts:?}");
        assert_eq!(facts[0].kind, LegacyChangeKind::FileAssignChanged);
        assert_eq!(facts[0].severity, LegacySeverity::Review);
        assert_eq!(
            (facts[0].old_attr.as_str(), facts[0].new_attr.as_str()),
            ("\"in.dat\"", "\"other.dat\"")
        );

        let reorg = MINI_PROGRAM.replace(
            "ORGANIZATION IS LINE SEQUENTIAL",
            "ORGANIZATION IS SEQUENTIAL",
        );
        let new = mini(&reorg, MINI_COPYBOOK);
        let facts = old.diff(&new);
        assert_eq!(facts.len(), 1, "one fact: {facts:?}");
        assert_eq!(facts[0].kind, LegacyChangeKind::FileOrganizationChanged);
        assert_eq!(facts[0].severity, LegacySeverity::Review);

        let seam = old.diff_evidence(&new).expect("seam");
        assert_eq!(seam.len(), 1);
        assert_eq!(seam[0].change_severity(), Some(Severity::Review));

        // Dropping the file removes it: breaking, like a paragraph removal.
        let nofile = MINI_PROGRAM.replace(
            "    SELECT IN-FILE ASSIGN TO \"in.dat\"\n        ORGANIZATION IS LINE SEQUENTIAL.\n",
            "",
        );
        let new = mini(&nofile, MINI_COPYBOOK);
        let facts = old.diff(&new);
        assert_eq!(facts.len(), 1, "one fact: {facts:?}");
        assert_eq!(facts[0].kind, LegacyChangeKind::FileRemoved);
        assert_eq!(facts[0].severity, LegacySeverity::Breaking);
    }

    #[test]
    fn reformatting_alone_changes_nothing() {
        let old = base_mini();
        let reformatted = MINI_PROGRAM
            .replace(
                "    COMPUTE X = Y * 0.015",
                "         compute   x   =   y * 0.015",
            )
            .replace("WORK-PARA.", "WORK-PARA. *> the worker");
        let new = mini(&reformatted, MINI_COPYBOOK);
        assert!(
            old.diff(&new).is_empty(),
            "case, spacing and comments are not changes"
        );
    }

    #[test]
    fn sections_add_remove_and_change() {
        let sec_v1 = "IDENTIFICATION DIVISION.\nPROGRAM-ID. TESTPGM.\nENVIRONMENT DIVISION.\nDATA DIVISION.\nPROCEDURE DIVISION.\nMY-SEC SECTION.\n    DISPLAY \"ONE\".\nFIRST-PARA.\n    DISPLAY \"HI\".\n";
        let sec_v2 = "IDENTIFICATION DIVISION.\nPROGRAM-ID. TESTPGM.\nENVIRONMENT DIVISION.\nDATA DIVISION.\nPROCEDURE DIVISION.\nMY-SEC SECTION.\n    DISPLAY \"TWO\".\nFIRST-PARA.\n    DISPLAY \"HI\".\n";
        let empty = "01 TEST-REC.\n    05 F1 PIC X.\n";
        let old = mini(sec_v1, empty);
        let new = mini(sec_v2, empty);
        // The section element exists and the body change fires.
        let section = ids::element_id(&ids::section_key("TESTPGM", "MY-SEC"));
        assert!(old.legacy_element(&section).is_some());
        let facts = old.diff(&new);
        assert_eq!(facts.len(), 1, "one fact: {facts:?}");
        assert_eq!(facts[0].kind, LegacyChangeKind::SectionStatementsChanged);
        assert_eq!(facts[0].severity, LegacySeverity::Breaking);

        let nosec = "IDENTIFICATION DIVISION.\nPROGRAM-ID. TESTPGM.\nENVIRONMENT DIVISION.\nDATA DIVISION.\nPROCEDURE DIVISION.\nFIRST-PARA.\n    DISPLAY \"HI\".\n";
        let new = mini(nosec, empty);
        let facts = old.diff(&new);
        let kinds: Vec<LegacyChangeKind> = facts.iter().map(|f| f.kind.clone()).collect();
        assert!(
            kinds.contains(&LegacyChangeKind::SectionRemoved),
            "{kinds:?}"
        );
        assert!(!kinds.contains(&LegacyChangeKind::SectionAdded));
        // The reverse direction adds the section: non-breaking, seam-silent.
        let facts = new.diff(&old);
        assert_eq!(facts.len(), 1, "one fact: {facts:?}");
        assert_eq!(facts[0].kind, LegacyChangeKind::SectionAdded);
        assert_eq!(facts[0].severity, LegacySeverity::NonBreaking);
    }

    #[test]
    fn record_removal_takes_its_fields_with_it() {
        let two = "01 TEST-REC.\n    05 F1 PIC 9(6).\n    05 F2 PIC X(20).\n01 GONE-REC.\n    05 G1 PIC 9(4).\n";
        let one = "01 TEST-REC.\n    05 F1 PIC 9(6).\n    05 F2 PIC X(20).\n";
        let prog = "IDENTIFICATION DIVISION.\nPROGRAM-ID. TESTPGM.\nENVIRONMENT DIVISION.\nDATA DIVISION.\nFILE SECTION.\nCOPY \"test.cpy\".\nPROCEDURE DIVISION.\nMAIN-PARA.\n    STOP RUN.\n";
        let old = mini(prog, two);
        let new = mini(prog, one);
        assert_eq!(
            old.elements().len(),
            7,
            "program, paragraph, two records, three fields"
        );
        let facts = old.diff(&new);
        let kinds: Vec<LegacyChangeKind> = facts.iter().map(|f| f.kind.clone()).collect();
        assert_eq!(facts.len(), 2, "record plus its field: {facts:?}");
        assert!(kinds.contains(&LegacyChangeKind::RecordRemoved));
        assert!(kinds.contains(&LegacyChangeKind::FieldRemoved));
        assert!(
            facts.iter().all(|f| f.severity == LegacySeverity::Breaking),
            "removals break"
        );
    }

    #[test]
    fn program_rename_is_a_removal() {
        let old = base_mini();
        let renamed = MINI_PROGRAM.replace("PROGRAM-ID. TESTPGM.", "PROGRAM-ID. OTHERPGM.");
        let new = mini(&renamed, MINI_COPYBOOK);
        let facts = old.diff(&new);
        assert_eq!(facts.len(), 1, "one fact: {facts:?}");
        assert_eq!(facts[0].kind, LegacyChangeKind::ProgramRemoved);
        assert_eq!(facts[0].severity, LegacySeverity::Breaking);
    }

    // -- Guard boundary: unknown versions and unknown prefixes -------------
    //
    // Decide-level multi-source loading waits for the screens and
    // differential-run sources, so the guard still loads its versions as
    // contracts. These tests pin what that means: a legacy flag's versions
    // fail to load and the action is allowed with the one warning, and
    // `le2_` elements under loadable versions resolve to nothing and allow
    // with the one warning. Both run the real binary the gate builds.

    fn bin_path() -> String {
        format!("{}/target/debug/doyle", env!("CARGO_MANIFEST_DIR"))
    }

    fn temp_state_dir(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static CTR: AtomicU64 = AtomicU64::new(0);
        let n = CTR.fetch_add(1, Ordering::Relaxed);
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "doyle-legacy-guard-{}-{}-{}",
            std::process::id(),
            tag,
            n
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn seed_legacy_flag(
        dir: &std::path::Path,
        old_version: &str,
        new_version: &str,
        elements: &[&str],
    ) {
        use crate::core::{Flag, Store};
        let mut store = Store::open(dir).unwrap();
        let memory = MemoryRef {
            adapter: "muse-code".to_string(),
            id: "L2".to_string(),
        };
        store
            .record_citation(Citation {
                memory: memory.clone(),
                elements: elements
                    .iter()
                    .map(|e| ElementId((*e).to_string()))
                    .collect(),
                claim: "each record is 34 chars".to_string(),
                recorded_by: "curator".to_string(),
                confidence: 0.9,
                source_text: "each record is 34 chars".to_string(),
            })
            .unwrap();
        store
            .record_flag(Flag {
                id: String::new(),
                memory,
                changes: vec![crate::core::ChangeId(
                    "lc2_test0000000000000000000000000000000000000000000000000000000001"
                        .to_string(),
                )],
                severity: Severity::Breaking,
                old_version: old_version.to_string(),
                new_version: new_version.to_string(),
                evidence: Some("each record is 34 chars".to_string()),
            })
            .unwrap();
    }

    fn run_guard(state_dir: &std::path::Path, tool_input: &str) -> std::process::Output {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let state = state_dir.to_string_lossy().to_string();
        let mut child = Command::new(bin_path())
            .args(["guard", "--state-dir", &state])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn doyle");
        let hook = format!(
            "{{\"cwd\":\"/tmp/scratch/repo-a\",\"hook_event_name\":\"PreToolUse\",\"tool_input\":{tool_input},\"tool_name\":\"write_file\"}}"
        );
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(hook.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    #[test]
    fn legacy_flags_hold_no_free_text_step() {
        // T9.15: real .cbl versions no longer match a cited paragraph named
        // in free text. Reading or shipping notes that name it is allowed,
        // silently: the differential run guards the rewrite instead.
        let calc = ids::element_id(&ids::paragraph_key("BILLBATCH", "CALC-LATE-FEE")).0;
        let old = std::path::absolute(fixture("legacy/story/v1/BILLBATCH.cbl"))
            .unwrap()
            .to_string_lossy()
            .to_string();
        let new = std::path::absolute(fixture("legacy/story/v2/BILLBATCH.cbl"))
            .unwrap()
            .to_string_lossy()
            .to_string();
        let missing = "/nonexistent/BILLBATCH.cbl".to_string();
        for (tag, old, new) in [("versions", &old, &new), ("unloadable", &missing, &missing)] {
            let dir = temp_state_dir(tag);
            seed_legacy_flag(&dir, old, new, &[&calc]);
            let out = run_guard(
                &dir,
                "{\"content\":\"ship CALC-LATE-FEE now\",\"path\":\"notes.txt\"}",
            );
            assert!(out.status.success(), "allow exits 0 ({tag})");
            assert!(
                String::from_utf8_lossy(&out.stdout).is_empty(),
                "allow is silent on stdout ({tag})"
            );
            assert!(
                String::from_utf8_lossy(&out.stderr).is_empty(),
                "no warning for a legacy flag ({tag}): {}",
                String::from_utf8_lossy(&out.stderr)
            );
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn unknown_prefix_elements_allow_with_a_warning() {
        let dir = temp_state_dir("prefix");
        // Loadable contract versions, but the citation is a legacy element
        // the contract index can never resolve.
        let old = std::path::absolute(fixture("contracts/patients-v1.yaml"))
            .unwrap()
            .to_string_lossy()
            .to_string();
        let new = std::path::absolute(fixture("contracts/patients-v2.yaml"))
            .unwrap()
            .to_string_lossy()
            .to_string();
        let record = ids::record_key("BILLBATCH", "PATIENT-RECORD");
        let record_id = ids::element_id(&record).0;
        seed_legacy_flag(&dir, &old, &new, &[&record_id]);
        let out = run_guard(
            &dir,
            "{\"content\":\"the PATIENT-RECORD layout grew\",\"path\":\"notes.txt\"}",
        );
        assert!(out.status.success(), "allow exits 0");
        assert!(
            String::from_utf8_lossy(&out.stdout).is_empty(),
            "allow is silent on stdout"
        );
        let err = String::from_utf8_lossy(&out.stderr).to_string();
        assert!(
            err.contains("warning") && err.contains("allowing"),
            "one warning, fail-open: {err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
