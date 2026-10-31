//! The vision prompt: one Wonder call per console version.
//!
//! Doyle's eyes are Wonder's (product.md, Beyond the spec): one call to
//! `muse-wonder-1.0` turns a version's frames and training recording into
//! screen elements and timestamped steps. The diff between two versions is
//! computed afterwards in `src/evidence/screens.rs`, never asked of the
//! model. This module mirrors `src/prompts/curator.rs`: the system prompt,
//! the JSON schema, the request builder, and a strict parser. Unknown
//! step-screen names are dropped in code (the curator's ID rule), with a
//! warning so a detached step is visible instead of silent.
//!
//! Wording follows the measured shapes in `docs/wonder-vision.md` §1–§2:
//! `image_url`/`video_url` data URLs, `json_schema` with `strict: false`,
//! element fields `text`/`row`/`col`/`kind`/`protected`, step fields
//! `description`/`start_s`/`end_s`. The per-step `screen` name is new: it
//! links a step to the screen it demonstrates, which is what lets a memory
//! citing a training-video span flag when that screen's procedure changes.
//! Live callers should set reasoning effort `"low"` on the client
//! (measured in T9.2); the request carries no effort field.

use crate::models::{Budget, MediaPart, ModelRequest};

/// The model that reads screens and recordings.
pub const WONDER_MODEL: &str = "muse-wonder-1.0";

/// The output cap on a version read. T9.2's rule: budget at least twice the
/// expected JSON for reasoning at low effort. The v1 console (two ~30-element
/// screens plus a six-step recording) replies with roughly 3–4k output
/// tokens; 8000 leaves the measured headroom.
pub const MAX_OUTPUT_TOKENS: u32 = 8000;

/// The closed kind vocabulary from the T9.2 screen probe. Anything else is a
/// confused read and fails parsing.
pub const KIND_VOCABULARY: &[&str] = &[
    "title",
    "screen-id",
    "heading",
    "datetime",
    "label",
    "value",
    "input",
    "message",
    "status",
    "footer",
    "pfkey",
];

/// The vision system's prompt.
pub const VERSION_SYSTEM: &str = "You read IBM 3270 green-screen operator consoles for Doyle, which tracks what each agent memory rests on. You are given still frames of named screens and, for some versions, a recording of an operator using the console. Reply with exactly the JSON the schema asks for: one entry per text element visible on each screen (positions are 1-indexed: row 1 is the top line, column 1 is the leftmost column), and one entry per step the operator performs in the recording with its start and end seconds. kinds: title, screen-id, heading, datetime, label, value, input, message, status, footer, pfkey. Name only the screens you were given; a step that shows none of them names no screen.";

/// The JSON schema a version read must conform to.
pub fn version_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "screens": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "The screen's name, exactly as given in the request.",
                        },
                        "elements": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "text": { "type": "string" },
                                    "row": { "type": "integer", "minimum": 1, "maximum": 24 },
                                    "col": { "type": "integer", "minimum": 1, "maximum": 80 },
                                    "kind": { "type": "string" },
                                    "protected": { "type": "boolean" },
                                },
                                "required": ["text", "row", "col", "kind", "protected"],
                                "additionalProperties": false,
                            },
                        },
                    },
                    "required": ["name", "elements"],
                    "additionalProperties": false,
                },
            },
            "steps": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "description": { "type": "string" },
                        "start_s": { "type": "number", "minimum": 0.0 },
                        "end_s": { "type": "number", "minimum": 0.0 },
                        "screen": {
                            "type": "string",
                            "description": "The given screen this step shows, or empty when it shows none of them.",
                        },
                    },
                    "required": ["description", "start_s", "end_s", "screen"],
                    "additionalProperties": false,
                },
            },
        },
        "required": ["screens", "steps"],
        "additionalProperties": false,
    })
}

/// One raw screen element, as the model reported it. Doyle assigns the
/// stable ID afterwards in `src/evidence/screens.rs`.
#[derive(Debug, Clone, PartialEq)]
pub struct RawElement {
    /// The exact visible text.
    pub text: String,
    /// 1-indexed row.
    pub row: u32,
    /// 1-indexed column.
    pub col: u32,
    /// One of [`KIND_VOCABULARY`].
    pub kind: String,
    /// True for protected fields, false for editable inputs.
    pub protected: bool,
}

