//! Live test for the Meta Model API client (T4.1b, sibling of T4.1).
//!
//! One structured call on `muse-spark-1.3` through [`MetaModelClient`],
//! recorded through the recording mode from T1.4 so it replays offline.
//!
//! This test is `#[ignore]`d: it calls the real API, needs a key, and never
//! runs in CI or the gate. Run it explicitly with the key in the
//! environment:
//!
//! ```sh
//! MODEL_API_KEY="$(secret-tool lookup service meta-model-api)" \
//!     cargo test --test live_meta -- --ignored live_ --nocapture
//! ```
//!
//! The key is read from the environment at call time and is never printed,
//! logged, or written to any file. The recording holds only the parsed
//! `ModelResponse` (value + usage), keyed by [`request_key`], in a temp dir
//! outside the repository.
//!
//! Budget: five calls. This file makes exactly one.

use doyle_tms::models::meta::{HttpResponse, HttpTransport, HttpTransportError, MetaModelClient};
use doyle_tms::models::{
    Budget, ModelClient, ModelRequest, RecordingClient, ReplayClient, request_key,
};
use std::time::Duration;

/// Minimal [`HttpTransport`] over ureq, for the live call only.
///
/// (The ureq-backed transport for production use lands with T3.3's wiring;
/// this file owns nothing outside `tests/live_meta.rs`, so the seam is
/// implemented here and never committed to `src/`.)
struct UreqTransport {
    agent: ureq::Agent,
}

impl UreqTransport {
    fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(60)))
            .build();
        Self {
            agent: ureq::Agent::new_with_config(config),
        }
    }
}

impl HttpTransport for UreqTransport {
    fn post(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> Result<HttpResponse, HttpTransportError> {
        let mut request = self.agent.post(url);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let response = request
            .header("Content-Type", "application/json")
            .send(body)
            .map_err(|e| HttpTransportError {
                message: format!("live transport: {e}"),
            })?;
        let status = response.status().as_u16();
        let text = response
            .into_body()
            .read_to_string()
            .map_err(|e| HttpTransportError {
                message: format!("live transport: read body: {e}"),
            })?;
        Ok(HttpResponse { status, body: text })
    }
}

/// One structured call on `muse-spark-1.3`, recorded and replayed.
///
/// Runs only under `cargo test -- --ignored live_`; needs `MODEL_API_KEY`.
#[test]
#[ignore]
fn live_meta_spark_structured_call() {
    assert!(
        std::env::var(MetaModelClient::DEFAULT_ENV_VAR).is_ok(),
        "set {} in the environment (see module docs)",
        MetaModelClient::DEFAULT_ENV_VAR
    );

    let request = ModelRequest {
        model: "muse-spark-1.3".to_string(),
        system: "Reply with exactly the requested JSON object and nothing else.".to_string(),
        user: r#"Return the object {"answer": 42}."#.to_string(),
        schema: serde_json::json!({
            "type": "object",
            "properties": { "answer": { "type": "integer" } },
            "required": ["answer"],
            "additionalProperties": false,
        }),
        budget: Budget::tokens(512),
    };

    let dir = std::env::temp_dir().join(format!("doyle-t41b-live-{}", std::process::id()));
    let recorder = RecordingClient::new(MetaModelClient::new(Box::new(UreqTransport::new())), &dir);

    let response = recorder
        .complete(&request)
        .expect("live structured call must succeed");
    assert_eq!(
        response.value.get("answer"),
        Some(&serde_json::json!(42)),
        "unexpected structured value: {}",
        response.value
    );

    // Evidence for the report. No key, no headers, no personal data.
    eprintln!("model: {}", request.model);
    eprintln!("value: {}", response.value);
    eprintln!(
        "usage: input_tokens={:?} output_tokens={:?} cost_usd={:?}",
        response.usage.input_tokens, response.usage.output_tokens, response.usage.cost_usd
    );

    // The recording replays offline through the replay client.
    let key = request_key(&request);
    assert!(
        dir.join(format!("{key}.json")).exists(),
        "recording missing for key {key}"
    );
    let replayed = ReplayClient::new(&dir)
        .complete(&request)
        .expect("replay must serve the recording");
    assert_eq!(replayed, response, "replay must match the live reply");
}
