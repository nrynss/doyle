//! The re-check prompt (T4.4): a flagged memory + the new contract →
//! correct, retire, or keep.
//!
//! The re-check runs on Muse Spark ([`MODEL`]) through T4.1's client.
//! [`build_request`] renders a [`ModelRequest`](crate::models::ModelRequest)
//! from a [`RecheckInput`]; [`parse_response`] maps the structured reply onto
//! Doyle's [`ProposalAction`](crate::core::ProposalAction) plus a rationale.
//!
//! Guideline 7 — the no-instruction-quote rule — as this module enforces it:
//! replacement text is short plain prose stating what is true of the new
//! contract. It must not quote contract phrasing that could read as an
//! instruction. Operationally ([`check_replacement_text`]): at most
//! [`MAX_REPLACEMENT_CHARS`] characters, a single line, no markdown (no
//! backticks of any kind, no leading `#`); no instruction verb from
//! [`FORBIDDEN_CLAUSE_OPENERS`] used as a whole word anywhere in the text
//! (word boundaries, not raw substrings); and none of the contract-derived
//! instruction phrases in [`FORBIDDEN_CONTRACT_PHRASES`], with inflection
//! coverage (`permanently delet…`, `confirmation token(s)`). What the check
//! cannot see — novel instruction phrasing — stays the reviewer's job on the
//! review page.

use crate::core::ProposalAction;
use crate::models::{Budget, ModelRequest};

/// The model that runs the re-check (product.md: one job each).
pub const MODEL: &str = "muse-spark-1.3";

/// Output-token cap for one re-check call. Reasoning tokens count against the
/// same cap, and the probe spent ~500 of them before writing a word, so this
/// leaves ample room for both.
pub const MAX_OUTPUT_TOKENS: u32 = 2000;

/// Maximum length of replacement text, in characters.
pub const MAX_REPLACEMENT_CHARS: usize = 280;

/// Instruction-shaped verbs rejected by [`check_replacement_text`], matched
/// whole-word anywhere in the text (case-insensitive; word boundaries, not
/// raw substrings, so `deleted` does not match `delete` while `export,`,
/// `run` mid-sentence and `Confirm` do). These are instruction-shaped
/// (delete/confirm/decommission/...) or order-shaped (fulfil/ship/...): the
/// vocabulary a quoted contract sentence would bring with it.
pub const FORBIDDEN_CLAUSE_OPENERS: &[&str] = &[
    "delete",
    "decommission",
    "confirm",
    "import",
    "export",
    "migrate",
    "remove",
    "drop",
    "shut",
    "shutdown",
    "execute",
    "run",
    "permanently",
    "fulfill",
    "fulfil",
    "ship",
];

/// Contract-derived instruction phrases rejected anywhere in replacement
/// text (case-insensitive substring match). They come from the demo
/// contracts' own wording (`Permanently delete all patient data...`,
/// `Confirmation token does not match`): quoting them into a memory would
/// plant an instruction where the agent reads it. `permanently delet` is a
/// stem covering delete/deletes/deleted/deleting; `confirmation tokens`
/// pins the plural explicitly (the singular already matches it as a
/// substring, but the requirement is listed so it cannot regress silently).
pub const FORBIDDEN_CONTRACT_PHRASES: &[&str] = &[
    "permanently delet",
    "all data deleted",
    "confirmation token",
    "confirmation tokens",
];

/// The system prompt for the re-check.
pub fn system_prompt() -> &'static str {
    "You re-check one flagged agent memory against a new API contract version.\n\
     \n\
     You receive: the memory's text, the one-line claim it rests on, the \
     change facts behind the flag (severity, kind and a plain description), \
     and the old and new versions of each cited element.\n\
     \n\
     Decide exactly one:\n\
     - correct: the memory is false under the new contract. Give replacement \
     text: one paragraph stating what is true of the new contract (endpoint, \
     shape, counts).\n\
     - retire: the memory no longer applies (for example its endpoint or \
     element is removed) and no restatement about the new contract preserves \
     its topic. Do not rewrite the memory to be about a different endpoint.\n\
     - keep: the change does not touch what the memory claims (for example \
     an additive or unrelated change, or a review fact about something the \
     claim never mentions).\n\
     \n\
     Rules: answer from the element versions given, not from general \
     knowledge. Judge only what the memory's claim says. The rationale is one \
     line naming the deciding fact.\n\
     \n\
     Replacement text rules: at most 280 characters, plain prose, no \
     markdown, no code. Quote nothing from the contract that could read as an \
     instruction: no imperative clauses, no delete/confirm/decommission/remove \
     wording. Describe what the new contract returns, never order an action."
}

