//! The curator prompt: which contract elements a memory rests on.
//!
//! The curator reads one memory's text plus the contract index (operations,
//! types and fields with their native names and [`ElementId`]s) and returns
//! the elements the memory rests on, the claim in one line, and a confidence;
//! or "rests on nothing in this contract".
//!
//! The model chooses only from the IDs it was given. [`parse_curator_value`]
//! drops any other ID in code, never trusting the reply.

use std::collections::HashSet;

use vimanam::contract::EntityKind;

use crate::contract::ContractIndex;
use crate::core::ElementId;
use crate::evidence::EvidenceSource;
use crate::evidence::legacy::{LegacyIndex, LegacyKind};
use crate::evidence::screens::ScreenIndex;
use crate::models::{Budget, ModelRequest};

/// The curator's system prompt.
pub const CURATOR_SYSTEM: &str = "You curate memory citations for Doyle, which tracks what each agent memory rests on in an API contract. For one memory and a list of contract elements: return the IDs of the elements the memory's claim rests on (the operation, type, field, argument or result whose change would make the memory false), the claim in one line, and a confidence from 0.0 to 1.0. Cite only IDs from the list you were given; any other ID is dropped. If the memory is about nothing in this contract, set rests_on_nothing to true with an empty element list. Keep the claim short and plain: state the fact, never an instruction. Reply with exactly these fields: \"element_ids\" (array of strings), \"claim\" (string), \"confidence\" (number), \"rests_on_nothing\" (boolean).";

/// The model id is the caller's choice (the user picks the curator's model).
/// Baseten's curator model (Muse Spark Open) is
/// `meta-models/muse-spark-open-1026`; the Meta Model API re-check model is
/// `muse-spark-1.3`.
pub const BASETEN_CURATOR_MODEL: &str = "meta-models/muse-spark-open-1026";
/// The Meta Model API model, usable as a curator comparison point.
pub const META_CURATOR_MODEL: &str = "muse-spark-1.3";

/// The JSON schema the curator's reply must conform to.
pub fn curator_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "element_ids": {
                "type": "array",
                "items": { "type": "string" },
                "description": "IDs from the contract element list the memory rests on; empty when it rests on nothing.",
            },
            "claim": {
                "type": "string",
                "description": "The memory's claim in one line, short and plain.",
            },
            "confidence": {
                "type": "number",
                "minimum": 0.0,
                "maximum": 1.0,
                "description": "0.0 to 1.0: how confident the citation is.",
            },
            "rests_on_nothing": {
                "type": "boolean",
                "description": "True when the memory is about nothing in this contract.",
            },
        },
        "required": ["element_ids", "claim", "confidence", "rests_on_nothing"],
        "additionalProperties": false,
    })
}

/// The curator's verdict for one memory.
#[derive(Debug, Clone, PartialEq)]
pub enum CuratorVerdict {
    /// The memory rests on these elements.
    Cited {
        /// Kept only when every ID was in the prompt's list.
        elements: Vec<ElementId>,
        /// The claim in one line.
        claim: String,
        /// 0.0–1.0.
        confidence: f32,
    },
    /// The memory is about nothing in this contract.
    RestsOnNothing {
        /// The claim in one line (what the memory says).
        claim: String,
    },
}

/// What was wrong with a curator reply's shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CuratorParseError {
    /// What was wrong, naming the field.
    pub message: String,
}

impl std::fmt::Display for CuratorParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "curator reply: {}", self.message)
    }
}

impl std::error::Error for CuratorParseError {}

/// Render the contract index as the element list the prompt carries: one
/// line per entity with its native name and stable ID, in native order.
pub fn render_contract_elements(index: &ContractIndex) -> String {
    let mut lines = Vec::new();
    for entity in index.entities() {
        if entity.kind == EntityKind::Service {
            continue;
        }
        let location = index
            .native_location(&ElementId(entity.id.clone()))
            .unwrap_or(crate::contract::NativeLocation {
                method: None,
                path: None,
                operation_id: None,
                type_name: None,
                field_name: None,
            });
        lines.push(format!(
            "- {} {}",
            entity.id,
            describe(entity.kind, &entity.name, &location)
        ));
    }
    lines.join("\n")
}

