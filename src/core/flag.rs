//! Flagging: from citations and Vimanam change facts to flags.
//!
//! Pure computation, no model involved. For each change, every memory whose
//! citation touches the change is flagged: one flag per memory per check,
//! listing every change behind it. A memory with an open flag (already
//! flagged and not yet decided) is not flagged twice.
//!
//! Propagation (decided in T2.2; the rules are written under the task in
//! `dev-diary/PHASE-2-flagging.md`). A change to entity S flags a memory
//! citing element C when:
//!
//! 1. **Subject.** C is S itself.
//! 2. **Ancestors.** C is an ancestor of S (the entity itself first, then up
//!    the parent chain). A response change flags citers of its operation; a
//!    field change flags citers of its type.
//! 3. **Shared types, then up.** When S is a named type, or a member of one
//!    (a field whose parent chain holds a type), C is a referencer of that
//!    type, or an ancestor of such a referencer. A change to a shared type
//!    reaches memories citing the operations that use it (the operation is
//!    the parent of the response that references the type), but the hop is
//!    single: referencers of referencers are not followed, so no other
//!    operation in the service is flagged.
//!
//! Operation-scoped subjects (operations, arguments, results and their
//! inline fields) never expand through the types they reference: a change to
//! one response does not flag memories citing another operation through a
//! shared `$ref`. Propagation is upward-only; citing a descendant of S
//! never flags.
//!
//! Severity: Vimanam's `breaking` flags, `review` flags marked for review,
//! `non_breaking` is filtered before it can flag (Doyle's [`Severity`] cannot
//! represent it). One exception: a type-level `review` for an added field is
//! treated as additive and flags nothing (see [`is_additive_type_review`]).

use std::collections::{BTreeMap, BTreeSet};

use vimanam::contract::{Change, ChangeKind, EntityKind, Severity as VimanamSeverity};

use super::model::{ChangeId, Citation, ElementId, Flag, Severity};
use crate::contract::ContractIndex;

/// Flag the cited memories a set of changes has made stale.
///
/// * `citations` — what each memory rests on (element IDs are Vimanam's
///   stable `ve2_…` strings, so a citation matches a change subject by
///   string equality even when the index knows neither).
/// * `changes` — Vimanam's facts, in Vimanam's order; kept as given.
/// * `old` — the index of the contract the citations rest on. Subjects
///   unknown to it still match direct citations; ancestor and referencer
///   enrichment just finds nothing for them.
/// * `open_flags` — the store's open flags. A memory with an open flag is
///   skipped: flagged and not yet decided means not flagged twice.
/// * `old_version` / `new_version` — the contract versions either side of
///   the change, recorded on each flag.
///
/// Returns one [`Flag`] per newly flagged memory, in first-citation order.
/// [`Flag::id`] is left empty: the store assigns `flag-N` on write.
pub fn flag_citations(
    citations: &[Citation],
    changes: &[Change],
    old: &ContractIndex,
    open_flags: &[Flag],
    old_version: &str,
    new_version: &str,
) -> Vec<Flag> {
    let open: BTreeSet<(&str, &str)> = open_flags
        .iter()
        .map(|f| (f.memory.adapter.as_str(), f.memory.id.as_str()))
        .collect();

    // Memory key (adapter, id) -> (first citation order, claim, change ids).
    let mut hits: BTreeMap<(&str, &str), (usize, &str, Vec<&Change>)> = BTreeMap::new();

    for change in changes {
        if flag_severity(change).is_none() {
            continue;
        };
        if is_additive_type_review(change, old) {
            continue;
        }
        let affected = affected_elements(change, old);
        for (order, citation) in citations.iter().enumerate() {
            if citation
                .elements
                .iter()
                .any(|e| affected.contains(e.0.as_str()))
            {
                hits.entry((
                    citation.memory.adapter.as_str(),
                    citation.memory.id.as_str(),
                ))
                .or_insert((order, citation.claim.as_str(), Vec::new()))
                .2
                .push(change);
            }
        }
    }

    let mut flags: Vec<Flag> = Vec::new();
    let mut by_order: Vec<(usize, &str, &str, &str, Vec<&Change>)> = hits
        .into_iter()
        .map(|((adapter, id), (order, claim, changes))| (order, adapter, id, claim, changes))
        .collect();
    by_order.sort_by_key(|(order, _, _, _, _)| *order);

    for (_, adapter, id, claim, matched) in by_order {
        if open.contains(&(adapter, id)) {
            continue;
        }
        let mut ids: Vec<ChangeId> = Vec::new();
        let mut severity = Severity::Review;
        for change in matched {
            if !ids.iter().any(|c| c.0 == change.id) {
                ids.push(ChangeId(change.id.clone()));
            }
            if change.severity == VimanamSeverity::Breaking {
                severity = Severity::Breaking;
            }
        }
        flags.push(Flag {
            id: String::new(),
            memory: super::model::MemoryRef {
                adapter: adapter.to_string(),
                id: id.to_string(),
            },
            changes: ids,
            severity,
            old_version: old_version.to_string(),
            new_version: new_version.to_string(),
            evidence: Some(claim.to_string()),
        });
    }
    flags
}

