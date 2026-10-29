//! The Lambo adapter: Lambo over MCP (streamable HTTP).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use super::{AdapterError, Capabilities, MemoryAdapter};
use crate::core::{Memory, MemoryRef};
use crate::mcp::{McpClient, McpError, Transport};

pub const BROAD_WATCH_QUERY: &str = "*";

pub struct LamboAdapter {
    name: String,
    agent_id: String,
    session: String,
    client: RefCell<McpClient>,
    content_cache: RefCell<HashMap<String, String>>,
    scope_cache: RefCell<HashMap<String, String>>,
    supersede_cache: RefCell<HashMap<String, (String, String)>>,
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
        if let Some((corr_id, _)) = self.supersede_cache.borrow().get(id).cloned()
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
                    "wait_ms": 5000
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
                    "wait_ms": 5000
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
}
