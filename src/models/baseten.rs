//! A [`ModelClient`] for Muse Spark Open on Baseten (the curator).
//!
//! The transport is deliberately behind a small seam ([`HttpTransport`])
//! so the client, its retry, parsing and usage/cost logic can be exercised
//! offline with a fake transport serving recorded HTTP replies. The
//! ureq (already a dependency on main via T3.3) is not used directly here:
//! the ureq-backed transport is wired in separately; this module never
//! touches `Cargo.toml`.
//!
//! The key is read from the environment at call time (see
//! [`BasetenModelClient::DEFAULT_ENV_VAR`], `BASETEN_API_KEY`), never stored,
//! never logged, and never placed in an error or `Display` output.
//!
//! The endpoint and model name come from T0.2 as fixed configuration:
//! `https://inference.baseten.co/v1` and `meta-models/muse-spark-open-1026`.
//! The undated `meta-models/muse-spark-open` returns 403 for the same key,
//! so the dated slug is the one to use. Both are overridable for tests
//! (base URL via [`BasetenModelClient::with_base_url`], model via the
//! request), not for production.
//!
//! Conformance to the request's JSON schema is requested via
//! `response_format`; Muse Spark Open accepts `json_schema` but does not
//! enforce it, so this client parses the returned JSON the same way the
//! Meta client does.
use crate::models::{ModelClient, ModelError, ModelRequest, ModelResponse, Usage};

/// Default model slug for the curator (Muse Spark Open on Baseten).
pub const DEFAULT_MODEL: &str = "meta-models/muse-spark-open-1026";
/// Alias for [`DEFAULT_MODEL`].
pub const CURATOR_MODEL: &str = DEFAULT_MODEL;
/// Alias for [`DEFAULT_MODEL`].
pub const MODEL: &str = DEFAULT_MODEL;
/// Alias for [`DEFAULT_MODEL`].
pub const DEFAULT_MODEL_ID: &str = DEFAULT_MODEL;
/// Alias for [`DEFAULT_MODEL`].
pub const CURATOR_MODEL_ID: &str = DEFAULT_MODEL;
/// Alias for [`DEFAULT_MODEL`].
pub const MODEL_ID: &str = DEFAULT_MODEL;

/// A single HTTP response as seen by the client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    /// HTTP status code.
    pub status: u16,
    /// Response body as text.
    pub body: String,
}

/// An error raised by an [`HttpTransport`] implementation (network failure,
/// TLS failure, ...). It carries a message only; no headers or keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpTransportError {
    /// What went wrong, in the transport's own terms.
    pub message: String,
}

impl std::fmt::Display for HttpTransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "http transport: {}", self.message)
    }
}

impl std::error::Error for HttpTransportError {}

/// The transport seam. Implementations perform one POST and return the
/// status + body; they must not follow redirects in a way that hides the
/// status, and must never log request headers or the authorization key.
pub trait HttpTransport {
    /// POST `body` (JSON) to `url` with the given headers, returning the
    /// response or a transport-level error.
    fn post(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> Result<HttpResponse, HttpTransportError>;
}

/// A [`ModelClient`] for Muse Spark Open on Baseten.
pub struct BasetenModelClient {
    transport: Box<dyn HttpTransport>,
    base_url: String,
    env_var: String,
    reasoning_effort: Option<String>,
    max_retries: u32,
}

impl BasetenModelClient {
    /// Default environment variable holding the API key.
    pub const DEFAULT_ENV_VAR: &'static str = "BASETEN_API_KEY";

    /// Default base URL for the Baseten inference API.
    pub const DEFAULT_BASE_URL: &'static str = "https://inference.baseten.co/v1";

    /// Default model slug for the curator (Muse Spark Open on Baseten).
    pub const DEFAULT_MODEL: &'static str = "meta-models/muse-spark-open-1026";
    /// Alias for [`DEFAULT_MODEL`].
    pub const CURATOR_MODEL: &'static str = "meta-models/muse-spark-open-1026";
    /// Alias for [`DEFAULT_MODEL`].
    pub const MODEL: &'static str = "meta-models/muse-spark-open-1026";
    /// Alias for [`DEFAULT_MODEL`].
    pub const DEFAULT_MODEL_ID: &'static str = "meta-models/muse-spark-open-1026";
    /// Alias for [`DEFAULT_MODEL`].
    pub const CURATOR_MODEL_ID: &'static str = "meta-models/muse-spark-open-1026";
    /// Alias for [`DEFAULT_MODEL`].
    pub const MODEL_ID: &'static str = "meta-models/muse-spark-open-1026";