/// Doyle's severity for a change, or `None` when it flags nothing.
/// Non-breaking is unrepresentable in [`Severity`], so it never flags.
fn flag_severity(change: &Change) -> Option<Severity> {
    match change.severity {
        VimanamSeverity::Breaking => Some(Severity::Breaking),
        VimanamSeverity::Review => Some(Severity::Review),
        // NonBreaking, and any severity Vimanam adds later: flags nothing.
        _ => None,
    }
}

/// A type-level `review` for an added field is the same additive change
/// Vimanam rates `non_breaking` on the operation response that carries it
/// (the Quiet Migration's `Stats.generated_at`: type-level `review`, but
/// `non_breaking` on `GET /stats`' 200), seen through the shared schema.
/// An optional field breaks no existing reader, so it propagates nowhere:
/// not to referencers, not even to direct citers of the type. Additive
/// changes flag nothing. Limitation: should Vimanam ever rate a
/// *required* field addition as `review`, this suppresses it too; no such
/// fact exists in the fixtures, and the response-level fact would still
/// carry the honest verdict.
fn is_additive_type_review(change: &Change, old: &ContractIndex) -> bool {
    change.severity == VimanamSeverity::Review
        && change.kind == ChangeKind::FieldAdded
        && old
            .entity(&ElementId(change.subject.clone()))
            .is_some_and(|e| e.kind == EntityKind::Type)
}

/// Every element ID a change to `change.subject` reaches: the subject, its
/// ancestors, and — only when the subject is a named type or a member of
/// one — the referencers of that type and their ancestors.
fn affected_elements(change: &Change, old: &ContractIndex) -> BTreeSet<String> {
    let subject = ElementId(change.subject.clone());
    let mut affected = BTreeSet::new();
    affected.insert(change.subject.clone());
    for ancestor in old.parent_chain(&subject) {
        affected.insert(ancestor.id.clone());
    }
    if let Some(named) = named_type_of(&subject, old) {
        for referencer in old.referencers(&ElementId(named)) {
            affected.insert(referencer.id.clone());
            for ancestor in old.parent_chain(&ElementId(referencer.id.clone())) {
                affected.insert(ancestor.id.clone());
            }
        }
    }
    affected
}

