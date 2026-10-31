//! A ureq-backed [`HttpTransport`] for the Baseten and Meta model clients.
//!
//! Both clients ([`BasetenModelClient`](crate::models::baseten::BasetenModelClient)
//! and [`MetaModelClient`](crate::models::meta::MetaModelClient)) speak the
//! same OpenAI-shaped `POST {base}/chat/completions` wire shape and declare
//! their own transport seam with identical signatures. One
//! [`UreqTransport`] implements both seams, so `--live` needs no per-client
//! glue.
//!
//! Timeouts are finite everywhere, like [`crate::mcp`]'s transport: a global
//! timeout bounds the whole call (connect, TLS, send, wait, read). The
//! default is 60 s — model calls spend most of their time waiting on
//! reasoning, so the 5 s MCP default would starve them. Non-2xx statuses
//! are returned as data, never transport errors, so each client's own
//! retry/error logic (429/5xx retry, 4xx fail-fast) sees the real wire
//! shape. Nothing sensitive is logged: errors carry the URL and the status
//! only, never headers or the key.

use std::time::Duration;

/// Default global timeout for one model HTTP call.
pub const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(60);

/// A ureq-backed transport for the model clients (rustls, blocking).
///
/// Implements both [`crate::models::baseten::HttpTransport`] and
/// [`crate::models::meta::HttpTransport`].
pub struct UreqTransport {
    agent: ureq::Agent,
}

impl UreqTransport {
    /// Build with [`DEFAULT_HTTP_TIMEOUT`].
    pub fn new() -> Self {
        Self::with_timeout(DEFAULT_HTTP_TIMEOUT)
    }

    /// Build with an explicit global timeout.
    pub fn with_timeout(timeout: Duration) -> Self {
        let config = ureq::config::Config::builder()
            .timeout_global(Some(timeout))
            .http_status_as_error(false)
            .build();
        let agent: ureq::Agent = config.into();
        Self { agent }
    }
}

impl Default for UreqTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::models::baseten::HttpTransport for UreqTransport {
    fn post(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> Result<crate::models::baseten::HttpResponse, crate::models::baseten::HttpTransportError>
    {
        post(&self.agent, url, headers, body)
            .map(|(status, body)| crate::models::baseten::HttpResponse { status, body })
            .map_err(|message| crate::models::baseten::HttpTransportError { message })
    }
}

impl crate::models::meta::HttpTransport for UreqTransport {
    fn post(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> Result<crate::models::meta::HttpResponse, crate::models::meta::HttpTransportError> {
        post(&self.agent, url, headers, body)
            .map(|(status, body)| crate::models::meta::HttpResponse { status, body })
            .map_err(|message| crate::models::meta::HttpTransportError { message })
    }
}

fn post(
    agent: &ureq::Agent,
    url: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Result<(u16, String), String> {
    let mut request = agent.post(url);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = request.send(body).map_err(|e| format!("ureq post: {e}"))?;
    let status = response.status().as_u16();
    let text = response
        .into_body()
        .read_to_string()
        .map_err(|e| format!("ureq read body: {e}"))?;
    Ok((status, text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_baseten_transport<T: crate::models::baseten::HttpTransport>() {}
    fn assert_meta_transport<T: crate::models::meta::HttpTransport>() {}

    #[test]
    fn ureq_transport_implements_both_client_seams() {
        assert_baseten_transport::<UreqTransport>();
        assert_meta_transport::<UreqTransport>();
    }

    #[test]
    fn default_constructs_with_finite_timeout() {
        assert_eq!(DEFAULT_HTTP_TIMEOUT, Duration::from_secs(60));
        let _ = UreqTransport::new();
        let _ = UreqTransport::default();
        let _ = UreqTransport::with_timeout(Duration::from_secs(2));
    }

    #[test]
    fn refused_connection_is_a_transport_error_not_a_hang() {
        use crate::models::baseten::HttpTransport as BasetenTransport;
        // Port 0 can never accept: the failure is immediate and local.
        let transport = UreqTransport::with_timeout(Duration::from_secs(2));
        let err = transport
            .post("http://127.0.0.1:0/chat/completions", &[], "{}")
            .expect_err("port 0 must fail");
        assert!(
            !err.message.is_empty(),
            "transport error must say something"
        );
    }
}
