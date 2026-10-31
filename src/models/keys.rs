//! Model key loading: environment first, then the system keyring.
//!
//! The Baseten and Meta clients read their key from the environment at call
//! time. `--live` resolves the key through this module first: the
//! environment variable wins when set, otherwise `secret-tool lookup service
//! <service>` is consulted. A missing key is an error naming the variable,
//! never the value: [`KeyError`] carries only the variable and service
//! names, and its `Display` repeats only those.

/// Environment variable holding the Baseten API key.
pub const BASETEN_ENV_VAR: &str = "BASETEN_API_KEY";
/// Keyring service name for the Baseten API key.
pub const BASETEN_SERVICE: &str = "baseten";
/// Environment variable holding the Meta Model API key.
pub const META_ENV_VAR: &str = "MODEL_API_KEY";
/// Keyring service name for the Meta Model API key.
pub const META_SERVICE: &str = "meta-model-api";

/// What can go wrong loading a model key. Carries names only, never the
/// key value, so formatting it (logs, errors, reports) cannot leak the key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyError {
    /// No key in the environment and nothing under the service in the keyring.
    NotFound {
        /// The environment variable that was checked (e.g. `BASETEN_API_KEY`).
        env_var: String,
        /// The keyring service that was checked (e.g. `baseten`).
        service: String,
    },
    /// The keyring lookup itself failed (no `secret-tool`, no D-Bus, ...).
    LookupFailed {
        /// The environment variable the caller can set instead.
        env_var: String,
        /// The keyring service that was attempted.
        service: String,
        /// Why the lookup failed (exit status only, never key material).
        message: String,
    },
}

impl std::fmt::Display for KeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound { env_var, service } => write!(
                f,
                "no model key: set {env_var} in the environment or store it in the keyring under service \"{service}\""
            ),
            Self::LookupFailed {
                env_var,
                service,
                message,
            } => write!(
                f,
                "keyring lookup for service \"{service}\" failed ({message}); set {env_var} in the environment instead"
            ),
        }
    }
}

impl std::error::Error for KeyError {}

/// Load the Baseten API key: [`BASETEN_ENV_VAR`], then the keyring.
pub fn baseten_key() -> Result<String, KeyError> {
    load_key(BASETEN_ENV_VAR, BASETEN_SERVICE)
}

/// Load the Meta Model API key: [`META_ENV_VAR`], then the keyring.
pub fn meta_key() -> Result<String, KeyError> {
    load_key(META_ENV_VAR, META_SERVICE)
}

/// Load a key: the environment variable first, then `secret-tool lookup
/// service <service>`. The value is returned, never logged.
pub fn load_key(env_var: &str, service: &str) -> Result<String, KeyError> {
    load_key_with(env_var, service, &|svc| keyring_lookup(svc))
}

/// Load a key with an injectable keyring lookup (for tests): the
/// environment variable first, then `lookup(service)`. `lookup` returns
/// `Ok(Some(key))` when the keyring holds the key, `Ok(None)` when it does
/// not, and `Err(message)` when the lookup itself failed.
pub fn load_key_with(
    env_var: &str,
    service: &str,
    lookup: &dyn Fn(&str) -> Result<Option<String>, String>,
) -> Result<String, KeyError> {
    if let Ok(value) = std::env::var(env_var)
        && !value.is_empty()
    {
        return Ok(value);
    }
    match lookup(service) {
        Ok(Some(value)) if !value.is_empty() => Ok(value),
        Ok(_) => Err(KeyError::NotFound {
            env_var: env_var.to_string(),
            service: service.to_string(),
        }),
        Err(message) => Err(KeyError::LookupFailed {
            env_var: env_var.to_string(),
            service: service.to_string(),
            message,
        }),
    }
}

