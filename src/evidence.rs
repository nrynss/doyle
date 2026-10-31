//! Doyle's evidence-source seam: the trait every evidence source plugs into.
//!
//! A memory rests on evidence, and an API contract is one kind (product.md,
//! Beyond the spec). This module is the seam behind flagging: index a
//! version into elements (ID, kind, parent, references, native location),
//! and diff two versions into change facts (ID, subject, kind, severity).
//! [`flag_citations`](crate::core::flag_citations) runs over
//! [`EvidenceSource`] and [`ChangeFact`], and knows no particular source.
//! Vimanam is the first implementation, in `src/contract.rs`, with no change
//! in behaviour; the legacy and screen sources plug in beside it (T9.6,
//! T9.7).
//!
//! Element and change IDs carry their source in a prefix (`ve2_`/`vc2_`
//! stay Vimanam's, `le2_`/`lc2_` the legacy source's), so [`ElementId`] and
//! [`ChangeId`] keep their shape and nothing in `testdata/` changes.
//! Non-breaking facts are never written: a source reports only what can
//! flag, and [`ChangeFact::change_severity`] returns `None` for anything
//! that flags nothing.

pub mod diffrun;
pub mod legacy;
pub mod screens;

use crate::contract::ContractError;
use crate::core::{ChangeId, ElementId, Severity};

/// An element's native location in its source's own terms: the HTTP method
/// and path, the operation ID, and the named type and field. For display and
/// for the guard's match. Fields that don't apply to an element are `None`.
///
/// Shared by every source: Vimanam fills the HTTP slots; a legacy paragraph
/// fills the operation-ID slot with its name, a record the type-name slot.
/// The guard matches method and path, operation ID and type name, never a
/// field name, whatever source served the location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeLocation {
    /// HTTP method (`GET`), when the element is operation-scoped.
    pub method: Option<String>,
    /// Exact path template (`/patients`), when operation-scoped.
    pub path: Option<String>,
    /// Operation or paragraph name (`listPatients`), for operations and
    /// their children, or a paragraph in a legacy source.
    pub operation_id: Option<String>,
    /// Named type or record (`Patient`), for types and their fields.
    pub type_name: Option<String>,
    /// Field name within its type, for fields.
    pub field_name: Option<String>,
}

/// What an element is, in source-neutral terms. Vimanam's operations, types,
/// fields, arguments and results map directly; anything else (a COBOL
/// program, a screen, a video step) is [`EvidenceElementKind::Other`] with
/// the source's own name. Only [`EvidenceElementKind::Type`] is a shared
/// named type: a change to one reaches the elements that reference it (the
/// propagation rule in `src/core/flag.rs`). Anything Vimanam adds to its own
/// vocabulary later lands in `Other`, so the seam never breaks the build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceElementKind {
    Operation,
    Type,
    Field,
    Argument,
    Result,
    Other(String),
}

impl EvidenceElementKind {
    /// Whether this kind is a shared named type, whose changes reach its
    /// referencers.
    pub fn is_shared_type(&self) -> bool {
        matches!(self, Self::Type)
    }
}

impl From<&vimanam::contract::EntityKind> for EvidenceElementKind {
    fn from(kind: &vimanam::contract::EntityKind) -> Self {
        match kind {
            vimanam::contract::EntityKind::Operation => Self::Operation,
            vimanam::contract::EntityKind::Type => Self::Type,
            vimanam::contract::EntityKind::Field => Self::Field,
            vimanam::contract::EntityKind::Argument => Self::Argument,
            vimanam::contract::EntityKind::Result => Self::Result,
            // Service, Value, and anything Vimanam adds later.
            other => Self::Other(format!("{other:?}")),
        }
    }
}

/// One indexed element: what flagging and the guard read of a version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceElement {
    /// The source's stable ID (`ve2_…`, `le2_…`).
    pub id: ElementId,
    /// What the element is.
    pub kind: EvidenceElementKind,
    /// The containing element, absent for roots.
    pub parent: Option<ElementId>,
    /// Named types this element references (shared `$ref`s).
    pub references: Vec<ElementId>,
    /// The element's location in its source's own terms.
    pub native: NativeLocation,
}