    /// Build a client over `transport`, reading the key from
    /// [`DEFAULT_ENV_VAR`] at call time.
    pub fn new(transport: Box<dyn HttpTransport>) -> Self {
        Self {
            transport,
            base_url: Self::DEFAULT_BASE_URL.to_string(),
            env_var: Self::DEFAULT_ENV_VAR.to_string(),
            reasoning_effort: None,
            max_retries: 3,
        }
    }

    /// Override the base URL (useful for tests or proxies).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Override the environment variable the key is read from.
    pub fn with_env_var(mut self, env_var: impl Into<String>) -> Self {
        self.env_var = env_var.into();
        self
    }

    /// Set the reasoning effort passed through to the API
    /// (for example `"low"`, `"medium"`, `"high"`). When `None` the
    /// parameter is omitted.
    pub fn with_reasoning_effort(mut self, effort: impl Into<String>) -> Self {
        self.reasoning_effort = Some(effort.into());
        self
    }

    /// Cap the number of retries on 429 / 5xx / transport errors.
    pub fn with_max_retries(mut self, n: u32) -> Self {
        self.max_retries = n;
        self
    }

    fn api_key(&self) -> Result<String, ModelError> {
        std::env::var(&self.env_var).map_err(|_| ModelError::InvalidRequest {
            message: format!("missing {} in the environment", self.env_var),
        })
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    fn build_body(&self, request: &ModelRequest) -> serde_json::Value {
        let mut body = serde_json::json!({
            "model": request.model,
            "messages": [
                { "role": "system", "content": request.system },
                { "role": "user", "content": request.user },
            ],
            "response_format": {
                "type": "json_schema",
                "json_schema": {
                    "name": "response",
                    "schema": request.schema,
                    "strict": false,
                }
            }
        });
        if let Some(max) = request.budget.max_output_tokens {
            body["max_tokens"] = serde_json::json!(max);
        }
        if let Some(effort) = &self.reasoning_effort {
            body["reasoning_effort"] = serde_json::json!(effort);
        }
        body
    }
}

impl std::fmt::Debug for BasetenModelClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BasetenModelClient")
            .field("base_url", &self.base_url)
            .field("env_var", &self.env_var)
            .field("reasoning_effort", &self.reasoning_effort)
            .field("max_retries", &self.max_retries)
            .finish()
    }
}

impl ModelClient for BasetenModelClient {
    fn complete(&self, request: &ModelRequest) -> Result<ModelResponse, ModelError> {
        let key = self.api_key()?;
        let url = self.endpoint();
        let body = self.build_body(request);
        let body_text = serde_json::to_string(&body)
            .map_err(|e| ModelError::backend(format!("serialize request: {e}")))?;
        let auth_header = format!("Bearer {key}");
        let header_vec: Vec<(&str, &str)> = vec![
            ("Authorization", auth_header.as_str()),
            ("Content-Type", "application/json"),
        ];

        let model_id = request.model.clone();
        let max_attempts = self.max_retries.saturating_add(1);
        let mut last_error: Option<ModelError> = None;
        for attempt in 0..max_attempts {
            match self.transport.post(&url, &header_vec, &body_text) {
                Ok(resp) => {
                    if resp.status == 429 || (500..600).contains(&resp.status) {
                        let label = if resp.status == 429 {
                            "rate limited (429)"
                        } else {
                            "server error"
                        };
                        let err = ModelError::Backend {
                            message: format!(
                                "model {}: {} after attempt {}/{}: {}",
                                model_id,
                                label,
                                attempt + 1,
                                max_attempts,
                                first_lines(&resp.body, 5)
                            ),
                        };
                        if attempt + 1 < max_attempts {
                            last_error = Some(err);
                            continue;
                        }
                        return Err(err);
                    }
                    if !(200..300).contains(&resp.status) {
                        return Err(ModelError::Backend {
                            message: format!(
                                "model {}: HTTP {} : {}",
                                model_id,
                                resp.status,
                                first_lines(&resp.body, 5)
                            ),
                        });
                    }
                    return parse_reply(&model_id, &resp.body);
                }
                Err(te) => {
                    let err = ModelError::Backend {
                        message: format!(
                            "model {}: transport error on attempt {}/{}: {}",
                            model_id,
                            attempt + 1,
                            max_attempts,
                            te.message
                        ),
                    };
                    if attempt + 1 < max_attempts {
                        last_error = Some(err);
                        continue;
                    }
                    return Err(err);
                }
            }
        }
        Err(last_error.unwrap_or_else(|| ModelError::backend("model call failed")))
    }
}

