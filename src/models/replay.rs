//! The replay client and the recording wrapper.
//!
//! [`ReplayClient`] serves recorded replies from a directory, keyed by
//! [`request_key`](crate::models::request_key); a miss fails loudly with the
//! request's key in the message. [`RecordingClient`] wraps any
//! [`ModelClient`] and writes what it gets, so a live run can be captured
//! once and replayed afterwards.

use crate::models::{ModelClient, ModelError, ModelRequest, ModelResponse, request_key};
use std::path::{Path, PathBuf};

/// A client that serves recorded replies from a directory.
///
/// Each recording is a file named `<key>.json`, where `<key>` is
/// [`request_key`] of the request. The file holds a serialized
/// [`ModelResponse`] (a `value` field plus a `usage` field).
#[derive(Debug, Clone)]
pub struct ReplayClient {
    dir: PathBuf,
}

impl ReplayClient {
    /// Build a replay client reading from `dir`.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// The directory recordings are read from.
    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

impl ModelClient for ReplayClient {
    fn complete(&self, request: &ModelRequest) -> Result<ModelResponse, ModelError> {
        let key = request_key(request);
        let path = self.dir.join(format!("{key}.json"));
        if !path.exists() {
            return Err(ModelError::ReplayMiss { key });
        }
        let bytes = std::fs::read(&path)
            .map_err(|e| ModelError::backend(format!("read recording {key}: {e}")))?;
        let response: ModelResponse = serde_json::from_slice(&bytes)
            .map_err(|e| ModelError::backend(format!("parse recording {key}: {e}")))?;
        Ok(response)
    }
}

/// A client that wraps another client and writes every reply it gets to a
/// directory, keyed by [`request_key`], so it can be replayed later.
#[derive(Debug)]
pub struct RecordingClient<C> {
    inner: C,
    dir: PathBuf,
}

impl<C> RecordingClient<C> {
    /// Wrap `inner`, writing recordings into `dir`.
    pub fn new(inner: C, dir: impl Into<PathBuf>) -> Self {
        Self {
            inner,
            dir: dir.into(),
        }
    }

    /// The wrapped client.
    pub fn inner(&self) -> &C {
        &self.inner
    }

    /// The directory recordings are written to.
    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

impl<C: ModelClient> ModelClient for RecordingClient<C> {
    fn complete(&self, request: &ModelRequest) -> Result<ModelResponse, ModelError> {
        let response = self.inner.complete(request)?;
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| ModelError::backend(format!("create recording dir: {e}")))?;
        let key = request_key(request);
        let path = self.dir.join(format!("{key}.json"));
        let bytes = serde_json::to_vec_pretty(&response)
            .map_err(|e| ModelError::backend(format!("serialize recording {key}: {e}")))?;
        std::fs::write(&path, bytes)
            .map_err(|e| ModelError::backend(format!("write recording {key}: {e}")))?;
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Budget, ModelRequest, Usage};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn temp_dir(label: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "doyle-model-{}-{}-{}",
            label,
            std::process::id(),
            n
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn sample_request() -> ModelRequest {
        ModelRequest {
            model: "muse-spark-1.3".to_string(),
            system: "you are a curator".to_string(),
            user: "memory text".to_string(),
            schema: serde_json::json!({"type": "object"}),
            budget: Budget::tokens(100),
        }
    }

    fn sample_response() -> ModelResponse {
        ModelResponse {
            value: serde_json::json!({"elements": ["ve2_abc"]}),
            usage: Usage {
                input_tokens: Some(10),
                output_tokens: Some(5),
                cost_usd: Some(0.001),
            },
        }
    }

    /// A client returning a canned response, for tests.
    struct FakeClient {
        response: ModelResponse,
    }

    impl ModelClient for FakeClient {
        fn complete(&self, _request: &ModelRequest) -> Result<ModelResponse, ModelError> {
            Ok(self.response.clone())
        }
    }

    #[test]
    fn request_key_is_stable() {
        let a = request_key(&sample_request());
        let b = request_key(&sample_request());
        assert_eq!(a, b);
        assert!(!a.is_empty());
    }

    #[test]
    fn replay_serves_hand_written_recording() {
        let dir = temp_dir("serve");
        let request = sample_request();
        let key = request_key(&request);
        let response = sample_response();
        let path = dir.join(format!("{key}.json"));
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&response).expect("serialize"),
        )
        .expect("write recording");

        let client = ReplayClient::new(&dir);
        let got = client.complete(&request).expect("replay must succeed");
        assert_eq!(got, response);
    }

    #[test]
    fn replay_miss_fails_with_key_in_message() {
        let dir = temp_dir("miss");
        let request = sample_request();
        let key = request_key(&request);
        let client = ReplayClient::new(&dir);
        let err = client.complete(&request).expect_err("must miss");
        match err {
            ModelError::ReplayMiss { key: ref k } => assert_eq!(k, &key),
            other => panic!("expected ReplayMiss, got {other:?}"),
        }
        assert!(err.to_string().contains(&key));
    }

