//! Doyle's model clients: the plug-in seam between the core and the Meta
//! models (Muse Spark, Muse Spark Open on Baseten) and the replay client
//! used for tests and an offline demo.
//!
//! The core calls only [`ModelClient`]. Adding a provider means writing one
//! new client, with no change to the core. See architecture.md,
//! "The plug-in seams".

mod replay;

pub mod baseten;
pub mod meta;

use std::hash::Hash;
use std::hash::Hasher;

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
    /// Media attachments (console stills, training recordings). Empty for
    /// every text-only caller; the screen source fills it. Part of the
    /// replay key when non-empty, ignored by the key when empty so every
    /// existing recording keeps its key.
    pub media: Vec<MediaPart>,
    /// The budget for this call.
    pub budget: Budget,
}

impl ModelRequest {
    /// The user message's `content` in the chat-completions wire shape: a
    /// bare string when the request carries no media (the existing shape,
    /// unchanged), otherwise an array of one text part plus one
    /// `image_url`/`video_url` data-URL part per attachment, in order
    /// (`docs/wonder-vision.md` §6). The Meta client uses this for the live
    /// path; the replay client keys on the media instead of sending it.
    pub fn content_blocks(&self) -> serde_json::Value {
        if self.media.is_empty() {
            return serde_json::Value::String(self.user.clone());
        }
        let mut parts = Vec::with_capacity(self.media.len() + 1);
        parts.push(serde_json::json!({"type": "text", "text": self.user}));
        for part in &self.media {
            parts.push(serde_json::json!({
                "type": part.kind.part_type(),
                part.kind.part_type(): {"url": part.data_url()},
            }));
        }
        serde_json::Value::Array(parts)
    }
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

/// What a media attachment is: a still image or a recording. Only the two
/// shapes T9.2 measured as working on the Meta Model API are representable,
/// so no request can carry a shape the API rejects with a 400
/// (`docs/wonder-vision.md` §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    /// A PNG still, sent as an `image_url` data URL.
    ImagePng,
    /// An MP4 recording, sent as a `video_url` data URL.
    VideoMp4,
}

impl MediaKind {
    /// The MIME type carried in the data URL.
    pub fn mime(&self) -> &'static str {
        match self {
            Self::ImagePng => "image/png",
            Self::VideoMp4 => "video/mp4",
        }
    }

    /// The chat-completions content-part type for this kind.
    pub fn part_type(&self) -> &'static str {
        match self {
            Self::ImagePng => "image_url",
            Self::VideoMp4 => "video_url",
        }
    }
}

/// One media attachment on a [`ModelRequest`]: raw bytes plus their kind.
/// Sent as a base64 data URL; keyed by the bytes' hash (see
/// [`request_key`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaPart {
    /// What the bytes are.
    pub kind: MediaKind,
    /// The raw media bytes (PNG or MP4), never a path or URL.
    pub bytes: Vec<u8>,
}

impl MediaPart {
    /// A PNG still from raw bytes.
    pub fn image_png(bytes: Vec<u8>) -> Self {
        Self {
            kind: MediaKind::ImagePng,
            bytes,
        }
    }

    /// An MP4 recording from raw bytes.
    pub fn video_mp4(bytes: Vec<u8>) -> Self {
        Self {
            kind: MediaKind::VideoMp4,
            bytes,
        }
    }

    /// The `data:` URL for the wire shape T9.2 found works:
    /// `data:<mime>;base64,<payload>`.
    pub fn data_url(&self) -> String {
        format!(
            "data:{};base64,{}",
            self.kind.mime(),
            base64_encode(&self.bytes)
        )
    }
}