/// One raw screen, as reported: its given name plus its elements.
#[derive(Debug, Clone, PartialEq)]
pub struct RawScreen {
    /// The name from the request.
    pub name: String,
    /// The screen's elements, in the model's order.
    pub elements: Vec<RawElement>,
}

/// One raw recording step, as reported.
#[derive(Debug, Clone, PartialEq)]
pub struct RawStep {
    /// What the operator does.
    pub description: String,
    /// Start seconds.
    pub start_s: f64,
    /// End seconds.
    pub end_s: f64,
    /// The given screen this step shows, or empty. Unknown names are
    /// dropped to empty by the parser (see [`RawVersion::warnings`]).
    pub screen: String,
}

/// One version's reading, parsed: screens plus steps.
#[derive(Debug, Clone, PartialEq)]
pub struct RawVersion {
    /// The screens, in the request's order.
    pub screens: Vec<RawScreen>,
    /// The recording's steps, in time order.
    pub steps: Vec<RawStep>,
    /// Step-screen names the reply named that were not requested, dropped
    /// to no reference. A detached step cannot flag, so the caller sees
    /// every drop here instead of finding out by a missed flag.
    pub warnings: Vec<String>,
}

/// What was wrong with a version reply's shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisionParseError {
    /// What was wrong, naming the field.
    pub message: String,
}

impl std::fmt::Display for VisionParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "vision reply: {}", self.message)
    }
}

impl std::error::Error for VisionParseError {}

/// Build the single version request: the version's frames, then its
/// recording when it has one. Media order is screens in `screens` order,
/// then the recording; the user text names them in the same order.
pub fn build_version_request(
    console: &str,
    version: &str,
    screens: &[(&str, &MediaPart)],
    recording: Option<(&str, f64, &MediaPart)>,
) -> ModelRequest {
    let mut frames: Vec<String> = screens
        .iter()
        .enumerate()
        .map(|(i, (name, _))| format!("Frame {} is the '{name}' screen", i + 1))
        .collect();
    let mut media: Vec<MediaPart> =
        Vec::with_capacity(screens.len() + usize::from(recording.is_some()));
    for (_, part) in screens {
        media.push((*part).clone());
    }
    let recording_line = match recording {
        Some((name, duration_s, part)) => {
            media.push(part.clone());
            format!(
                " The attached recording '{name}' is {duration_s} seconds long and shows an operator using the version {version} console."
            )
        }
        None => String::new(),
    };
    if frames.is_empty() {
        frames.push("no still frames".to_string());
    }
    let user = format!(
        "Console {console}, version {version}, {frame_count} still frame{s}: {frames}.{recording_line} List every text element on each 80x24 green screen: titles, headings, labels, values, input fields, messages, the status line and function-key hints. For each give its exact text, 1-indexed row and column, kind, and whether the field is protected (true) or an editable input (false). For the recording, list each step the operator performs with its start and end seconds and the screen it shows (one of the given screens, or empty when it shows none of them).",
        frame_count = screens.len(),
        s = if screens.len() == 1 { "" } else { "s" },
        frames = frames.join(", "),
        recording_line = recording_line,
    );
    ModelRequest {
        model: WONDER_MODEL.to_string(),
        system: VERSION_SYSTEM.to_string(),
        user,
        schema: version_schema(),
        media,
        budget: Budget::tokens(MAX_OUTPUT_TOKENS),
    }
}