/// What a change fact did, in source-neutral terms. Only the distinction
/// flagging needs is first-class (an added field on a shared type reads as
/// additive and flags nothing); everything else rides in `Other` under the
/// source's own name, so a source's richer vocabulary never breaks the seam.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceChangeKind {
    FieldAdded,
    Other(String),
}

impl From<&vimanam::contract::ChangeKind> for EvidenceChangeKind {
    fn from(kind: &vimanam::contract::ChangeKind) -> Self {
        match kind {
            vimanam::contract::ChangeKind::FieldAdded => Self::FieldAdded,
            other => Self::Other(format!("{other:?}")),
        }
    }
}

/// One change fact between two versions: what flagging cites.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceChange {
    /// The source's change ID (`vc2_…`, `lc2_…`).
    pub id: ChangeId,
    /// The changed element.
    pub subject: ElementId,
    /// What changed.
    pub kind: EvidenceChangeKind,
    /// Doyle's severity. Non-breaking facts are never written, so this is
    /// always flag-worthy.
    pub severity: Severity,
}

/// A change fact from any source, as flagging reads it.
pub trait ChangeFact {
    /// The fact's ID.
    fn change_id(&self) -> &str;
    /// The changed element's ID.
    fn change_subject(&self) -> &str;
    /// Doyle's severity for this fact, or `None` when it flags nothing.
    /// Non-breaking is unrepresentable in [`Severity`], so it never flags.
    fn change_severity(&self) -> Option<Severity>;
    /// Whether this fact adds a field to its subject.
    fn is_field_added(&self) -> bool;
}

impl ChangeFact for vimanam::contract::Change {
    fn change_id(&self) -> &str {
        &self.id
    }

    fn change_subject(&self) -> &str {
        &self.subject
    }

    fn change_severity(&self) -> Option<Severity> {
        match self.severity {
            vimanam::contract::Severity::Breaking => Some(Severity::Breaking),
            vimanam::contract::Severity::Review => Some(Severity::Review),
            // NonBreaking, and any severity Vimanam adds later: flags nothing.
            _ => None,
        }
    }

    fn is_field_added(&self) -> bool {
        matches!(self.kind, vimanam::contract::ChangeKind::FieldAdded)
    }
}

impl ChangeFact for EvidenceChange {
    fn change_id(&self) -> &str {
        &self.id.0
    }

    fn change_subject(&self) -> &str {
        &self.subject.0
    }

    fn change_severity(&self) -> Option<Severity> {
        Some(self.severity)
    }

    fn is_field_added(&self) -> bool {
        matches!(self.kind, EvidenceChangeKind::FieldAdded)
    }
}

/// An evidence source: one version indexed for flagging, plus the diff to
/// the next version. Vimanam implements this in `src/contract.rs`; the
/// legacy (T9.6) and screen (T9.7) sources implement it beside Vimanam.
pub trait EvidenceSource {
    /// The element with this ID, if this version holds one.
    fn element(&self, id: &ElementId) -> Option<EvidenceElement>;

    /// Every element in this version, in the source's native order.
    fn elements(&self) -> Vec<EvidenceElement>;

    /// The element and its ancestors, nearest first. Empty when the ID is
    /// unknown.
    fn parent_chain(&self, id: &ElementId) -> Vec<EvidenceElement>;

    /// Every element that references this one, in native order.
    fn referencers(&self, id: &ElementId) -> Vec<EvidenceElement>;

    /// The element's native location for display and the guard. `None` when
    /// the ID is unknown.
    fn native_location(&self, id: &ElementId) -> Option<NativeLocation>;