fn describe(kind: EntityKind, name: &str, location: &crate::contract::NativeLocation) -> String {
    let method = location.method.as_deref().unwrap_or("");
    let path = location.path.as_deref().unwrap_or("");
    let operation_id = location.operation_id.as_deref().unwrap_or("");
    let type_name = location.type_name.as_deref().unwrap_or("");
    match kind {
        EntityKind::Operation => format!("operation {operation_id} ({method} {path})"),
        EntityKind::Result => format!("result '{name}' of {operation_id} ({method} {path})"),
        EntityKind::Argument => format!("argument '{name}' of {operation_id} ({method} {path})"),
        EntityKind::Type => format!("type {type_name}"),
        EntityKind::Field => format!("field {type_name}.{name}"),
        EntityKind::Value => format!("value '{name}' of {type_name}"),
        _ => format!("{kind:?} {name}"),
    }
}

/// The IDs the prompt offers the model: every entity in the index.
pub fn allowed_ids(index: &ContractIndex) -> HashSet<String> {
    index.entities().iter().map(|e| e.id.clone()).collect()
}

/// Build the curator request for one memory's text against the index.
pub fn build_curator_request(
    memory_text: &str,
    index: &ContractIndex,
    model: &str,
) -> ModelRequest {
    let user = format!(
        "Memory:\n{memory_text}\n\nContract elements (cite only these IDs):\n{}",
        render_contract_elements(index)
    );
    ModelRequest {
        model: model.to_string(),
        system: CURATOR_SYSTEM.to_string(),
        user,
        schema: curator_schema(),
        media: Vec::new(),
        // The reply is small, but the models spend reasoning tokens from
        // the same allowance: 512 starved both models in the probe (their
        // replies came back empty with finish_reason length).
        budget: Budget::tokens(2048),
    }
}

