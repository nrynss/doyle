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
use crate::core::{ChangeId, Citation, ElementId, Flag};
use crate::evidence::EvidenceSource;
use crate::evidence::diffrun::{DiffrunFlag, mismatch_evidence};
use crate::evidence::legacy::{LegacyChange, LegacyChangeKind, LegacyIndex};
use crate::evidence::screens::ScreenIndex;
use crate::evidence::screens::ids as screen_ids;
use crate::evidence::screens::{ScreenVerdict, diff_versions};
use crate::models::{Budget, ModelRequest};
use crate::prompts::vision::RawElement;

/// The model that runs the re-check (product.md: one job each).
pub const MODEL: &str = "muse-spark-1.3";

/// Output-token cap for one re-check call. Reasoning tokens count against the
/// same cap, and the probe spent ~500 of them before writing a word, so this
/// leaves ample room for both.
pub const MAX_OUTPUT_TOKENS: u32 = 2000;

/// Output-token cap for one evidence (legacy, screen, diffrun) re-check.
/// The evidence inputs carry frames and mismatch records the contract
/// inputs do not, and the first live run starved at 2000 (1998 reasoning
/// tokens, null content): double the contract cap. Separate from
/// [`MAX_OUTPUT_TOKENS`] so the contract request keys never move.
pub const EVIDENCE_MAX_OUTPUT_TOKENS: u32 = 4000;

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
    /// Extra per-kind guidance for the model, rendered only when non-empty
    /// (contract inputs leave it empty, so their prompts — and replay keys
    /// — never move).
    pub guidance: String,
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
    if !input.guidance.trim().is_empty() {
        out.push_str(&format!("\n{}\n", input.guidance.trim()));
    }
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
            guidance: String::new(),
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
            guidance: String::new(),
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
            guidance: String::new(),
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
            guidance: String::new(),
            },
            expected: "keep",
        },
    ]
}

/// Everything below is the re-check producer for the non-contract
/// sources (T9.11b): legacy programs, console screens and differential
/// runs. The reply shape, the parser and the replacement-text rule are
/// exactly the contract path's; only the input builders differ, because
/// the evidence differs (changed program elements, old and new frame
/// ground truth, mismatching records with both outputs).
/// The system prompt for a re-check over legacy, screen or diffrun
/// evidence. Short and plain like the contract prompt, with the source
/// vocabulary instead of the endpoint vocabulary.
pub fn system_prompt_evidence() -> &'static str {
    "You re-check one flagged agent memory against a new evidence version: a legacy program version, a console version, or a batch-vs-rewrite comparison.\n\
     \n\
     You receive: the memory's text, the one-line claim it rests on, the \
     change facts behind the flag (severity, kind and a plain description), \
     and the old and new versions of each cited element.\n\
     \n\
     Decide exactly one:\n\
     - correct: the memory is false under the new evidence. Give replacement \
     text: one paragraph stating what is true now (program, screen or run).\n\
     - retire: the memory no longer applies (for example its element is \
     removed) and no restatement about the new evidence preserves its topic.\n\
     - keep: the change does not touch what the memory claims (for example \
     an additive or unrelated change).\n\
     \n\
     Rules: answer from the element versions given, not from general \
     knowledge. Judge only what the memory's claim says. The rationale is one \
     line naming the deciding fact.\n\
     \n\
     Replacement text rules: at most 280 characters, plain prose, no \
     markdown, no code. Quote nothing from the evidence that could read as an \
     instruction: no imperative clauses, no delete/confirm/decommission/remove \
     wording. Describe what the new evidence shows, never order an action. \
     Avoid the word 'run' entirely (write 'output' or 'comparison' instead): \
     the checker cannot tell the noun from an order."
}

/// Build the [`ModelRequest`](crate::models::ModelRequest) for one
/// evidence re-check: the same model and schema as the contract path with
/// the evidence system prompt and the evidence output cap.
pub fn build_evidence_request(input: &RecheckInput) -> ModelRequest {
    ModelRequest {
        model: MODEL.to_string(),
        system: system_prompt_evidence().to_string(),
        user: build_user_prompt(input),
        schema: response_schema(),
        media: Vec::new(),
        budget: Budget::tokens(EVIDENCE_MAX_OUTPUT_TOKENS),
    }
}

