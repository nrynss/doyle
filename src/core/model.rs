//! Doyle's core model: the types every task codes against.
//!
//! The core knows contract elements (Vimanam's stable IDs) and memory
//! references (an adapter name plus that system's own ID). It never names a
//! memory system: [`MemoryRef::adapter`] is a string the adapter declares, so
//! a new adapter needs no change here.

use serde::{Deserialize, Serialize};

/// Vimanam's stable entity ID (`ve2_…`): an operation, type, field, argument,
/// result or enum value. Doyle stores the string verbatim and never re-derives it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ElementId(pub String);

/// Vimanam's change ID (`vc2_…`) for one change fact between two contract
/// versions. Stored verbatim, like [`ElementId`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChangeId(pub String);

/// A memory in a memory system: the adapter's declared name plus that
/// system's own memory ID. Doyle never copies memory text into its own store
/// except as evidence on a flag.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MemoryRef {
    /// The adapter's declared name for its memory system. A plain string,
    /// never an enum of known systems.
    pub adapter: String,
    /// The memory's ID in its own system (a note path, a node id, ...).
    pub id: String,
}

/// A memory read through an adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Memory {
    pub reference: MemoryRef,
    pub text: String,
    /// Last updated (RFC 3339) when the system knows it; `None` when it does not.
    pub updated_at: Option<String>,
    /// The scope the memory was read from, in the adapter's own terms
    /// (a project, a session, ...).
    pub scope: String,
}

/// A memory rests on one or more contract elements: the claim in one line,
/// who recorded it, and how confident the recording is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Citation {
    pub memory: MemoryRef,
    pub elements: Vec<ElementId>,
    /// The claim in one line.
    pub claim: String,
    /// Who recorded it: a curator model id or a person's name.
    pub recorded_by: String,
    /// 0.0–1.0.
    pub confidence: f32,
}

/// The severity of a flag. Only breaking and review changes flag; additive,
/// non-breaking changes flag nothing, so that severity is unrepresentable here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Breaking,
    Review,
}

/// A cited element changed: the memory, the Vimanam change facts behind the
/// flag, the severity, and the contract versions on either side of the change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Flag {
    /// Doyle's id for this flag, assigned by the store.
    pub id: String,
    pub memory: MemoryRef,
    /// The Vimanam change facts behind this flag.
    pub changes: Vec<ChangeId>,
    /// The highest severity among `changes`.
    pub severity: Severity,
    pub old_version: String,
    pub new_version: String,
    /// A short quote from the memory text, the evidence for the flag.
    pub evidence: Option<String>,
}

/// What to do about one flag: correct it with new text, retire it, or keep it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProposalAction {
    Correct { new_text: String },
    Retire,
    Keep,
}

/// A proposal for one flag: the action, the rationale, and the model that proposed it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    /// Doyle's id for this proposal, assigned by the store.
    pub id: String,
    /// The flag this proposal answers.
    pub flag_id: String,
    pub action: ProposalAction,
    pub rationale: String,
    /// The model that proposed it.
    pub proposed_by: String,
}

/// A person's verdict on a proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionVerdict {
    Approve,
    Retire,
    Keep,
    EditThenApprove { new_text: String },
}