/// Base64 of `bytes`, standard alphabet with `=` padding, implemented by
/// hand so media needs no new dependency.
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let mut word = [0u8; 3];
        word[..chunk.len()].copy_from_slice(chunk);
        let triple = (u32::from(word[0]) << 16) | (u32::from(word[1]) << 8) | u32::from(word[2]);
        let indices = [
            (triple >> 18) & 0x3f,
            (triple >> 12) & 0x3f,
            (triple >> 6) & 0x3f,
            triple & 0x3f,
        ];
        let chars = indices.map(|i| ALPHABET[i as usize] as char);
        match chunk.len() {
            1 => {
                out.push(chars[0]);
                out.push(chars[1]);
                out.push_str("==");
            }
            2 => {
                out.push(chars[0]);
                out.push(chars[1]);
                out.push(chars[2]);
                out.push('=');
            }
            _ => {
                for c in chars {
                    out.push(c);
                }
            }
        }
    }
    out
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
/// Hashing uses a version-stable FNV-1a hash (implemented by hand over
/// [`std::hash::Hasher`], no dependency) over the model id, system prompt,
/// user content, the canonical (recursively key-sorted) JSON rendering of the
/// schema, and the budget. Media attachments extend the key only when
/// present: a request with no media hashes exactly the fields above, so
/// every recording written before media existed keeps its key; a request
/// with media hashes each attachment's kind, byte length and bytes in order
/// after them, so the replay client serves media requests offline keyed on
/// the media's hash. The budget's `None`/`Some` discriminants are
/// hashed before any payload, so a missing cap and a zero cap never collide.
/// Documented so recordings can be named by hand: compute the key with this
/// function in a test, or run once in recording mode and read the filename it
/// writes.
///
/// Keys are stable across toolchains and Rust versions: the algorithm and
/// field sequence are fixed in this crate. They are *not* stable across
/// changes to that sequence, which is a deliberate, versioned contract.
pub fn request_key(request: &ModelRequest) -> String {
    let mut hasher = Fnv1aHasher::new();
    request.model.hash(&mut hasher);
    request.system.hash(&mut hasher);
    request.user.hash(&mut hasher);
    // Canonical rendering: sort object keys recursively so key order does
    // not change the key. Arrays keep their order (they are ordered data).
    let schema_text = canonical_schema_text(&request.schema);
    schema_text.hash(&mut hasher);
    request.budget.max_output_tokens.hash(&mut hasher);
    // f64 is not Hash; hash the discriminant first so None and Some(0.0)
    // cannot collide, then the bits.
    match request.budget.max_cost_usd {
        Some(cost) => {
            true.hash(&mut hasher);
            cost.to_bits().hash(&mut hasher);
        }
        None => {
            false.hash(&mut hasher);
        }
    }
    // Media extends the key only when present: every media-less request
    // keeps the key it always had. Lengths go through `write_u64`
    // explicitly (never `Hash for usize`, whose encoding is not pinned),
    // then the kind's MIME and the raw bytes.
    if !request.media.is_empty() {
        for part in &request.media {
            part.kind.mime().hash(&mut hasher);
            hasher.write_u64(part.bytes.len() as u64);
            hasher.write(&part.bytes);
        }
    }
    format!("{:x}", hasher.finish())
}

/// FNV-1a hasher, 64-bit, implemented by hand so the key does not depend on
/// `std::hash::DefaultHasher`'s toolchain-specific seeding.
struct Fnv1aHasher {
    state: u64,
}

impl Fnv1aHasher {
    fn new() -> Self {
        Self {
            state: 0xcbf2_9ce4_8422_2325,
        }
    }
}

impl Hasher for Fnv1aHasher {
    fn finish(&self) -> u64 {
        self.state
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.state ^= byte as u64;
            self.state = self.state.wrapping_mul(0x0100_0000_01b3);
        }
    }
}

/// Render `value` as compact JSON with object keys sorted recursively.
fn canonical_schema_text(value: &serde_json::Value) -> String {
    serde_json::to_string(&canonicalize_value(value)).unwrap_or_default()
}