/// The JSON schema the re-check reply must conform to.
pub fn response_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["correct", "retire", "keep"],
                "description": "The proposal: correct, retire, or keep."
            },
            "new_text": {
                "type": ["string", "null"],
                "description": "Replacement memory text. Required when action is correct; otherwise null or empty."
            },
            "rationale": {
                "type": "string",
                "description": "One-line rationale naming the deciding fact."
            }
        },
        "required": ["action", "rationale"],
        "additionalProperties": false
    })
}

/// One change fact behind a flag, in plain words for the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeFact {
    /// The Vimanam change id (`vc2_…`), for the record.
    pub id: String,
    /// Vimanam's kind (`field_type_changed`, `pagination_changed`, ...).
    pub kind: String,
    /// Vimanam's severity (`breaking`, `review`, ...).
    pub severity: String,
    /// One plain sentence saying what changed.
    pub description: String,
}

/// One cited element, old and new versions side by side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitedElement {
    /// Vimanam's stable entity id (`ve2_…`).
    pub element_id: String,
    /// Human name, e.g. `200 response of listPatients (GET /patients)`.
    pub name: String,
    /// What the element looked like in the old version.
    pub old: String,
    /// What it looks like in the new version (`(removed in v2...)` when gone).
    pub new: String,
}

/// Everything the re-check prompt needs for one flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecheckInput {
    /// The flagged memory's text.
    pub memory_text: String,
    /// The one-line claim the citation recorded.
    pub claim: String,
    /// The change facts behind the flag.
    pub changes: Vec<ChangeFact>,
    /// Old and new versions of each cited element.
    pub elements: Vec<CitedElement>,
    /// Old contract version label (e.g. `v1`).
    pub old_version: String,
    /// New contract version label (e.g. `v2`).
    pub new_version: String,
}

/// Render the user content for a re-check of `input`.
pub fn build_user_prompt(input: &RecheckInput) -> String {
    let mut out = String::new();
    out.push_str("Memory:\n");
    out.push_str(&input.memory_text);
    out.push_str("\n\nClaim:\n");
    out.push_str(&input.claim);
    out.push_str(&format!(
        "\n\nContract change: {} -> {}\n",
        input.old_version, input.new_version
    ));
    out.push_str("\nChanges behind the flag:\n");
    for change in &input.changes {
        out.push_str(&format!(
            "- [{}] {} ({}): {}\n",
            change.severity, change.kind, change.id, change.description
        ));
    }
    out.push_str("\nCited elements:\n");
    for element in &input.elements {
        out.push_str(&format!("- {} ({})\n", element.name, element.element_id));
        out.push_str(&format!("  OLD ({}): {}\n", input.old_version, element.old));
        out.push_str(&format!("  NEW ({}): {}\n", input.new_version, element.new));
    }
    out.push_str(
        "\nPropose: correct, retire, or keep, with replacement text and a one-line rationale.\n",
    );
    out
}

/// Build the [`ModelRequest`](crate::models::ModelRequest) for one re-check.
pub fn build_request(input: &RecheckInput) -> ModelRequest {
    ModelRequest {
        model: MODEL.to_string(),
        system: system_prompt().to_string(),
        user: build_user_prompt(input),
        schema: response_schema(),
        media: Vec::new(),
        budget: Budget::tokens(MAX_OUTPUT_TOKENS),
    }
}

/// A parsed re-check proposal: the action plus the one-line rationale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecheckProposal {
    pub action: ProposalAction,
    pub rationale: String,
}

/// What can go wrong parsing a re-check reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "re-check: unparsable reply: {}", self.0)
    }
}

impl std::error::Error for ParseError {}

