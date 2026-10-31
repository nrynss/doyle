//! One Wonder call per console version.
//!
//! The screen source reads a version once: its frames and, when it has one,
//! its training recording go out in a single [`ModelRequest`] with media
//! attachments, and the reply comes back through any [`ModelClient`]. The
//! replay client serves it offline, keyed on the media's hash; the Meta
//! client sends it live as `image_url`/`video_url` data URLs (see
//! [`ModelRequest::content_blocks`]. Parsing and IDs live in
//! `crate::prompts::vision` and `crate::evidence::screens`; this module only
//! runs the call and records what it cost.
//!
//! Cost is stated before a call is made ([`stated_cost`](crate::prompts::vision::stated_cost))
//! and each call is recorded with its usage: [`ReadOutcome::usage`] carries
//! the reply's [`Usage`].

use crate::evidence::screens::{RecordingMeta, ScreenIndex};
use crate::models::{MediaPart, ModelClient, ModelError, ModelRequest, Usage, request_key};
use crate::prompts::vision::{self, RawVersion, VisionParseError};

/// One still frame of a console version.
#[derive(Debug, Clone)]
pub struct ScreenInput {
    /// The screen's given name (for example `"lookup"`).
    pub name: String,
    /// The frame's raw PNG bytes.
    pub png: Vec<u8>,
}

/// One training recording of a console version.
#[derive(Debug, Clone)]
pub struct RecordingInput {
    /// The recording's given name (for example `"training-v1"`).
    pub name: String,
    /// Its duration in seconds, for span clamping.
    pub duration_s: f64,
    /// The recording's raw MP4 bytes.
    pub mp4: Vec<u8>,
}

/// What to read: one console version's frames plus its recording, if any.
#[derive(Debug, Clone)]
pub struct VersionSpec {
    /// The console (for example `"fernwhistle"`): scopes every element ID.
    pub console: String,
    /// The version label (for example `"v1"`): names the request only.
    pub version: String,
    /// The version's stills, in the order they go out.
    pub screens: Vec<ScreenInput>,
    /// The version's training recording, when it has one.
    pub recording: Option<RecordingInput>,
}

/// What one version read produced.
#[derive(Debug, Clone)]
pub struct ReadOutcome {
    /// The indexed version: screens, elements, steps, all with stable IDs.
    pub index: ScreenIndex,
    /// The call's usage, as the reply reported it.
    pub usage: Usage,
    /// Dropped step-screen names from parsing, plus indexing notes.
    pub warnings: Vec<String>,
    /// The request's replay key (media included).
    pub request_key: String,
}

/// What can go wrong reading a version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisionError {
    /// The model call itself failed (replay miss, backend, invalid).
    Model(ModelError),
    /// The reply did not parse against the requested screens.
    Parse(VisionParseError),
}

