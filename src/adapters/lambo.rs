//! The Lambo adapter: Lambo over MCP (streamable HTTP).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use super::{AdapterError, Capabilities, MemoryAdapter};
use crate::core::{Memory, MemoryRef};
use crate::mcp::{McpClient, McpError, Transport};

pub const BROAD_WATCH_QUERY: &str = "*";

/// `wait_ms` passed to `lambo_stats` after a derive. Kept strictly below the
/// default transport timeout (see `crate::mcp::DEFAULT_TRANSPORT_TIMEOUT`)
/// with margin so the receipt wait cannot outlive the HTTP call carrying it.
pub const STATS_WAIT_MS: u64 = 4000;

pub struct LamboAdapter {
    name: String,
    agent_id: String,
    session: String,
    client: RefCell<McpClient>,
    content_cache: RefCell<HashMap<String, String>>,
    scope_cache: RefCell<HashMap<String, String>>,
    // Restart loss (R2): supersede/scope caches are process-local; after a
    // restart, reads lose warnings/scopes until the next supersede in that process.
    supersede_cache: RefCell<HashMap<String, (String, String)>>,
    // Restart loss (R2): supersede/scope caches are process-local. A restart
    // loses recorded warnings/scopes until the next supersede in that process.
    recall_warnings: RefCell<Vec<String>>,
    seen: RefCell<HashSet<String>>,
}

impl LamboAdapter {
    pub fn new(
        url: impl Into<String>,
        agent_id: impl Into<String>,
        transport: Box<dyn Transport>,
    ) -> Self {
        let url = url.into();
        let agent_id = agent_id.into();
        let session = session_from_url(&url);
        let client = McpClient::new(url, transport);
        Self {
            name: "lambo".to_string(),
            agent_id,
            session,
            client: RefCell::new(client),
            content_cache: RefCell::new(HashMap::new()),
            scope_cache: RefCell::new(HashMap::new()),
            supersede_cache: RefCell::new(HashMap::new()),
            recall_warnings: RefCell::new(Vec::new()),
            seen: RefCell::new(HashSet::new()),
        }
    }

    pub fn http(url: impl Into<String>, agent_id: impl Into<String>) -> Self {
        Self::new(url, agent_id, Box::new(crate::mcp::UreqTransport::new()))
    }

    pub fn with_timeout(
        url: impl Into<String>,
        agent_id: impl Into<String>,
        timeout: std::time::Duration,
    ) -> Self {
        Self::new(
            url,
            agent_id,
            Box::new(crate::mcp::UreqTransport::with_timeout(timeout)),
        )
    }

    fn backend_error(&self, memory: Option<String>, message: String) -> AdapterError {
        AdapterError::Backend {
            adapter: self.name.clone(),
            memory,
            message,
        }
    }

    fn mcp_err(&self, e: McpError) -> AdapterError {
        AdapterError::Backend {
            adapter: self.name.clone(),
            memory: None,
            message: format!("{}: {}", e.url, e.message),
        }
    }

    pub fn recall_warnings(&self) -> Vec<String> {
        self.recall_warnings.borrow().clone()
    }

    pub fn recorded_requests(&self) -> Vec<crate::mcp::RecordedRequest> {
        self.client.borrow().recorded_requests()
    }
    pub fn recorded(&self) -> Vec<crate::mcp::RecordedRequest> {
        self.recorded_requests()
    }
    pub fn calls(&self) -> Vec<crate::mcp::RecordedRequest> {
        self.recorded_requests()
    }
    pub fn requests(&self) -> Vec<crate::mcp::RecordedRequest> {
        self.recorded_requests()
    }
    pub fn consumed_warnings(&self) -> Vec<String> {
        self.recall_warnings()
    }
    pub fn last_recall_warnings(&self) -> Vec<String> {
        self.recall_warnings()
    }
    pub fn server_warnings(&self) -> Vec<String> {
        self.recall_warnings()
    }
}

fn session_from_url(url: &str) -> String {
    if let Some(idx) = url.find("/mcp/s/") {
        let rest = &url[idx + "/mcp/s/".len()..];
        let seg = rest
            .split('/')
            .next()
            .unwrap_or("")
            .split('?')
            .next()
            .unwrap_or("");
        if !seg.is_empty() {
            return seg.to_string();
        }
    }
    "lambo-dev".to_string()
}

fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