/// Parse one structured re-check reply into a [`RecheckProposal`].
///
/// `correct` requires non-empty `new_text` that passes
/// [`check_replacement_text`]; `retire` and `keep` require `new_text` to be
/// absent, null or empty.
pub fn parse_response(value: &serde_json::Value) -> Result<RecheckProposal, ParseError> {
    let reject = |why: &str| ParseError(format!("{why}; reply was {value}"));
    let action = value
        .get("action")
        .and_then(|a| a.as_str())
        .ok_or_else(|| reject("missing string field `action`"))?;
    let rationale = value
        .get("rationale")
        .and_then(|r| r.as_str())
        .ok_or_else(|| reject("missing string field `rationale`"))?;
    let rationale = rationale.trim();
    if rationale.is_empty() {
        return Err(reject("`rationale` is empty"));
    }
    let new_text = value.get("new_text").and_then(|t| t.as_str()).unwrap_or("");
    match action {
        "correct" => {
            let text = new_text.trim();
            if text.is_empty() {
                return Err(reject("`correct` needs non-empty `new_text`"));
            }
            check_replacement_text(text).map_err(reject)?;
            Ok(RecheckProposal {
                action: ProposalAction::Correct {
                    new_text: text.to_string(),
                },
                rationale: rationale.to_string(),
            })
        }
        "retire" => {
            if !new_text.trim().is_empty() {
                return Err(reject("`retire` must leave `new_text` empty"));
            }
            Ok(RecheckProposal {
                action: ProposalAction::Retire,
                rationale: rationale.to_string(),
            })
        }
        "keep" => {
            if !new_text.trim().is_empty() {
                return Err(reject("`keep` must leave `new_text` empty"));
            }
            Ok(RecheckProposal {
                action: ProposalAction::Keep,
                rationale: rationale.to_string(),
            })
        }
        other => Err(reject(&format!("unknown `action` {other:?}"))),
    }
}

/// Enforce guideline 7 on replacement text. See the module docs for the rule.
///
/// Trade-off, chosen deliberately: the verbs are matched whole-word anywhere
/// in the text, so an imperative removal description ("remove the old
/// endpoint") would be rejected as well, while past-tense description
/// ("the endpoint was removed") passes. That is accepted because removals
/// normally resolve to `retire`, which carries no replacement text at all;
/// the check errs toward rejecting instruction-shaped wording and leaves
/// the rest to the reviewer on the review page.
pub fn check_replacement_text(text: &str) -> Result<(), &'static str> {
    if text.trim().is_empty() {
        return Err("replacement text is empty");
    }
    if text.chars().count() > MAX_REPLACEMENT_CHARS {
        return Err("replacement text exceeds 280 characters");
    }
    if text.contains('\n') || text.contains('\r') {
        return Err("replacement text must be a single line");
    }
    // Any backtick: triple-fence blocks and single-backtick code spans alike.
    if text.contains('`') || text.trim_start().starts_with('#') {
        return Err("replacement text must be plain prose, no markdown");
    }
    let lowered = text.to_lowercase();
    for phrase in FORBIDDEN_CONTRACT_PHRASES {
        if lowered.contains(phrase) {
            return Err("replacement text quotes contract instruction phrasing");
        }
    }
    for verb in FORBIDDEN_CLAUSE_OPENERS {
        if contains_word(&lowered, verb) {
            return Err("replacement text uses instruction wording");
        }
    }
    Ok(())
}

/// Whole-word search over already-lowercased text: `word` matches only where
/// the characters on both sides are not word characters (alphanumeric or
/// `_`, so `next_cursor` is one word). Boundaries — not raw substrings —
/// so `deleted` never matches `delete`, while `export,` and a mid-sentence
/// `run` do.
fn contains_word(haystack: &str, word: &str) -> bool {
    let haystack: Vec<char> = haystack.chars().collect();
    let word: Vec<char> = word.chars().collect();
    if word.is_empty() || word.len() > haystack.len() {
        return false;
    }
    let is_word_char = |c: char| c.is_alphanumeric() || c == '_';
    (0..=(haystack.len() - word.len())).any(|i| {
        haystack[i..(i + word.len())] == word[..]
            && (i == 0 || !is_word_char(haystack[i - 1]))
            && (i + word.len() == haystack.len() || !is_word_char(haystack[i + word.len()]))
    })
}

/// One measurement case: a fixed input plus the answer the re-check must give.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeasurementCase {
    /// Short name; also the stem of the notes in MEASUREMENT.md.
    pub name: &'static str,
    /// The prompt input.
    pub input: RecheckInput,
    /// `correct`, `retire` or `keep`.
    pub expected: &'static str,
}

fn change(id: &str, kind: &str, severity: &str, description: &str) -> ChangeFact {
    ChangeFact {
        id: id.to_string(),
        kind: kind.to_string(),
        severity: severity.to_string(),
        description: description.to_string(),
    }
}

