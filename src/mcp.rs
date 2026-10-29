//! A minimal MCP client over streamable HTTP.
//!
//! Used by the Lambo adapter. Transport is a small seam so tests can inject
//! recorded SSE without a server.

use std::cell::RefCell;
use std::fmt;
use std::time::Duration;

use serde_json::Value;

/// An HTTP request handed to a [`Transport`].
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct HttpRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// An HTTP response coming back from a [`Transport`].
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// Transport errors. Always carry the URL and a human-readable message.
#[derive(Debug, Clone)]
pub struct TransportError {
    pub url: String,
    pub message: String,
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.url, self.message)
    }
}

impl std::error::Error for TransportError {}

/// The seam tests use to inject recorded SSE. Implementations send one HTTP
/// POST and return the raw response (status, headers, body text).
pub trait Transport {
    fn post(
        &self,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> Result<HttpResponse, TransportError>;
}

/// A transport backed by `ureq` (rustls, blocking). Finite timeouts everywhere.
pub struct UreqTransport {
    agent: ureq::Agent,
}

impl UreqTransport {
    pub fn new() -> Self {
        Self::with_timeout(Duration::from_secs(5))
    }

    pub fn with_timeout(timeout: Duration) -> Self {
        let config = ureq::config::Config::builder()
            .timeout_global(Some(timeout))
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

impl Transport for UreqTransport {
    fn post(
        &self,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> Result<HttpResponse, TransportError> {
        let mut rb = self.agent.post(url);
        for (k, v) in headers {
            rb = rb.header(k, v);
        }
        match rb.send(body) {
            Ok(mut resp) => {
                let status = resp.status().as_u16();
                let mut hdrs = Vec::new();
                for (k, v) in resp.headers().iter() {
                    hdrs.push((k.to_string(), v.to_str().unwrap_or("").to_string()));
                }
                let body = resp.body_mut().read_to_string().unwrap_or_default();
                Ok(HttpResponse {
                    status,
                    headers: hdrs,
                    body,
                })
            }
            Err(e) => Err(TransportError {
                url: url.to_string(),
                message: format!("{e}"),
            }),
        }
    }
}

/// An in-memory transport for tests: returns queued responses in order, then
/// errors. Also supports a closure for scripted behaviour.
pub struct FakeTransport {
    inner: RefCell<FakeInner>,
}

#[allow(clippy::type_complexity)]
struct FakeInner {
    responses: Vec<HttpResponse>,
    cursor: usize,
    fallback: Option<Box<dyn Fn(&str, &str) -> Result<HttpResponse, TransportError> + Send + Sync>>,
}

impl FakeTransport {
    pub fn new(responses: Vec<HttpResponse>) -> Self {
        Self {
            inner: RefCell::new(FakeInner {
                responses,
                cursor: 0,
                fallback: None,
            }),
        }
    }

    pub fn empty() -> Self {
        Self::new(Vec::new())
    }

    pub fn scripted<F>(f: F) -> Self
    where
        F: Fn(&str, &str) -> Result<HttpResponse, TransportError> + Send + Sync + 'static,
    {
        Self {
            inner: RefCell::new(FakeInner {
                responses: Vec::new(),
                cursor: 0,
                fallback: Some(Box::new(f)),
            }),
        }
    }
}

impl Transport for FakeTransport {
    fn post(
        &self,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> Result<HttpResponse, TransportError> {
        let _ = headers;
        let mut inner = self.inner.borrow_mut();
        if inner.cursor < inner.responses.len() {
            let r = inner.responses[inner.cursor].clone();
            inner.cursor += 1;
            return Ok(r);
        }
        if let Some(f) = &inner.fallback {
            return f(url, body);
        }
        Err(TransportError {
            url: url.to_string(),
            message: "fake transport exhausted".to_string(),
        })
    }
}

/// Errors raised by the MCP client.
#[derive(Debug, Clone)]
pub struct McpError {
    pub url: String,
    pub message: String,
}

impl fmt::Display for McpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.url, self.message)
    }
}

impl std::error::Error for McpError {}

/// Result of a `tools/call`.
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub text: String,
    pub structured: Option<Value>,
    pub is_error: bool,
}

/// Minimal MCP client over streamable HTTP.
pub struct McpClient {
    transport: Box<dyn Transport>,
    url: String,
    session_id: Option<String>,
    next_id: i64,
    initialized: bool,
}

impl McpClient {
    pub fn new(url: impl Into<String>, transport: Box<dyn Transport>) -> Self {
        Self {
            transport,
            url: url.into(),
            session_id: None,
            next_id: 1,
            initialized: false,
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    fn post(&self, body: &str, extra_headers: &[(&str, &str)]) -> Result<HttpResponse, McpError> {
        let mut headers: Vec<(String, String)> = vec![
            ("Content-Type".to_string(), "application/json".to_string()),
            (
                "Accept".to_string(),
                "application/json, text/event-stream".to_string(),
            ),
        ];
        for (k, v) in extra_headers {
            headers.push((k.to_string(), v.to_string()));
        }
        if let Some(sid) = &self.session_id {
            headers.push(("Mcp-Session-Id".to_string(), sid.clone()));
        }
        self.transport
            .post(&self.url, &headers, body)
            .map_err(|e| McpError {
                url: e.url,
                message: e.message,
            })
    }

    pub fn initialize(&mut self) -> Result<(), McpError> {
        if self.initialized {
            return Ok(());
        }
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": self.next_id,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "doyle", "version": "0.1.0" }
            }
        });
        self.next_id += 1;
        let resp = self.post(&body.to_string(), &[])?;
        if resp.status != 200 {
            return Err(McpError {
                url: self.url.clone(),
                message: format!("initialize returned HTTP {}", resp.status),
            });
        }
        let msg = parse_sse(&resp.body)?;
        if let Some(err) = msg.get("error") {
            return Err(McpError {
                url: self.url.clone(),
                message: format!("initialize error: {err}"),
            });
        }
        for (k, v) in &resp.headers {
            if k.eq_ignore_ascii_case("Mcp-Session-Id") {
                self.session_id = Some(v.clone());
            }
        }
        let notif = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        let nresp = self.post(&notif.to_string(), &[])?;
        if nresp.status >= 400 {
            return Err(McpError {
                url: self.url.clone(),
                message: format!("notifications/initialized returned HTTP {}", nresp.status),
            });
        }
        self.initialized = true;
        Ok(())
    }