fn strip_type_suffix(label: &str) -> String {
    if let Some(idx) = label.rfind(" [")
        && label.ends_with(']')
    {
        return label[..idx].to_string();
    }
    label.to_string()
}

fn parse_contradicted_id(warning: &str) -> Option<String> {
    let marker = "(id ";
    let start = warning.find(marker)? + marker.len();
    let rest = &warning[start..];
    let end = rest.find(')')?;
    Some(rest[..end].trim().to_string())
}

impl MemoryAdapter for LamboAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            annotate: false,
            supersede: true,
            watch: true,
        }
    }

    fn list(&self, query: &str) -> Result<Vec<Memory>, AdapterError> {
        if query.trim().is_empty() {
            return Err(self.backend_error(
                None,
                "lambo_recall requires a non-empty query; empty query is unsupported".to_string(),
            ));
        }
        let mut client = self.client.borrow_mut();
        let result = client
            .call_tool(
                "lambo_recall",
                serde_json::json!({
                    "agent_id": self.agent_id,
                    "query": query,
                    "top_k": 50
                }),
            )
            .map_err(|e| self.mcp_err(e))?;
        if result.is_error {
            return Err(self.backend_error(None, result.text));
        }
        let structured = result.structured.clone().unwrap_or(serde_json::Value::Null);
        let hits = structured
            .get("hits")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let mut memories = Vec::new();
        for hit in hits {
            let node_id = hit
                .get("node_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if node_id.is_empty() {
                continue;
            }
            let content = hit
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            self.content_cache
                .borrow_mut()
                .insert(node_id.clone(), content.clone());
            let scope = self
                .scope_cache
                .borrow()
                .get(&node_id)
                .cloned()
                .unwrap_or_else(|| self.session.clone());
            memories.push(Memory {
                reference: MemoryRef {
                    adapter: self.name.clone(),
                    id: node_id,
                },
                text: content,
                updated_at: None,
                scope,
            });
        }
        // Consume any contradiction warnings the server attached to this recall
        // (transcript 13): they are the source of read-time warnings, not the
        // inspect neighbourhood (transcript 10 carries none).
        let server_warnings = structured
            .get("warnings")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for w in server_warnings {
            if let Some(t) = w.as_str() {
                self.recall_warnings.borrow_mut().push(t.to_string());
            }
        }
        Ok(memories)
    }

    fn read(&self, id: &str) -> Result<Memory, AdapterError> {
        let cached_text = self.content_cache.borrow().get(id).cloned();
        let mut text = match cached_text {
            Some(t) => t,
            None => {
                let mut client = self.client.borrow_mut();
                let result = client
                    .call_tool(
                        "lambo_inspect",
                        serde_json::json!({
                            "agent_id": self.agent_id,
                            "focus": id,
                            "depth": 2
                        }),
                    )
                    .map_err(|e| self.mcp_err(e))?;
                if result.is_error {
                    return Err(AdapterError::not_found(&self.name, id));
                }
                let structured = result.structured.clone().unwrap_or(serde_json::Value::Null);
                let label = structured
                    .get("focus")
                    .and_then(|f| f.get("label"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if label.is_empty() {
                    return Err(AdapterError::not_found(&self.name, id));
                }
                let parsed = strip_type_suffix(&label);
                self.content_cache
                    .borrow_mut()
                    .insert(id.to_string(), parsed.clone());
                parsed
            }
        };
        let scope = self
            .scope_cache
            .borrow()
            .get(id)
            .cloned()
            .unwrap_or_else(|| self.session.clone());
        let corr_from_cache = self
            .supersede_cache
            .borrow()
            .get(id)
            .cloned()
            .map(|(c, _)| c);
        let corr_from_recall = if corr_from_cache.is_none() {
            self.recall_warnings
                .borrow()
                .iter()
                .find(|w| {
                    w.contains("Contradicted") && (w.contains(id) || w.contains(&short_id(id)))
                })
                .and_then(|w| parse_contradicted_id(w))
        } else {
            None
        };
        if let Some(corr_id) = corr_from_cache.or(corr_from_recall)
            && !text.contains(&corr_id)
        {
            text.push_str("\n⚠ Contradicted (id ");
            text.push_str(&corr_id);
            text.push(')');
        }
        Ok(Memory {
            reference: MemoryRef {
                adapter: self.name.clone(),
                id: id.to_string(),
            },
            text,
            updated_at: None,
            scope,
        })
    }

    fn write(&self, text: &str, scope: &str) -> Result<MemoryRef, AdapterError> {
        let mut client = self.client.borrow_mut();
        let derive_result = client
            .call_tool(
                "lambo_derive",
                serde_json::json!({
                    "agent_id": self.agent_id,
                    "concepts": [{"content": text, "concept_type": "logic"}]
                }),
            )
            .map_err(|e| self.mcp_err(e))?;
        if derive_result.is_error {
            return Err(self.backend_error(None, derive_result.text));
        }
        let structured = derive_result
            .structured
            .clone()
            .unwrap_or(serde_json::Value::Null);
        let receipt = structured
            .get("receipt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if receipt.is_empty() {
            return Err(self.backend_error(None, "lambo_derive returned no receipt".to_string()));
        }
        let stats_result = client
            .call_tool(
                "lambo_stats",
                serde_json::json!({
                    "agent_id": self.agent_id,
                    "receipt": receipt,
                    "wait_ms": STATS_WAIT_MS
                }),
            )
            .map_err(|e| self.mcp_err(e))?;
        if stats_result.is_error {
            return Err(self.backend_error(None, stats_result.text));
        }
        let sstruct = stats_result
            .structured
            .clone()
            .unwrap_or(serde_json::Value::Null);
        let receipt_obj = sstruct
            .get("receipt")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let created = receipt_obj
            .get("created")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if created.is_empty() {
            return Err(
                self.backend_error(None, "lambo_stats returned no created node id".to_string())
            );
        }
        self.content_cache
            .borrow_mut()
            .insert(created.clone(), text.to_string());
        self.scope_cache
            .borrow_mut()
            .insert(created.clone(), scope.to_string());
        Ok(MemoryRef {
            adapter: self.name.clone(),
            id: created,
        })
    }

    fn annotate(&self, _id: &str, _warning: &str) -> Result<(), AdapterError> {
        Err(AdapterError::unsupported(&self.name, "annotate"))
    }

    fn supersede(&self, stale_id: &str, correction_text: &str) -> Result<MemoryRef, AdapterError> {
        if let Some((corr_id, text)) = self.supersede_cache.borrow().get(stale_id).cloned()
            && text == correction_text
        {
            return Ok(MemoryRef {
                adapter: self.name.clone(),
                id: corr_id,
            });
        }

        let mut existing_corr: Option<String> = None;
        {
            let mut client = self.client.borrow_mut();
            if let Ok(res) = client.call_tool(
                "lambo_inspect",
                serde_json::json!({
                    "agent_id": self.agent_id,
                    "focus": stale_id,
                    "depth": 2
                }),
            ) && !res.is_error
            {
                let structured = res.structured.clone().unwrap_or(serde_json::Value::Null);
                let mut warnings = Vec::new();
                if let Some(arr) = structured.get("warnings").and_then(|v| v.as_array()) {
                    for w in arr {
                        if let Some(t) = w.as_str() {
                            warnings.push(t.to_string());
                        }
                    }
                }
                for line in res.text.lines() {
                    if line.contains("Contradicted") {
                        warnings.push(line.to_string());
                    }
                }
                for w in &warnings {
                    if let Some(short) = parse_contradicted_id(w) {
                        existing_corr = Some(short);
                        break;
                    }
                }
            }
        }

        if let Some(short) = existing_corr {
            let full = {
                let mut client = self.client.borrow_mut();
                if let Ok(res) = client.call_tool(
                    "lambo_inspect",
                    serde_json::json!({
                        "agent_id": self.agent_id,
                        "focus": short,
                        "depth": 1
                    }),
                ) {
                    if !res.is_error {
                        let structured = res.structured.clone().unwrap_or(serde_json::Value::Null);
                        structured
                            .get("focus")
                            .and_then(|f| f.get("node_id"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            .unwrap_or(short.clone());

            let corr_text = self.inspect_content(&full);
            if corr_text.as_deref() == Some(correction_text) {
                self.content_cache
                    .borrow_mut()
                    .insert(full.clone(), correction_text.to_string());
                let scope = self
                    .scope_cache
                    .borrow()
                    .get(stale_id)
                    .cloned()
                    .unwrap_or_else(|| self.session.clone());
                self.scope_cache.borrow_mut().insert(full.clone(), scope);
                self.supersede_cache.borrow_mut().insert(
                    stale_id.to_string(),
                    (full.clone(), correction_text.to_string()),
                );
                return Ok(MemoryRef {
                    adapter: self.name.clone(),
                    id: full,
                });
            }
        }

        let short_stale = short_id(stale_id);
        let mut client = self.client.borrow_mut();
        let derive_result = client
            .call_tool("lambo_derive", serde_json::json!({
                "agent_id": self.agent_id,
                "concepts": [{"content": correction_text, "concept_type": "logic", "against": [short_stale]}]
            }))
            .map_err(|e| self.mcp_err(e))?;
        if derive_result.is_error {
            return Err(self.backend_error(Some(stale_id.to_string()), derive_result.text));
        }
        let structured = derive_result
            .structured
            .clone()
            .unwrap_or(serde_json::Value::Null);
        let receipt = structured
            .get("receipt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if receipt.is_empty() {
            return Err(self.backend_error(
                Some(stale_id.to_string()),
                "lambo_derive returned no receipt".to_string(),
            ));
        }
        let stats_result = client
            .call_tool(
                "lambo_stats",
                serde_json::json!({
                    "agent_id": self.agent_id,
                    "receipt": receipt,
                    "wait_ms": STATS_WAIT_MS
                }),
            )
            .map_err(|e| self.mcp_err(e))?;
        if stats_result.is_error {
            return Err(self.backend_error(Some(stale_id.to_string()), stats_result.text));
        }
        let sstruct = stats_result
            .structured
            .clone()
            .unwrap_or(serde_json::Value::Null);
        let receipt_obj = sstruct
            .get("receipt")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let created = receipt_obj
            .get("created")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if created.is_empty() {
            return Err(self.backend_error(
                Some(stale_id.to_string()),
                "lambo_stats returned no created node id".to_string(),
            ));
        }
        self.content_cache
            .borrow_mut()
            .insert(created.clone(), correction_text.to_string());
        let scope = self
            .scope_cache
            .borrow()
            .get(stale_id)
            .cloned()
            .unwrap_or_else(|| self.session.clone());
        self.scope_cache.borrow_mut().insert(created.clone(), scope);
        self.supersede_cache.borrow_mut().insert(
            stale_id.to_string(),
            (created.clone(), correction_text.to_string()),
        );
        Ok(MemoryRef {
            adapter: self.name.clone(),
            id: created,
        })
    }

    fn watch(&self) -> Result<Vec<Memory>, AdapterError> {
        let recalled = self.list(BROAD_WATCH_QUERY).unwrap_or_default();
        let mut collected: Vec<Memory> = Vec::new();
        let mut ids: HashSet<String> = HashSet::new();
        for m in recalled {
            if ids.insert(m.reference.id.clone()) {
                collected.push(m);
            }
        }
        for (id, text) in self.content_cache.borrow().iter() {
            if ids.insert(id.clone()) {
                let scope = self
                    .scope_cache
                    .borrow()
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| self.session.clone());
                collected.push(Memory {
                    reference: MemoryRef {
                        adapter: self.name.clone(),
                        id: id.clone(),
                    },
                    text: text.clone(),
                    updated_at: None,
                    scope,
                });
            }
        }
        let mut seen = self.seen.borrow_mut();
        let mut fresh = Vec::new();
        for m in collected {
            if seen.insert(m.reference.id.clone()) {
                fresh.push(m);
            }
        }
        Ok(fresh)
    }
}

impl LamboAdapter {
    fn inspect_content(&self, id: &str) -> Option<String> {
        let mut client = self.client.borrow_mut();
        let res = client
            .call_tool(
                "lambo_inspect",
                serde_json::json!({
                    "agent_id": self.agent_id,
                    "focus": id,
                    "depth": 2
                }),
            )
            .ok()?;
        if res.is_error {
            return None;
        }
        let structured = res.structured.clone().unwrap_or(serde_json::Value::Null);
        let label = structured
            .get("focus")
            .and_then(|f| f.get("label"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if label.is_empty() {
            None
        } else {
            Some(strip_type_suffix(label))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::FakeTransport;
    use std::fs;

    fn load_recorded(name: &str) -> serde_json::Value {
        // find file by prefix
        let entries = fs::read_dir("testdata/lambo").unwrap();
        for e in entries {
            let e = e.unwrap();
            let fname = e.file_name().to_string_lossy().to_string();
            if fname.starts_with(name) && fname.ends_with(".response.json") {
                let txt = fs::read_to_string(e.path()).unwrap();
                return serde_json::from_str(&txt).unwrap();
            }
        }
        panic!("recorded response not found for {}", name);
    }

    #[test]
    fn unreachable_server_fails_with_url() {
        let adapter = LamboAdapter::with_timeout(
            "http://127.0.0.1:1/mcp".to_string(),
            "doyle-probe".to_string(),
            std::time::Duration::from_millis(200),
        );
        let err = adapter.list("anything").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("127.0.0.1:1"), "msg: {}", msg);
    }

    #[test]
    fn recorded_full_scenario() {
        use crate::mcp::recorded_to_http;
        let q = vec![
            "01-initialize",
            "02-notifications-initialized",
            "09-recall-before-correction",
            "10-inspect-memory",
            "11-derive-correction-against",
            "12-stats-wait-correction",
            "13-recall-right-after-correction",
        ];
        let mut resps = Vec::new();
        for n in &q {
            let txt = std::fs::read_to_string(
                std::fs::read_dir("testdata/lambo")
                    .unwrap()
                    .map(|e| e.unwrap().path())
                    .find(|p| {
                        p.file_name().unwrap().to_string_lossy().starts_with(n)
                            && p.file_name()
                                .unwrap()
                                .to_string_lossy()
                                .ends_with(".response.json")
                    })
                    .unwrap(),
            )
            .unwrap();
            let v: serde_json::Value = serde_json::from_str(&txt).unwrap();
            resps.push(recorded_to_http(&v));
        }
        // supersede inspects stale first -> need an inspect response before derive.
        // Reorder: after recall, supersede calls inspect (10), derive(11), stats(12).
        // Our queue above has recall(09), inspect(10), derive(11), stats(12), recall(13). Good.
        let transport = crate::mcp::FakeTransport::new(resps);
        let adapter = LamboAdapter::new(
            "http://127.0.0.1:7700/mcp".to_string(),
            "doyle-probe".to_string(),
            Box::new(transport),
        );
        let listed = adapter.list("GET /patients export every patient").unwrap();
        assert!(!listed.is_empty());
        let stale_id = listed[0].reference.id.clone();
        let corr_text = "GET /patients returns 100 patients per page and a next_cursor; follow next_cursor until it is null to export every patient";
        let corr = adapter.supersede(&stale_id, corr_text).unwrap();
        assert_ne!(corr.id, stale_id);
        let stale_read = adapter.read(&stale_id).unwrap();
        assert!(stale_read.text.contains(&corr.id));
        let corr_read = adapter.read(&corr.id).unwrap();
        assert!(
            corr_read
                .text
                .contains("GET /patients returns 100 patients")
        );
    }

    #[test]
    fn recorded_read_path() {
        // Queue recorded responses for: initialize, notifications, recall(09), inspect(10)
        let r01 = load_recorded("01-initialize");
        let r02 = load_recorded("02-notifications-initialized");
        let r09 = load_recorded("09-recall-before-correction");
        let r10 = load_recorded("10-inspect-memory");
        let transport = FakeTransport::new(vec![
            crate::mcp::recorded_to_http(&r01),
            crate::mcp::recorded_to_http(&r02),
            crate::mcp::recorded_to_http(&r09),
            crate::mcp::recorded_to_http(&r10),
        ]);
        let adapter = LamboAdapter::new(
            "http://127.0.0.1:7700/mcp".to_string(),
            "doyle-probe".to_string(),
            Box::new(transport),
        );
        let listed = adapter.list("GET /patients export every patient").unwrap();
        assert!(!listed.is_empty());
        let first_id = listed[0].reference.id.clone();
        let read = adapter.read(&first_id).unwrap();
        assert!(read.text.contains("GET /patients returns every patient"));
    }

    // ---- helpers for scripted server ----
    #[derive(Default)]
    struct SrvState {
        concepts: Vec<(String, String)>,
        next: u32,
        derives: u32,
        receipts: std::collections::HashMap<String, Vec<String>>,
    }

    fn scripted_server() -> (FakeTransport, std::sync::Arc<std::sync::Mutex<SrvState>>) {
        let state = std::sync::Arc::new(std::sync::Mutex::new(SrvState::default()));
        let st = state.clone();
        let transport = FakeTransport::scripted(move |url: &str, body: &str| {
            let v: serde_json::Value =
                serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
            let method = v.get("method").and_then(|m| m.as_str()).unwrap_or("");
            if method == "initialize" {
                let msg = serde_json::json!({
                    "jsonrpc":"2.0",
                    "id": v.get("id"),
                    "result": {
                        "protocolVersion":"2025-06-18",
                        "capabilities":{"tools":{}},
                        "serverInfo":{"name":"lambo","version":"0.8.0"},
                        "instructions":"x"
                    }
                });
                return Ok(crate::mcp::HttpResponse {
                    status: 200,
                    headers: vec![("Mcp-Session-Id".to_string(), "<mcp-session-id>".to_string())],
                    body: format!("id: 0\nretry: 3000\ndata: {}\n\n", msg),
                });
            }
            if method == "notifications/initialized" {
                return Ok(crate::mcp::HttpResponse {
                    status: 202,
                    headers: vec![],
                    body: String::new(),
                });
            }
            if method != "tools/call" {
                return Err(crate::mcp::TransportError {
                    url: url.to_string(),
                    message: "unknown method".to_string(),
                });
            }
            let params = v.get("params").cloned().unwrap_or(serde_json::Value::Null);
            let tool = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let rid = v.get("id").cloned().unwrap_or(serde_json::Value::Null);
            let mut st = st.lock().unwrap();
            match tool {
                "lambo_derive" => {
                    let concepts = args
                        .get("concepts")
                        .and_then(|c| c.as_array())
                        .cloned()
                        .unwrap_or_default();
                    let mut created = Vec::new();
                    for c in &concepts {
                        st.next += 1;
                        let cid = format!("{:08x}-0000-0000-0000-000000000000", st.next);
                        let content = c
                            .get("content")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string();
                        st.concepts.push((cid.clone(), content));
                        created.push(cid);
                    }
                    st.derives += 1;
                    let receipt = format!("receipt-{}", st.derives);
                    st.receipts.insert(receipt.clone(), created.clone());
                    let msg = serde_json::json!({
                        "jsonrpc":"2.0","id": rid,
                        "result": {
                            "content":[{"type":"text","text":"accepted"}],
                            "structuredContent":{"agent_id": args.get("agent_id"), "receipt": receipt, "receipt_state":"pending", "warnings":[]},
                            "isError": false
                        }
                    });
                    Ok(crate::mcp::HttpResponse {
                        status: 200,
                        headers: vec![],
                        body: format!("id: 0\nretry: 3000\ndata: {}\n\n", msg),
                    })
                }
                "lambo_stats" => {
                    let receipt = args.get("receipt").and_then(|x| x.as_str()).unwrap_or("");
                    let created = st.receipts.get(receipt).cloned().unwrap_or_default();
                    let msg = serde_json::json!({
                        "jsonrpc":"2.0","id": rid,
                        "result": {
                            "content":[{"type":"text","text":"stats"}],
                            "structuredContent":{"receipt":{"created": created, "state":"applied"}},
                            "isError": false
                        }
                    });
                    Ok(crate::mcp::HttpResponse {
                        status: 200,
                        headers: vec![],
                        body: format!("id: 0\nretry: 3000\ndata: {}\n\n", msg),
                    })
                }
                "lambo_recall" => {
                    let hits: Vec<serde_json::Value> = st.concepts.iter().map(|(cid, content)| {
                        serde_json::json!({"node_id": cid, "content": content, "concept_type":"Logic", "score":1.0, "is_canonical":false, "blast_radius":null})
                    }).collect();
                    let msg = serde_json::json!({
                        "jsonrpc":"2.0","id": rid,
                        "result": {
                            "content":[{"type":"text","text":"ctx"}],
                            "structuredContent":{"context":"ctx", "hits": hits, "warnings":[]},
                            "isError": false
                        }
                    });
                    Ok(crate::mcp::HttpResponse {
                        status: 200,
                        headers: vec![],
                        body: format!("id: 0\nretry: 3000\ndata: {}\n\n", msg),
                    })
                }
                "lambo_inspect" => {
                    let focus = args.get("focus").and_then(|x| x.as_str()).unwrap_or("");
                    let found = st
                        .concepts
                        .iter()
                        .find(|(cid, _)| cid == focus || cid.starts_with(focus));
                    let (label, node_id) = match found {
                        Some((cid, content)) => (format!("{} [Logic]", content), cid.clone()),
                        None => (String::new(), String::new()),
                    };
                    let msg = serde_json::json!({
                        "jsonrpc":"2.0","id": rid,
                        "result": {
                            "content":[{"type":"text","text": format!("focus: {}", label)}],
                            "structuredContent":{"focus":{"label": label, "node_id": node_id}, "warnings":[]},
                            "isError": false
                        }
                    });
                    Ok(crate::mcp::HttpResponse {
                        status: 200,
                        headers: vec![],
                        body: format!("id: 0\nretry: 3000\ndata: {}\n\n", msg),
                    })
                }
                _ => Err(crate::mcp::TransportError {
                    url: url.to_string(),
                    message: format!("unknown tool {}", tool),
                }),
            }
        });
        (transport, state)
    }

    #[test]
    fn lambo_passes_conformance() {
        let (transport, _state) = scripted_server();
        let adapter = LamboAdapter::new(
            "http://127.0.0.1:7700/mcp".to_string(),
            "doyle-probe".to_string(),
            Box::new(transport),
        );
        crate::adapters::check_conformance(&adapter);
    }

    #[test]
    fn stats_wait_ms_below_default_timeout() {
        assert!(STATS_WAIT_MS < crate::mcp::DEFAULT_TRANSPORT_TIMEOUT.as_millis() as u64);
    }

    #[test]
    fn body_capture_recall_and_derive() {
        let (transport, _state) = scripted_server();
        let adapter = LamboAdapter::new(
            "http://127.0.0.1:7700/mcp".to_string(),
            "doyle-probe".to_string(),
            Box::new(transport),
        );
        // trigger initialize + a list (recall)
        let _ = adapter.list("GET /patients").ok();
        let calls = adapter.recorded_requests();
        // first two calls: initialize then notifications
        assert!(calls.len() >= 2);
        assert!(calls[0].body.contains("\"initialize\""));
        assert!(calls[1].body.contains("notifications/initialized"));
        // find recall call
        let recall = calls
            .iter()
            .find(|c| c.body.contains("lambo_recall"))
            .unwrap();
        assert!(recall.body.contains("GET /patients"));
        assert!(recall.body.contains("\"top_k\": 50") || recall.body.contains("\"top_k\":50"));
        // agent_id on every call
        for c in &calls {
            if c.body.contains("agent_id") {
                assert!(c.body.contains("doyle-probe"));
            }
        }
        // session id echoed after initialize
        let has_session = calls.iter().skip(2).any(|c| {
            c.headers
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case("Mcp-Session-Id") && v == "<mcp-session-id>")
        });
        assert!(has_session, "session id must be echoed");
    }

    #[test]
    fn body_capture_derive_against() {
        let (transport, _state) = scripted_server();
        let adapter = LamboAdapter::new(
            "http://127.0.0.1:7700/mcp".to_string(),
            "doyle-probe".to_string(),
            Box::new(transport),
        );
        let first = adapter.write("first mem", "s").unwrap();
        let corr_text = "corr text";
        let _ = adapter.supersede(&first.id, corr_text).unwrap();
        let calls = adapter.recorded_requests();
        let derive = calls
            .iter()
            .rev()
            .find(|c| c.body.contains("lambo_derive") && c.body.contains("against"))
            .unwrap();
        assert!(
            derive.body.contains("\"concept_type\": \"logic\"")
                || derive.body.contains("\"concept_type\":\"logic\"")
        );
        assert!(derive.body.contains("against"));
        // short id (8 hex) in against
        let short = &first.id[..8];
        assert!(derive.body.contains(short));
        // stats wait_ms
        let stats = calls
            .iter()
            .rev()
            .find(|c| c.body.contains("lambo_stats"))
            .unwrap();
        assert!(
            stats
                .body
                .contains(&format!("\"wait_ms\": {}", STATS_WAIT_MS))
                || stats
                    .body
                    .contains(&format!("\"wait_ms\":{}", STATS_WAIT_MS))
        );
    }

    #[test]
    fn watch_offline_report_once() {
        let (transport, _state) = scripted_server();
        let adapter = LamboAdapter::new(
            "http://127.0.0.1:7700/mcp".to_string(),
            "doyle-probe".to_string(),
            Box::new(transport),
        );
        let a = adapter.write("m1", "s").unwrap();
        let b = adapter.write("m2", "s").unwrap();
        let first = adapter.watch().unwrap();
        let ids: Vec<&str> = first.iter().map(|m| m.reference.id.as_str()).collect();
        assert!(ids.contains(&a.id.as_str()));
        assert!(ids.contains(&b.id.as_str()));
        assert!(adapter.watch().unwrap().is_empty());
        let c = adapter.write("m3", "s").unwrap();
        let later = adapter.watch().unwrap();
        let later_ids: Vec<&str> = later.iter().map(|m| m.reference.id.as_str()).collect();
        assert!(later_ids.contains(&c.id.as_str()));
        assert!(!later_ids.contains(&a.id.as_str()));
        assert!(adapter.watch().unwrap().is_empty());
    }

    // LB5: supersede retry branches
    #[test]
    fn supersede_retry_cache_hit() {
        let (transport, _state) = scripted_server();
        let adapter = LamboAdapter::new(
            "http://127.0.0.1:7700/mcp".to_string(),
            "doyle-probe".to_string(),
            Box::new(transport),
        );
        let first = adapter.write("stale", "s").unwrap();
        let text = "correction text";
        let r1 = adapter.supersede(&first.id, text).unwrap();
        let derives_before = adapter
            .recorded_requests()
            .iter()
            .filter(|c| c.body.contains("lambo_derive"))
            .count();
        let r2 = adapter.supersede(&first.id, text).unwrap();
        assert_eq!(r1.id, r2.id);
        let derives_after = adapter
            .recorded_requests()
            .iter()
            .filter(|c| c.body.contains("lambo_derive"))
            .count();
        assert_eq!(derives_before, derives_after, "cache hit must not derive");
    }

    // inspect-found retry path: a fresh adapter (empty caches) whose server already
    // records the contradiction. Queued responses drive inspect -> inspect -> inspect.
    #[test]
    fn supersede_retry_inspect_found() {
        let stale_id = "810c1cbf-0000-0000-0000-000000000000";
        let corr_short = "1be4e4a6";
        let corr_full = "1be4e4a6-0000-0000-0000-000000000000";
        let corr_text = "correction text";
        // inspect(stale) -> warning with short corr id; focus label stale content
        let mut r_stale = serde_json::json!({
            "jsonrpc":"2.0","id":1,
            "result": {
                "content":[{"type":"text","text":"focus: stale [Logic]"}],
                "structuredContent": {
                    "focus": {"label": "stale [Logic]", "node_id": stale_id},
                    "warnings": [format!("⚠ Contradicted at t by doyle-probe: \"...\" (id {})", corr_short)]
                },
                "isError": false
            }
        });
        let mut r_short = serde_json::json!({
            "jsonrpc":"2.0","id":1,
            "result": {
                "content":[{"type":"text","text":"focus: corr [Logic]"}],
                "structuredContent": {
                    "focus": {"label": "corr [Logic]", "node_id": corr_full},
                    "warnings": []
                },
                "isError": false
            }
        });
        let mut r_full = serde_json::json!({
            "jsonrpc":"2.0","id":1,
            "result": {
                "content":[{"type":"text","text":"focus: corr text [Logic]"}],
                "structuredContent": {
                    "focus": {"label": format!("{} [Logic]", corr_text), "node_id": corr_full},
                    "warnings": []
                },
                "isError": false
            }
        });
        let _ = (&mut r_stale, &mut r_short, &mut r_full);
        let wrap = |m: serde_json::Value| serde_json::json!({"http_status":200,"response_headers":{"Mcp-Session-Id":"<mcp-session-id>"}, "message": m});
        let init_msg = serde_json::json!({"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"lambo","version":"0.8.0"},"instructions":"x"}});
        let notif_status = 202;
        let _ = notif_status;
        let transport = FakeTransport::new(vec![
            crate::mcp::recorded_to_http(
                &serde_json::json!({"http_status":200,"response_headers":{"Mcp-Session-Id":"<mcp-session-id>"}, "message": init_msg}),
            ),
            crate::mcp::recorded_to_http(
                &serde_json::json!({"http_status":202,"response_headers":{}, "message": null}),
            ),
            crate::mcp::recorded_to_http(&wrap(r_stale)),
            crate::mcp::recorded_to_http(&wrap(r_short)),
            crate::mcp::recorded_to_http(&wrap(r_full)),
        ]);
        let adapter = LamboAdapter::new(
            "http://127.0.0.1:7700/mcp".to_string(),
            "doyle-probe".to_string(),
            Box::new(transport),
        );
        // seed content cache? adapter empty; supersede inspect path calls inspect_content(full) which uses inspect -> label corr_text. Good.
        // Need stale_id in content cache? read not required. supersede returns corr_full.
        let r = adapter.supersede(stale_id, corr_text).unwrap();
        assert_eq!(r.id, corr_full);
    }
}