/// Parse a version reply against the requested screen names. Screen sections
/// must name exactly the requested screens (a misattributed screen would
/// poison every element ID, so it fails loud); step-screen names outside the
/// requested set are dropped to empty with a warning (the curator's
/// drop-unknown rule).
pub fn parse_version_value(
    value: &serde_json::Value,
    requested: &[&str],
) -> Result<RawVersion, VisionParseError> {
    let object = value.as_object().ok_or_else(|| VisionParseError {
        message: "reply is not an object".to_string(),
    })?;
    let screens_value = object
        .get("screens")
        .and_then(|v| v.as_array())
        .ok_or_else(|| VisionParseError {
            message: "missing array field 'screens'".to_string(),
        })?;
    let steps_value = object
        .get("steps")
        .and_then(|v| v.as_array())
        .ok_or_else(|| VisionParseError {
            message: "missing array field 'steps'".to_string(),
        })?;

    let mut screens = Vec::with_capacity(screens_value.len());
    for (i, screen_value) in screens_value.iter().enumerate() {
        let screen_object = screen_value.as_object().ok_or_else(|| VisionParseError {
            message: format!("screens[{i}] is not an object"),
        })?;
        let name = screen_object
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| VisionParseError {
                message: format!("screens[{i}] is missing string field 'name'"),
            })?;
        if !requested.contains(&name) {
            return Err(VisionParseError {
                message: format!("screens[{i}] names unrequested screen '{name}'"),
            });
        }
        if screens.iter().any(|s: &RawScreen| s.name == name) {
            return Err(VisionParseError {
                message: format!("screens[{i}] repeats screen '{name}'"),
            });
        }
        let elements_value = screen_object
            .get("elements")
            .and_then(|v| v.as_array())
            .ok_or_else(|| VisionParseError {
                message: format!("screens[{i}] is missing array field 'elements'"),
            })?;
        let mut elements = Vec::with_capacity(elements_value.len());
        for (j, element_value) in elements_value.iter().enumerate() {
            elements.push(parse_element(element_value, i, j)?);
        }
        screens.push(RawScreen {
            name: name.to_string(),
            elements,
        });
    }
    let missing: Vec<&str> = requested
        .iter()
        .filter(|name| !screens.iter().any(|s| &s.name == *name))
        .copied()
        .collect();
    if !missing.is_empty() {
        return Err(VisionParseError {
            message: format!("reply is missing requested screens: {}", missing.join(", ")),
        });
    }

    let mut steps = Vec::with_capacity(steps_value.len());
    let mut warnings = Vec::new();
    for (i, step_value) in steps_value.iter().enumerate() {
        let step_object = step_value.as_object().ok_or_else(|| VisionParseError {
            message: format!("steps[{i}] is not an object"),
        })?;
        let description = step_object
            .get("description")
            .and_then(|v| v.as_str())
            .ok_or_else(|| VisionParseError {
                message: format!("steps[{i}] is missing string field 'description'"),
            })?
            .trim()
            .to_string();
        if description.is_empty() {
            return Err(VisionParseError {
                message: format!("steps[{i}] has an empty 'description'"),
            });
        }
        let start_s = step_object
            .get("start_s")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| VisionParseError {
                message: format!("steps[{i}] is missing number field 'start_s'"),
            })?;
        let end_s = step_object
            .get("end_s")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| VisionParseError {
                message: format!("steps[{i}] is missing number field 'end_s'"),
            })?;
        if !start_s.is_finite() || start_s < 0.0 {
            return Err(VisionParseError {
                message: format!("steps[{i}] has a bad 'start_s': {start_s}"),
            });
        }
        if !end_s.is_finite() || end_s < start_s {
            return Err(VisionParseError {
                message: format!("steps[{i}] has a bad 'end_s': {end_s}"),
            });
        }
        let screen = step_object
            .get("screen")
            .and_then(|v| v.as_str())
            .ok_or_else(|| VisionParseError {
                message: format!("steps[{i}] is missing string field 'screen'"),
            })?;
        let screen = if screen.is_empty() || requested.contains(&screen) {
            screen.to_string()
        } else {
            warnings.push(format!(
                "steps[{i}] names unrequested screen '{screen}'; no reference"
            ));
            String::new()
        };
        steps.push(RawStep {
            description,
            start_s,
            end_s,
            screen,
        });
    }

    Ok(RawVersion {
        screens,
        steps,
        warnings,
    })
}