/// Parse a curator reply, dropping any ID that was not in `allowed`.
/// Unknown IDs are never trusted: they are removed in code, and when
/// nothing citable remains the verdict is [`CuratorVerdict::RestsOnNothing`].
pub fn parse_curator_value(
    value: &serde_json::Value,
    allowed: &HashSet<String>,
) -> Result<CuratorVerdict, CuratorParseError> {
    let object = value.as_object().ok_or_else(|| CuratorParseError {
        message: "reply is not an object".to_string(),
    })?;
    // Defensive alias only, never observed live: every committed reply
    // uses "element_ids" for the ID list. Accept "rests_on" anyway in case
    // a model ignores the schema's field name; anything else malformed
    // still fails by field. Covered by a synthetic unit test, not by a
    // recording.
    let raw_ids = object
        .get("element_ids")
        .or_else(|| object.get("rests_on"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| CuratorParseError {
            message: "missing array field 'element_ids'".to_string(),
        })?;
    let claim = object
        .get("claim")
        .and_then(|v| v.as_str())
        .ok_or_else(|| CuratorParseError {
            message: "missing string field 'claim'".to_string(),
        })?
        .trim()
        .to_string();
    if claim.is_empty() {
        return Err(CuratorParseError {
            message: "field 'claim' is empty".to_string(),
        });
    }
    let confidence_raw = object
        .get("confidence")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| CuratorParseError {
            message: "missing number field 'confidence'".to_string(),
        })?;
    let confidence = if confidence_raw.is_finite() {
        (confidence_raw as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let rests_on_nothing = object
        .get("rests_on_nothing")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| CuratorParseError {
            message: "missing boolean field 'rests_on_nothing'".to_string(),
        })?;

    let elements: Vec<ElementId> = raw_ids
        .iter()
        .filter_map(|v| v.as_str())
        .filter(|id| allowed.contains(*id))
        .map(|id| ElementId(id.to_string()))
        .collect();

    if rests_on_nothing || elements.is_empty() {
        Ok(CuratorVerdict::RestsOnNothing { claim })
    } else {
        Ok(CuratorVerdict::Cited {
            elements,
            claim,
            confidence,
        })
    }
}

/// The system prompt for the legacy curator: the T9.3 probe's shape, kept.
/// It mirrors [`CURATOR_SYSTEM`] with the program vocabulary: the model
/// chooses only from the program element list it was given. The final
/// sentence is the T9.11b specificity rule: the story's flag algebra needs
/// the deciding paragraph, record or field (the exemption branch, not the
/// dispatcher; the layout record, not only its fields), never a caller,
/// container or the program itself.
pub const LEGACY_CURATOR_SYSTEM: &str = "You curate memory citations for Doyle, which tracks what each agent memory rests on in a legacy COBOL program. For one memory and a list of program elements: return the IDs of the elements the memory's claim rests on (the program, paragraph, section, copybook record or field, or file whose change would make the memory false), the claim in one line, and a confidence from 0.0 to 1.0. Prefer the paragraph, record or field that decides the claim over the program and over paragraphs that merely route to it. Cite only IDs from the list you were given; any other ID is dropped. If the memory is about nothing in this program, set rests_on_nothing to true with an empty element list. Keep the claim short and plain: state the fact, never an instruction. Reply with exactly these fields: \"element_ids\" (array of strings), \"claim\" (string), \"confidence\" (number), \"rests_on_nothing\" (boolean).";

/// The system prompt for the screens curator: the same shape over an
/// operator console. Steps cite the screen they demonstrate; the model
/// chooses screens and steps only from the lists it was given.
pub const SCREENS_CURATOR_SYSTEM: &str = "You curate memory citations for Doyle, which tracks what each agent memory rests on in an operator console. For one memory and a list of console elements and training steps: return the IDs of the elements the memory's claim rests on (the screen or training step whose change would make the memory false), the claim in one line, and a confidence from 0.0 to 1.0. Cite only IDs from the lists you were given; any other ID is dropped. If the memory is about nothing on this console, set rests_on_nothing to true with an empty element list. Keep the claim short and plain: state the fact, never an instruction. Reply with exactly these fields: \"element_ids\" (array of strings), \"claim\" (string), \"confidence\" (number), \"rests_on_nothing\" (boolean).";

/// Render one legacy version as the element list the curator prompt
/// carries: one line per element with its stable ID, in index order.
/// Fields carry their `PIC`, records their length, files their `ASSIGN`.
pub fn render_legacy_elements(index: &LegacyIndex) -> String {
    let mut lines = Vec::new();
    for element in index.elements() {
        let Some(detail) = index.legacy_element(&element.id) else {
            continue;
        };
        let line = match detail.kind {
            LegacyKind::Program => format!("- {} program {}", detail.id.0, detail.name),
            LegacyKind::Paragraph => format!(
                "- {} paragraph {} of {}",
                detail.id.0,
                detail.name,
                index.program()
            ),
            LegacyKind::Section => format!(
                "- {} section {} of {}",
                detail.id.0,
                detail.name,
                index.program()
            ),
            LegacyKind::Record => format!(
                "- {} record {} (copybook {}, {} chars)",
                detail.id.0, detail.name, detail.source, detail.length
            ),
            LegacyKind::Field => {
                format!("- {} field {} PIC {}", detail.id.0, detail.path, detail.pic)
            }
            LegacyKind::File => format!(
                "- {} file {} ASSIGN {}",
                detail.id.0, detail.name, detail.assign
            ),
        };
        lines.push(line);
    }
    lines.join("\n")
}

/// Render one console version as the lists the screens curator carries:
/// the screens, the recording, then the training steps with their spans
/// and the screen each demonstrates.
pub fn render_screen_elements(index: &ScreenIndex) -> String {
    let mut lines = Vec::new();
    for element in index.elements() {
        let name = element
            .native
            .type_name
            .clone()
            .unwrap_or_else(|| element.id.0.clone());
        if element.kind == crate::evidence::EvidenceElementKind::Type {
            lines.push(format!("- {} screen {}", element.id.0, name));
        } else if element.kind
            == crate::evidence::EvidenceElementKind::Other("recording".to_string())
        {
            lines.push(format!("- {} recording {}", element.id.0, name));
        }
    }
    for element in index.elements() {
        if element.kind != crate::evidence::EvidenceElementKind::Other("step".to_string()) {
            continue;
        }
        let description = element
            .native
            .operation_id
            .clone()
            .unwrap_or_else(|| element.id.0.clone());
        let recording = element.native.type_name.clone().unwrap_or_default();
        let span = index
            .step_span(&element.id)
            .map(|(a, b)| format!("{a}-{b} s"))
            .unwrap_or_else(|| "unknown span".to_string());
        let shows = element
            .references
            .first()
            .and_then(|r| index.element(r))
            .and_then(|s| s.native.type_name.clone())
            .map(|s| format!(", shows {s}"))
            .unwrap_or_default();
        lines.push(format!(
            "- {} step '{description}' ({recording}, {span}{shows})",
            element.id.0
        ));
    }
    lines.join("\n")
}

/// The IDs the legacy prompt offers the model: every element of the index.
pub fn allowed_legacy_ids(index: &LegacyIndex) -> HashSet<String> {
    index.elements().iter().map(|e| e.id.0.clone()).collect()
}

/// The IDs the screens prompt offers the model: every element of the index.
pub fn allowed_screen_ids(index: &ScreenIndex) -> HashSet<String> {
    index.elements().iter().map(|e| e.id.0.clone()).collect()
}

/// Build the curator request for one memory's text against a legacy index.
pub fn build_legacy_curator_request(
    memory_text: &str,
    index: &LegacyIndex,
    model: &str,
) -> ModelRequest {
    let user = format!(
        "Memory:\n{memory_text}\n\nProgram elements (cite only these IDs):\n{}",
        render_legacy_elements(index)
    );
    ModelRequest {
        model: model.to_string(),
        system: LEGACY_CURATOR_SYSTEM.to_string(),
        user,
        schema: curator_schema(),
        media: Vec::new(),
        // Same allowance as the contract curator: the models spend
        // reasoning tokens from the same budget before writing a word.
        budget: Budget::tokens(2048),
    }
}

/// Build the curator request for one memory's text against a console index.
pub fn build_screens_curator_request(
    memory_text: &str,
    index: &ScreenIndex,
    model: &str,
) -> ModelRequest {
    let user = format!(
        "Memory:\n{memory_text}\n\nConsole elements (cite only these IDs):\n{}",
        render_screen_elements(index)
    );
    ModelRequest {
        model: model.to_string(),
        system: SCREENS_CURATOR_SYSTEM.to_string(),
        user,
        schema: curator_schema(),
        media: Vec::new(),
        budget: Budget::tokens(2048),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn schema_has_the_four_required_fields() {
        let schema = curator_schema();
        let required = schema["required"].as_array().expect("required is an array");
        let names: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();
        assert_eq!(
            names,
            ["element_ids", "claim", "confidence", "rests_on_nothing"]
        );
        for field in names {
            assert!(
                schema["properties"].get(field).is_some(),
                "schema defines {field}"
            );
        }
    }

    #[test]
    fn request_carries_memory_text_and_element_ids() {
        let index = ContractIndex::load(format!(
            "{}/testdata/contracts/patients-v1.yaml",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("v1 loads");
        let request = build_curator_request("GET /patients returns everyone", &index, "some-model");
        assert!(request.user.contains("GET /patients returns everyone"));
        assert!(
            request
                .user
                .contains("ve2_c81e37b7c42c484a1859c09e95bf8cec89a360db6743a691afab818f177c7bc6")
        );
        assert!(request.user.contains("listPatients"));
        assert_eq!(request.schema, curator_schema());
        assert_eq!(request.system, CURATOR_SYSTEM);
    }

    #[test]
    fn schema_shaped_reply_parses_to_cited() {
        let allow = allowed(&["ve2_op", "ve2_res"]);
        let value = serde_json::json!({
            "element_ids": ["ve2_op", "ve2_res"],
            "claim": "GET /patients returns everyone",
            "confidence": 0.9,
            "rests_on_nothing": false,
        });
        let verdict = parse_curator_value(&value, &allow).expect("parses");
        match verdict {
            CuratorVerdict::Cited {
                elements,
                claim,
                confidence,
            } => {
                assert_eq!(
                    elements,
                    vec![
                        ElementId("ve2_op".to_string()),
                        ElementId("ve2_res".to_string())
                    ]
                );
                assert_eq!(claim, "GET /patients returns everyone");
                assert!((confidence - 0.9).abs() < 1e-6);
            }
            other => panic!("expected Cited, got {other:?}"),
        }
    }

    #[test]
    fn forged_id_is_dropped_in_code() {
        let allow = allowed(&["ve2_real"]);
        let value = serde_json::json!({
            "element_ids": ["ve2_real", "ve2_forged_by_model"],
            "claim": "GET /patients returns everyone",
            "confidence": 0.8,
            "rests_on_nothing": false,
        });
        let verdict = parse_curator_value(&value, &allow).expect("parses");
        match verdict {
            CuratorVerdict::Cited { elements, .. } => {
                assert_eq!(elements, vec![ElementId("ve2_real".to_string())]);
            }
            other => panic!("expected Cited, got {other:?}"),
        }
    }

    #[test]
    fn forged_only_ids_become_rests_on_nothing() {
        let allow = allowed(&["ve2_real"]);
        let value = serde_json::json!({
            "element_ids": ["ve2_invented_a", "ve2_invented_b"],
            "claim": "Something unrelated",
            "confidence": 0.7,
            "rests_on_nothing": false,
        });
        let verdict = parse_curator_value(&value, &allow).expect("parses");
        match verdict {
            CuratorVerdict::RestsOnNothing { claim } => {
                assert_eq!(claim, "Something unrelated");
            }
            other => panic!("expected RestsOnNothing, got {other:?}"),
        }
    }

    #[test]
    fn rests_on_nothing_reply_parses() {
        let allow = allowed(&["ve2_real"]);
        let value = serde_json::json!({
            "element_ids": [],
            "claim": "The nightly report runs at 02:00",
            "confidence": 0.95,
            "rests_on_nothing": true,
        });
        let verdict = parse_curator_value(&value, &allow).expect("parses");
        match verdict {
            CuratorVerdict::RestsOnNothing { claim } => {
                assert_eq!(claim, "The nightly report runs at 02:00");
            }
            other => panic!("expected RestsOnNothing, got {other:?}"),
        }
    }

    #[test]
    // Synthetic-only: "rests_on" was never observed in a live reply; the
    // parser accepts it defensively.
    fn defensive_rests_on_alias_is_accepted() {
        let allow = allowed(&["ve2_real"]);
        let value = serde_json::json!({
            "rests_on": ["ve2_real"],
            "claim": "GET /patients returns everyone",
            "confidence": 0.9,
            "rests_on_nothing": false,
        });
        match parse_curator_value(&value, &allow).expect("parses") {
            CuratorVerdict::Cited { elements, .. } => {
                assert_eq!(elements, vec![ElementId("ve2_real".to_string())]);
            }
            other => panic!("expected Cited, got {other:?}"),
        }
    }

    #[test]
    fn malformed_replies_fail_by_field() {
        let allow = allowed(&["ve2_real"]);
        for (label, value) in [
            ("not-an-object", serde_json::json!("a string")),
            (
                "missing-ids",
                serde_json::json!({"claim": "c", "confidence": 0.5, "rests_on_nothing": false}),
            ),
            (
                "missing-claim",
                serde_json::json!({"element_ids": [], "confidence": 0.5, "rests_on_nothing": true}),
            ),
            (
                "missing-confidence",
                serde_json::json!({"element_ids": [], "claim": "c", "rests_on_nothing": true}),
            ),
            (
                "missing-flag",
                serde_json::json!({"element_ids": [], "claim": "c", "confidence": 0.5}),
            ),
            (
                "empty-claim",
                serde_json::json!({"element_ids": [], "claim": "  ", "confidence": 0.5, "rests_on_nothing": true}),
            ),
        ] {
            let error = parse_curator_value(&value, &allow).expect_err(label);
            assert!(!error.message.is_empty(), "{label}");
            assert!(
                error.to_string().contains("curator reply"),
                "{label}: {error}"
            );
        }
    }

    #[test]
    fn confidence_is_clamped_to_unit_range() {
        let allow = allowed(&["ve2_real"]);
        let high = serde_json::json!({
            "element_ids": ["ve2_real"],
            "claim": "c",
            "confidence": 7.5,
            "rests_on_nothing": false,
        });
        match parse_curator_value(&high, &allow).expect("parses") {
            CuratorVerdict::Cited { confidence, .. } => assert_eq!(confidence, 1.0),
            other => panic!("expected Cited, got {other:?}"),
        }
    }

    // ------------------------------------------------------------------
    // Offline replay of the live measurement in testdata/replies/curator/.
    // No network: every recording is served from disk through ReplayClient.
    // ------------------------------------------------------------------

    fn replies_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/replies/curator")
    }

    fn recorded_cases() -> Vec<(
        String,
        crate::models::ModelRequest,
        crate::models::ModelResponse,
    )> {
        let dir = replies_dir();
        let mut out = Vec::new();
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .expect("replies dir reads")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with("-meta.json") || n.ends_with("-baseten.json"))
            .collect();
        names.sort();
        assert!(!names.is_empty(), "no recordings in {}", dir.display());
        for name in names {
            let raw = std::fs::read_to_string(dir.join(&name)).expect("recording reads");
            let record: serde_json::Value = serde_json::from_str(&raw).expect("recording parses");
            let req = &record["request"];
            let budget = &req["budget"];
            let request = crate::models::ModelRequest {
                model: req["model"].as_str().expect("model").to_string(),
                system: req["system"].as_str().expect("system").to_string(),
                user: req["user"].as_str().expect("user").to_string(),
                schema: req["schema"].clone(),
                budget: crate::models::Budget {
                    max_output_tokens: budget["max_output_tokens"].as_u64().map(|n| n as u32),
                    max_cost_usd: budget["max_cost_usd"].as_f64(),
                },
                media: Vec::new(),
            };
            let response: crate::models::ModelResponse =
                serde_json::from_value(record["response"].clone()).expect("response parses");
            out.push((name, request, response));
        }
        out
    }

    #[test]
    fn recorded_replies_replay_offline_with_stable_prompt() {
        use crate::models::{ModelClient, ReplayClient, request_key};
        let dir = std::env::temp_dir().join(format!(
            "doyle-t43-replay-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let index = ContractIndex::load(format!(
            "{}/testdata/contracts/patients-v1.yaml",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("v1 loads");
        let allow = allowed_ids(&index);
        for (name, request, expected) in recorded_cases() {
            // The prompt is pinned: every recording must carry this module's
            // exact system prompt and schema, or the comparison is void.
            assert_eq!(request.system, CURATOR_SYSTEM, "{name}");
            assert_eq!(request.schema, curator_schema(), "{name}");
            // Serve the recorded reply through the replay client: what the
            // models said replays byte-identical with no network.
            let key = request_key(&request);
            std::fs::write(
                dir.join(format!("{key}.json")),
                serde_json::to_string_pretty(&expected).expect("serialize"),
            )
            .expect("write replay");
            let client = ReplayClient::new(&dir);
            let replayed = client.complete(&request).expect("replay must hit");
            assert_eq!(replayed, expected, "{name}");
            // The reply parses to a well-formed verdict citing only known IDs.
            match parse_curator_value(&replayed.value, &allow).expect("parses") {
                CuratorVerdict::Cited {
                    elements, claim, ..
                } => {
                    assert!(!elements.is_empty(), "{name}");
                    assert!(!claim.is_empty(), "{name}");
                }
                CuratorVerdict::RestsOnNothing { claim } => {
                    assert!(!claim.is_empty(), "{name}");
                }
            }
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn recorded_outside_contract_memories_rest_on_nothing() {
        let index = ContractIndex::load(format!(
            "{}/testdata/contracts/patients-v1.yaml",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("v1 loads");
        let allow = allowed_ids(&index);
        // Unanimous across both models in the measurement: the report time
        // (X4), the Lambo warning (X8) and the nonexistent POST (X11) are
        // about nothing in this contract.
        for name in [
            "X4-meta.json",
            "X4-baseten.json",
            "X8-meta.json",
            "X8-baseten.json",
            "X11-meta.json",
            "X11-baseten.json",
        ] {
            let raw = std::fs::read_to_string(replies_dir().join(name)).expect("reads");
            let record: serde_json::Value = serde_json::from_str(&raw).expect("parses");
            match parse_curator_value(&record["response"]["value"], &allow).expect("parses") {
                CuratorVerdict::RestsOnNothing { .. } => {}
                other => panic!("{name}: expected RestsOnNothing, got {other:?}"),
            }
        }
    }

    #[test]
    fn recorded_replies_match_expected_set_scores() {
        let expected: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(replies_dir().join("expected.json")).expect("reads"),
        )
        .expect("parses");
        let id_of = |key: &str| {
            expected["elements"][key]
                .as_str()
                .expect("known element key")
                .to_string()
        };
        let index = ContractIndex::load(format!(
            "{}/testdata/contracts/patients-v1.yaml",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("v1 loads");
        let allow = allowed_ids(&index);
        for model in ["meta", "baseten"] {
            let (mut tp, mut fp, mut f_n) = (0u32, 0u32, 0u32);
            for (case, spec) in expected["cases"].as_object().expect("cases").iter() {
                let raw =
                    std::fs::read_to_string(replies_dir().join(format!("{case}-{model}.json")))
                        .expect("recording reads");
                let record: serde_json::Value = serde_json::from_str(&raw).expect("parses");
                let predicted: std::collections::HashSet<String> = match parse_curator_value(
                    &record["response"]["value"],
                    &allow,
                )
                .expect("parses")
                {
                    CuratorVerdict::Cited { elements, .. } => {
                        elements.into_iter().map(|e| e.0).collect()
                    }
                    CuratorVerdict::RestsOnNothing { .. } => std::collections::HashSet::new(),
                };
                let wanted: std::collections::HashSet<String> = spec
                    .get("expected")
                    .and_then(|v| v.as_array())
                    .map(|ids| ids.iter().filter_map(|v| v.as_str()).map(id_of).collect())
                    .unwrap_or_default();
                let accepted: std::collections::HashSet<String> = spec
                    .get("also_accepted")
                    .and_then(|v| v.as_array())
                    .map(|ids| ids.iter().filter_map(|v| v.as_str()).map(id_of).collect())
                    .unwrap_or_default();
                let good: std::collections::HashSet<String> =
                    wanted.union(&accepted).cloned().collect();
                tp += predicted.intersection(&good).count() as u32;
                fp += predicted.difference(&good).count() as u32;
                f_n += wanted.difference(&predicted).count() as u32;
            }
            let precision = tp as f64 / (tp + fp) as f64;
            let recall = tp as f64 / (tp + f_n) as f64;
            let (want_p, want_r) = match model {
                "meta" => (27.0 / 28.0, 27.0 / 28.0),
                _ => (31.0 / 35.0, 31.0 / 32.0),
            };
            assert!(
                (precision - want_p).abs() < 1e-9,
                "{model}: precision {precision} != {want_p}"
            );
            assert!(
                (recall - want_r).abs() < 1e-9,
                "{model}: recall {recall} != {want_r}"
            );
        }
    }
}

// ------------------------------------------------------------------
// Legacy and screens curator (T9.11b): rendering and request shape.
// Replayed citation tests land with the live recordings; these pin the
// prompt shape offline.
// ------------------------------------------------------------------

#[cfg(test)]
mod legacy_screens_tests {
    use super::*;
    use crate::evidence::legacy::ids as legacy_ids;

    fn story(rel: &str) -> String {
        format!("{}/testdata/legacy/story/{rel}", env!("CARGO_MANIFEST_DIR"))
    }

    fn v1_index() -> crate::evidence::legacy::LegacyIndex {
        crate::evidence::legacy::LegacyIndex::load(story("v1/BILLBATCH.cbl")).expect("v1 loads")
    }

    /// The combined v1 screens version the check screens source reads:
    /// both stills plus the training steps, built the way the story e2e
    /// builds it. Returns the file path and the loaded index.
    fn combined_v1() -> (std::path::PathBuf, crate::evidence::screens::ScreenIndex) {
        let lookup: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(story("screens/lookup-v1.json")).unwrap(),
        )
        .unwrap();
        let decomm: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(story("screens/decomm-v1.json")).unwrap(),
        )
        .unwrap();
        let steps: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(story("training-v1-steps.json")).unwrap(),
        )
        .unwrap();
        let dir = std::env::temp_dir().join(format!(
            "doyle-cur-t911b-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("screens-v1.json");
        let screen_doc = |doc: &serde_json::Value| serde_json::json!({"name": doc["screen"], "elements": doc["elements"]});
        let step_rows: Vec<serde_json::Value> = steps["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                let screen = match s["id"].as_str().unwrap() {
                    "T2" | "T3" | "T4" => "lookup",
                    "T5" | "T6" => "decommission",
                    _ => "",
                };
                serde_json::json!({
                    "description": s["description"],
                    "start_s": s["start_s"],
                    "end_s": s["end_s"],
                    "screen": screen,
                })
            })
            .collect();
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&serde_json::json!({
                "console": "fernwhistle",
                "screens": [screen_doc(&lookup), screen_doc(&decomm)],
                "steps": step_rows,
                "recording": {"name": "training-v1", "duration_s": 45.0},
            }))
            .unwrap(),
        )
        .unwrap();
        let (index, _) = crate::check::load_screen_version(path.to_str().unwrap()).expect("loads");
        (dir, index)
    }

    #[test]
    fn legacy_elements_list_every_index_element_once() {
        let index = v1_index();
        let listing = render_legacy_elements(&index);
        let lines: Vec<&str> = listing.lines().collect();
        assert_eq!(lines.len(), index.elements().len());
        assert!(
            listing.contains(&format!(
                "- {} program BILLBATCH",
                legacy_ids::element_id(&legacy_ids::program_key("BILLBATCH")).0
            )),
            "{listing}"
        );
        assert!(
            listing.contains("paragraph CALC-LATE-FEE of BILLBATCH"),
            "{listing}"
        );
        assert!(
            listing.contains("field PATIENT-RECORD.BALANCE PIC 9(5)V99"),
            "{listing}"
        );
        assert!(
            listing.contains("record PATIENT-RECORD (copybook patient.cpy, 34 chars)"),
            "{listing}"
        );
        assert!(
            listing.contains("file ACCT-FILE ASSIGN \"patients.dat\""),
            "{listing}"
        );
    }

    #[test]
    fn legacy_request_carries_memory_and_system_and_budget() {
        let index = v1_index();
        let request = build_legacy_curator_request("some billing rule", &index, "some-model");
        assert_eq!(request.model, "some-model");
        assert_eq!(request.system, LEGACY_CURATOR_SYSTEM);
        assert_eq!(request.schema, curator_schema());
        assert_eq!(request.budget, Budget::tokens(2048));
        assert!(request.user.contains("some billing rule"));
        assert!(
            request
                .user
                .contains("Program elements (cite only these IDs):")
        );
        assert!(request.media.is_empty());
    }

    #[test]
    fn legacy_system_names_the_program_vocabulary() {
        assert!(LEGACY_CURATOR_SYSTEM.contains("legacy COBOL program"));
        assert!(SCREENS_CURATOR_SYSTEM.contains("operator console"));
        for system in [LEGACY_CURATOR_SYSTEM, SCREENS_CURATOR_SYSTEM] {
            assert!(system.contains("rests_on_nothing"));
            assert!(system.contains("never an instruction"));
        }
    }

    #[test]
    fn screens_request_lists_screens_and_steps_with_spans() {
        // The version shape the check screens source reads: the combined
        // v1 file (stills + steps) the story e2e builds.
        let (dir, index) = combined_v1();
        let request = build_screens_curator_request("some console rule", &index, "some-model");
        assert_eq!(request.system, SCREENS_CURATOR_SYSTEM);
        assert_eq!(request.schema, curator_schema());
        assert!(
            request
                .user
                .contains("Console elements (cite only these IDs):")
        );
        assert!(request.user.contains("screen lookup"));
        assert!(request.user.contains("screen decommission"));
        assert!(request.user.contains("training-v1"));
        assert!(
            request.user.contains("28-33 s"),
            "T5 span: {}",
            request.user
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn screens_system_and_allowed_ids_cover_screens_and_steps() {
        let (dir, index) = combined_v1();
        let allowed = allowed_screen_ids(&index);
        assert_eq!(allowed.len(), index.elements().len());
        let listing = render_screen_elements(&index);
        assert!(listing.contains("screen decommission"), "{listing}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_allowed_ids_are_exactly_the_index() {
        let index = v1_index();
        let allowed = allowed_legacy_ids(&index);
        assert_eq!(allowed.len(), index.elements().len());
        assert!(allowed.contains(
            &legacy_ids::element_id(&legacy_ids::paragraph_key("BILLBATCH", "CALC-LATE-FEE")).0
        ));
    }
}
