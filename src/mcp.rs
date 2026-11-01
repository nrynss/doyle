//! A minimal MCP client over streamable HTTP.
//!
//! Used by the Lambo adapter. Transport is a small seam so tests can inject
//! recorded SSE without a server.

use std::cell::RefCell;
use std::fmt;
use std::time::Duration;

use serde_json::Value;

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

    /// Recorded requests, when the transport captures them (fake only).
    fn recorded(&self) -> Vec<RecordedRequest> {
        Vec::new()
    }
}

/// Default transport timeout. `STATS_WAIT_MS` in the Lambo adapter is kept
/// strictly below this with margin so a receipt wait cannot outlive the HTTP
/// call that carries it.
pub const DEFAULT_TRANSPORT_TIMEOUT: Duration = Duration::from_secs(5);

/// A transport backed by `ureq` (rustls, blocking). Finite timeouts everywhere.
pub struct UreqTransport {
    agent: ureq::Agent,
}

impl UreqTransport {
    pub fn new() -> Self {
        Self::with_timeout(DEFAULT_TRANSPORT_TIMEOUT)
    }

    pub fn with_timeout(timeout: Duration) -> Self {
        let config = ureq::config::Config::builder()
            .timeout_global(Some(timeout))
            // Every HTTP status is a response: McpClient reads 404 to renew
            // an expired session and reports other non-success statuses itself.
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

/// A recorded HTTP request captured by [`FakeTransport`].
#[derive(Debug, Clone)]
pub struct RecordedRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
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
    recorded: Vec<RecordedRequest>,
}

impl FakeTransport {
    pub fn new(responses: Vec<HttpResponse>) -> Self {
        Self {
            inner: RefCell::new(FakeInner {
                responses,
                cursor: 0,
                fallback: None,
                recorded: Vec::new(),
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
                recorded: Vec::new(),
            }),
        }
    }
}

impl FakeTransport {
    pub fn recorded(&self) -> Vec<RecordedRequest> {
        self.inner.borrow().recorded.clone()
    }
    pub fn recorded_requests(&self) -> Vec<RecordedRequest> {
        self.recorded()
    }
    pub fn calls(&self) -> Vec<RecordedRequest> {
        self.recorded()
    }
    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.recorded()
    }
    pub fn captured(&self) -> Vec<RecordedRequest> {
        self.recorded()
    }
    pub fn captured_requests(&self) -> Vec<RecordedRequest> {
        self.recorded()
    }
}

impl Transport for FakeTransport {
    fn recorded(&self) -> Vec<RecordedRequest> {
        self.inner.borrow().recorded.clone()
    }