    #[test]
    fn recording_then_replaying_gives_same_reply() {
        let dir = temp_dir("record-replay");
        let request = sample_request();
        let response = sample_response();
        let inner = FakeClient {
            response: response.clone(),
        };
        let recorder = RecordingClient::new(inner, &dir);
        let recorded = recorder.complete(&request).expect("record");
        assert_eq!(recorded, response);

        // The recording file exists under the request's key.
        let key = request_key(&request);
        assert!(dir.join(format!("{key}.json")).exists());

        // Replaying from the same directory yields the same reply.
        let replayer = ReplayClient::new(&dir);
        let replayed = replayer.complete(&request).expect("replay");
        assert_eq!(replayed, response);
    }

    #[test]
    fn recording_creates_directory_if_missing() {
        let base = temp_dir("nested");
        let dir = base.join("nested").join("recordings");
        let request = sample_request();
        let inner = FakeClient {
            response: sample_response(),
        };
        let recorder = RecordingClient::new(inner, &dir);
        recorder.complete(&request).expect("record");
        assert!(dir.exists());
    }

    // LB3: golden value pins the FNV-1a key for a known request.
    #[test]
    fn request_key_golden_value() {
        assert_eq!(request_key(&sample_request()), "76c0de1fa816862c");
    }

    // LB1: a missing cost cap and a zero cost cap must not collide.
    #[test]
    fn request_key_budget_cost_none_vs_zero_differs() {
        let base = sample_request();
        let mut zero_cost = sample_request();
        zero_cost.budget = Budget {
            max_output_tokens: base.budget.max_output_tokens,
            max_cost_usd: Some(0.0),
        };
        assert_ne!(request_key(&base), request_key(&zero_cost));
    }

    // LB2: reordering schema keys must not change the key.
    #[test]
    fn request_key_schema_key_order_invariant() {
        let mut a = sample_request();
        a.schema = serde_json::json!({"type": "object", "properties": {"b": {"type": "string"}, "a": {"type": "integer"}}});
        let mut b = sample_request();
        b.schema = serde_json::json!({"properties": {"a": {"type": "integer"}, "b": {"type": "string"}}, "type": "object"});
        assert_eq!(request_key(&a), request_key(&b));
    }

    // LB4: varying each keyed field alone flips the key.
    #[test]
    fn request_key_varies_model() {
        let base = sample_request();
        let mut v = sample_request();
        v.model = "other-model".to_string();
        assert_ne!(request_key(&base), request_key(&v));
    }

    #[test]
    fn request_key_varies_system() {
        let base = sample_request();
        let mut v = sample_request();
        v.system = "different system".to_string();
        assert_ne!(request_key(&base), request_key(&v));
    }

    #[test]
    fn request_key_varies_user() {
        let base = sample_request();
        let mut v = sample_request();
        v.user = "different user".to_string();
        assert_ne!(request_key(&base), request_key(&v));
    }

    #[test]
    fn request_key_varies_schema() {
        let base = sample_request();
        let mut v = sample_request();
        v.schema = serde_json::json!({"type": "array"});
        assert_ne!(request_key(&base), request_key(&v));
    }

    #[test]
    fn request_key_varies_max_output_tokens() {
        let base = sample_request();
        let mut v = sample_request();
        v.budget = Budget::tokens(101);
        assert_ne!(request_key(&base), request_key(&v));
    }

    #[test]
    fn request_key_varies_max_cost_usd() {
        let base = sample_request();
        // base has no cost cap; set a cap on the variant
        let mut v = sample_request();
        v.budget = Budget {
            max_output_tokens: base.budget.max_output_tokens,
            max_cost_usd: Some(1.0),
        };
        assert_ne!(request_key(&base), request_key(&v));
    }

    #[test]
    fn request_key_varies_cost_none_vs_some_zero() {
        // LB1 dimension: None vs Some(0.0) on cost flips the key even with
        // identical tokens.
        let a = Budget {
            max_output_tokens: Some(100),
            max_cost_usd: None,
        };
        let b = Budget {
            max_output_tokens: Some(100),
            max_cost_usd: Some(0.0),
        };
        let mut ra = sample_request();
        ra.budget = a;
        let mut rb = sample_request();
        rb.budget = b;
        assert_ne!(request_key(&ra), request_key(&rb));
    }

    #[test]
    fn request_key_varies_tokens_none_vs_some_zero() {
        let mut ra = sample_request();
        ra.budget = Budget {
            max_output_tokens: None,
            max_cost_usd: Some(1.0),
        };
        let mut rb = sample_request();
        rb.budget = Budget {
            max_output_tokens: Some(0),
            max_cost_usd: Some(1.0),
        };
        assert_ne!(request_key(&ra), request_key(&rb));
    }
}