/// A person's verdict on a proposal: the verdict, who decided, and when.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    /// Doyle's id for this decision, assigned by the store.
    pub id: String,
    /// The proposal this decision settles.
    pub proposal_id: String,
    pub verdict: DecisionVerdict,
    pub decided_by: String,
    /// RFC 3339.
    pub decided_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Entity and change IDs from testdata/vimanam (patients-v1 → patients-v2).
    const LIST_PATIENTS: &str =
        "ve2_c81e37b7c42c484a1859c09e95bf8cec89a360db6743a691afab818f177c7bc6";
    const LIST_PATIENTS_200: &str =
        "ve2_86d3cdb7ce3a7f0b1a2fefa696f0acbbcf2d5406c717bb1f1f1cb5f4f62667ba";
    const GET_PATIENT: &str =
        "ve2_c69ae2bf6960e6f28e01bf913a93aa0a325f6d3ee49d445a68b1f580f27af2b5";
    const GET_PATIENT_404: &str =
        "ve2_6add08d53def059f4e973257dc9e174a5398208bce2fa4d719e0418ca523349e";
    const GET_STATS: &str = "ve2_88f1296b143cdad99952319ae783095b3386438f0c8932538d9bfbe1b7fcfb0c";
    const PATIENT_COUNT: &str =
        "ve2_99aaf77708e08db6ee0c7bd151e3044ef33e089e1bbe915fe889e7330b79058f";
    const DECOMMISSION: &str =
        "ve2_9e9663295485ddf808651f117c8ca42ab58e33fbe49a27309e0547f77c866846";
    const M1_BREAKING: &str =
        "vc2_9bedbbb47825fa87ea41aa38342d0492948286a7dd4a5ecbb378310261df6841";
    const M1_CONSTRAINT: &str =
        "vc2_d7e1b2acbebe45b089106951b949710e3112d1586ed74cfe5fab47d702987277";
    const M1_PAGINATION: &str =
        "vc2_d82adaee7e47799b8027d3c1b9f458e16be8a20df1696186f351369f07e011e0";

    fn round_trip<T>(value: &T) -> T
    where
        T: Serialize + for<'de> Deserialize<'de>,
    {
        let text = serde_json::to_string(value).expect("serialize");
        serde_json::from_str(&text).expect("deserialize")
    }

    fn element(id: &str) -> ElementId {
        ElementId(id.to_string())
    }

    fn change(id: &str) -> ChangeId {
        ChangeId(id.to_string())
    }

    #[test]
    fn element_id_round_trip() {
        let id = element(LIST_PATIENTS);
        assert_eq!(
            serde_json::to_value(&id).expect("to value"),
            json!(LIST_PATIENTS)
        );
        assert_eq!(round_trip(&id), id);
    }

    #[test]
    fn change_id_round_trip() {
        let id = change(M1_BREAKING);
        assert_eq!(
            serde_json::to_value(&id).expect("to value"),
            json!(M1_BREAKING)
        );
        assert_eq!(round_trip(&id), id);
    }

    #[test]
    fn memory_ref_round_trip() {
        let reference = MemoryRef {
            adapter: "test-adapter".to_string(),
            id: "mem-1".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&reference).expect("to value"),
            json!({"adapter": "test-adapter", "id": "mem-1"})
        );
        assert_eq!(round_trip(&reference), reference);
    }

    #[test]
    fn memory_round_trip() {
        let memory = Memory {
            reference: MemoryRef {
                adapter: "test-adapter".to_string(),
                id: "mem-1".to_string(),
            },
            text: "GET /patients returns every patient in one response".to_string(),
            updated_at: Some("2026-10-28T09:30:00Z".to_string()),
            scope: "test-scope".to_string(),
        };
        assert_eq!(round_trip(&memory), memory);
    }

    #[test]
    fn memory_without_timestamp_round_trip() {
        let memory = Memory {
            reference: MemoryRef {
                adapter: "test-adapter".to_string(),
                id: "mem-2".to_string(),
            },
            text: "a note with no timestamp".to_string(),
            updated_at: None,
            scope: "test-scope".to_string(),
        };
        let back = round_trip(&memory);
        assert_eq!(back, memory);
        assert!(back.updated_at.is_none());
    }

    #[test]
    fn citation_round_trip() {
        let citation = Citation {
            memory: MemoryRef {
                adapter: "test-adapter".to_string(),
                id: "mem-1".to_string(),
            },
            elements: vec![element(LIST_PATIENTS), element(LIST_PATIENTS_200)],
            claim: "GET /patients returns every patient in one response".to_string(),
            recorded_by: "curator".to_string(),
            confidence: 0.9,
        };
        assert_eq!(round_trip(&citation), citation);
    }

    #[test]
    fn severity_vocabulary() {
        assert_eq!(
            serde_json::to_value(Severity::Breaking).expect("to value"),
            json!("breaking")
        );
        assert_eq!(
            serde_json::to_value(Severity::Review).expect("to value"),
            json!("review")
        );
        assert_eq!(
            serde_json::from_str::<Severity>("\"review\"").expect("parse"),
            Severity::Review
        );
        assert!(serde_json::from_str::<Severity>("\"non_breaking\"").is_err());
    }

    #[test]
    fn flag_round_trip() {
        let flag = Flag {
            id: "flag-1".to_string(),
            memory: MemoryRef {
                adapter: "test-adapter".to_string(),
                id: "mem-1".to_string(),
            },
            changes: vec![change(M1_BREAKING), change(M1_PAGINATION)],
            severity: Severity::Breaking,
            old_version: "v1".to_string(),
            new_version: "v2".to_string(),
            evidence: Some("one call exports the whole clinic".to_string()),
        };
        assert_eq!(round_trip(&flag), flag);
    }

    #[test]
    fn proposal_actions_round_trip() {
        for action in [
            ProposalAction::Correct {
                new_text: "page through every page".to_string(),
            },
            ProposalAction::Retire,
            ProposalAction::Keep,
        ] {
            let proposal = Proposal {
                id: "proposal-1".to_string(),
                flag_id: "flag-1".to_string(),
                action,
                rationale: "the response now pages".to_string(),
                proposed_by: "recheck".to_string(),
            };
            assert_eq!(round_trip(&proposal), proposal);
        }
        assert_eq!(
            serde_json::to_value(ProposalAction::Retire).expect("to value"),
            json!("retire")
        );
        assert_eq!(
            serde_json::to_value(ProposalAction::Correct {
                new_text: "t".to_string()
            })
            .expect("to value"),
            json!({"correct": {"new_text": "t"}})
        );
    }

    #[test]
    fn decision_verdicts_round_trip() {
        for verdict in [
            DecisionVerdict::Approve,
            DecisionVerdict::Retire,
            DecisionVerdict::Keep,
            DecisionVerdict::EditThenApprove {
                new_text: "edited".to_string(),
            },
        ] {
            let decision = Decision {
                id: "decision-1".to_string(),
                proposal_id: "proposal-1".to_string(),
                verdict,
                decided_by: "reviewer".to_string(),
                decided_at: "2026-10-29T12:00:00Z".to_string(),
            };
            assert_eq!(round_trip(&decision), decision);
        }
        assert_eq!(
            serde_json::to_value(DecisionVerdict::EditThenApprove {
                new_text: "t".to_string()
            })
            .expect("to value"),
            json!({"edit_then_approve": {"new_text": "t"}})
        );
    }

    /// The Quiet Migration's four memories (testdata/README.md) and their
    /// expected citations, written as values of these types. Muse Code notes
    /// carry no timestamp, so `updated_at` is `None`.
    #[test]
    fn demo_memories_and_citations_are_expressible() {
        let memories = vec![
            (
                "GET /patients returns every patient in one response; one call exports the whole clinic",
                "MEMORY.md",
                vec![LIST_PATIENTS, LIST_PATIENTS_200],
            ),
            (
                "GET /patients/{id} returns 404 for an unknown patient id",
                "MEMORY.md",
                vec![GET_PATIENT, GET_PATIENT_404],
            ),
            (
                "GET /stats patient_count is the total number of patients",
                "patients-stats.md",
                vec![GET_STATS, PATIENT_COUNT],
            ),
            (
                "POST /admin/decommission is permanent once confirmed; there is no undo",
                "MEMORY.md",
                vec![DECOMMISSION],
            ),
        ];
        let pairs: Vec<(Memory, Citation)> = memories
            .into_iter()
            .map(|(text, file, elements)| {
                let reference = MemoryRef {
                    adapter: "muse-code".to_string(),
                    id: file.to_string(),
                };
                let memory = Memory {
                    reference: reference.clone(),
                    text: text.to_string(),
                    updated_at: None,
                    scope: "clinic-ops".to_string(),
                };
                let citation = Citation {
                    memory: reference,
                    elements: elements.into_iter().map(element).collect(),
                    claim: text.to_string(),
                    recorded_by: "meta-models/muse-spark-open-1026".to_string(),
                    confidence: 0.9,
                };
                (memory, citation)
            })
            .collect();
        assert_eq!(pairs.len(), 4);
        assert_eq!(
            pairs[0].1.elements,
            vec![element(LIST_PATIENTS), element(LIST_PATIENTS_200)]
        );
        assert_eq!(pairs[3].1.elements, vec![element(DECOMMISSION)]);
        for (memory, citation) in &pairs {
            assert_eq!(round_trip(memory), *memory);
            assert_eq!(round_trip(citation), *citation);
        }

        // M1's flag, proposal and decision close the loop through every type.
        let flag = Flag {
            id: "flag-1".to_string(),
            memory: pairs[0].0.reference.clone(),
            changes: vec![
                change(M1_BREAKING),
                change(M1_CONSTRAINT),
                change(M1_PAGINATION),
            ],
            severity: Severity::Breaking,
            old_version: "v1".to_string(),
            new_version: "v2".to_string(),
            evidence: Some("one call exports the whole clinic".to_string()),
        };
        assert_eq!(flag.changes.len(), 3);
        assert_eq!(round_trip(&flag), flag);
        let proposal = Proposal {
            id: "proposal-1".to_string(),
            flag_id: flag.id.clone(),
            action: ProposalAction::Correct {
                new_text:
                    "GET /patients returns one page of 100 plus next_cursor; follow cursor to the end"
                        .to_string(),
            },
            rationale: "v2 pages at 100; one call returns 100 of 4812".to_string(),
            proposed_by: "muse-spark-1.3".to_string(),
        };
        assert_eq!(round_trip(&proposal), proposal);
        let decision = Decision {
            id: "decision-1".to_string(),
            proposal_id: proposal.id.clone(),
            verdict: DecisionVerdict::Approve,
            decided_by: "reviewer".to_string(),
            decided_at: "2026-10-29T12:00:00Z".to_string(),
        };
        assert_eq!(round_trip(&decision), decision);
    }

    #[test]
    fn lambo_node_id_is_expressible() {
        let reference = MemoryRef {
            adapter: "lambo".to_string(),
            id: "5e1d0a73".to_string(),
        };
        assert_eq!(round_trip(&reference), reference);
    }
}
