//! `doyle screens read`: Doyle's eyes from the command line.
//!
//! One [`read_version`](vision::read_version) call turns a console version's
//! stills (and its training recording, when it has one) into screens,
//! elements and steps, and this command writes them as the screens version
//! file [`crate::check::load_screen_version`] reads: `console`, `screens`
//! (each with its `elements`), `steps` and `recording {name, duration_s}`.
//!
//! Offline (the default) the reply is replayed from `DOYLE_REPLAY_DIR`, or
//! `testdata/replies/vision` when unset; a miss is an error naming the
//! request key and nothing is written. With `--live` the call goes to the
//! Meta Model API (`muse-wonder-1.0`) with the key from `MODEL_API_KEY` or
//! the keyring; when `DOYLE_RECORD_DIR` is also set, the reply is recorded
//! there under its request key so a live read can become a committed
//! recording. The key is never printed.

use std::cell::RefCell;
use std::process::ExitCode;

use crate::cli;
use crate::models::http::UreqTransport;
use crate::models::keys;
use crate::models::meta::MetaModelClient;
use crate::models::vision::{self, RecordingInput, ScreenInput, VersionSpec};
use crate::models::{
    ModelClient, ModelError, ModelRequest, ModelResponse, RecordingClient, ReplayClient,
};
use crate::prompts::vision::{RawVersion, parse_version_value, stated_cost};

/// Where replies replay from when `DOYLE_REPLAY_DIR` is unset.
pub const DEFAULT_REPLAY_DIR: &str = "testdata/replies/vision";

/// Live reads wait on a reasoning model and a video: allow five minutes.
const LIVE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// Run the `screens` command.
pub fn run(cli: &cli::Cli) -> ExitCode {
    match execute(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("doyle screens read: {e}");
            ExitCode::from(2)
        }
    }
}

/// Keeps the reply value so the command can write exactly what was read.
struct Capture<'a> {
    inner: &'a dyn ModelClient,
    value: RefCell<Option<serde_json::Value>>,
}

impl ModelClient for Capture<'_> {
    fn complete(&self, request: &ModelRequest) -> Result<ModelResponse, ModelError> {
        let response = self.inner.complete(request)?;
        *self.value.borrow_mut() = Some(response.value.clone());
        Ok(response)
    }
}

fn split_pair<'a>(flag: &str, pair: &'a str) -> Result<(&'a str, &'a str), String> {
    match pair.split_once('=') {
        Some((name, path)) if !name.is_empty() && !path.is_empty() => Ok((name, path)),
        _ => Err(format!("{flag} wants NAME=PATH, got {pair:?}")),
    }
}

fn read_file(path: &str) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("{path}: {e}"))
}

/// The model client for this run: live Meta (optionally recording) or replay.
fn client_for(live: bool) -> Result<Box<dyn ModelClient>, String> {
    if !live {
        let dir = std::env::var("DOYLE_REPLAY_DIR").unwrap_or_else(|_| DEFAULT_REPLAY_DIR.into());
        return Ok(Box::new(ReplayClient::new(dir)));
    }
    if std::env::var(MetaModelClient::DEFAULT_ENV_VAR)
        .map(|v| v.is_empty())
        .unwrap_or(true)
    {
        let key = keys::meta_key().map_err(|e| e.to_string())?;
        // SAFETY: the live path runs before any thread spawns, and the
        // client reads the process environment at call time.
        unsafe {
            std::env::set_var(MetaModelClient::DEFAULT_ENV_VAR, key);
        }
    }
    let meta = MetaModelClient::new(Box::new(UreqTransport::with_timeout(LIVE_TIMEOUT)))
        .with_reasoning_effort("low");
    match std::env::var("DOYLE_RECORD_DIR") {
        Ok(dir) if !dir.is_empty() => Ok(Box::new(RecordingClient::new(meta, dir))),
        _ => Ok(Box::new(meta)),
    }
}