    pub fn call_tool(&mut self, name: &str, arguments: Value) -> Result<ToolResult, McpError> {
        if !self.initialized {
            self.initialize()?;
        }
        let id = self.next_id;
        self.next_id += 1;
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments
            }
        });
        let resp = self.post(&body.to_string(), &[])?;
        if resp.status != 200 {
            return Err(McpError {
                url: self.url.clone(),
                message: format!("tools/call {} returned HTTP {}", name, resp.status),
            });
        }
        let msg = parse_sse(&resp.body)?;
        if let Some(err) = msg.get("error") {
            return Err(McpError {
                url: self.url.clone(),
                message: format!("tools/call {} error: {err}", name),
            });
        }
        let result = msg.get("result").cloned().unwrap_or(Value::Null);
        let is_error = result
            .get("isError")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let text = result
            .get("content")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|c| c.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let structured = result.get("structuredContent").cloned();
        Ok(ToolResult {
            text,
            structured,
            is_error,
        })
    }
}

fn parse_sse(body: &str) -> Result<Value, McpError> {
    let mut data = String::new();
    let mut saw_data = false;
    for line in body.lines() {
        let trimmed = line.trim_end_matches('\r');
        if let Some(rest) = trimmed.strip_prefix("data:") {
            saw_data = true;
            let rest = rest.trim_start();
            if !rest.is_empty() {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(rest);
            }
        }
    }
    if !saw_data {
        data = body.trim().to_string();
    }
    if data.is_empty() {
        return Err(McpError {
            url: String::new(),
            message: "empty response body".to_string(),
        });
    }
    if let Ok(v) = serde_json::from_str::<Value>(&data) {
        return Ok(v);
    }
    for line in data.lines() {
        if let Ok(v) = serde_json::from_str::<Value>(line.trim()) {
            return Ok(v);
        }
    }
    Err(McpError {
        url: String::new(),
        message: format!("failed to parse SSE body as JSON: {data}"),
    })
}

/// Convert a recorded Lambo response JSON (the file contents) into an
/// [`HttpResponse`] whose body is SSE, for use in fake transports.
pub fn recorded_to_http(recorded: &Value) -> HttpResponse {
    let status = recorded
        .get("http_status")
        .and_then(|v| v.as_u64())
        .unwrap_or(200) as u16;
    let mut headers = Vec::new();
    if let Some(hs) = recorded.get("response_headers").and_then(|v| v.as_object()) {
        for (k, v) in hs {
            headers.push((k.clone(), v.as_str().unwrap_or("").to_string()));
        }
    }
    let msg = recorded.get("message").cloned().unwrap_or(Value::Null);
    let body = format!("id: 0\nretry: 3000\ndata: {}\n\n", msg);
    HttpResponse {
        status,
        headers,
        body,
    }
}