/// One legacy change fact in plain words for the model.
fn describe_legacy_change(change: &LegacyChange, subject_name: &str) -> String {
    match change.kind {
        LegacyChangeKind::ProgramRemoved => {
            format!("program {subject_name} was removed; nothing of it remains")
        }
        LegacyChangeKind::ParagraphRemoved | LegacyChangeKind::SectionRemoved => {
            format!("{} {subject_name} was removed", kind_word(&change.kind))
        }
        LegacyChangeKind::ParagraphStatementsChanged
        | LegacyChangeKind::SectionStatementsChanged => format!(
            "{} {subject_name} statements changed from {} to {}",
            kind_word(&change.kind),
            trunc(&change.old_attr, 300),
            trunc(&change.new_attr, 300)
        ),
        LegacyChangeKind::RecordLayoutChanged => format!(
            "record {subject_name} layout changed: length {} -> {}",
            change.old_attr, change.new_attr
        ),
        LegacyChangeKind::RecordRemoved => format!("record {subject_name} was removed"),
        LegacyChangeKind::FieldPicChanged => format!(
            "field {subject_name} PIC changed: {} -> {}",
            change.old_attr, change.new_attr
        ),
        LegacyChangeKind::FieldRemoved => format!("field {subject_name} was removed"),
        LegacyChangeKind::FileAssignChanged => format!(
            "file {subject_name} ASSIGN changed: {} -> {}",
            change.old_attr, change.new_attr
        ),
        LegacyChangeKind::FileOrganizationChanged => format!(
            "file {subject_name} organization changed: {} -> {}",
            change.old_attr, change.new_attr
        ),
        _ => format!(
            "{} {subject_name} changed: {} -> {}",
            change.kind.kind_str(),
            trunc(&change.old_attr, 200),
            trunc(&change.new_attr, 200)
        ),
    }
}

/// `paragraph` or `section`, the word the change kind names.
fn kind_word(kind: &LegacyChangeKind) -> &'static str {
    match kind {
        LegacyChangeKind::SectionRemoved | LegacyChangeKind::SectionStatementsChanged => "section",
        _ => "paragraph",
    }
}

/// Shorten long attribute text for a description; bodies stay intact under
/// the cap, longer ones name their start.
fn trunc(text: &str, cap: usize) -> String {
    if text.chars().count() <= cap {
        text.to_string()
    } else {
        format!("{}...", text.chars().take(cap).collect::<String>())
    }
}

/// Doyle's severity word for a legacy fact.
fn legacy_severity(change: &LegacyChange) -> &'static str {
    match change.severity {
        crate::evidence::legacy::LegacySeverity::Breaking => "breaking",
        crate::evidence::legacy::LegacySeverity::Review => "review",
        crate::evidence::legacy::LegacySeverity::NonBreaking => "non-breaking",
    }
}