    fn post(
        &self,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> Result<HttpResponse, TransportError> {
        let mut inner = self.inner.borrow_mut();
        inner.recorded.push(RecordedRequest {
            url: url.to_string(),
            headers: headers.to_vec(),
            body: body.to_string(),
        });
        if inner.cursor < inner.responses.len() {
            let r = inner.responses[inner.cursor].clone();
            inner.cursor += 1;
            return Ok(r);
        }
        if let Some(f) = &inner.fallback {
            // need to drop borrow before calling? f is Fn; borrow still held. Call while borrowed is okay but recording already pushed. However f may call? no.
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

    pub fn recorded_requests(&self) -> Vec<RecordedRequest> {
        self.transport.recorded()
    }
    pub fn recorded(&self) -> Vec<RecordedRequest> {
        self.recorded_requests()
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
        let msg = parse_sse(&self.url, &resp.body)?;
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
        if !(200..300).contains(&nresp.status) {
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
        let body = body.to_string();
        let carried_session = self.session_id.is_some();
        let mut resp = self.post(&body, &[])?;
        if resp.status == 404 && carried_session {
            // The server forgot our session. Start a new one and retry once.
            self.session_id = None;
            self.initialized = false;
            self.initialize()?;
            resp = self.post(&body, &[])?;
        }
        if resp.status != 200 {
            return Err(McpError {
                url: self.url.clone(),
                message: format!("tools/call {} returned HTTP {}", name, resp.status),
            });
        }
        let msg = parse_sse(&self.url, &resp.body)?;
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

fn parse_sse(url: &str, body: &str) -> Result<Value, McpError> {
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
            url: url.to_string(),
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
        url: url.to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    struct Shared(Rc<FakeTransport>);
    impl Transport for Shared {
        fn post(
            &self,
            url: &str,
            headers: &[(String, String)],
            body: &str,
        ) -> Result<HttpResponse, TransportError> {
            self.0.post(url, headers, body)
        }
        fn recorded(&self) -> Vec<RecordedRequest> {
            self.0.recorded()
        }
    }

    fn init_resp(sid: &str) -> HttpResponse {
        HttpResponse {
            status: 200,
            headers: vec![("Mcp-Session-Id".into(), sid.into())],
            body: "data: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n\n".into(),
        }
    }
    fn accepted() -> HttpResponse {
        HttpResponse {
            status: 202,
            headers: vec![],
            body: String::new(),
        }
    }
    fn status(code: u16) -> HttpResponse {
        HttpResponse {
            status: code,
            headers: vec![],
            body: "nope".into(),
        }
    }
    fn ok_call(text: &str) -> HttpResponse {
        HttpResponse {
            status: 200,
            headers: vec![],
            body: format!(
                "data: {{\"jsonrpc\":\"2.0\",\"id\":3,\"result\":{{\"content\":[{{\"type\":\"text\",\"text\":\"{text}\"}}]}}}}\n\n"
            ),
        }
    }
    fn client(responses: Vec<HttpResponse>) -> (McpClient, Rc<FakeTransport>) {
        let fake = Rc::new(FakeTransport::new(responses));
        let c = McpClient::new("http://fake/mcp", Box::new(Shared(fake.clone())));
        (c, fake)
    }
    fn sid(r: &RecordedRequest) -> Option<String> {
        r.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Mcp-Session-Id"))
            .map(|(_, v)| v.clone())
    }
    fn method(r: &RecordedRequest) -> String {
        let v: Value = serde_json::from_str(&r.body).unwrap();
        v["method"].as_str().unwrap().to_string()
    }

    #[test]
    fn call_tool_404_reinitializes_and_retries_with_new_session() {
        let (mut c, fake) = client(vec![
            init_resp("old"),
            accepted(),
            status(404),
            init_resp("new"),
            accepted(),
            ok_call("fresh"),
        ]);
        let r = c.call_tool("t", serde_json::json!({})).unwrap();
        assert_eq!(r.text, "fresh");
        let reqs = fake.recorded();
        let methods: Vec<String> = reqs.iter().map(method).collect();
        assert_eq!(
            methods,
            [
                "initialize",
                "notifications/initialized",
                "tools/call",
                "initialize",
                "notifications/initialized",
                "tools/call"
            ]
        );
        assert_eq!(sid(&reqs[2]).as_deref(), Some("old"));
        assert_eq!(sid(&reqs[3]), None, "re-initialize carries no session id");
        assert_eq!(sid(&reqs[4]).as_deref(), Some("new"));
        assert_eq!(sid(&reqs[5]).as_deref(), Some("new"));
        assert_eq!(reqs[2].body, reqs[5].body);
        assert_eq!(c.session_id(), Some("new"));
    }

    #[test]
    fn second_404_after_reinitialize_errors_without_further_retry() {
        let (mut c, fake) = client(vec![
            init_resp("old"),
            accepted(),
            status(404),
            init_resp("new"),
            accepted(),
            status(404),
        ]);
        let e = c.call_tool("t", serde_json::json!({})).unwrap_err();
        assert!(e.message.contains("HTTP 404"), "{e}");
        assert_eq!(fake.recorded().len(), 6);
    }

    #[test]
    fn call_tool_404_without_session_id_does_not_reinitialize() {
        let no_sid = HttpResponse {
            status: 200,
            headers: vec![],
            body: "data: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n\n".into(),
        };
        let (mut c, fake) = client(vec![no_sid, accepted(), status(404)]);
        let e = c.call_tool("t", serde_json::json!({})).unwrap_err();
        assert!(e.message.contains("HTTP 404"), "{e}");
        let reqs = fake.recorded();
        assert_eq!(reqs.len(), 3);
        assert_eq!(method(&reqs[2]), "tools/call");
    }

    #[test]
    fn call_tool_500_is_not_retried() {
        let (mut c, fake) = client(vec![init_resp("old"), accepted(), status(500)]);
        let e = c.call_tool("t", serde_json::json!({})).unwrap_err();
        assert!(e.message.contains("HTTP 500"), "{e}");
        assert_eq!(fake.recorded().len(), 3);
    }

    // ---- real UreqTransport over a local listener ----

    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// One scripted reply: (status line, extra headers, body).
    type Reply = (&'static str, Vec<(&'static str, String)>, String);

    /// Serves `replies` in order, one connection per request, and records each
    /// raw request. The thread ends after the last reply or on a read timeout.
    fn serve(replies: Vec<Reply>) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/mcp", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            for (status, headers, body) in replies {
                let Ok((mut s, _)) = listener.accept() else {
                    return;
                };
                s.set_read_timeout(Some(Duration::from_secs(5))).ok();
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    let Ok(n) = s.read(&mut chunk) else { return };
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf).to_string();
                    if let Some(i) = text.find("\r\n\r\n") {
                        let len = text[..i]
                            .lines()
                            .find_map(|l| {
                                let l = l.to_ascii_lowercase();
                                l.strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                            })
                            .unwrap_or(0);
                        if buf.len() >= i + 4 + len {
                            break;
                        }
                    }
                }
                log.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&buf).to_string());
                let mut out = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n",
                    body.len()
                );
                for (k, v) in headers {
                    out.push_str(&format!("{k}: {v}\r\n"));
                }
                out.push_str("\r\n");
                out.push_str(&body);
                s.write_all(out.as_bytes()).ok();
            }
        });
        (url, seen)
    }

    fn init_reply(sid: &str) -> Reply {
        (
            "200 OK",
            vec![("Mcp-Session-Id", sid.to_string())],
            "data: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n\n".to_string(),
        )
    }
    fn accepted_reply() -> Reply {
        ("202 Accepted", vec![], String::new())
    }

    #[test]
    fn ureq_transport_returns_404_as_a_response() {
        let (url, _) = serve(vec![("404 Not Found", vec![], "gone".into())]);
        let t = UreqTransport::with_timeout(Duration::from_secs(5));
        let r = t
            .post(&url, &[], "{}")
            .expect("404 is a response, not an error");
        assert_eq!(r.status, 404);
        assert_eq!(r.body, "gone");
    }

    #[test]
    fn real_transport_call_tool_404_reinitializes_and_retries_with_new_session() {
        let result = "data: {\"jsonrpc\":\"2.0\",\"id\":3,\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"fresh\"}]}}\n\n";
        let (url, seen) = serve(vec![
            init_reply("old"),
            accepted_reply(),
            ("404 Not Found", vec![], "expired".into()),
            init_reply("new"),
            accepted_reply(),
            ("200 OK", vec![], result.to_string()),
        ]);
        let mut c = McpClient::new(
            url,
            Box::new(UreqTransport::with_timeout(Duration::from_secs(5))),
        );
        let r = c.call_tool("t", serde_json::json!({})).unwrap();
        assert_eq!(r.text, "fresh");
        let reqs = seen.lock().unwrap().clone();
        assert_eq!(reqs.len(), 6);
        let has = |req: &str, sid: &str| {
            req.to_ascii_lowercase()
                .contains(&format!("mcp-session-id: {sid}"))
        };
        assert!(has(&reqs[2], "old"));
        assert!(!reqs[3].to_ascii_lowercase().contains("mcp-session-id"));
        assert!(has(&reqs[4], "new"));
        assert!(reqs[5].contains("tools/call") && has(&reqs[5], "new"));
        assert_eq!(c.session_id(), Some("new"));
    }

    #[test]
    fn real_transport_call_tool_500_errors_naming_500_without_retry() {
        let (url, seen) = serve(vec![
            init_reply("s"),
            accepted_reply(),
            ("500 Internal Server Error", vec![], "boom".into()),
        ]);
        let mut c = McpClient::new(
            url,
            Box::new(UreqTransport::with_timeout(Duration::from_secs(5))),
        );
        let e = c.call_tool("t", serde_json::json!({})).unwrap_err();
        assert!(e.message.contains("HTTP 500"), "{e}");
        assert_eq!(seen.lock().unwrap().len(), 3);
    }

    #[test]
    fn real_transport_initialize_and_initialized_non_success_error() {
        let (url, _) = serve(vec![("503 Service Unavailable", vec![], String::new())]);
        let mut c = McpClient::new(
            url,
            Box::new(UreqTransport::with_timeout(Duration::from_secs(5))),
        );
        let e = c.initialize().unwrap_err();
        assert!(e.message.contains("initialize returned HTTP 503"), "{e}");

        let (url, _) = serve(vec![
            init_reply("s"),
            ("500 Internal Server Error", vec![], String::new()),
        ]);
        let mut c = McpClient::new(
            url,
            Box::new(UreqTransport::with_timeout(Duration::from_secs(5))),
        );
        let e = c.initialize().unwrap_err();
        assert!(
            e.message
                .contains("notifications/initialized returned HTTP 500"),
            "{e}"
        );
    }
}