/// The version file for a reading, in the shape `check::load_screen_version`
/// parses.
fn version_json(
    console: &str,
    raw: &RawVersion,
    recording: Option<&RecordingInput>,
) -> serde_json::Value {
    let screens: Vec<serde_json::Value> = raw
        .screens
        .iter()
        .map(|s| {
            let elements: Vec<serde_json::Value> = s
                .elements
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "text": e.text,
                        "row": e.row,
                        "col": e.col,
                        "kind": e.kind,
                        "protected": e.protected,
                    })
                })
                .collect();
            serde_json::json!({"name": s.name, "elements": elements})
        })
        .collect();
    let steps: Vec<serde_json::Value> = raw
        .steps
        .iter()
        .map(|s| {
            serde_json::json!({
                "description": s.description,
                "start_s": s.start_s,
                "end_s": s.end_s,
                "screen": s.screen,
            })
        })
        .collect();
    let mut doc = serde_json::json!({"console": console, "screens": screens, "steps": steps});
    if let Some(r) = recording {
        doc["recording"] = serde_json::json!({"name": r.name, "duration_s": r.duration_s});
    }
    doc
}

fn execute(cli: &cli::Cli) -> Result<(), String> {
    let cli::Command::Screens(args) = &cli.command else {
        return Err(String::from("expected a Screens command"));
    };
    let cli::ScreensArgs {
        console,
        version,
        screens,
        recording,
        duration_s,
        output,
    } = args.as_ref();

    let mut inputs = Vec::new();
    for pair in screens {
        let (name, path) = split_pair("--screen", pair)?;
        inputs.push(ScreenInput {
            name: name.to_string(),
            png: read_file(path)?,
        });
    }
    let recording_input = match (recording, duration_s) {
        (Some(pair), Some(secs)) => {
            let (name, path) = split_pair("--recording", pair)?;
            let duration: f64 = secs
                .parse()
                .ok()
                .filter(|d: &f64| d.is_finite() && *d > 0.0)
                .ok_or_else(|| format!("--duration-s wants positive seconds, got {secs:?}"))?;
            Some(RecordingInput {
                name: name.to_string(),
                duration_s: duration,
                mp4: read_file(path)?,
            })
        }
        _ => None,
    };
    let spec = VersionSpec {
        console: console.clone(),
        version: version.clone(),
        screens: inputs,
        recording: recording_input,
    };

    let client = client_for(cli.common.live)?;
    let capture = Capture {
        inner: client.as_ref(),
        value: RefCell::new(None),
    };

    let cost = stated_cost(
        spec.screens.len(),
        spec.recording.as_ref().map(|r| r.duration_s),
    );
    eprintln!(
        "doyle screens read: {console} {version}: {} stills{}, stated cost {cost}",
        spec.screens.len(),
        spec.recording
            .as_ref()
            .map(|r| format!(" + {} ({}s)", r.name, r.duration_s))
            .unwrap_or_default(),
    );

    let outcome = vision::read_version(&capture, &spec).map_err(|e| e.to_string())?;

    let show = |n: Option<u64>| n.map(|v| v.to_string()).unwrap_or_else(|| "?".into());
    eprintln!(
        "doyle screens read: usage {} input / {} output tokens (request {})",
        show(outcome.usage.input_tokens),
        show(outcome.usage.output_tokens),
        outcome.request_key
    );
    for warning in &outcome.warnings {
        eprintln!("doyle screens read: warning: {warning}");
    }

    let value = capture
        .value
        .borrow_mut()
        .take()
        .ok_or_else(|| String::from("the model returned no reply"))?;
    let names: Vec<&str> = spec.screens.iter().map(|s| s.name.as_str()).collect();
    let raw = parse_version_value(&value, &names).map_err(|e| e.to_string())?;
    let doc = version_json(console, &raw, spec.recording.as_ref());
    let text = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    std::fs::write(output, format!("{text}\n")).map_err(|e| format!("{output}: {e}"))?;

    let elements: usize = raw.screens.iter().map(|s| s.elements.len()).sum();
    eprintln!(
        "doyle screens read: wrote {output}: {} screens, {elements} elements, {} steps",
        raw.screens.len(),
        raw.steps.len()
    );
    Ok(())
}
