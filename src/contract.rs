//! Doyle's contract index: a contract file loaded through Vimanam's
//! library, and the queries flagging and the guard ask of it.
//!
//! The index holds Vimanam's neutral surface (entities with stable `ve2_`
//! IDs, parent links and references) exactly as Vimanam gives it, plus the
//! parsed document the diff needs. [`ContractIndex::diff`] keeps Vimanam's
//! change facts untouched. No flagging logic lives here; no model is called.

use std::collections::HashMap;
use std::path::Path;

use vimanam::contract::{Change, Entity, EntityKind};

use crate::core::ElementId;

/// Loading or diffing a contract failed: which file, and Vimanam's reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractError {
    /// The file that failed to load, or the pair being diffed as `old -> new`.
    pub file: String,
    /// Vimanam's reason, verbatim.
    pub reason: String,
}

impl std::fmt::Display for ContractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.file, self.reason)
    }
}

impl std::error::Error for ContractError {}

/// An entity's native location in the contract's own terms: the HTTP method
/// and path, the operation ID, and the named type and field. For display and
/// for the guard's match. Fields that don't apply to an entity are `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeLocation {
    /// HTTP method (`GET`), when the entity is operation-scoped.
    pub method: Option<String>,
    /// Exact path template (`/patients`), when operation-scoped.
    pub path: Option<String>,
    /// Operation ID (`listPatients`), for operations and their children.
    pub operation_id: Option<String>,
    /// Named type (`Patient`), for types and their fields.
    pub type_name: Option<String>,
    /// Field name within its type, for fields.
    pub field_name: Option<String>,
}

/// One contract version, loaded through Vimanam and indexed for queries.
#[derive(Debug)]
pub struct ContractIndex {
    /// Display form of the path this index was loaded from; used in errors.
    file: String,
    /// The parsed document, kept because Vimanam's diff reads it, not the surface.
    api: vimanam::ApiDocumentation,
    /// The neutral surface: entities in deterministic native order.
    contract: vimanam::contract::Contract,
    /// Entity ID to position in [`ContractIndex::contract`]'s entity list.
    by_id: HashMap<String, usize>,
}

impl ContractIndex {
    /// Load a contract file through Vimanam's library. A malformed file
    /// fails with the file name and Vimanam's reason.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ContractError> {
        let path = path.as_ref();
        let file = path.display().to_string();
        let api = vimanam::parse_openapi(path).map_err(|e| ContractError {
            file: file.clone(),
            reason: format!("{e:#}"),
        })?;
        let contract = vimanam::contract::openapi::adapt(&api);
        let by_id = contract
            .entities
            .iter()
            .enumerate()
            .map(|(i, e)| (e.id.clone(), i))
            .collect();
        Ok(Self {
            file,
            api,
            contract,
            by_id,
        })
    }

    /// The entity with this ID, if the contract has one.
    pub fn entity(&self, id: &ElementId) -> Option<&Entity> {
        self.by_id_str(&id.0)
    }

    /// The entity and its ancestors, nearest first: the entity itself, its
    /// parent, and so on up to the root. Empty when the ID is unknown.
    pub fn parent_chain(&self, id: &ElementId) -> Vec<&Entity> {
        self.chain(self.entity(id))
    }

    /// Every entity that references this one (shared `$ref`s), in the
    /// surface's native order.
    pub fn referencers(&self, id: &ElementId) -> Vec<&Entity> {
        self.contract
            .entities
            .iter()
            .filter(|e| e.references.iter().any(|r| r == &id.0))
            .collect()
    }

    /// The entity's native location for display and the guard. `None` when
    /// the ID is unknown.
    pub fn native_location(&self, id: &ElementId) -> Option<NativeLocation> {
        let entity = self.entity(id)?;
        let (method, path) = match &entity.native {
            vimanam::contract::NativeLocation::Openapi { method, path, .. } => {
                (method.clone(), path.clone())
            }
            _ => (None, None),
        };
        let operation_id = self
            .chain(Some(entity))
            .into_iter()
            .find(|e| e.kind == EntityKind::Operation)
            .and_then(operation_id_of);
        let (type_name, field_name) = match entity.kind {
            EntityKind::Type => (Some(entity.name.clone()), None),
            EntityKind::Field => (
                entity
                    .parent
                    .as_deref()
                    .and_then(|p| self.by_id_str(p))
                    .map(|p| p.name.clone()),
                Some(entity.name.clone()),
            ),
            _ => (None, None),
        };
        Some(NativeLocation {
            method,
            path,
            operation_id,
            type_name,
            field_name,
        })
    }

    /// Diff this index (old) against another (new): Vimanam's change facts,
    /// kept as Vimanam gives them, in Vimanam's order.
    pub fn diff(&self, new: &Self) -> Result<Vec<Change>, ContractError> {
        vimanam::contract::openapi::diff(&self.api, &new.api).map_err(|e| ContractError {
            file: format!("{} -> {}", self.file, new.file),
            reason: format!("{e:#}"),
        })
    }

    fn by_id_str(&self, id: &str) -> Option<&Entity> {
        self.by_id.get(id).map(|&i| &self.contract.entities[i])
    }

    /// An entity and its ancestors, nearest first. The length guard stops a
    /// cycle (parent links form a forest, but the index doesn't assume it).
    fn chain<'a>(&'a self, first: Option<&'a Entity>) -> Vec<&'a Entity> {
        let mut chain = Vec::new();
        let mut next = first;
        while let Some(current) = next {
            chain.push(current);
            if chain.len() > self.contract.entities.len() {
                break;
            }
            next = current.parent.as_deref().and_then(|p| self.by_id_str(p));
        }
        chain
    }
}