impl std::fmt::Display for VisionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Model(e) => write!(f, "{e}"),
            Self::Parse(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for VisionError {}

/// Read one console version with one Wonder call.
pub fn read_version(
    client: &impl ModelClient,
    spec: &VersionSpec,
) -> Result<ReadOutcome, VisionError> {
    let screen_parts: Vec<MediaPart> = spec
        .screens
        .iter()
        .map(|s| MediaPart::image_png(s.png.clone()))
        .collect();
    let screen_refs: Vec<(&str, &MediaPart)> = spec
        .screens
        .iter()
        .zip(screen_parts.iter())
        .map(|(s, part)| (s.name.as_str(), part))
        .collect();
    let video_part = spec
        .recording
        .as_ref()
        .map(|r| MediaPart::video_mp4(r.mp4.clone()));
    let recording_ref = spec
        .recording
        .as_ref()
        .zip(video_part.as_ref())
        .map(|(r, part)| (r.name.as_str(), r.duration_s, part));
    let request: ModelRequest =
        vision::build_version_request(&spec.console, &spec.version, &screen_refs, recording_ref);
    let key = request_key(&request);
    let response = client.complete(&request).map_err(VisionError::Model)?;
    let names: Vec<&str> = spec.screens.iter().map(|s| s.name.as_str()).collect();
    let raw: RawVersion =
        vision::parse_version_value(&response.value, &names).map_err(VisionError::Parse)?;
    let recording_meta = spec.recording.as_ref().map(|r| RecordingMeta {
        name: r.name.clone(),
        duration_s: r.duration_s,
    });
    let (index, mut warnings) = ScreenIndex::from_raw(&spec.console, &raw, recording_meta.as_ref());
    let mut all_warnings = raw.warnings.clone();
    all_warnings.append(&mut warnings);
    Ok(ReadOutcome {
        index,
        usage: response.usage,
        warnings: all_warnings,
        request_key: key,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ModelResponse;
    use std::cell::RefCell;

    struct CapturingClient {
        response: ModelResponse,
        seen: RefCell<Vec<ModelRequest>>,
    }

    impl ModelClient for CapturingClient {
        fn complete(&self, request: &ModelRequest) -> Result<ModelResponse, ModelError> {
            self.seen.borrow_mut().push(request.clone());
            Ok(self.response.clone())
        }
    }

    fn reply_value() -> serde_json::Value {
        serde_json::json!({
            "screens": [
                {"name": "lookup", "elements": [
                    {"text": "PATIENT LOOKUP", "row": 3, "col": 4, "kind": "heading", "protected": true},
                ]},
            ],
            "steps": [],
        })
    }

    fn spec() -> VersionSpec {
        VersionSpec {
            console: "fernwhistle".to_string(),
            version: "v1".to_string(),
            screens: vec![ScreenInput {
                name: "lookup".to_string(),
                png: vec![0x89, b'P', b'N', b'G'],
            }],
            recording: None,
        }
    }

    #[test]
    fn read_runs_one_call_and_records_its_usage() {
        let client = CapturingClient {
            response: ModelResponse {
                value: reply_value(),
                usage: Usage {
                    input_tokens: Some(1200),
                    output_tokens: Some(900),
                    cost_usd: None,
                },
            },
            seen: RefCell::new(Vec::new()),
        };
        let outcome = read_version(&client, &spec()).expect("reads");
        let seen = client.seen.borrow();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].model, vision::WONDER_MODEL);
        assert_eq!(seen[0].media.len(), 1);
        assert_eq!(outcome.usage.input_tokens, Some(1200));
        assert_eq!(outcome.usage.output_tokens, Some(900));
        assert!(outcome.warnings.is_empty());
        assert!(!outcome.request_key.is_empty());
        assert_eq!(outcome.index.screens().len(), 1);
    }

    #[test]
    fn read_reports_the_request_key_for_replay() {
        let client = CapturingClient {
            response: ModelResponse {
                value: reply_value(),
                usage: Usage::NONE,
            },
            seen: RefCell::new(Vec::new()),
        };
        let outcome = read_version(&client, &spec()).expect("reads");
        let expected = request_key(&client.seen.borrow()[0]);
        assert_eq!(outcome.request_key, expected);
    }

    #[test]
    fn read_passes_model_errors_through() {
        struct Missing;
        impl ModelClient for Missing {
            fn complete(&self, request: &ModelRequest) -> Result<ModelResponse, ModelError> {
                Err(ModelError::ReplayMiss {
                    key: request_key(request),
                })
            }
        }
        let error = read_version(&Missing, &spec()).expect_err("must miss");
        assert!(matches!(
            error,
            VisionError::Model(ModelError::ReplayMiss { .. })
        ));
    }

    #[test]
    fn read_passes_parse_errors_through() {
        let client = CapturingClient {
            response: ModelResponse {
                value: serde_json::json!({"screens": [], "steps": []}),
                usage: Usage::NONE,
            },
            seen: RefCell::new(Vec::new()),
        };
        let error = read_version(&client, &spec()).expect_err("must fail");
        assert!(matches!(error, VisionError::Parse(_)));
    }
}
