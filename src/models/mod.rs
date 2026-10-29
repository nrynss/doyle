//! Doyle's model clients: the plug-in seam between the core and the Meta
//! models (Muse Spark, Muse Spark Open on Baseten) and the replay client
//! used for tests and an offline demo.
//!
//! The core calls only [`ModelClient`]. Adding a provider means writing one
//! new client, with no change to the core. See architecture.md,
//! "The plug-in seams".

mod replay;

use std::hash::Hash;
use std::hash::Hasher;

// Re-exported for callers and tests; the allow keeps the gate quiet until
// T1.5 turns the crate into a library.
#[allow(unused_imports)]
pub use replay::{RecordingClient, ReplayClient};

use serde::{Deserialize, Serialize};

/// A request to a model: which model to call, the system prompt, the user
/// content, the JSON schema the reply must conform to, and a budget.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelRequest {
    /// The model id (for example `muse-spark-1.3` or
    /// `meta-models/muse-spark-open-1026`).
    pub model: String,
    /// The system prompt.
    pub system: String,
    /// The user content.
    pub user: String,
    /// The JSON schema the reply must conform to.
    pub schema: serde_json::Value,
    /// The budget for this call.
    pub budget: Budget,
}

/// A budget for one model call.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Budget {
    /// Maximum output tokens, when the caller wants to cap them.
    pub max_output_tokens: Option<u32>,
    /// Maximum cost in US dollars, when the caller wants to cap it.
    pub max_cost_usd: Option<f64>,
}

impl Budget {
    /// No cap on either dimension.
    pub const UNLIMITED: Self = Self {
        max_output_tokens: None,
        max_cost_usd: None,
    };

    /// A budget capped only on output tokens.
    pub fn tokens(n: u32) -> Self {
        Self {
            max_output_tokens: Some(n),
            max_cost_usd: None,
        }
    }
}

/// Usage reported by a model call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    /// Input tokens consumed, when the provider reports them.
    pub input_tokens: Option<u64>,
    /// Output tokens produced, when the provider reports them.
    pub output_tokens: Option<u64>,
    /// Cost in US dollars, when it is known.
    pub cost_usd: Option<f64>,
}

impl Usage {
    /// No usage information.
    pub const NONE: Self = Self {
        input_tokens: None,
        output_tokens: None,
        cost_usd: None,
    };
}

/// The parsed reply from a model call, plus usage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelResponse {
    /// The reply, parsed as JSON.
    pub value: serde_json::Value,
    /// Token and cost usage.
    pub usage: Usage,
}

/// What can go wrong in a model call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelError {
    /// The replay client found no recording for this request; the message
    /// (and the variant) carry the request's key.
    ReplayMiss {
        /// The request's key (the hash of the request).
        key: String,
    },
    /// The underlying client failed.
    Backend {
        /// What happened, in the client's own terms.
        message: String,
    },
    /// The request was invalid.
    InvalidRequest {
        /// What was wrong.
        message: String,
    },
}

impl ModelError {
    /// Build a [`ModelError::Backend`] from a message.
    pub fn backend(message: impl Into<String>) -> Self {
        Self::Backend {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReplayMiss { key } => write!(f, "model: replay miss: no recording for key {key}"),
            Self::Backend { message } => write!(f, "model: backend error: {message}"),
            Self::InvalidRequest { message } => write!(f, "model: invalid request: {message}"),
        }
    }
}

impl std::error::Error for ModelError {}

/// The contract every model provider implements. One call: a prompt and a
/// JSON schema in, structured JSON out.
pub trait ModelClient {
    /// Run one call and return the parsed reply plus usage.
    fn complete(&self, request: &ModelRequest) -> Result<ModelResponse, ModelError>;

    /// Convenience alias for [`ModelClient::complete`]. Some callers prefer
    /// the shorter name; both delegate to the same implementation.
    fn call(&self, request: &ModelRequest) -> Result<ModelResponse, ModelError> {
        self.complete(request)
    }
}

/// Compute the stable key for a request: a hash of its contents, rendered as
/// hex. The replay and recording clients key their files by this value, so a
/// hand-written recording must be named `<key>.json`.
///
/// Hashing uses [`std::hash::DefaultHasher`] over the model id, system
/// prompt, user content, the canonical JSON rendering of the schema, and the
/// budget. Documented so recordings can be named by hand: compute the key
/// with this function in a test, or run once in recording mode and read the
/// filename it writes.
pub fn request_key(request: &ModelRequest) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    request.model.hash(&mut hasher);
    request.system.hash(&mut hasher);
    request.user.hash(&mut hasher);
    // Canonical rendering keeps the hash stable across equivalent JSON.
    let schema_text = serde_json::to_string(&request.schema).unwrap_or_default();
    schema_text.hash(&mut hasher);
    request.budget.max_output_tokens.hash(&mut hasher);
    // f64 is not Hash; render it canonically.
    if let Some(cost) = request.budget.max_cost_usd {
        cost.to_bits().hash(&mut hasher);
    } else {
        0u64.hash(&mut hasher);
    }
    format!("{:x}", hasher.finish())
}