fn canonicalize_value(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for key in keys {
                if let Some(v) = map.get(key) {
                    out.insert(key.clone(), canonicalize_value(v));
                }
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(canonicalize_value).collect())
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_request() -> ModelRequest {
        ModelRequest {
            model: "muse-wonder-1.0".to_string(),
            system: "you read screens".to_string(),
            user: "list the elements".to_string(),
            schema: serde_json::json!({"type": "object"}),
            media: Vec::new(),
            budget: Budget::tokens(8000),
        }
    }

    #[test]
    fn media_kinds_carry_the_measured_wire_names() {
        assert_eq!(MediaKind::ImagePng.mime(), "image/png");
        assert_eq!(MediaKind::ImagePng.part_type(), "image_url");
        assert_eq!(MediaKind::VideoMp4.mime(), "video/mp4");
        assert_eq!(MediaKind::VideoMp4.part_type(), "video_url");
    }

    #[test]
    fn base64_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        // All 256 byte values round-trip through the alphabet without panic.
        let all: Vec<u8> = (0..=255u8).collect();
        let encoded = base64_encode(&all);
        assert_eq!(encoded.len(), 344);
        assert!(encoded.ends_with("=="));
    }

    #[test]
    fn data_url_has_the_measured_shape() {
        let part = MediaPart::image_png(vec![0x89, b'P', b'N', b'G']);
        assert!(part.data_url().starts_with("data:image/png;base64,"));
        let video = MediaPart::video_mp4(vec![0, 1, 2]);
        assert!(video.data_url().starts_with("data:video/mp4;base64,"));
    }

    #[test]
    fn content_blocks_stays_a_string_without_media() {
        let request = text_request();
        assert_eq!(
            request.content_blocks(),
            serde_json::Value::String("list the elements".to_string())
        );
    }

    #[test]
    fn content_blocks_arrays_text_then_media_parts() {
        let mut request = text_request();
        request.media = vec![
            MediaPart::image_png(vec![1, 2, 3]),
            MediaPart::video_mp4(vec![4, 5]),
        ];
        let blocks = request.content_blocks();
        let parts = blocks.as_array().expect("array with media");
        assert_eq!(parts.len(), 3);
        assert_eq!(
            parts[0],
            serde_json::json!({"type": "text", "text": "list the elements"})
        );
        assert_eq!(parts[1]["type"], "image_url");
        assert!(
            parts[1]["image_url"]["url"]
                .as_str()
                .expect("url")
                .starts_with("data:image/png;base64,")
        );
        assert_eq!(parts[2]["type"], "video_url");
        assert!(
            parts[2]["video_url"]["url"]
                .as_str()
                .expect("url")
                .starts_with("data:video/mp4;base64,")
        );
    }

    #[test]
    fn empty_media_keeps_the_old_key() {
        // The pre-media contract: media-less requests hash exactly the old
        // fields, so every existing recording still replays.
        let request = text_request();
        let key = request_key(&request);
        assert!(!key.is_empty());
        let mut cloned = request.clone();
        cloned.media = Vec::new();
        assert_eq!(request_key(&cloned), key);
    }

    #[test]
    fn media_changes_the_key_by_bytes_kind_and_order() {
        let base = text_request();
        let base_key = request_key(&base);
        let with_image = ModelRequest {
            media: vec![MediaPart::image_png(vec![1, 2, 3])],
            ..base.clone()
        };
        let same_image = ModelRequest {
            media: vec![MediaPart::image_png(vec![1, 2, 3])],
            ..base.clone()
        };
        let other_bytes = ModelRequest {
            media: vec![MediaPart::image_png(vec![1, 2, 4])],
            ..base.clone()
        };
        let other_kind = ModelRequest {
            media: vec![MediaPart::video_mp4(vec![1, 2, 3])],
            ..base.clone()
        };
        let two_parts = ModelRequest {
            media: vec![
                MediaPart::image_png(vec![1, 2, 3]),
                MediaPart::video_mp4(vec![4]),
            ],
            ..base.clone()
        };
        let two_parts_swapped = ModelRequest {
            media: vec![
                MediaPart::video_mp4(vec![4]),
                MediaPart::image_png(vec![1, 2, 3]),
            ],
            ..base.clone()
        };
        assert_ne!(request_key(&with_image), base_key);
        assert_eq!(request_key(&with_image), request_key(&same_image));
        assert_ne!(request_key(&with_image), request_key(&other_bytes));
        assert_ne!(request_key(&with_image), request_key(&other_kind));
        assert_ne!(request_key(&with_image), request_key(&two_parts));
        assert_ne!(request_key(&two_parts), request_key(&two_parts_swapped));
    }
}