fn parse_element(
    element_value: &serde_json::Value,
    screen: usize,
    index: usize,
) -> Result<RawElement, VisionParseError> {
    let label = format!("screens[{screen}].elements[{index}]");
    let element_object = element_value.as_object().ok_or_else(|| VisionParseError {
        message: format!("{label} is not an object"),
    })?;
    let text = element_object
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| VisionParseError {
            message: format!("{label} is missing string field 'text'"),
        })?
        .to_string();
    if text.trim().is_empty() {
        return Err(VisionParseError {
            message: format!("{label} has an empty 'text'"),
        });
    }
    let row = element_object
        .get("row")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| VisionParseError {
            message: format!("{label} is missing integer field 'row'"),
        })?;
    if !(1..=24).contains(&row) {
        return Err(VisionParseError {
            message: format!("{label} has row {row} outside 1-24"),
        });
    }
    let col = element_object
        .get("col")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| VisionParseError {
            message: format!("{label} is missing integer field 'col'"),
        })?;
    if !(1..=80).contains(&col) {
        return Err(VisionParseError {
            message: format!("{label} has col {col} outside 1-80"),
        });
    }
    let kind = element_object
        .get("kind")
        .and_then(|v| v.as_str())
        .ok_or_else(|| VisionParseError {
            message: format!("{label} is missing string field 'kind'"),
        })?;
    if !KIND_VOCABULARY.contains(&kind) {
        return Err(VisionParseError {
            message: format!("{label} has unknown kind '{kind}'"),
        });
    }
    let protected = element_object
        .get("protected")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| VisionParseError {
            message: format!("{label} is missing boolean field 'protected'"),
        })?;
    Ok(RawElement {
        text,
        row: u32::try_from(row).expect("row fits u32"),
        col: u32::try_from(col).expect("col fits u32"),
        kind: kind.to_string(),
        protected,
    })
}

/// The stated cost of one version call before it is made: the output cap on
/// the request plus the input estimate from T9.2 §5 (§7 for the basis).
/// Screens cost roughly a thousand input tokens each; recordings cost
/// roughly 240 input tokens per second at 960x648. No reply ever carries a
/// dollar cost, so spend is stated in tokens with the T9.2 dollar basis
/// beside it (on the order of $15 per million tokens).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatedCost {
    /// Expected input tokens, rounded.
    pub input_tokens: u64,
    /// The output cap the request will carry.
    pub output_cap: u32,
}

impl std::fmt::Display for StatedCost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "~{} input tokens, {} output cap",
            self.input_tokens, self.output_cap
        )
    }
}