/// The shared named type a change belongs to, if any: the subject itself
/// when it is a type, else the nearest ancestor that is one (a field of a
/// type belongs to that type). Operation-scoped entities — operations,
/// arguments, results and their inline fields — belong to no shared type,
/// so changes to them never expand through `$ref`s.
fn named_type_of(subject: &ElementId, old: &ContractIndex) -> Option<String> {
    old.parent_chain(subject)
        .into_iter()
        .find(|e| e.kind == EntityKind::Type)
        .map(|e| e.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Entity IDs from testdata/vimanam/README.md, stable across v1 and v2.
    const LIST_PATIENTS: &str =
        "ve2_c81e37b7c42c484a1859c09e95bf8cec89a360db6743a691afab818f177c7bc6";
    const LIST_PATIENTS_200: &str =
        "ve2_86d3cdb7ce3a7f0b1a2fefa696f0acbbcf2d5406c717bb1f1f1cb5f4f62667ba";
    const GET_PATIENT: &str =
        "ve2_c69ae2bf6960e6f28e01bf913a93aa0a325f6d3ee49d445a68b1f580f27af2b5";
    const GET_PATIENT_404: &str =
        "ve2_6add08d53def059f4e973257dc9e174a5398208bce2fa4d719e0418ca523349e";
    const GET_STATS: &str = "ve2_88f1296b143cdad99952319ae783095b3386438f0c8932538d9bfbe1b7fcfb0c";
    const GET_STATS_200: &str =
        "ve2_6c3962f4033a5832c062e5d2bce8458ec018d67d8ff15cdbf109543d63662c5c";
    const PATIENT: &str = "ve2_ca77e31b900362a36a579ba59cdebfd8302cbfad2ec4b2d544857c0fcd30167a";
    const STATS: &str = "ve2_c09c07ec8983f09afc5dafed67ee0e3e0d271de3b8f8f1bd61daa1111f063809";
    const PATIENT_PAGE: &str =
        "ve2_4e6b19cef1c5b0524fb5c8b9f921fc6df3117360c29c2c2d22e58aa536c547a4";
    const PATIENT_COUNT: &str =
        "ve2_99aaf77708e08db6ee0c7bd151e3044ef33e089e1bbe915fe889e7330b79058f";
    const DECOMMISSION: &str =
        "ve2_9e9663295485ddf808651f117c8ca42ab58e33fbe49a27309e0547f77c866846";
    // Change IDs from testdata/vimanam/README.md (patients-v1 → patients-v2).
    const M1_BREAKING: &str =
        "vc2_9bedbbb47825fa87ea41aa38342d0492948286a7dd4a5ecbb378310261df6841";
    const M1_CONSTRAINT: &str =
        "vc2_d7e1b2acbebe45b089106951b949710e3112d1586ed74cfe5fab47d702987277";
    const M1_PAGINATION: &str =
        "vc2_d82adaee7e47799b8027d3c1b9f458e16be8a20df1696186f351369f07e011e0";
    const STATS_TYPE_REVIEW: &str =
        "vc2_6cfe1d7e93fec49f1e8a37c39c64364e23dbcd0ae3384e882ee8850d533903e0";
    // Change ID for the synthetic breaking fact built in
    // already_flagged_memory_is_not_flagged_twice: a clone of M1_BREAKING
    // retargeted at getPatient, so it would flag M2.
    const SYNTHETIC_GET_PATIENT_BREAKING: &str =
        "vc2_synthetic00000000000000000000000000000000getpatient";

    fn fixture(name: &str) -> String {
        format!("{}/testdata/{name}", env!("CARGO_MANIFEST_DIR"))
    }

    fn old_index() -> ContractIndex {
        ContractIndex::load(fixture("contracts/patients-v1.yaml")).expect("v1 loads")
    }

    fn new_index() -> ContractIndex {
        ContractIndex::load(fixture("contracts/patients-v2.yaml")).expect("v2 loads")
    }

    fn demo_changes() -> Vec<Change> {
        let old = old_index();
        let new = new_index();
        old.diff(&new).expect("diff works")
    }

    fn citation(memory: &str, claim: &str, elements: &[&str]) -> Citation {
        Citation {
            memory: super::super::model::MemoryRef {
                adapter: "muse-code".to_string(),
                id: memory.to_string(),
            },
            elements: elements
                .iter()
                .map(|e| ElementId((*e).to_string()))
                .collect(),
            claim: claim.to_string(),
            recorded_by: "curator".to_string(),
            confidence: 0.9,
        }
    }

    /// The Quiet Migration's four memories (testdata/README.md): what each
    /// rests on.
    fn demo_citations() -> Vec<Citation> {
        vec![
            citation(
                "M1",
                "GET /patients returns every patient in one response",
                &[LIST_PATIENTS, LIST_PATIENTS_200],
            ),
            citation(
                "M2",
                "GET /patients/{id} returns 404 for an unknown patient id",
                &[GET_PATIENT, GET_PATIENT_404],
            ),
            citation(
                "M3",
                "GET /stats patient_count is the total number of patients",
                &[GET_STATS, PATIENT_COUNT],
            ),
            citation("M4", "decommission is permanent", &[DECOMMISSION]),
        ]
    }

    fn change_ids(flag: &Flag) -> Vec<&str> {
        flag.changes.iter().map(|c| c.0.as_str()).collect()
    }

    #[test]
    fn demo_flags_only_m1_with_its_three_changes() {
        let old = old_index();
        let flags = flag_citations(&demo_citations(), &demo_changes(), &old, &[], "v1", "v2");
        assert_eq!(flags.len(), 1, "expected exactly {{M1}}, got {flags:?}");
        let flag = &flags[0];
        assert_eq!(flag.memory.id, "M1");
        assert_eq!(
            change_ids(flag),
            [M1_CONSTRAINT, M1_BREAKING, M1_PAGINATION],
            "Vimanam's order: constraint, breaking, pagination"
        );
        assert_eq!(flag.severity, Severity::Breaking);
        assert_eq!(
            (flag.old_version.as_str(), flag.new_version.as_str()),
            ("v1", "v2")
        );
        assert_eq!(
            flag.evidence.as_deref(),
            Some("GET /patients returns every patient in one response")
        );
    }

    #[test]
    fn additive_changes_flag_nothing() {
        let old = old_index();
        let changes = demo_changes();
        // Every non-breaking fact on its own: the cursor/limit arguments,
        // the items/next_cursor additions, items becoming required, the
        // generated_at field on getStats' response, and PatientPage.
        let additive: Vec<Change> = changes
            .into_iter()
            .filter(|c| c.severity == VimanamSeverity::NonBreaking)
            .collect();
        assert_eq!(additive.len(), 7);
        let mut citations = demo_citations();
        // Cite everything the additive facts touch, including the new type
        // and the shared Patient, to prove none of it flags.
        citations.push(citation(
            "M5",
            "PatientPage pages",
            &[PATIENT_PAGE, PATIENT],
        ));
        citations.push(citation(
            "M6",
            "getStats 200 carries generated_at",
            &[GET_STATS_200, STATS],
        ));
        let flags = flag_citations(&citations, &additive, &old, &[], "v1", "v2");
        assert!(
            flags.is_empty(),
            "additive changes flag nothing, got {flags:?}"
        );
    }

    #[test]
    fn response_change_flags_citers_of_response_and_operation() {
        let old = old_index();
        let breaking: Vec<Change> = demo_changes()
            .into_iter()
            .filter(|c| c.id == M1_BREAKING)
            .collect();
        assert_eq!(breaking.len(), 1);
        let citations = vec![
            citation("op-citer", "lists patients", &[LIST_PATIENTS]),
            citation("response-citer", "200 is an array", &[LIST_PATIENTS_200]),
        ];
        let flags = flag_citations(&citations, &breaking, &old, &[], "v1", "v2");
        assert_eq!(flags.len(), 2);
        for flag in &flags {
            assert_eq!(change_ids(flag), [M1_BREAKING]);
            assert_eq!(flag.severity, Severity::Breaking);
        }
    }

    #[test]
    fn operation_change_does_not_flag_citers_of_descendant_only() {
        let old = old_index();
        // M1_PAGINATION subjects the listPatients operation itself. A memory
        // citing only its descendant (the 200 response) must stay quiet:
        // propagation is upward-only, never down to descendants.
        let pagination: Vec<Change> = demo_changes()
            .into_iter()
            .filter(|c| c.id == M1_PAGINATION)
            .collect();
        assert_eq!(pagination.len(), 1);
        let citations = vec![citation(
            "response-only-citer",
            "200 is an array",
            &[LIST_PATIENTS_200],
        )];
        let flags = flag_citations(&citations, &pagination, &old, &[], "v1", "v2");
        assert!(
            flags.is_empty(),
            "citing a descendant of the subject never flags, got {flags:?}"
        );
    }

    #[test]
    fn response_change_does_not_cross_the_shared_ref() {
        let old = old_index();
        let changes = demo_changes();
        // getPatient cites its operation and 404; Patient is the shared
        // item type of both list operations. No listPatients change may
        // reach it through the $ref.
        let citations = vec![
            citation("M2", "404 for unknown id", &[GET_PATIENT, GET_PATIENT_404]),
            citation("patient-citer", "Patient has vaccinations", &[PATIENT]),
        ];
        let flags = flag_citations(&citations, &changes, &old, &[], "v1", "v2");
        assert!(
            flags.is_empty(),
            "no flag through the shared Patient, got {flags:?}"
        );
    }

    #[test]
    fn breaking_shared_type_change_reaches_using_operations_only() {
        let old = old_index();
        // A hypothetical breaking change to the shared Stats type: cloned
        // from a real breaking fact so the shape is Vimanam's own.
        let mut breaking: Vec<Change> = demo_changes()
            .into_iter()
            .filter(|c| c.id == M1_BREAKING)
            .collect();
        assert_eq!(breaking.len(), 1);
        breaking[0].subject = STATS.to_string();
        let citations = vec![
            citation(
                "M3",
                "patient_count is the total",
                &[GET_STATS, PATIENT_COUNT],
            ),
            citation("response-citer", "getStats 200", &[GET_STATS_200]),
            citation("type-citer", "Stats shape", &[STATS]),
            citation("M2", "404 for unknown id", &[GET_PATIENT, GET_PATIENT_404]),
            citation("M4", "decommission is permanent", &[DECOMMISSION]),
        ];
        let flags = flag_citations(&citations, &breaking, &old, &[], "v1", "v2");
        let flagged: Vec<&str> = flags.iter().map(|f| f.memory.id.as_str()).collect();
        assert!(
            flagged.contains(&"M3"),
            "operation using the type flags: {flagged:?}"
        );
        assert!(flagged.contains(&"response-citer"), "{flagged:?}");
        assert!(flagged.contains(&"type-citer"), "{flagged:?}");
        assert!(
            !flagged.contains(&"M2"),
            "other operations do not flag: {flagged:?}"
        );
        assert!(!flagged.contains(&"M4"), "{flagged:?}");
        assert_eq!(flags.len(), 3);
    }

    #[test]
    fn breaking_shared_field_change_reaches_the_returning_operation() {
        let old = old_index();
        // A hypothetical breaking change to Stats.patient_count: its parent
        // chain holds the Stats type, so citers of getStats flag, while the
        // rest of the service stays quiet.
        let mut breaking: Vec<Change> = demo_changes()
            .into_iter()
            .filter(|c| c.id == M1_BREAKING)
            .collect();
        breaking[0].subject = PATIENT_COUNT.to_string();
        let flags = flag_citations(&demo_citations(), &breaking, &old, &[], "v1", "v2");
        let flagged: Vec<&str> = flags.iter().map(|f| f.memory.id.as_str()).collect();
        assert_eq!(
            flagged,
            ["M3"],
            "only the operation returning Stats: {flagged:?}"
        );
    }

    #[test]
    fn type_level_review_on_added_optional_field_flags_nothing() {
        let old = old_index();
        let review: Vec<Change> = demo_changes()
            .into_iter()
            .filter(|c| c.id == STATS_TYPE_REVIEW)
            .collect();
        assert_eq!(review.len(), 1);
        // Even a citation of Stats itself, of its response, and of the
        // operation stay quiet: the same field is non-breaking on the
        // response, so the type-level review is one additive change.
        let mut citations = demo_citations();
        citations.push(citation("type-citer", "Stats shape", &[STATS]));
        citations.push(citation("response-citer", "getStats 200", &[GET_STATS_200]));
        let flags = flag_citations(&citations, &review, &old, &[], "v1", "v2");
        assert!(flags.is_empty(), "M3 must not flag: got {flags:?}");
    }

    #[test]
    fn review_only_flag_is_marked_review() {
        let old = old_index();
        let pagination: Vec<Change> = demo_changes()
            .into_iter()
            .filter(|c| c.id == M1_PAGINATION)
            .collect();
        assert_eq!(pagination.len(), 1);
        let citations = vec![citation("op-citer", "lists patients", &[LIST_PATIENTS])];
        let flags = flag_citations(&citations, &pagination, &old, &[], "v1", "v2");
        assert_eq!(flags.len(), 1);
        assert_eq!(flags[0].severity, Severity::Review);
        assert_eq!(change_ids(&flags[0]), [M1_PAGINATION]);
    }

    #[test]
    fn already_flagged_memory_is_not_flagged_twice() {
        let old = old_index();
        let open = vec![Flag {
            id: "flag-1".to_string(),
            memory: super::super::model::MemoryRef {
                adapter: "muse-code".to_string(),
                id: "M1".to_string(),
            },
            changes: vec![ChangeId(M1_BREAKING.to_string())],
            severity: Severity::Breaking,
            old_version: "v1".to_string(),
            new_version: "v2".to_string(),
            evidence: None,
        }];
        // A synthetic breaking change retargeted at getPatient would flag
        // M2. Suppression must be per-memory: M1 stays quiet while M2's
        // flag is still produced. (A buggy "return empty if ANY open flag
        // exists" would produce nothing and fail the M2 assertions.)
        let mut changes = demo_changes();
        let mut extra: Vec<Change> = demo_changes()
            .into_iter()
            .filter(|c| c.id == M1_BREAKING)
            .collect();
        assert_eq!(extra.len(), 1);
        extra[0].subject = GET_PATIENT.to_string();
        extra[0].id = SYNTHETIC_GET_PATIENT_BREAKING.to_string();
        changes.push(extra.into_iter().next().unwrap());
        let flags = flag_citations(&demo_citations(), &changes, &old, &open, "v1", "v2");
        let flagged: Vec<&str> = flags.iter().map(|f| f.memory.id.as_str()).collect();
        assert!(
            !flagged.contains(&"M1"),
            "M1 is already open: got {flags:?}"
        );
        assert_eq!(
            flagged,
            ["M2"],
            "other memories still flag while M1 is suppressed: {flagged:?}"
        );
        assert_eq!(change_ids(&flags[0]), [SYNTHETIC_GET_PATIENT_BREAKING]);
        assert_eq!(flags[0].severity, Severity::Breaking);
    }

    #[test]
    fn one_flag_per_memory_lists_every_change_behind_it() {
        let old = old_index();
        let changes = demo_changes();
        // Two citations for the same memory: still one flag, each change once.
        let citations = vec![
            citation("M1", "one call exports all", &[LIST_PATIENTS]),
            citation("M1", "one call exports all", &[LIST_PATIENTS_200]),
        ];
        let flags = flag_citations(&citations, &changes, &old, &[], "v1", "v2");
        assert_eq!(flags.len(), 1);
        assert_eq!(
            change_ids(&flags[0]),
            [M1_CONSTRAINT, M1_BREAKING, M1_PAGINATION]
        );
        assert_eq!(flags[0].severity, Severity::Breaking);
    }

    #[test]
    fn unknown_subject_still_matches_direct_citations() {
        let old = old_index();
        // A breaking change on an entity the old index never knew (as a new
        // entity would be): direct citers still flag, nothing else does.
        let mut breaking: Vec<Change> = demo_changes()
            .into_iter()
            .filter(|c| c.id == M1_BREAKING)
            .collect();
        breaking[0].subject = "ve2_doesnotexist00000000000000000000000000000000".to_string();
        let mut citations = demo_citations();
        citations.push(citation(
            "direct-citer",
            "the new thing",
            &["ve2_doesnotexist00000000000000000000000000000000"],
        ));
        let flags = flag_citations(&citations, &breaking, &old, &[], "v1", "v2");
        let flagged: Vec<&str> = flags.iter().map(|f| f.memory.id.as_str()).collect();
        assert_eq!(flagged, ["direct-citer"], "{flagged:?}");
    }
}