/// The operation ID in an operation entity's attributes, if present.
fn operation_id_of(entity: &Entity) -> Option<String> {
    entity
        .attributes
        .get("operation_id")?
        .as_str()
        .map(str::to_string)
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
    const GET_STATS_200: &str =
        "ve2_6c3962f4033a5832c062e5d2bce8458ec018d67d8ff15cdbf109543d63662c5c";
    const PATIENT: &str = "ve2_ca77e31b900362a36a579ba59cdebfd8302cbfad2ec4b2d544857c0fcd30167a";
    const STATS: &str = "ve2_c09c07ec8983f09afc5dafed67ee0e3e0d271de3b8f8f1bd61daa1111f063809";
    const PATIENT_PAGE: &str =
        "ve2_4e6b19cef1c5b0524fb5c8b9f921fc6df3117360c29c2c2d22e58aa536c547a4";
    const PATIENT_COUNT: &str =
        "ve2_99aaf77708e08db6ee0c7bd151e3044ef33e089e1bbe915fe889e7330b79058f";
    // Change IDs from testdata/vimanam/README.md (patients-v1 → patients-v2).
    const M1_BREAKING: &str =
        "vc2_9bedbbb47825fa87ea41aa38342d0492948286a7dd4a5ecbb378310261df6841";
    const M1_CONSTRAINT: &str =
        "vc2_d7e1b2acbebe45b089106951b949710e3112d1586ed74cfe5fab47d702987277";
    const M1_PAGINATION: &str =
        "vc2_d82adaee7e47799b8027d3c1b9f458e16be8a20df1696186f351369f07e011e0";

    fn fixture(name: &str) -> String {
        format!("{}/testdata/{name}", env!("CARGO_MANIFEST_DIR"))
    }

    fn load_v1() -> ContractIndex {
        ContractIndex::load(fixture("contracts/patients-v1.yaml")).expect("v1 loads")
    }

    fn load_v2() -> ContractIndex {
        ContractIndex::load(fixture("contracts/patients-v2.yaml")).expect("v2 loads")
    }

    fn element(id: &str) -> ElementId {
        ElementId(id.to_string())
    }

    #[test]
    fn loads_v1_with_recorded_entities() {
        let index = load_v1();
        assert_eq!(
            index.contract.title,
            "Fernwhistle Veterinary Group patient records"
        );
        assert_eq!(index.contract.version, "1.0.0");
        assert_eq!(index.contract.entities.len(), 32);

        // The index carries the same entities, in the same order, as the
        // recorded library parse in testdata/vimanam/entities-v1.json.
        let recorded: vimanam::contract::Contract = serde_json::from_str(
            &std::fs::read_to_string(fixture("vimanam/entities-v1.json")).expect("fixture reads"),
        )
        .expect("fixture parses");
        let ours: Vec<&str> = index
            .contract
            .entities
            .iter()
            .map(|e| e.id.as_str())
            .collect();
        let theirs: Vec<&str> = recorded.entities.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ours, theirs);
    }

    #[test]
    fn loads_v2_with_recorded_entities() {
        let index = load_v2();
        assert_eq!(index.contract.entities.len(), 38);

        let recorded: vimanam::contract::Contract = serde_json::from_str(
            &std::fs::read_to_string(fixture("vimanam/entities-v2.json")).expect("fixture reads"),
        )
        .expect("fixture parses");
        let ours: Vec<&str> = index
            .contract
            .entities
            .iter()
            .map(|e| e.id.as_str())
            .collect();
        let theirs: Vec<&str> = recorded.entities.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ours, theirs);
    }

    #[test]
    fn entity_lookup_and_parent_chain() {
        let index = load_v1();

        let response = index
            .entity(&element(LIST_PATIENTS_200))
            .expect("200 found");
        assert_eq!(response.name, "200");
        assert_eq!(response.kind, EntityKind::Result);

        // The chain runs from the entity up to the root.
        let chain: Vec<&str> = index
            .parent_chain(&element(LIST_PATIENTS_200))
            .iter()
            .map(|e| e.id.as_str())
            .collect();
        assert_eq!(chain, [LIST_PATIENTS_200, LIST_PATIENTS]);

        // A root operation's chain is itself.
        let chain: Vec<&str> = index
            .parent_chain(&element(LIST_PATIENTS))
            .iter()
            .map(|e| e.id.as_str())
            .collect();
        assert_eq!(chain, [LIST_PATIENTS]);

        // Unknown IDs give nothing, not a panic.
        assert!(index.entity(&element("ve2_nonexistent")).is_none());
        assert!(index.parent_chain(&element("ve2_nonexistent")).is_empty());
        assert!(index.native_location(&element("ve2_nonexistent")).is_none());
    }

    #[test]
    fn referencers_of_shared_patient_type() {
        let v1 = load_v1();

        // In v1 both GET /patients 200 and GET /patients/{id} 200 reference
        // Patient; no operation cites it directly.
        let refs: Vec<&str> = v1
            .referencers(&element(PATIENT))
            .iter()
            .map(|e| e.id.as_str())
            .collect();
        assert_eq!(refs.len(), 2);
        assert!(refs.contains(&LIST_PATIENTS_200));
        assert!(!refs.contains(&LIST_PATIENTS));
        assert!(!refs.contains(&GET_PATIENT));
        for e in v1.referencers(&element(PATIENT)) {
            assert_eq!(e.kind, EntityKind::Result);
            let parents: Vec<&str> = v1
                .parent_chain(&element(&e.id))
                .iter()
                .map(|p| p.id.as_str())
                .collect();
            assert!(parents.contains(&LIST_PATIENTS) || parents.contains(&GET_PATIENT));
        }

        // In v2 the new PatientPage type and its items field reference
        // Patient too; getPatient's 200 still does.
        let v2 = load_v2();
        let refs: Vec<&str> = v2
            .referencers(&element(PATIENT))
            .iter()
            .map(|e| e.id.as_str())
            .collect();
        assert!(refs.contains(&PATIENT_PAGE));
        assert!(!refs.contains(&LIST_PATIENTS_200));

        // getStats' 200 response is the only referencer of Stats.
        let stats_refs: Vec<&str> = v1
            .referencers(&element(STATS))
            .iter()
            .map(|e| e.id.as_str())
            .collect();
        assert_eq!(stats_refs, [GET_STATS_200]);
    }

    #[test]
    fn native_locations_for_display_and_guard() {
        let index = load_v1();

        let op = index
            .native_location(&element(LIST_PATIENTS))
            .expect("found");
        assert_eq!(
            op,
            NativeLocation {
                method: Some("GET".to_string()),
                path: Some("/patients".to_string()),
                operation_id: Some("listPatients".to_string()),
                type_name: None,
                field_name: None,
            }
        );

        // A child result carries its operation's method, path and ID.
        let not_found = index
            .native_location(&element(GET_PATIENT_404))
            .expect("found");
        assert_eq!(not_found.method.as_deref(), Some("GET"));
        assert_eq!(not_found.path.as_deref(), Some("/patients/{id}"));
        assert_eq!(not_found.operation_id.as_deref(), Some("getPatient"));
        assert_eq!((not_found.type_name, not_found.field_name), (None, None));

        // A type names itself; a field names its type and itself.
        let patient = index.native_location(&element(PATIENT)).expect("found");
        assert_eq!(
            (patient.method, patient.path, patient.operation_id),
            (None, None, None)
        );
        assert_eq!(patient.type_name.as_deref(), Some("Patient"));
        assert_eq!(patient.field_name, None);

        let count = index
            .native_location(&element(PATIENT_COUNT))
            .expect("found");
        assert_eq!(count.type_name.as_deref(), Some("Stats"));
        assert_eq!(count.field_name.as_deref(), Some("patient_count"));

        // The Stats type itself carries the type name for the guard.
        let stats = index.native_location(&element(STATS)).expect("found");
        assert_eq!(stats.type_name.as_deref(), Some("Stats"));
    }

    #[test]
    fn diff_matches_recorded_change_set() {
        let old = load_v1();
        let new = load_v2();
        let changes = old.diff(&new).expect("diff works");
        assert_eq!(changes.len(), 11);

        // The facts are the recorded ones, in Vimanam's order.
        let recorded: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(fixture("vimanam/changes-v1-v2.json")).expect("fixture reads"),
        )
        .expect("fixture parses");
        let expected: Vec<&str> = recorded["changes"]
            .as_array()
            .expect("array")
            .iter()
            .map(|c| c["id"].as_str().expect("id"))
            .collect();
        let ours: Vec<&str> = changes.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ours, expected);

        // M1's three facts: the breaking response change, the review
        // constraint on the same response, and pagination on the operation.
        for (id, subject) in [
            (M1_BREAKING, LIST_PATIENTS_200),
            (M1_CONSTRAINT, LIST_PATIENTS_200),
            (M1_PAGINATION, LIST_PATIENTS),
        ] {
            let fact = changes
                .iter()
                .find(|c| c.id == id)
                .expect("M1 fact present");
            assert_eq!(fact.subject, subject);
        }

        // Severity summary as recorded: 1 breaking, 3 review, 7 non-breaking.
        let mut counts = [0, 0, 0];
        for c in &changes {
            if c.severity == vimanam::contract::Severity::Breaking {
                counts[0] += 1;
            } else if c.severity == vimanam::contract::Severity::Review {
                counts[1] += 1;
            } else if c.severity == vimanam::contract::Severity::NonBreaking {
                counts[2] += 1;
            } else {
                panic!("unknown severity on {}", c.id);
            }
        }
        assert_eq!(counts, [1, 3, 7]);
    }

    #[test]
    fn malformed_contract_fails_with_file_and_reason() {
        let path = std::env::temp_dir().join("doyle-t2-1-malformed.yaml");
        std::fs::write(&path, "not an openapi contract: [unclosed").expect("write temp");
        let error = ContractIndex::load(&path).expect_err("malformed fails");
        std::fs::remove_file(&path).ok();
        assert_eq!(error.file, path.display().to_string());
        assert!(!error.reason.is_empty(), "Vimanam's reason is kept");
        assert!(
            error.to_string().contains(&path.display().to_string()),
            "display names the file"
        );
    }
}