/// State the cost of one version call: `screens` stills plus an optional
/// recording of `video_seconds`.
pub fn stated_cost(n_screens: usize, video_seconds: Option<f64>) -> StatedCost {
    let video = video_seconds.unwrap_or(0.0).max(0.0).ceil() as u64 * 240;
    StatedCost {
        input_tokens: n_screens as u64 * 1500 + video,
        output_cap: MAX_OUTPUT_TOKENS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn requested() -> Vec<&'static str> {
        vec!["lookup", "decommission"]
    }

    fn reply_value() -> serde_json::Value {
        serde_json::json!({
            "screens": [
                {"name": "lookup", "elements": [
                    {"text": "PATIENT LOOKUP", "row": 3, "col": 4, "kind": "heading", "protected": true},
                    {"text": "____", "row": 20, "col": 16, "kind": "input", "protected": false},
                ]},
                {"name": "decommission", "elements": [
                    {"text": "WARNING: DECOMMISSION IS PERMANENT.", "row": 9, "col": 4, "kind": "message", "protected": true},
                ]},
            ],
            "steps": [
                {"description": "Type the clinic name", "start_s": 33.0, "end_s": 45.0, "screen": "decommission"},
                {"description": "Show the main menu", "start_s": 0.0, "end_s": 6.0, "screen": ""},
            ],
        })
    }

    #[test]
    fn schema_has_screens_and_steps() {
        let schema = version_schema();
        let required = schema["required"].as_array().expect("required is an array");
        let names: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();
        assert_eq!(names, ["screens", "steps"]);
    }

    #[test]
    fn request_carries_frames_then_recording_in_order() {
        let lookup = MediaPart::image_png(vec![1]);
        let decomm = MediaPart::image_png(vec![2]);
        let video = MediaPart::video_mp4(vec![3]);
        let request = build_version_request(
            "fernwhistle",
            "v1",
            &[("lookup", &lookup), ("decommission", &decomm)],
            Some(("training-v1", 45.0, &video)),
        );
        assert_eq!(request.model, WONDER_MODEL);
        assert_eq!(request.system, VERSION_SYSTEM);
        assert_eq!(request.schema, version_schema());
        assert_eq!(request.budget, Budget::tokens(MAX_OUTPUT_TOKENS));
        assert_eq!(request.media.len(), 3);
        assert_eq!(request.media[0].bytes, vec![1]);
        assert_eq!(request.media[2].bytes, vec![3]);
        assert!(request.user.contains("Frame 1 is the 'lookup' screen"));
        assert!(
            request
                .user
                .contains("Frame 2 is the 'decommission' screen")
        );
        assert!(request.user.contains("training-v1"));
        assert!(request.user.contains("45"));
    }

    #[test]
    fn request_without_recording_names_no_recording() {
        let lookup = MediaPart::image_png(vec![1]);
        let request = build_version_request("fernwhistle", "v2", &[("lookup", &lookup)], None);
        assert_eq!(request.media.len(), 1);
        assert!(!request.user.contains("attached recording"));
    }

    #[test]
    fn shaped_reply_parses() {
        let parsed = parse_version_value(&reply_value(), &requested()).expect("parses");
        assert_eq!(parsed.screens.len(), 2);
        assert_eq!(parsed.screens[0].name, "lookup");
        assert_eq!(parsed.screens[0].elements.len(), 2);
        assert!(!parsed.screens[0].elements[1].protected);
        assert_eq!(parsed.steps.len(), 2);
        assert!(parsed.warnings.is_empty());
    }

    #[test]
    fn unknown_step_screen_is_dropped_with_a_warning() {
        let mut value = reply_value();
        value["steps"][0]["screen"] = serde_json::json!("menu");
        let parsed = parse_version_value(&value, &requested()).expect("parses");
        assert_eq!(parsed.steps[0].screen, "");
        assert_eq!(parsed.warnings.len(), 1);
        assert!(parsed.warnings[0].contains("menu"));
    }

    #[test]
    fn unrequested_screen_section_fails() {
        let mut value = reply_value();
        value["screens"][1]["name"] = serde_json::json!("menu");
        let error = parse_version_value(&value, &requested()).expect_err("must fail");
        assert!(error.to_string().contains("menu"));
    }

    #[test]
    fn missing_requested_screen_fails() {
        let mut value = reply_value();
        value["screens"] = serde_json::json!([value["screens"][0].clone()]);
        let error = parse_version_value(&value, &requested()).expect_err("must fail");
        assert!(error.to_string().contains("decommission"));
    }

    #[test]
    fn malformed_replies_fail_by_field() {
        let bad_row = serde_json::json!({
            "screens": [{"name": "lookup", "elements": [
                {"text": "X", "row": 0, "col": 4, "kind": "heading", "protected": true},
            ]}, {"name": "decommission", "elements": []}],
            "steps": [],
        });
        let bad_kind = serde_json::json!({
            "screens": [{"name": "lookup", "elements": [
                {"text": "X", "row": 3, "col": 4, "kind": "widget", "protected": true},
            ]}, {"name": "decommission", "elements": []}],
            "steps": [],
        });
        let bad_span = serde_json::json!({
            "screens": [
                {"name": "lookup", "elements": []},
                {"name": "decommission", "elements": []},
            ],
            "steps": [{"description": "X", "start_s": 10.0, "end_s": 5.0, "screen": ""}],
        });
        for (label, value) in [
            ("not-an-object", serde_json::json!("a string")),
            ("missing-screens", serde_json::json!({"steps": []})),
            ("missing-steps", serde_json::json!({"screens": []})),
            ("row-zero", bad_row),
            ("unknown-kind", bad_kind),
            ("end-before-start", bad_span),
        ] {
            let error = parse_version_value(&value, &requested()).expect_err(label);
            assert!(
                error.to_string().contains("vision reply"),
                "{label}: {error}"
            );
        }
    }

    #[test]
    fn stated_cost_follows_the_measured_magnitudes() {
        let cost = stated_cost(2, Some(45.0));
        assert_eq!(cost.output_cap, MAX_OUTPUT_TOKENS);
        assert_eq!(cost.input_tokens, 2 * 1500 + 45 * 240);
        assert!(cost.to_string().contains("output cap"));
        assert_eq!(stated_cost(1, None).input_tokens, 1500);
    }
}