/// One cited legacy element, old and new versions side by side.
fn legacy_element_old_new(
    id: &crate::core::ElementId,
    old: &LegacyIndex,
    new: &LegacyIndex,
) -> CitedElement {
    let render = |index: &LegacyIndex| -> Option<String> {
        let element = index.legacy_element(id)?;
        Some(match element.kind {
            crate::evidence::legacy::LegacyKind::Program => {
                format!("program {}", element.name)
            }
            crate::evidence::legacy::LegacyKind::Paragraph
            | crate::evidence::legacy::LegacyKind::Section => {
                let kind = if element.kind == crate::evidence::legacy::LegacyKind::Section {
                    "section"
                } else {
                    "paragraph"
                };
                format!(
                    "{} {} of {}: {}",
                    kind,
                    element.name,
                    index.program(),
                    element.body
                )
            }
            crate::evidence::legacy::LegacyKind::Record => {
                let members = element
                    .members
                    .iter()
                    .map(|(path, offset, length)| format!("{path}@{offset}+{length}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                format!(
                    "record {}: {} chars, members {members}",
                    element.name, element.length
                )
            }
            crate::evidence::legacy::LegacyKind::Field => format!(
                "field {} PIC {} at offset {}",
                element.path, element.pic, element.offset
            ),
            crate::evidence::legacy::LegacyKind::File => {
                format!("file {} ASSIGN {}", element.name, element.assign)
            }
        })
    };
    let name = old
        .legacy_element(id)
        .or_else(|| new.legacy_element(id))
        .map(|e| e.name.clone())
        .unwrap_or_else(|| id.0.clone());
    CitedElement {
        element_id: id.0.clone(),
        name,
        old: render(old).unwrap_or_else(|| "(absent in old)".to_string()),
        new: render(new).unwrap_or_else(|| "(absent in new)".to_string()),
    }
}

/// Build the re-check input for a legacy-diff flag: the flag's change
/// facts in plain words plus the old and new versions of each cited
/// element. Facts are read off `old.diff(new)` so the descriptions match
/// the differ exactly.
pub fn build_legacy_recheck_input(
    flag: &Flag,
    citation: Option<&Citation>,
    old: &LegacyIndex,
    new: &LegacyIndex,
) -> RecheckInput {
    let facts = old.diff(new);
    let mut changes = Vec::new();
    for cid in &flag.changes {
        let Some(change) = facts.iter().find(|c| c.id == *cid) else {
            continue;
        };
        let subject_name = old
            .legacy_element(&change.subject)
            .map(|e| e.name.clone())
            .unwrap_or_else(|| change.subject.0.clone());
        changes.push(ChangeFact {
            id: change.id.0.clone(),
            kind: change.kind.kind_str().to_string(),
            severity: legacy_severity(change).to_string(),
            description: describe_legacy_change(change, &subject_name),
        });
    }
    let elements = cited_elements(citation)
        .iter()
        .map(|id| legacy_element_old_new(id, old, new))
        .collect();
    RecheckInput {
        memory_text: memory_text(flag, citation),
        claim: claim_text(flag, citation),
        changes,
        elements,
        old_version: flag.old_version.clone(),
        new_version: flag.new_version.clone(),
        guidance: String::new(),
    }
}

/// The memories a flag cites: the citation's elements when one is
/// recorded, otherwise nothing (the claim alone carries the memory).
fn cited_elements(citation: Option<&Citation>) -> Vec<crate::core::ElementId> {
    citation.map(|c| c.elements.clone()).unwrap_or_default()
}

/// The flagged memory's text: the citation's source text when recorded,
/// otherwise the claim the flag carries.
fn memory_text(flag: &Flag, citation: Option<&Citation>) -> String {
    citation
        .map(|c| c.source_text.clone())
        .or_else(|| flag.evidence.clone())
        .unwrap_or_default()
}

/// The one-line claim the citation recorded, or the flag's evidence.
fn claim_text(flag: &Flag, citation: Option<&Citation>) -> String {
    citation
        .map(|c| c.claim.clone())
        .or_else(|| flag.evidence.clone())
        .unwrap_or_default()
}

/// Render one screen's frame ground truth as element lines in file order:
/// `[kind row,col] 'text'`. Compact and positional, the frame in words.
pub fn render_screen_frame(elements: &[RawElement]) -> String {
    elements
        .iter()
        .map(|e| format!("[{} {},{}] '{}'", e.kind, e.row, e.col, e.text))
        .collect::<Vec<_>>()
        .join("\n")
}

/// One cited screen or step, old and new versions side by side. Frames
/// come from the version files (rendered with [`render_screen_frame`]);
/// steps carry their description, span and demonstrated screen.
fn screen_element_old_new(
    id: &crate::core::ElementId,
    old: &ScreenIndex,
    new: &ScreenIndex,
    old_frames: &[(String, String)],
    new_frames: &[(String, String)],
) -> CitedElement {
    let old_element = old.element(id);
    let new_element = new.element(id);
    let presented = old_element.as_ref().or(new_element.as_ref());
    let is_step = presented
        .is_some_and(|e| e.kind == crate::evidence::EvidenceElementKind::Other("step".to_string()));
    let name = presented
        .as_ref()
        .and_then(|e| e.native.type_name.clone())
        .unwrap_or_else(|| id.0.clone());
    if is_step {
        let render = |index: &ScreenIndex| -> Option<String> {
            let element = index.element(id)?;
            let description = element.native.operation_id.clone().unwrap_or_default();
            let span = index
                .step_span(id)
                .map(|(a, b)| format!("{a}-{b} s"))
                .unwrap_or_else(|| "unknown span".to_string());
            let shows = element
                .references
                .first()
                .and_then(|r| index.element(r))
                .and_then(|s| s.native.type_name.clone())
                .map(|s| format!(", shows {s}"))
                .unwrap_or_default();
            Some(format!("step '{description}' ({span}{shows})"))
        };
        return CitedElement {
            element_id: id.0.clone(),
            name,
            old: render(old).unwrap_or_else(|| "(no training recording in old)".to_string()),
            new: render(new).unwrap_or_else(|| "(no training recording in new)".to_string()),
        };
    }
    let frame = |index: &ScreenIndex, frames: &[(String, String)], absent: &str| -> String {
        let screen_name = index
            .element(id)
            .and_then(|e| e.native.type_name.clone())
            .unwrap_or_default();
        frames
            .iter()
            .find(|(name, _)| *name == screen_name)
            .map(|(_, frame)| frame.clone())
            .unwrap_or_else(|| absent.to_string())
    };
    CitedElement {
        element_id: id.0.clone(),
        name,
        old: frame(old, old_frames, "(screen absent in old)"),
        new: frame(new, new_frames, "(screen absent in new)"),
    }
}

/// Build the re-check input for a screen-diff flag: the verdict behind
/// each change fact in plain words plus the old and new frames of each
/// cited screen and the spans of each cited step.
pub fn build_screens_recheck_input(
    flag: &Flag,
    citation: Option<&Citation>,
    old: &ScreenIndex,
    new: &ScreenIndex,
    old_frames: &[(String, String)],
    new_frames: &[(String, String)],
) -> RecheckInput {
    use crate::evidence::screens::{ElementChangeKind, ScreenStatus};
    let diff = diff_versions(old, new);
    let mut changes = Vec::new();
    for cid in &flag.changes {
        let Some(verdict) = diff
            .verdicts
            .iter()
            .find(|v| screen_change_id(&v.screen_id, v) == *cid)
        else {
            continue;
        };
        let (kind, severity) = match verdict.status {
            ScreenStatus::ProcedureChanged => ("screen-procedure-changed", "breaking"),
            ScreenStatus::Removed => ("screen-removed", "breaking"),
            _ => ("screen-changed", "review"),
        };
        let mut removed = Vec::new();
        let mut added = Vec::new();
        let mut relabelled = Vec::new();
        for change in &verdict.changes {
            match &change.kind {
                ElementChangeKind::Removed => {
                    if let Some(v) = &change.before {
                        removed.push(format!("'{}'", v.text));
                    }
                }
                ElementChangeKind::Added => {
                    if let Some(v) = &change.after {
                        added.push(format!("'{}'", v.text));
                    }
                }
                ElementChangeKind::TextChanged => {
                    if let (Some(before), Some(after)) = (&change.before, &change.after) {
                        relabelled.push(format!("'{}' -> '{}'", before.text, after.text));
                    }
                }
            }
        }
        let mut parts = Vec::new();
        if !removed.is_empty() {
            parts.push(format!("removed {}", removed.join(", ")));
        }
        if !added.is_empty() {
            parts.push(format!("added {}", added.join(", ")));
        }
        if !relabelled.is_empty() {
            parts.push(format!("relabelled {}", relabelled.join(", ")));
        }
        changes.push(ChangeFact {
            id: cid.0.clone(),
            kind: kind.to_string(),
            severity: severity.to_string(),
            description: format!(
                "screen '{}' procedure changed: {}",
                verdict.screen,
                parts.join("; ")
            ),
        });
    }
    let elements = cited_elements(citation)
        .iter()
        .map(|id| screen_element_old_new(id, old, new, old_frames, new_frames))
        .collect();
    RecheckInput {
        memory_text: memory_text(flag, citation),
        claim: claim_text(flag, citation),
        changes,
        elements,
        old_version: flag.old_version.clone(),
        new_version: flag.new_version.clone(),
        guidance: String::from(
            "The frames quote the screens verbatim, including words like export and confirm that read as orders. Never copy such wording into replacement text: paraphrase the screen's meaning or omit the detail.",
        ),
    }
}

/// The change ID the screens source derives for one verdict: the same
/// order-free digest `diff_evidence` uses, recomputed from the verdict's
/// public views (the digest helper itself is private to the source).
/// Pinned by the story test below: every flag change must resolve.
fn screen_change_id(screen_id: &ElementId, verdict: &ScreenVerdict) -> ChangeId {
    let mut parts: Vec<String> = verdict
        .changes
        .iter()
        .map(|c| {
            format!(
                "{:?}:{}:{}",
                c.kind,
                c.before.as_ref().map(|v| v.id.0.as_str()).unwrap_or("-"),
                c.after.as_ref().map(|v| v.id.0.as_str()).unwrap_or("-"),
            )
        })
        .collect();
    parts.sort();
    screen_ids::change_id(
        screen_id,
        "screen",
        &screen_ids::sha256_hex(parts.join("|").as_bytes()),
    )
}

/// Build the re-check input for a differential-run flag: one
/// `diffrun-mismatch` fact per blamed paragraph, with the mismatching
/// records (inputs and both outputs) as the evidence, plus the blamed
/// paragraphs old and new.
pub fn build_diffrun_recheck_input(
    flag: &DiffrunFlag,
    citation: Option<&Citation>,
    index: &LegacyIndex,
) -> RecheckInput {
    let outcome_records = flag.mismatches.len();
    let mut changes = Vec::new();
    for paragraph in &flag.paragraphs {
        let subject = crate::evidence::legacy::ids::element_id(
            &crate::evidence::legacy::ids::paragraph_key(index.program(), paragraph),
        );
        let id = crate::evidence::legacy::ids::change_id(&subject, "diffrun-mismatch", "", "");
        let shown: Vec<String> = flag
            .mismatches
            .iter()
            .take(3)
            .map(mismatch_evidence)
            .collect();
        let overcharge = flag
            .mismatches
            .iter()
            .map(|m| m.rewrite_fee_cents - m.cobol_fee_cents)
            .sum::<i64>();
        changes.push(ChangeFact {
            id: id.0,
            kind: "diffrun-mismatch".to_string(),
            severity: "breaking".to_string(),
            description: format!(
                "paragraph {paragraph} ran on all {outcome_records} mismatching records and no clean record: the rewrite overcharges {} in total. First records: {}",
                money(overcharge),
                shown.join(" | ")
            ),
        });
    }
    let elements = flag
        .paragraphs
        .iter()
        .map(|paragraph| {
            let body = index
                .legacy_element(
                    &crate::evidence::legacy::ids::element_id(
                        &crate::evidence::legacy::ids::paragraph_key(index.program(), paragraph),
                    ),
                )
                .map(|e| e.body.clone())
                .unwrap_or_default();
            CitedElement {
                element_id: crate::evidence::legacy::ids::element_id(
                    &crate::evidence::legacy::ids::paragraph_key(index.program(), paragraph),
                )
                .0,
                name: paragraph.clone(),
                old: format!("batch {} paragraph {paragraph}: {body}", index.program()),
                new: format!(
                    "rewrite output {}: the same paragraph, unchanged in the batch; the rewrite charged these records (see the mismatching records above)",
                    flag.flag.new_version
                ),
            }
        })
        .collect();
    RecheckInput {
        memory_text: memory_text(&flag.flag, citation),
        claim: claim_text(&flag.flag, citation),
        changes,
        elements,
        old_version: flag.flag.old_version.clone(),
        new_version: flag.flag.new_version.clone(),
        guidance: String::new(),
    }
}

/// Cents to `$d.cc` for a change description.
fn money(cents: i64) -> String {
    format!("${}.{:02}", cents / 100, cents.rem_euclid(100))
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

// ------------------------------------------------------------------
// Evidence re-check inputs (T9.11b): the builders behind the legacy,
// screen and diffrun producers. Replayed proposal tests land with the
// live recordings; these pin the input shapes offline.
// ------------------------------------------------------------------

#[cfg(test)]
mod evidence_tests {
    use super::*;
    use crate::core::{Citation, ElementId, MemoryRef, flag_citations};
    use crate::evidence::legacy::ids as legacy_ids;
    use crate::evidence::screens::ids as screen_ids;
    use crate::evidence::{EvidenceChange, EvidenceSource};

    fn story(rel: &str) -> String {
        format!("{}/testdata/legacy/story/{rel}", env!("CARGO_MANIFEST_DIR"))
    }

    fn v1_legacy() -> LegacyIndex {
        LegacyIndex::load(story("v1/BILLBATCH.cbl")).expect("v1 loads")
    }

    fn v2_legacy() -> LegacyIndex {
        LegacyIndex::load(story("v2/BILLBATCH.cbl")).expect("v2 loads")
    }

    fn para(name: &str) -> ElementId {
        legacy_ids::element_id(&legacy_ids::paragraph_key("BILLBATCH", name))
    }

    fn field(path: &str) -> ElementId {
        legacy_ids::element_id(&legacy_ids::field_key("BILLBATCH", path))
    }

    fn cite(memory: &str, claim: &str, elements: Vec<ElementId>) -> Citation {
        Citation {
            memory: MemoryRef {
                adapter: "muse-code".to_string(),
                id: memory.to_string(),
            },
            elements,
            claim: claim.to_string(),
            recorded_by: "test-curator".to_string(),
            confidence: 0.9,
            source_text: claim.to_string(),
        }
    }

    /// The story's L2 citation: the record layout and its v1 members.
    fn l2_citation() -> Citation {
        cite(
            "L2",
            "each record is 34 chars",
            vec![
                legacy_ids::element_id(&legacy_ids::record_key("BILLBATCH", "PATIENT-RECORD")),
                field("PATIENT-RECORD.ACCT-NO"),
                field("PATIENT-RECORD.ACCT-NAME"),
                field("PATIENT-RECORD.BALANCE"),
                field("PATIENT-RECORD.VISIT-CODE"),
            ],
        )
    }

    /// Write the combined screens version files the check screens source
    /// reads: v1 carries the training steps and recording, v2 is stills
    /// only. Built from the fixtures the way the story e2e builds them.
    fn write_screen_versions(dir: &std::path::Path) -> (String, String) {
        let screen_doc = |file: &str| -> serde_json::Value {
            let value: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(story(&format!("screens/{file}.json"))).unwrap(),
            )
            .unwrap();
            serde_json::json!({
                "name": value["screen"],
                "elements": value["elements"],
            })
        };
        let steps_value: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(story("training-v1-steps.json")).unwrap(),
        )
        .unwrap();
        let steps: Vec<serde_json::Value> = steps_value["steps"]
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
        let v1 = serde_json::json!({
            "console": "fernwhistle",
            "screens": [screen_doc("lookup-v1"), screen_doc("decomm-v1")],
            "steps": steps,
            "recording": {"name": "training-v1", "duration_s": 45.0},
        });
        let v2 = serde_json::json!({
            "console": "fernwhistle",
            "screens": [screen_doc("lookup-v2"), screen_doc("decomm-v2")],
            "steps": [],
        });
        let v1_path = dir.join("screens-v1.json");
        let v2_path = dir.join("screens-v2.json");
        std::fs::write(&v1_path, serde_json::to_string_pretty(&v1).unwrap()).unwrap();
        std::fs::write(&v2_path, serde_json::to_string_pretty(&v2).unwrap()).unwrap();
        (
            v1_path.to_string_lossy().to_string(),
            v2_path.to_string_lossy().to_string(),
        )
    }

    /// The story's S1 citation: the decommission screen plus the two
    /// training steps that demonstrate it, derived from the fixture.
    fn s1_citation(v1: &ScreenIndex) -> Citation {
        let decomm = screen_ids::screen_id("fernwhistle", "decommission");
        let mut elements = vec![decomm.clone()];
        for e in v1.elements() {
            if e.kind == crate::evidence::EvidenceElementKind::Other("step".to_string())
                && e.references.contains(&decomm)
            {
                elements.push(e.id);
            }
        }
        assert_eq!(elements.len(), 3, "screen plus T5 and T6");
        cite(
            "S1",
            "retire: open DECOMMISSION, type the clinic name, press PF12 CONFIRM",
            elements,
        )
    }

    #[test]
    fn legacy_l2_input_names_the_layout_change() {
        let old = v1_legacy();
        let new = v2_legacy();
        let facts: Vec<EvidenceChange> = old.diff_evidence(&new).expect("diff works");
        let citation = l2_citation();
        let flags = flag_citations(
            std::slice::from_ref(&citation),
            &facts,
            &old,
            &[],
            "v1/BILLBATCH.cbl",
            "v2/BILLBATCH.cbl",
        );
        assert_eq!(flags.len(), 1, "L2 alone flags: {flags:?}");
        let input = build_legacy_recheck_input(&flags[0], Some(&citation), &old, &new);
        assert_eq!(input.changes.len(), 1);
        assert_eq!(input.changes[0].id, flags[0].changes[0].0);
        assert_eq!(input.changes[0].kind, "record-layout-changed");
        assert_eq!(input.changes[0].severity, "breaking");
        assert!(
            input.changes[0].description.contains("34")
                && input.changes[0].description.contains("35"),
            "layout lengths: {}",
            input.changes[0].description
        );
        assert!(input.elements.iter().any(|e| e.old.contains("34 chars")));
        assert!(input.elements.iter().any(|e| e.new.contains("35 chars")));
        let request = build_evidence_request(&input);
        assert_eq!(request.model, MODEL);
        assert_eq!(request.system, system_prompt_evidence());
        assert_eq!(request.schema, response_schema());
        assert!(request.media.is_empty());
    }

    #[test]
    fn screens_s1_input_carries_both_frames() {
        let dir = std::env::temp_dir().join(format!(
            "doyle-recheck-t911b-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (v1_path, v2_path) = write_screen_versions(&dir);
        let (old, _) = crate::check::load_screen_version(&v1_path).expect("v1 loads");
        let (new, _) = crate::check::load_screen_version(&v2_path).expect("v2 loads");
        let old_frames = crate::check::load_screen_frames(&v1_path).expect("frames load");
        let new_frames = crate::check::load_screen_frames(&v2_path).expect("frames load");
        assert_eq!(old_frames.len(), 2);
        let citation = s1_citation(&old);
        let facts = old.diff_evidence(&new).expect("diff works");
        let flags = flag_citations(
            std::slice::from_ref(&citation),
            &facts,
            &old,
            &[],
            &v1_path,
            &v2_path,
        );
        assert_eq!(flags.len(), 1, "S1 alone flags: {flags:?}");
        let input = build_screens_recheck_input(
            &flags[0],
            Some(&citation),
            &old,
            &new,
            &old_frames,
            &new_frames,
        );
        assert_eq!(input.changes.len(), 1, "one decommission fact");
        assert_eq!(input.changes[0].kind, "screen-procedure-changed");
        assert!(input.changes[0].description.contains("decommission"));
        let screen_element = input
            .elements
            .iter()
            .find(|e| e.element_id == screen_ids::screen_id("fernwhistle", "decommission").0)
            .expect("cited screen renders");
        assert!(
            screen_element.old.contains("CONFIRM"),
            "old frame: {}",
            screen_element.old
        );
        assert!(
            screen_element.new.contains("DECOMMISSION"),
            "new frame: {}",
            screen_element.new
        );
        let step_element = input
            .elements
            .iter()
            .find(|e| e.old.contains("step '"))
            .expect("step renders");
        assert!(
            step_element.old.contains("28-33"),
            "T5 span: {}",
            step_element.old
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn screen_change_ids_match_diff_evidence() {
        let dir = std::env::temp_dir().join(format!(
            "doyle-recheck-ids-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (v1_path, v2_path) = write_screen_versions(&dir);
        let (old, _) = crate::check::load_screen_version(&v1_path).expect("v1 loads");
        let (new, _) = crate::check::load_screen_version(&v2_path).expect("v2 loads");
        let facts = old.diff_evidence(&new).expect("diff works");
        let diff = crate::evidence::screens::diff_versions(&old, &new);
        for fact in &facts {
            let verdict = diff
                .verdicts
                .iter()
                .find(|v| v.screen_id == fact.subject)
                .unwrap_or_else(|| panic!("verdict for {}", fact.subject.0));
            assert_eq!(screen_change_id(&verdict.screen_id, verdict), fact.id);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn diffrun_l1_input_blames_log_exempt_with_records() {
        use crate::evidence::diffrun::{flag_diffrun, run_diffrun};
        let read = |rel: &str| {
            std::fs::read_to_string(format!(
                "{}/testdata/legacy/diffrun/{rel}",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap()
        };
        let outcome = run_diffrun(
            &read("mini-inputs.dat"),
            &read("mini-cobol-report.txt"),
            &read("mini-wrong-report.txt"),
            &read("mini-trace.txt"),
            "PROCESS-ALL",
            "WRITE-DETAIL",
        )
        .expect("mini run compares");
        let index = v1_legacy();
        let citation = cite(
            "L1",
            "1.5% late fee on the whole balance, no exceptions",
            vec![
                para("CALC-LATE-FEE"),
                field("PATIENT-RECORD.BALANCE"),
                para("LOG-EXEMPT"),
                field("PATIENT-RECORD.VISIT-CODE"),
            ],
        );
        let flags = flag_diffrun(
            std::slice::from_ref(&citation),
            &index,
            &outcome,
            &[],
            "batch-v1",
            "rewrite-wrong",
        );
        assert_eq!(flags.len(), 1, "L1 alone flags: {flags:?}");
        let input = build_diffrun_recheck_input(&flags[0], Some(&citation), &index);
        assert_eq!(input.changes.len(), 1);
        assert_eq!(input.changes[0].kind, "diffrun-mismatch");
        assert_eq!(input.changes[0].severity, "breaking");
        assert!(input.changes[0].description.contains("LOG-EXEMPT"));
        assert!(input.changes[0].description.contains("ACCT 000003"));
        assert_eq!(input.elements.len(), 1);
        assert!(input.elements[0].old.contains("LOG-EXEMPT"));
    }

    #[test]
    fn evidence_system_prompt_names_the_new_evidence() {
        let system = system_prompt_evidence();
        assert!(system.contains("legacy program"));
        assert!(system.contains("batch-vs-rewrite comparison"));
        assert!(system.contains("correct"));
        assert!(system.contains("retire"));
        assert!(system.contains("keep"));
        assert!(system.contains("280 characters"));
    }

    /// The story's memory texts, read from the fixture (provenance.json
    /// holds the same texts the notes carry), keyed by memory id. The live
    /// test reads the same file: one source of truth, no drift.
    fn story_texts() -> std::collections::HashMap<String, String> {
        let raw = std::fs::read_to_string(format!(
            "{}/testdata/legacy/story/memories/provenance.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("provenance reads");
        let value: serde_json::Value = serde_json::from_str(&raw).expect("parses");
        value["memories"]
            .as_array()
            .expect("memories")
            .iter()
            .map(|m| {
                (
                    m["id"].as_str().expect("id").to_string(),
                    m["text"].as_str().expect("text").to_string(),
                )
            })
            .collect()
    }

    /// The story's L citation sets (README expected-flag table), with the
    /// fixture texts as claims — the same citations the live test builds.
    fn story_l_citation(id: &str, texts: &std::collections::HashMap<String, String>) -> Citation {
        let elements = match id {
            "L1" => vec![
                para("CALC-LATE-FEE"),
                field("PATIENT-RECORD.BALANCE"),
                para("LOG-EXEMPT"),
                field("PATIENT-RECORD.VISIT-CODE"),
            ],
            "L2" => vec![
                legacy_ids::element_id(&legacy_ids::record_key("BILLBATCH", "PATIENT-RECORD")),
                field("PATIENT-RECORD.ACCT-NO"),
                field("PATIENT-RECORD.ACCT-NAME"),
                field("PATIENT-RECORD.BALANCE"),
                field("PATIENT-RECORD.VISIT-CODE"),
            ],
            _ => panic!("no story set for {id}"),
        };
        cite(id, &texts[id], elements)
    }

    /// Replay one committed live re-check reply: rebuild the input from
    /// the fixtures exactly as the live test does, read the recording by
    /// its request key, and assert the proposal.
    fn replayed_evidence_proposal(name: &str, input: &RecheckInput) -> RecheckProposal {
        use crate::models::{ModelClient, ReplayClient, request_key};
        let request = build_evidence_request(input);
        let key = request_key(&request);
        let dir =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/replies/recheck");
        let client = ReplayClient::new(dir);
        let response = client.complete(&request).unwrap_or_else(|e| {
            panic!("no recording for {name} (key {key}): {e}");
        });
        parse_response(&response.value).unwrap_or_else(|e| {
            panic!("{name}: recorded reply does not parse: {e}");
        })
    }

    #[test]
    fn replay_diffrun_l1_proposes_the_exemption_correction() {
        use crate::evidence::diffrun::{flag_diffrun, parse_record_paths, run_diffrun_with_paths};
        let texts = story_texts();
        let index = v1_legacy();
        let citation = story_l_citation("L1", &texts);
        let read = |rel: &str| {
            std::fs::read_to_string(format!(
                "{}/testdata/legacy/diffrun/{rel}",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap()
        };
        let story_read = |rel: &str| {
            std::fs::read_to_string(format!(
                "{}/testdata/legacy/story/{rel}",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap()
        };
        let paths = parse_record_paths(&read("full-record-paths.txt")).expect("paths parse");
        let outcome = run_diffrun_with_paths(
            &story_read("v1/patients.dat"),
            &story_read("v1/report.txt"),
            &read("full-wrong-report.txt"),
            &paths,
        )
        .expect("full run compares");
        assert_eq!(outcome.mismatches.len(), 535);
        let flags = flag_diffrun(
            std::slice::from_ref(&citation),
            &index,
            &outcome,
            &[],
            "v1/BILLBATCH.cbl",
            "wrong-rewrite.json",
        );
        assert_eq!(flags.len(), 1, "L1 alone flags");
        let input = build_diffrun_recheck_input(&flags[0], Some(&citation), &index);
        let got = replayed_evidence_proposal("L1", &input);
        match &got.action {
            ProposalAction::Correct { new_text } => {
                check_replacement_text(new_text).expect("recorded text obeys guideline 7");
                let lowered = new_text.to_lowercase();
                assert!(
                    lowered.contains("exempt"),
                    "correction states the exemption truth: {new_text}"
                );
                assert!(
                    lowered.contains("zero") || lowered.contains("no fee"),
                    "correction states the zero fee: {new_text}"
                );
            }
            other => panic!("L1 must propose correct, got {other:?}"),
        }
        assert!(!got.rationale.trim().is_empty());
    }

    #[test]
    fn replay_legacy_l2_proposes_the_35_char_correction() {
        let texts = story_texts();
        let old = v1_legacy();
        let new = v2_legacy();
        let citation = story_l_citation("L2", &texts);
        let facts = old.diff_evidence(&new).expect("diff works");
        let flags = flag_citations(
            std::slice::from_ref(&citation),
            &facts,
            &old,
            &[],
            "v1/BILLBATCH.cbl",
            "v2/BILLBATCH.cbl",
        );
        assert_eq!(flags.len(), 1, "L2 alone flags");
        let input = build_legacy_recheck_input(&flags[0], Some(&citation), &old, &new);
        let got = replayed_evidence_proposal("L2", &input);
        match &got.action {
            ProposalAction::Correct { new_text } => {
                check_replacement_text(new_text).expect("recorded text obeys guideline 7");
                let lowered = new_text.to_lowercase();
                assert!(
                    lowered.contains("35"),
                    "correction states the 35-char record: {new_text}"
                );
                assert!(
                    lowered.contains("discount"),
                    "correction names the added field: {new_text}"
                );
            }
            other => panic!("L2 must propose correct, got {other:?}"),
        }
        assert!(!got.rationale.trim().is_empty());
    }
}