/// Run `secret-tool lookup service <service>` and return its stdout (the
/// key) when present. Stderr/exit details stay out of errors except for a
/// bounded first line, which cannot carry the key (the key goes to stdout).
fn keyring_lookup(service: &str) -> Result<Option<String>, String> {
    let output = std::process::Command::new("secret-tool")
        .args(["lookup", "service", service])
        .output()
        .map_err(|e| format!("secret-tool not runnable: {e}"))?;
    if !output.status.success() {
        let detail = first_line_bounded(&output.stderr);
        return Err(format!("exit {}: {detail}", output.status));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let key = stdout.trim_end_matches(['\n', '\r']).to_string();
    if key.is_empty() {
        return Ok(None);
    }
    Ok(Some(key))
}

fn first_line_bounded(bytes: &[u8]) -> String {
    const MAX_CHARS: usize = 200;
    let text = String::from_utf8_lossy(bytes);
    let first = text.lines().next().unwrap_or("").trim();
    if first.chars().count() <= MAX_CHARS {
        first.to_string()
    } else {
        first.chars().take(MAX_CHARS).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn unique_env_var(label: &str) -> String {
        static CTR: AtomicUsize = AtomicUsize::new(0);
        let n = CTR.fetch_add(1, Ordering::SeqCst);
        format!("DOYLE_T54_KEY_{label}_{}_{n}", std::process::id())
    }

    fn panic_lookup(_service: &str) -> Result<Option<String>, String> {
        panic!("keyring must not be consulted when the environment holds the key");
    }

    #[test]
    fn env_wins_and_keyring_is_not_consulted() {
        let var = unique_env_var("env");
        // SAFETY: unique name, no other test touches it.
        unsafe {
            std::env::set_var(&var, "env-key-sentinel");
        }
        let got = load_key_with(&var, "some-service", &panic_lookup).expect("env must win");
        assert_eq!(got, "env-key-sentinel");
        // SAFETY: restoring our own unique variable.
        unsafe {
            std::env::remove_var(&var);
        }
    }

    #[test]
    fn keyring_is_second_when_env_is_absent() {
        let var = unique_env_var("ring");
        // SAFETY: unique name, never set.
        unsafe {
            std::env::remove_var(&var);
        }
        let got = load_key_with(&var, "some-service", &|svc| {
            assert_eq!(svc, "some-service");
            Ok(Some("keyring-key-sentinel".to_string()))
        })
        .expect("keyring must serve");
        assert_eq!(got, "keyring-key-sentinel");
    }

    #[test]
    fn empty_env_falls_through_to_keyring() {
        let var = unique_env_var("empty");
        // SAFETY: unique name, no other test touches it.
        unsafe {
            std::env::set_var(&var, "");
        }
        let got = load_key_with(&var, "some-service", &|_| {
            Ok(Some("keyring-key-sentinel".to_string()))
        })
        .expect("empty env must fall through");
        assert_eq!(got, "keyring-key-sentinel");
        // SAFETY: restoring our own unique variable.
        unsafe {
            std::env::remove_var(&var);
        }
    }

    #[test]
    fn missing_key_is_a_clear_error_naming_the_variable() {
        let var = unique_env_var("missing");
        // SAFETY: unique name, never set.
        unsafe {
            std::env::remove_var(&var);
        }
        let err = load_key_with(&var, "some-service", &|_| Ok(None)).expect_err("must miss");
        let msg = err.to_string();
        assert!(msg.contains(&var), "error must name the variable: {msg}");
        assert!(
            msg.contains("some-service"),
            "error must name the service: {msg}"
        );
        assert!(
            !msg.contains("keyring-key-sentinel"),
            "error must never carry a value: {msg}"
        );
        assert!(matches!(err, KeyError::NotFound { .. }));
    }

    #[test]
    fn lookup_failure_names_the_variable_not_the_value() {
        let var = unique_env_var("fail");
        // SAFETY: unique name, never set.
        unsafe {
            std::env::remove_var(&var);
        }
        let err = load_key_with(&var, "some-service", &|_| {
            Err("exit 1: no such thing".to_string())
        })
        .expect_err("must fail");
        let msg = err.to_string();
        assert!(msg.contains(&var), "error must name the variable: {msg}");
        assert!(matches!(err, KeyError::LookupFailed { .. }));
    }

    #[test]
    fn error_display_never_carries_a_candidate_value() {
        let err = KeyError::NotFound {
            env_var: "BASETEN_API_KEY".to_string(),
            service: "baseten".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("BASETEN_API_KEY"), "{msg}");
        // The shape pins the contract: names only, no value slot exists.
        assert!(!msg.contains("sk-"), "{msg}");
    }

    #[test]
    fn first_line_bounded_truncates() {
        let long = "x".repeat(500);
        assert!(first_line_bounded(long.as_bytes()).chars().count() <= 200);
        assert_eq!(first_line_bounded(b"a\nb"), "a");
    }
}
