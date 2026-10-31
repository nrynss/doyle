//! Live structured call on the Baseten curator model (T4.2b).
//!
//! This test calls Muse Spark Open on Baseten through
//! [`BasetenModelClient`](doyle_tms::models::baseten::BasetenModelClient)
//! over a small ureq-backed transport, records the reply with
//! [`RecordingClient`](doyle_tms::models::RecordingClient) into a temp dir,
//! and replays it with
//! [`ReplayClient`](doyle_tms::models::ReplayClient) to prove the recording
//! replays offline.
//!
//! Ignored by default: it needs a network and `BASETEN_API_KEY` in the
//! environment. Run it with `cargo test -- --ignored live_`. It never runs
//! in CI and never gates. The key is read from the environment at call
//! time and is never printed, logged, or written anywhere.

use doyle_tms::models::baseten::{
    BasetenModelClient, HttpResponse, HttpTransport, HttpTransportError,
};
use doyle_tms::models::{Budget, ModelClient, ModelRequest, RecordingClient, ReplayClient};

/// Minimal ureq-backed [`HttpTransport`], local to this live test.
///
/// Non-2xx statuses are returned as data (not errors) so the client's own
/// retry/error logic sees the real wire shape. Nothing sensitive is logged.
struct UreqTransport;

impl HttpTransport for UreqTransport {
    fn post(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> Result<HttpResponse, HttpTransportError> {
        let mut request = ureq::post(url).config().http_status_as_error(false).build();
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let response = request.send(body).map_err(|e| HttpTransportError {
            message: format!("ureq post failed: {e}"),
        })?;
        let status = response.status().as_u16();
        let text = response
            .into_body()
            .read_to_string()
            .map_err(|e| HttpTransportError {
                message: format!("ureq read body failed: {e}"),
            })?;
        Ok(HttpResponse { status, body: text })
    }
}

fn probe_request() -> ModelRequest {
    ModelRequest {
        model: BasetenModelClient::DEFAULT_MODEL.to_string(),
        system: "You are a test probe. Reply with exactly the requested JSON object.".to_string(),
        user: "Return the integer 42 as `answer` with a short `note`.".to_string(),
        schema: serde_json::json!({
            "type": "object",
            "properties": {
                "answer": {"type": "integer"},
                "note": {"type": "string"}
            },
            "required": ["answer", "note"],
            "additionalProperties": false
        }),
        // 2048: a smaller cap starves content because the model spends
        // the budget on reasoning first (content comes back null).
        media: Vec::new(),
        budget: Budget::tokens(2048),
    }
}

#[test]
#[ignore = "live: calls Baseten; run with cargo test -- --ignored live_"]
fn live_baseten_curator_structured_call() {
    assert!(
        std::env::var_os(BasetenModelClient::DEFAULT_ENV_VAR).is_some(),
        "live test needs BASETEN_API_KEY set in the environment"
    );
    // Pin the live wire shape before calling.
    assert_eq!(
        BasetenModelClient::DEFAULT_BASE_URL,
        "https://inference.baseten.co/v1"
    );
    assert_eq!(
        BasetenModelClient::DEFAULT_MODEL,
        "meta-models/muse-spark-open-1026"
    );

    let request = probe_request();
    let dir = std::env::temp_dir().join(format!(
        "doyle-live-baseten-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_secs()
    ));
    let _ = std::fs::remove_dir_all(&dir);

    let client = BasetenModelClient::new(Box::new(UreqTransport));
    let recorder = RecordingClient::new(client, &dir);
    let response = recorder.complete(&request).expect("live call must succeed");

    eprintln!(
        "endpoint: {}/chat/completions",
        BasetenModelClient::DEFAULT_BASE_URL
    );
    eprintln!("model: {}", request.model);
    eprintln!("value: {}", response.value);
    eprintln!(
        "usage: input={:?} output={:?} cost_usd={:?}",
        response.usage.input_tokens, response.usage.output_tokens, response.usage.cost_usd
    );

    assert!(
        response.value.is_object(),
        "reply must parse as a JSON object, got: {}",
        response.value
    );

    // The recording must exist and replay byte-equal offline.
    let replayed = ReplayClient::new(&dir)
        .complete(&request)
        .expect("recording must replay");
    assert_eq!(
        replayed, response,
        "replayed reply must equal the live reply"
    );
}