fn parse_reply(model_id: &str, body: &str) -> Result<ModelResponse, ModelError> {
    let parsed: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => {
            return Err(ModelError::Backend {
                message: format!(
                    "model {}: malformed reply: {e}\n{}",
                    model_id,
                    first_lines(body, 5)
                ),
            });
        }
    };

    let content = &parsed["choices"][0]["message"]["content"];
    let value: serde_json::Value = match content {
        serde_json::Value::String(s) => match serde_json::from_str(s) {
            Ok(v) => v,
            Err(e) => {
                return Err(ModelError::Backend {
                    message: format!(
                        "model {}: malformed reply: {e}\n{}",
                        model_id,
                        first_lines(s, 5)
                    ),
                });
            }
        },
        serde_json::Value::Null => {
            return Err(ModelError::Backend {
                message: format!(
                    "model {}: malformed reply: missing choices[0].message.content\n{}",
                    model_id,
                    first_lines(body, 5)
                ),
            });
        }
        other => other.clone(),
    };

    let usage = parse_usage(&parsed);
    Ok(ModelResponse { value, usage })
}

fn parse_usage(body: &serde_json::Value) -> Usage {
    let usage = body.get("usage");
    let input_tokens = usage
        .and_then(|u| u.get("prompt_tokens").or_else(|| u.get("input_tokens")))
        .and_then(|v| v.as_u64());
    let output_tokens = usage
        .and_then(|u| {
            u.get("completion_tokens")
                .or_else(|| u.get("output_tokens"))
        })
        .and_then(|v| v.as_u64());
    let cost_usd = usage
        .and_then(|u| {
            u.get("cost")
                .or_else(|| u.get("cost_usd"))
                .or_else(|| u.get("total_cost"))
        })
        .and_then(|v| v.as_f64())
        .or_else(|| body.get("cost").and_then(|v| v.as_f64()));
    Usage {
        input_tokens,
        output_tokens,
        cost_usd,
    }
}

fn first_lines(s: &str, n: usize) -> String {
    const MAX_CHARS: usize = 500;
    let joined = s.lines().take(n).collect::<Vec<_>>().join("\n");
    if joined.chars().count() <= MAX_CHARS {
        joined
    } else {
        joined.chars().take(MAX_CHARS).collect()
    }
}