fn element(id: &str, name: &str, old: &str, new: &str) -> CitedElement {
    CitedElement {
        element_id: id.to_string(),
        name: name.to_string(),
        old: old.to_string(),
        new: new.to_string(),
    }
}

/// The committed measurement set: the demo's M1 flag (expects `correct`),
/// a removed-endpoint flag (expects `retire`), a type-level review that
/// touches nothing the memory claims (expects `keep`), and a flag on an
/// element the memory never cites (expects `keep`).
///
/// IDs come from `testdata/vimanam/README.md`, except `s1_retire`, whose
/// IDs are constructed (the shape mirrors the real S1 removal; the IDs
/// themselves are synthetic, so they appear nowhere under
/// `testdata/vimanam/`). Memory texts come from `testdata/README.md` and
/// `testdata/contracts/second/`.
pub fn measurement_cases() -> Vec<MeasurementCase> {
    vec![
        MeasurementCase {
            name: "m1_correct",
            input: RecheckInput {
                memory_text: "GET /patients returns every patient in one response; one call exports the whole clinic"
                    .to_string(),
                claim: "GET /patients returns every patient in one response"
                    .to_string(),
                changes: vec![
                    change(
                        "vc2_9bedbbb47825fa87ea41aa38342d0492948286a7dd4a5ecbb378310261df6841",
                        "field_type_changed",
                        "breaking",
                        "the 200 response of GET /patients changed from a bare array of patients to an object",
                    ),
                    change(
                        "vc2_d7e1b2acbebe45b089106951b949710e3112d1586ed74cfe5fab47d702987277",
                        "constraint_changed",
                        "review",
                        "the top-level patient items of the 200 response were removed",
                    ),
                    change(
                        "vc2_d82adaee7e47799b8027d3c1b9f458e16be8a20df1696186f351369f07e011e0",
                        "pagination_changed",
                        "review",
                        "GET /patients now pages: optional cursor and limit (default 100, maximum 100) with a next_cursor continuation",
                    ),
                ],
                elements: vec![
                    element(
                        "ve2_c81e37b7c42c484a1859c09e95bf8cec89a360db6743a691afab818f177c7bc6",
                        "listPatients (GET /patients)",
                        "no query arguments; 200 returns a bare array of every Patient",
                        "optional query arguments cursor (string) and limit (integer, default 100, maximum 100); 200 returns a PatientPage object with items (up to 100 Patients) and next_cursor",
                    ),
                    element(
                        "ve2_86d3cdb7ce3a7f0b1a2fefa696f0acbbcf2d5406c717bb1f1f1cb5f4f62667ba",
                        "200 response of listPatients",
                        "application/json, a bare array holding every patient",
                        "application/json, a PatientPage object: items plus next_cursor",
                    ),
                ],
                old_version: "v1".to_string(),
                new_version: "v2".to_string(),
            },
            expected: "correct",
        },
        MeasurementCase {
            name: "s1_retire",
            // constructed: shape mirrors the real S1 removal, IDs synthetic
            input: RecheckInput {
                memory_text: "POST /charges captures the payment at once; a 200 means the customer paid, fulfill the order"
                    .to_string(),
                claim: "POST /charges captures the payment at once".to_string(),
                changes: vec![change(
                    "vc2_removed_create_charge",
                    "operation_removed",
                    "breaking",
                    "POST /charges (createCharge) was removed; the operation no longer exists",
                )],
                elements: vec![element(
                    "ve2_removed_create_charge",
                    "createCharge (POST /charges)",
                    "POST /charges with amount, currency and card_token captures the payment at once; a 200 means the customer paid",
                    "(removed in v2: POST /charges no longer exists)",
                )],
                old_version: "v1".to_string(),
                new_version: "v2".to_string(),
            },
            expected: "retire",
        },
        MeasurementCase {
            name: "m3_keep",
            input: RecheckInput {
                memory_text: "GET /stats patient_count is the total number of patients"
                    .to_string(),
                claim: "GET /stats patient_count is the total number of patients".to_string(),
                changes: vec![change(
                    "vc2_6cfe1d7e93fec49f1e8a37c39c64364e23dbcd0ae3384e882ee8850d533903e0",
                    "field_added",
                    "review",
                    "the Stats type gained an optional generated_at timestamp alongside patient_count",
                )],
                elements: vec![element(
                    "ve2_c09c07ec8983f09afc5dafed67ee0e3e0d271de3b8f8f1bd61daa1111f063809",
                    "Stats (type)",
                    "an object with patient_count (integer, required)",
                    "an object with patient_count (integer, required) plus an optional generated_at timestamp",
                )],
                old_version: "v1".to_string(),
                new_version: "v2".to_string(),
            },
            expected: "keep",
        },
        MeasurementCase {
            name: "m4_keep_unchanged",
            input: RecheckInput {
                memory_text: "POST /admin/decommission is permanent once confirmed; there is no undo"
                    .to_string(),
                claim: "POST /admin/decommission is permanent once confirmed".to_string(),
                changes: vec![change(
                    "vc2_d82adaee7e47799b8027d3c1b9f458e16be8a20df1696186f351369f07e011e0",
                    "pagination_changed",
                    "review",
                    "GET /patients now pages: optional cursor and limit (default 100, maximum 100) with a next_cursor continuation",
                )],
                elements: vec![element(
                    "ve2_9e9663295485ddf808651f117c8ca42ab58e33fbe49a27309e0547f77c866846",
                    "decommission (POST /admin/decommission)",
                    "permanently deletes all patient data once the confirmation token matches; no undo",
                    "identical in v2: permanently deletes all patient data once the confirmation token matches; no undo",
                )],
                old_version: "v1".to_string(),
                new_version: "v2".to_string(),
            },
            expected: "keep",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replay_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/replies/recheck")
    }

    #[test]
    fn schema_has_three_actions_and_rationale() {
        let schema = response_schema();
        let action = &schema["properties"]["action"];
        assert_eq!(
            action["enum"],
            serde_json::json!(["correct", "retire", "keep"])
        );
        let required = schema["required"]
            .as_array()
            .expect("required must be an array");
        assert!(required.contains(&serde_json::json!("action")));
        assert!(required.contains(&serde_json::json!("rationale")));
    }

    #[test]
    fn request_targets_spark_with_schema_and_budget() {
        let cases = measurement_cases();
        let request = build_request(&cases[0].input);
        assert_eq!(request.model, MODEL);
        assert_eq!(request.schema, response_schema());
        assert_eq!(request.budget, Budget::tokens(MAX_OUTPUT_TOKENS));
        assert!(request.user.contains("GET /patients"));
        assert!(!request.system.is_empty());
    }

    #[test]
    fn parse_correct() {
        let value = serde_json::json!({
            "action": "correct",
            "new_text": "GET /patients returns one page of up to 100 plus next_cursor",
            "rationale": "v2 pages at 100 with a cursor"
        });
        let got = parse_response(&value).expect("must parse");
        assert_eq!(
            got.action,
            ProposalAction::Correct {
                new_text: "GET /patients returns one page of up to 100 plus next_cursor"
                    .to_string()
            }
        );
        assert_eq!(got.rationale, "v2 pages at 100 with a cursor");
    }

    #[test]
    fn parse_retire() {
        let value = serde_json::json!({
            "action": "retire",
            "new_text": null,
            "rationale": "the endpoint no longer exists"
        });
        let got = parse_response(&value).expect("must parse");
        assert_eq!(got.action, ProposalAction::Retire);
    }

    #[test]
    fn parse_keep_with_missing_new_text() {
        let value = serde_json::json!({
            "action": "keep",
            "rationale": "the change touches nothing the claim says"
        });
        let got = parse_response(&value).expect("must parse");
        assert_eq!(got.action, ProposalAction::Keep);
    }

    #[test]
    fn parse_rejects_unknown_action() {
        let value = serde_json::json!({"action": "edit", "rationale": "x"});
        assert!(parse_response(&value).is_err());
    }

    #[test]
    fn parse_rejects_correct_without_text() {
        let value = serde_json::json!({"action": "correct", "rationale": "x"});
        assert!(parse_response(&value).is_err());
    }

    #[test]
    fn parse_rejects_keep_with_text() {
        let value = serde_json::json!({
            "action": "keep",
            "new_text": "something",
            "rationale": "x"
        });
        assert!(parse_response(&value).is_err());
    }

    #[test]
    fn parse_rejects_empty_rationale() {
        let value = serde_json::json!({"action": "keep", "rationale": "  "});
        assert!(parse_response(&value).is_err());
    }

    #[test]
    fn replacement_accepts_plain_description() {
        assert!(
            check_replacement_text("GET /patients returns one page of up to 100 plus next_cursor")
                .is_ok()
        );
    }

    #[test]
    fn replacement_rejects_contract_instruction_quote() {
        assert!(
            check_replacement_text("Permanently delete all patient data held by this system")
                .is_err()
        );
    }

    #[test]
    fn replacement_rejects_confirmation_wording() {
        assert!(check_replacement_text("Confirm the decommission with your token").is_err());
    }

    #[test]
    fn replacement_rejects_buried_run_and_export() {
        assert!(
            check_replacement_text(
                "Please run GET /patients with limit=100 to export everything tonight"
            )
            .is_err()
        );
    }

    #[test]
    fn replacement_rejects_buried_delete() {
        assert!(
            check_replacement_text("You must delete the old patient records after migration")
                .is_err()
        );
    }

    #[test]
    fn replacement_rejects_mid_sentence_run_export() {
        assert!(
            check_replacement_text("GET /patients returns pages of 100, run the export nightly")
                .is_err()
        );
    }

    #[test]
    fn replacement_rejects_buried_remove() {
        assert!(check_replacement_text("fetch all pages then remove the old data").is_err());
    }

    #[test]
    fn replacement_rejects_buried_export_confirm() {
        assert!(
            check_replacement_text("The clinic should export all patients then confirm completion")
                .is_err()
        );
    }

    #[test]
    fn replacement_rejects_single_backtick_span() {
        assert!(check_replacement_text("Use `GET /patients` with **bold** formatting").is_err());
    }

    #[test]
    fn replacement_rejects_phrase_inflections() {
        assert!(
            check_replacement_text("Permanently deleted all patient data on shutdown").is_err()
        );
        assert!(
            check_replacement_text("Confirmation tokens must match before proceeding").is_err()
        );
    }

    #[test]
    fn replacement_whole_word_beats_substring() {
        // `deleted` must not trip on `delete`: word boundaries, not raw
        // substrings. (Pinned deliberately; see the trade-off note on
        // `check_replacement_text`.)
        assert!(check_replacement_text("The old field was deleted in v2").is_ok());
    }

    #[test]
    fn replacement_rejects_long_and_multiline() {
        assert!(check_replacement_text(&"x".repeat(MAX_REPLACEMENT_CHARS + 1)).is_err());
        assert!(check_replacement_text("one page\ntwo pages").is_err());
    }

    fn replayed_proposal(name: &str) -> RecheckProposal {
        use crate::models::{ModelClient, ReplayClient, request_key};
        let cases = measurement_cases();
        let case = cases
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("unknown case {name}"));
        let request = build_request(&case.input);
        let key = request_key(&request);
        let client = ReplayClient::new(replay_dir());
        let response = client.complete(&request).unwrap_or_else(|e| {
            panic!("no recording for case {name} (key {key}): {e}");
        });
        parse_response(&response.value).unwrap_or_else(|e| {
            panic!("case {name}: recorded reply does not parse: {e}");
        })
    }

    #[test]
    fn replay_demo_flag_proposes_correct() {
        let got = replayed_proposal("m1_correct");
        match &got.action {
            ProposalAction::Correct { new_text } => {
                check_replacement_text(new_text).expect("recorded text obeys guideline 7");
                let lowered = new_text.to_lowercase();
                assert!(
                    lowered.contains("100") || lowered.contains("page"),
                    "correction must state the paging truth: {new_text}"
                );
                assert!(
                    lowered.contains("cursor") || lowered.contains("page"),
                    "correction must name the continuation: {new_text}"
                );
            }
            other => panic!("m1 must propose correct, got {other:?}"),
        }
        assert!(!got.rationale.trim().is_empty());
    }

    #[test]
    fn replay_removed_endpoint_proposes_retire() {
        let got = replayed_proposal("s1_retire");
        assert_eq!(got.action, ProposalAction::Retire);
        assert!(!got.rationale.trim().is_empty());
    }

    #[test]
    fn replay_type_review_proposes_keep() {
        let got = replayed_proposal("m3_keep");
        assert_eq!(got.action, ProposalAction::Keep);
    }

    #[test]
    fn replay_unrelated_change_proposes_keep() {
        let got = replayed_proposal("m4_keep_unchanged");
        assert_eq!(got.action, ProposalAction::Keep);
    }
}