    /// Diff this index (old) against another (new): the change facts a flag
    /// can cite, in the source's order. Non-breaking facts are never
    /// written: additive changes flag nothing, so the seam drops them here.
    fn diff_evidence(&self, new: &Self) -> Result<Vec<EvidenceChange>, ContractError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Citation, MemoryRef, flag_citations};

    // Real legacy IDs from docs/legacy-ids.md: the fake source honours the
    // scheme (le2_ plus the SHA-256 of the canonical key) without a parser.
    const PROGRAM: &str = "le2_be3caaf70f3125fa6cc719da40eb9b356e258afe83ba1527943e65867dead079";
    const CALC_LATE_FEE: &str =
        "le2_3422f89619e902b31f7fe202a2fc9a05701bbc7ae83ba09cf366989f5e95f749e";
    const WRITE_DETAIL: &str = "le2_3e24136be0c79237bb6fd791ab71a3cb871e1d0e7cbb0c4e3a9fc41b6b867b";
    const PATIENT_RECORD: &str =
        "le2_690243c98922cdcded58beed19f46cb8725f969039b9f8e1c297cc465c4f21533";
    const BALANCE: &str = "le2_115161c1611668d048b209f67556c356ed6396a74c05ecd39bf85c515909ce2";
    const EXEMPT_FLAG: &str = "le2_72c916279d8685a43a018ce5914b790b00816441abfd7e4444ace1cc15cc0f9";

    // Vimanam IDs from testdata/vimanam/README.md, for the behind-the-trait
    // check on the demo fixtures.
    const LIST_PATIENTS: &str =
        "ve2_c81e37b7c42c484a1859c09e95bf8cec89a360db6743a691afab818f177c7bc6";
    const LIST_PATIENTS_200: &str =
        "ve2_86d3cdb7ce3a7f0b1a2fefa696f0acbbcf2d5406c717bb1f1f1cb5f4f62667ba";
    const M1_BREAKING: &str =
        "vc2_9bedbbb47825fa87ea41aa38342d0492948286a7dd4a5ecbb378310261df6841";
    const M1_CONSTRAINT: &str =
        "vc2_d7e1b2acbebe45b089106951b949710e3112d1586ed74cfe5fab47d702987277";
    const M1_PAGINATION: &str =
        "vc2_d82adaee7e47799b8027d3c1b9f458e16be8a20df1696186f351369f07e011e0";

    fn location(
        method: Option<&str>,
        path: Option<&str>,
        operation_id: Option<&str>,
        type_name: Option<&str>,
        field_name: Option<&str>,
    ) -> NativeLocation {
        NativeLocation {
            method: method.map(str::to_string),
            path: path.map(str::to_string),
            operation_id: operation_id.map(str::to_string),
            type_name: type_name.map(str::to_string),
            field_name: field_name.map(str::to_string),
        }
    }

    /// A second source beside Vimanam: legacy-flavoured elements with `le2_`
    /// IDs. Paragraphs read like operations (a name the guard can match),
    /// the copybook record reads like a shared named type, fields hang
    /// under it. The native-location mapping here is illustrative for the
    /// seam test; T9.6 decides the real one.
    struct FakeSource {
        elements: Vec<EvidenceElement>,
    }

    impl FakeSource {
        fn legacy_like() -> Self {
            let element = |id: &str,
                           kind: EvidenceElementKind,
                           parent: Option<&str>,
                           references: &[&str],
                           native: NativeLocation| {
                EvidenceElement {
                    id: ElementId(id.to_string()),
                    kind,
                    parent: parent.map(|p| ElementId(p.to_string())),
                    references: references
                        .iter()
                        .map(|r| ElementId((*r).to_string()))
                        .collect(),
                    native,
                }
            };
            Self {
                elements: vec![
                    element(
                        PROGRAM,
                        EvidenceElementKind::Other("program".to_string()),
                        None,
                        &[],
                        location(None, None, None, Some("BILLBATCH"), None),
                    ),
                    element(
                        CALC_LATE_FEE,
                        EvidenceElementKind::Operation,
                        Some(PROGRAM),
                        &[],
                        location(None, None, Some("CALC-LATE-FEE"), None, None),
                    ),
                    element(
                        WRITE_DETAIL,
                        EvidenceElementKind::Operation,
                        Some(PROGRAM),
                        &[PATIENT_RECORD],
                        location(None, None, Some("WRITE-DETAIL"), None, None),
                    ),
                    element(
                        PATIENT_RECORD,
                        EvidenceElementKind::Type,
                        Some(PROGRAM),
                        &[],
                        location(None, None, None, Some("PATIENT-RECORD"), None),
                    ),
                    element(
                        BALANCE,
                        EvidenceElementKind::Field,
                        Some(PATIENT_RECORD),
                        &[],
                        location(None, None, None, Some("PATIENT-RECORD"), Some("BALANCE")),
                    ),
                    element(
                        EXEMPT_FLAG,
                        EvidenceElementKind::Field,
                        Some(PATIENT_RECORD),
                        &[],
                        location(
                            None,
                            None,
                            None,
                            Some("PATIENT-RECORD"),
                            Some("EXEMPT-FLAG"),
                        ),
                    ),
                ],
            }
        }
    }

    impl EvidenceSource for FakeSource {
        fn element(&self, id: &ElementId) -> Option<EvidenceElement> {
            self.elements.iter().find(|e| e.id == *id).cloned()
        }

        fn elements(&self) -> Vec<EvidenceElement> {
            self.elements.clone()
        }

        fn parent_chain(&self, id: &ElementId) -> Vec<EvidenceElement> {
            let mut chain = Vec::new();
            let mut next = self.element(id);
            while let Some(current) = next {
                if chain.len() > self.elements.len() {
                    break;
                }
                next = current.parent.as_ref().and_then(|p| self.element(p));
                chain.push(current);
            }
            chain
        }

        fn referencers(&self, id: &ElementId) -> Vec<EvidenceElement> {
            self.elements
                .iter()
                .filter(|e| e.references.iter().any(|r| r == id))
                .cloned()
                .collect()
        }

        fn native_location(&self, id: &ElementId) -> Option<NativeLocation> {
            self.element(id).map(|e| e.native)
        }

        fn diff_evidence(&self, _new: &Self) -> Result<Vec<EvidenceChange>, ContractError> {
            Ok(Vec::new())
        }
    }

    fn fake_change(
        id: &str,
        subject: &str,
        kind: EvidenceChangeKind,
        severity: Severity,
    ) -> EvidenceChange {
        EvidenceChange {
            id: ChangeId(id.to_string()),
            subject: ElementId(subject.to_string()),
            kind,
            severity,
        }
    }

    fn fake_citation(memory: &str, elements: &[&str]) -> Citation {
        Citation {
            memory: MemoryRef {
                adapter: "muse-code".to_string(),
                id: memory.to_string(),
            },
            elements: elements
                .iter()
                .map(|e| ElementId((*e).to_string()))
                .collect(),
            claim: format!("memory {memory}"),
            recorded_by: "curator".to_string(),
            confidence: 0.9,
            source_text: format!("memory {memory}"),
        }
    }

    fn fixture(name: &str) -> String {
        format!("{}/testdata/{name}", env!("CARGO_MANIFEST_DIR"))
    }

    #[test]
    fn kinds_map_from_vimanam() {
        use vimanam::contract::EntityKind;
        assert!(!EvidenceElementKind::from(&EntityKind::Operation).is_shared_type());
        assert!(EvidenceElementKind::from(&EntityKind::Type).is_shared_type());
        assert!(!EvidenceElementKind::from(&EntityKind::Field).is_shared_type());
        assert!(!EvidenceElementKind::from(&EntityKind::Argument).is_shared_type());
        assert!(!EvidenceElementKind::from(&EntityKind::Result).is_shared_type());
        // Service, Value, and anything added later: carried, never a type.
        for kind in [EntityKind::Service, EntityKind::Value] {
            let mapped = EvidenceElementKind::from(&kind);
            assert!(!mapped.is_shared_type(), "{kind:?} maps to {mapped:?}");
            assert!(matches!(mapped, EvidenceElementKind::Other(_)));
        }
    }

    #[test]
    fn change_kinds_map_from_vimanam() {
        use vimanam::contract::ChangeKind;
        assert!(matches!(
            EvidenceChangeKind::from(&ChangeKind::FieldAdded),
            EvidenceChangeKind::FieldAdded
        ));
        assert!(matches!(
            EvidenceChangeKind::from(&ChangeKind::PaginationChanged),
            EvidenceChangeKind::Other(_)
        ));
    }

    #[test]
    fn fake_source_flags_through_the_same_path() {
        let old = FakeSource::legacy_like();
        // A breaking statement change on CALC-LATE-FEE: the paragraph
        // itself and its program ancestor flag; the other paragraph and
        // the exempt flag stay quiet.
        let changes = vec![fake_change(
            "lc2_test00000000000000000000000000000001",
            CALC_LATE_FEE,
            EvidenceChangeKind::Other("statement_changed".to_string()),
            Severity::Breaking,
        )];
        let citations = vec![
            fake_citation("M1", &[CALC_LATE_FEE]),
            fake_citation("M2", &[PROGRAM]),
            fake_citation("M3", &[WRITE_DETAIL]),
            fake_citation("M4", &[EXEMPT_FLAG]),
        ];
        let flags = flag_citations(&citations, &changes, &old, &[], "v1", "v2");
        let flagged: Vec<&str> = flags.iter().map(|f| f.memory.id.as_str()).collect();
        assert_eq!(flagged, ["M1", "M2"], "{flags:?}");
        for flag in &flags {
            assert_eq!(flag.severity, Severity::Breaking);
            assert_eq!(flag.changes.len(), 1);
        }
    }

    #[test]
    fn fake_shared_type_change_reaches_referencers_only() {
        let old = FakeSource::legacy_like();
        // A breaking change to the PATIENT-RECORD record: citers of the
        // record, of the program above it, and of WRITE-DETAIL (which
        // references the record) flag. Citing a member field is downward
        // and stays quiet, as does the unrelated paragraph.
        let changes = vec![fake_change(
            "lc2_test00000000000000000000000000000002",
            PATIENT_RECORD,
            EvidenceChangeKind::Other("layout_changed".to_string()),
            Severity::Breaking,
        )];
        let citations = vec![
            fake_citation("record-citer", &[PATIENT_RECORD]),
            fake_citation("program-citer", &[PROGRAM]),
            fake_citation("writer-citer", &[WRITE_DETAIL]),
            fake_citation("field-citer", &[BALANCE]),
            fake_citation("calc-citer", &[CALC_LATE_FEE]),
        ];
        let flags = flag_citations(&citations, &changes, &old, &[], "v1", "v2");
        let flagged: Vec<&str> = flags.iter().map(|f| f.memory.id.as_str()).collect();
        assert_eq!(
            flagged,
            ["record-citer", "program-citer", "writer-citer"],
            "{flags:?}"
        );
    }

    #[test]
    fn fake_type_level_field_added_review_flags_nothing() {
        let old = FakeSource::legacy_like();
        // The additive case, mirroring the Stats generated_at review:
        // FieldAdded on the record itself still flags nothing, not even
        // direct citers of the record.
        let changes = vec![fake_change(
            "lc2_test00000000000000000000000000000003",
            PATIENT_RECORD,
            EvidenceChangeKind::FieldAdded,
            Severity::Review,
        )];
        let citations = vec![
            fake_citation("record-citer", &[PATIENT_RECORD]),
            fake_citation("field-citer", &[BALANCE]),
            fake_citation("writer-citer", &[WRITE_DETAIL]),
        ];
        let flags = flag_citations(&citations, &changes, &old, &[], "v1", "v2");
        assert!(flags.is_empty(), "additive changes flag nothing: {flags:?}");
    }

    #[test]
    fn fake_review_only_flag_is_marked_review() {
        let old = FakeSource::legacy_like();
        let changes = vec![fake_change(
            "lc2_test00000000000000000000000000000004",
            CALC_LATE_FEE,
            EvidenceChangeKind::Other("comment_changed".to_string()),
            Severity::Review,
        )];
        let citations = vec![fake_citation("M1", &[CALC_LATE_FEE])];
        let flags = flag_citations(&citations, &changes, &old, &[], "v1", "v2");
        assert_eq!(flags.len(), 1);
        assert_eq!(flags[0].severity, Severity::Review);
    }

    #[test]
    fn non_vimanam_native_locations_resolve_through_the_trait() {
        let old = FakeSource::legacy_like();
        // The guard matches method and path, operation ID and type name,
        // never a field name. A non-Vimanam source serves the same fields
        // through the same query, so the guard reads them unchanged.
        let calc = old
            .native_location(&ElementId(CALC_LATE_FEE.to_string()))
            .expect("paragraph resolves");
        assert_eq!(calc.operation_id.as_deref(), Some("CALC-LATE-FEE"));
        assert_eq!(
            (calc.method, calc.path, calc.type_name, calc.field_name),
            (None, None, None, None)
        );

        let balance = old
            .native_location(&ElementId(BALANCE.to_string()))
            .expect("field resolves");
        assert_eq!(balance.type_name.as_deref(), Some("PATIENT-RECORD"));
        assert_eq!(balance.field_name.as_deref(), Some("BALANCE"));

        let program = old
            .native_location(&ElementId(PROGRAM.to_string()))
            .expect("program resolves");
        assert_eq!(program.type_name.as_deref(), Some("BILLBATCH"));

        // Unknown IDs give nothing, whatever the prefix.
        assert!(
            old.native_location(&ElementId("le2_nonexistent".to_string()))
                .is_none()
        );
    }

    #[test]
    fn vimanam_diff_evidence_keeps_facts_and_drops_additive() {
        use crate::contract::ContractIndex;
        let old = ContractIndex::load(fixture("contracts/patients-v1.yaml")).expect("v1 loads");
        let new = ContractIndex::load(fixture("contracts/patients-v2.yaml")).expect("v2 loads");
        let raw = old.diff(&new).expect("diff works");
        assert_eq!(raw.len(), 11);
        let facts = old.diff_evidence(&new).expect("seam diff works");
        // The 7 non-breaking facts are never written; the 4 review-or-worse
        // facts stay in Vimanam's order with their IDs and subjects.
        let raw_kept: Vec<&str> = raw
            .iter()
            .filter(|c| {
                c.severity == vimanam::contract::Severity::Breaking
                    || c.severity == vimanam::contract::Severity::Review
            })
            .map(|c| c.id.as_str())
            .collect();
        let ours: Vec<&str> = facts.iter().map(|f| f.change_id()).collect();
        assert_eq!(ours, raw_kept);
        assert_eq!(ours.len(), 4);
        for (id, subject) in [
            (M1_BREAKING, LIST_PATIENTS_200),
            (M1_CONSTRAINT, LIST_PATIENTS_200),
            (M1_PAGINATION, LIST_PATIENTS),
        ] {
            let fact = facts
                .iter()
                .find(|f| f.change_id() == id)
                .expect("M1 fact kept");
            assert_eq!(fact.change_subject(), subject);
            assert!(fact.change_severity().is_some());
        }
    }

    #[test]
    fn vimanam_behind_the_trait_flags_the_demo() {
        use crate::contract::ContractIndex;
        let old = ContractIndex::load(fixture("contracts/patients-v1.yaml")).expect("v1 loads");
        let new = ContractIndex::load(fixture("contracts/patients-v2.yaml")).expect("v2 loads");
        // The same M1 memory through the seam's facts, not Vimanam's type:
        // one flag, M1's three change IDs, breaking.
        let facts = old.diff_evidence(&new).expect("seam diff works");
        let citations = vec![fake_citation("M1", &[LIST_PATIENTS, LIST_PATIENTS_200])];
        let flags = flag_citations(&citations, &facts, &old, &[], "v1", "v2");
        assert_eq!(flags.len(), 1, "{flags:?}");
        assert_eq!(flags[0].memory.id, "M1");
        let ids: Vec<&str> = flags[0].changes.iter().map(|c| c.0.as_str()).collect();
        assert_eq!(ids, [M1_CONSTRAINT, M1_BREAKING, M1_PAGINATION]);
        assert_eq!(flags[0].severity, Severity::Breaking);
    }
}