// ---------------------------------------------------------------------------
// Convenience aliases for alternate names callers/tests may use.
// ---------------------------------------------------------------------------
pub type BasetenClient = BasetenModelClient;
pub type BasetenHttpClient = BasetenModelClient;
pub type CuratorClient = BasetenModelClient;
pub type BasetenCuratorClient = BasetenModelClient;
pub type BasetenCurator = BasetenModelClient;
pub type Baseten = BasetenModelClient;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeTransport {
        status: u16,
        body: String,
        calls: Arc<AtomicUsize>,
    }

    impl HttpTransport for FakeTransport {
        fn post(
            &self,
            _url: &str,
            _headers: &[(&str, &str)],
            _body: &str,
        ) -> Result<HttpResponse, HttpTransportError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(HttpResponse {
                status: self.status,
                body: self.body.clone(),
            })
        }
    }

    fn env_guard() {
        unsafe {
            std::env::set_var(BasetenModelClient::DEFAULT_ENV_VAR, "test-key-not-real");
        }
    }

    fn sample_request() -> ModelRequest {
        crate::models::ModelRequest {
            model: BasetenModelClient::DEFAULT_MODEL.to_string(),
            system: "you are a curator".to_string(),
            user: "memory text".to_string(),
            schema: serde_json::json!({"type": "object", "properties": {"answer": {"type": "integer"}}}),
            media: Vec::new(),
            budget: crate::models::Budget::tokens(100),
        }
    }

    fn ok_body() -> String {
        serde_json::json!({
            "id": "resp-1",
            "choices": [{
                "message": {"role": "assistant", "content": "{\"answer\": 42}"}
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "cost": 0.001}
        })
        .to_string()
    }

    #[test]
    fn recorded_reply_parses_into_schema() {
        env_guard();
        let calls = Arc::new(AtomicUsize::new(0));
        let transport = FakeTransport {
            status: 200,
            body: ok_body(),
            calls: calls.clone(),
        };
        let client = BasetenModelClient::new(Box::new(transport));
        let resp = client.complete(&sample_request()).expect("must parse");
        assert_eq!(resp.value, serde_json::json!({"answer": 42}));
        assert_eq!(resp.usage.input_tokens, Some(10));
        assert_eq!(resp.usage.output_tokens, Some(5));
        assert_eq!(resp.usage.cost_usd, Some(0.001));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn malformed_reply_fails_with_model_id_and_first_lines() {
        env_guard();
        let calls = Arc::new(AtomicUsize::new(0));
        let body = "not json at all\nsecond line here".to_string();
        let transport = FakeTransport {
            status: 200,
            body: body.clone(),
            calls: calls.clone(),
        };
        let client = BasetenModelClient::new(Box::new(transport));
        let err = client.complete(&sample_request()).expect_err("must fail");
        let msg = err.to_string();
        assert!(
            msg.contains(BasetenModelClient::DEFAULT_MODEL),
            "missing model id: {msg}"
        );
        assert!(
            msg.contains("not json at all"),
            "missing first lines: {msg}"
        );
    }

    #[test]
    fn rate_limit_retries_then_gives_up() {
        env_guard();
        let calls = Arc::new(AtomicUsize::new(0));
        let transport = FakeTransport {
            status: 429,
            body: "rate limited\ntry later".to_string(),
            calls: calls.clone(),
        };
        let client = BasetenModelClient::new(Box::new(transport)).with_max_retries(2);
        let err = client
            .complete(&sample_request())
            .expect_err("must give up");
        let msg = err.to_string();
        assert!(
            msg.contains("429") || msg.to_lowercase().contains("rate limited"),
            "unclear 429: {msg}"
        );
        assert_eq!(
            calls.load(Ordering::SeqCst),
            3,
            "should attempt 1 + 2 retries"
        );
    }

    #[test]
    fn missing_key_is_invalid_request() {
        let calls = Arc::new(AtomicUsize::new(0));
        let transport = FakeTransport {
            status: 200,
            body: ok_body(),
            calls: calls.clone(),
        };
        let client = BasetenModelClient::new(Box::new(transport))
            .with_env_var("DOYLE_T4_2_DEFINITELY_UNSET_VAR");
        let err = client.complete(&sample_request()).expect_err("must fail");
        match err {
            ModelError::InvalidRequest { .. } => {}
            other => panic!("expected InvalidRequest, got {other:?}"),
        }
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "transport must not be called"
        );
    }

    #[test]
    fn reasoning_effort_is_passed_through() {
        env_guard();
        struct Capturing {
            body: Arc<std::sync::Mutex<String>>,
            calls: Arc<AtomicUsize>,
        }
        impl HttpTransport for Capturing {
            fn post(
                &self,
                _url: &str,
                _headers: &[(&str, &str)],
                body: &str,
            ) -> Result<HttpResponse, HttpTransportError> {
                *self.body.lock().unwrap() = body.to_string();
                self.calls.fetch_add(1, Ordering::SeqCst);
                Ok(HttpResponse {
                    status: 200,
                    body: ok_body(),
                })
            }
        }
        let captured = Arc::new(std::sync::Mutex::new(String::new()));
        let calls = Arc::new(AtomicUsize::new(0));
        let transport = Capturing {
            body: captured.clone(),
            calls: calls.clone(),
        };
        let client = BasetenModelClient::new(Box::new(transport)).with_reasoning_effort("high");
        let _ = client.complete(&sample_request()).expect("ok");
        let body = captured.lock().unwrap().clone();
        assert!(body.contains("\"reasoning_effort\""), "body: {body}");
        assert!(body.contains("\"high\""), "body: {body}");
    }

    #[test]
    fn five_xx_retries_then_gives_up() {
        env_guard();
        let calls = Arc::new(AtomicUsize::new(0));
        let transport = FakeTransport {
            status: 503,
            body: "unavailable".to_string(),
            calls: calls.clone(),
        };
        let client = BasetenModelClient::new(Box::new(transport)).with_max_retries(1);
        let err = client
            .complete(&sample_request())
            .expect_err("must give up");
        let msg = err.to_string();
        assert!(
            msg.contains("503") || msg.contains("server error"),
            "msg: {msg}"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn non_retryable_status_fails_immediately() {
        env_guard();
        let calls = Arc::new(AtomicUsize::new(0));
        let transport = FakeTransport {
            status: 400,
            body: "bad request".to_string(),
            calls: calls.clone(),
        };
        let client = BasetenModelClient::new(Box::new(transport));
        let err = client.complete(&sample_request()).expect_err("must fail");
        let msg = err.to_string();
        assert!(msg.contains("400"), "msg: {msg}");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn malformed_content_string_fails_with_model_id() {
        env_guard();
        let calls = Arc::new(AtomicUsize::new(0));
        let body = serde_json::json!({
            "id": "resp-2",
            "choices": [{"message": {"role": "assistant", "content": "not { valid json"}}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1}
        })
        .to_string();
        let transport = FakeTransport {
            status: 200,
            body,
            calls: calls.clone(),
        };
        let client = BasetenModelClient::new(Box::new(transport));
        let err = client.complete(&sample_request()).expect_err("must fail");
        let msg = err.to_string();
        assert!(
            msg.contains(BasetenModelClient::DEFAULT_MODEL),
            "msg: {msg}"
        );
        assert!(
            msg.contains("not { valid json") || msg.contains("not {"),
            "msg: {msg}"
        );
    }

    #[test]
    fn request_body_and_auth_header_are_pinned() {
        env_guard();
        let captured_body = Arc::new(std::sync::Mutex::new(String::new()));
        let captured_headers = Arc::new(std::sync::Mutex::new(Vec::<(String, String)>::new()));
        let calls = Arc::new(AtomicUsize::new(0));
        struct CapturingAll {
            body: Arc<std::sync::Mutex<String>>,
            headers: Arc<std::sync::Mutex<Vec<(String, String)>>>,
            calls: Arc<AtomicUsize>,
        }
        impl HttpTransport for CapturingAll {
            fn post(
                &self,
                _url: &str,
                headers: &[(&str, &str)],
                body: &str,
            ) -> Result<HttpResponse, HttpTransportError> {
                *self.body.lock().unwrap() = body.to_string();
                *self.headers.lock().unwrap() = headers
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                self.calls.fetch_add(1, Ordering::SeqCst);
                Ok(HttpResponse {
                    status: 200,
                    body: ok_body(),
                })
            }
        }
        let transport = CapturingAll {
            body: captured_body.clone(),
            headers: captured_headers.clone(),
            calls: calls.clone(),
        };
        let client = BasetenModelClient::new(Box::new(transport)).with_reasoning_effort("high");
        let req = sample_request();
        let _ = client.complete(&req).expect("ok");
        let body_text = captured_body.lock().unwrap().clone();
        let body: serde_json::Value =
            serde_json::from_str(&body_text).expect("captured body is json");
        assert_eq!(body["model"], req.model);
        assert_eq!(body["response_format"]["json_schema"]["schema"], req.schema);
        assert_eq!(body["max_tokens"], 100);
        assert_eq!(body["reasoning_effort"], "high");
        let headers = captured_headers.lock().unwrap().clone();
        let auth = headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Authorization"))
            .map(|(_, v)| v.clone())
            .expect("Authorization header present");
        assert!(auth.starts_with("Bearer "), "auth: {auth}");
    }

    #[test]
    fn long_single_line_body_is_bounded_in_error() {
        env_guard();
        let calls = Arc::new(AtomicUsize::new(0));
        let body = "x".repeat(10_000);
        let transport = FakeTransport {
            status: 200,
            body: body.clone(),
            calls: calls.clone(),
        };
        let client = BasetenModelClient::new(Box::new(transport));
        let err = client.complete(&sample_request()).expect_err("must fail");
        let msg = err.to_string();
        assert!(msg.len() <= 700, "unbounded error: {} chars", msg.len());
    }

    #[test]
    fn transport_error_retries_then_gives_up() {
        env_guard();
        let calls = Arc::new(AtomicUsize::new(0));
        struct FailingTransport {
            calls: Arc<AtomicUsize>,
        }
        impl HttpTransport for FailingTransport {
            fn post(
                &self,
                _url: &str,
                _headers: &[(&str, &str)],
                _body: &str,
            ) -> Result<HttpResponse, HttpTransportError> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                Err(HttpTransportError {
                    message: "connection refused".to_string(),
                })
            }
        }
        let transport = FailingTransport {
            calls: calls.clone(),
        };
        let client = BasetenModelClient::new(Box::new(transport)).with_max_retries(2);
        let err = client.complete(&sample_request()).expect_err("must fail");
        let msg = err.to_string();
        assert!(
            msg.contains(BasetenModelClient::DEFAULT_MODEL),
            "missing model id: {msg}"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 3, "attempts");
    }

    fn body_with_usage(usage: serde_json::Value, top_level_cost: Option<f64>) -> String {
        let mut v = serde_json::json!({
            "id": "resp-u",
            "choices": [{"message": {"role": "assistant", "content": "{\"answer\": 7}"}}],
            "usage": usage,
        });
        if let Some(c) = top_level_cost {
            v["cost"] = serde_json::json!(c);
        }
        v.to_string()
    }

    #[test]
    fn usage_cost_usd_spelling() {
        env_guard();
        let calls = Arc::new(AtomicUsize::new(0));
        let body = body_with_usage(
            serde_json::json!({"prompt_tokens": 3, "completion_tokens": 4, "cost_usd": 0.02}),
            None,
        );
        let transport = FakeTransport {
            status: 200,
            body,
            calls: calls.clone(),
        };
        let client = BasetenModelClient::new(Box::new(transport));
        let resp = client.complete(&sample_request()).expect("ok");
        assert_eq!(resp.usage.input_tokens, Some(3));
        assert_eq!(resp.usage.output_tokens, Some(4));
        assert_eq!(resp.usage.cost_usd, Some(0.02));
    }

    #[test]
    fn usage_total_cost_spelling() {
        env_guard();
        let calls = Arc::new(AtomicUsize::new(0));
        let body = body_with_usage(
            serde_json::json!({"prompt_tokens": 3, "completion_tokens": 4, "total_cost": 0.03}),
            None,
        );
        let transport = FakeTransport {
            status: 200,
            body,
            calls: calls.clone(),
        };
        let client = BasetenModelClient::new(Box::new(transport));
        let resp = client.complete(&sample_request()).expect("ok");
        assert_eq!(resp.usage.cost_usd, Some(0.03));
    }

    #[test]
    fn usage_top_level_cost() {
        env_guard();
        let calls = Arc::new(AtomicUsize::new(0));
        let body = body_with_usage(
            serde_json::json!({"prompt_tokens": 3, "completion_tokens": 4}),
            Some(0.04),
        );
        let transport = FakeTransport {
            status: 200,
            body,
            calls: calls.clone(),
        };
        let client = BasetenModelClient::new(Box::new(transport));
        let resp = client.complete(&sample_request()).expect("ok");
        assert_eq!(resp.usage.cost_usd, Some(0.04));
    }

    #[test]
    fn usage_input_output_tokens_spelling() {
        env_guard();
        let calls = Arc::new(AtomicUsize::new(0));
        let body = body_with_usage(
            serde_json::json!({"input_tokens": 11, "output_tokens": 22, "cost": 0.05}),
            None,
        );
        let transport = FakeTransport {
            status: 200,
            body,
            calls: calls.clone(),
        };
        let client = BasetenModelClient::new(Box::new(transport));
        let resp = client.complete(&sample_request()).expect("ok");
        assert_eq!(resp.usage.input_tokens, Some(11));
        assert_eq!(resp.usage.output_tokens, Some(22));
        assert_eq!(resp.usage.cost_usd, Some(0.05));
    }

    #[test]
    fn usage_absent_is_all_none() {
        env_guard();
        let calls = Arc::new(AtomicUsize::new(0));
        let body = serde_json::json!({
            "id": "resp-n",
            "choices": [{"message": {"role": "assistant", "content": "{\"answer\": 9}"}}]
        })
        .to_string();
        let transport = FakeTransport {
            status: 200,
            body,
            calls: calls.clone(),
        };
        let client = BasetenModelClient::new(Box::new(transport));
        let resp = client.complete(&sample_request()).expect("ok");
        assert_eq!(resp.usage, Usage::NONE);
    }

    #[test]
    fn constants_match_t0_2() {
        assert_eq!(
            BasetenModelClient::DEFAULT_BASE_URL,
            "https://inference.baseten.co/v1"
        );
        assert_eq!(
            BasetenModelClient::DEFAULT_MODEL,
            "meta-models/muse-spark-open-1026"
        );
        assert_eq!(BasetenModelClient::DEFAULT_ENV_VAR, "BASETEN_API_KEY");
    }

    #[test]
    fn debug_does_not_leak_key() {
        env_guard();
        let transport = FakeTransport {
            status: 200,
            body: ok_body(),
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let client = BasetenModelClient::new(Box::new(transport));
        let dbg = format!("{:?}", client);
        assert!(
            !dbg.contains("test-key-not-real"),
            "key leaked in Debug: {dbg}"
        );
    }
}
