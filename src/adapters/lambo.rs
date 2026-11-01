//! The Lambo adapter: Lambo over MCP (streamable HTTP).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use super::{AdapterError, Capabilities, MemoryAdapter};
use crate::core::{Memory, MemoryRef};
use crate::mcp::{McpClient, McpError, Transport};

/// Broad query used for watch. The `*` match-all behavior is unrecorded
/// (no transcript shows a server response to `*`); live verification is
/// deferred to T3.3b.
pub const BROAD_WATCH_QUERY: &str = "*";

/// `wait_ms` passed to `lambo_stats` after a derive. Kept strictly below the
/// default transport timeout (see `crate::mcp::DEFAULT_TRANSPORT_TIMEOUT`)
/// with margin so the receipt wait cannot outlive the HTTP call carrying it.
pub const STATS_WAIT_MS: u64 = 4000;

/// Lambo adapter over MCP.
///
/// Restart-loss disclosure: the supersede and scope caches are process-local;
/// after a restart, reads lose recorded warnings and scopes until the next
/// in-process supersede.
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
    // Warnings are retained per recall batch (one Vec per list() call) so a
    // read() can bind a Contradicted warning to the stale memory it belongs to.
    recall_warning_batches: RefCell<Vec<Vec<String>>>,
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
            recall_warning_batches: RefCell::new(Vec::new()),
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
        self.recall_warning_batches
            .borrow()
            .iter()
            .flat_map(|b| b.iter().cloned())
            .collect()
    }

    pub fn recall_warning_batches(&self) -> Vec<Vec<String>> {
        self.recall_warning_batches.borrow().clone()
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

/// Doyle's own Lambo session. The bare `/mcp` path is the server's default
/// session, `lambo-dev`, which Doyle never writes to.
pub const DEFAULT_URL: &str = "http://127.0.0.1:7700/mcp/s/doyle";

/// The Lambo URL to use: `LAMBO_URL` when set and non-empty, else `DEFAULT_URL`.
pub fn lambo_url() -> String {
    url_from_env(std::env::var("LAMBO_URL").ok().as_deref())
}

fn url_from_env(value: Option<&str>) -> String {
    match value {
        Some(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ => DEFAULT_URL.to_string(),
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
    "doyle".to_string()
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

/// The (node id, label) of every neighbour that has a `Contradicts` edge into the
/// inspected concept, from an `lambo_inspect` result (`<-` on the stale concept).
fn contradicting_neighbours(structured: &serde_json::Value) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let levels = structured
        .get("focus")
        .and_then(|f| f.get("levels"))
        .and_then(|l| l.as_array());
    for level in levels.into_iter().flatten() {
        let neighbours = level.get("neighbours").and_then(|n| n.as_array());
        for n in neighbours.into_iter().flatten() {
            let text = |k: &str| n.get(k).and_then(|v| v.as_str()).unwrap_or("");
            if text("edge_type") == "Contradicts"
                && text("direction") == "<-"
                && !text("node_id").is_empty()
            {
                found.push((text("node_id").to_string(), text("label").to_string()));
            }
        }
    }
    found
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
        // inspect neighbourhood (transcript 10 carries none). Retain them as a
        // single batch for this list() call so read() can bind warnings to the
        // stale memory they belong to.
        let server_warnings = structured
            .get("warnings")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let mut batch: Vec<String> = Vec::new();
        for w in server_warnings {
            if let Some(t) = w.as_str() {
                batch.push(t.to_string());
            }
        }
        self.recall_warning_batches.borrow_mut().push(batch);
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
            let short = short_id(id);
            let batches = self.recall_warning_batches.borrow();
            let mut collected: Vec<String> = Vec::new();
            for batch in batches.iter() {
                // L1: a "Contradicts" (not "Contradicted") line naming this id
                // or its short form — the stale-side marker.
                let has_l1 = batch.iter().any(|w| {
                    w.contains("Contradicts")
                        && !w.contains("Contradicted")
                        && (w.contains(id) || w.contains(&short))
                });
                if !has_l1 {
                    continue;
                }
                // L2: "Contradicted" lines in the same batch.
                let l2_indices: Vec<usize> = batch
                    .iter()
                    .enumerate()
                    .filter(|(_, w)| w.contains("Contradicted"))
                    .map(|(i, _)| i)
                    .collect();
                if l2_indices.is_empty() {
                    continue;
                }
                let l1_positions: Vec<usize> = batch
                    .iter()
                    .enumerate()
                    .filter(|(_, w)| {
                        w.contains("Contradicts")
                            && !w.contains("Contradicted")
                            && (w.contains(id) || w.contains(&short))
                    })
                    .map(|(i, _)| i)
                    .collect();
                for &li in &l1_positions {
                    // Pair this L1 with the nearest L2 positionally.
                    let mut best: Option<(usize, usize)> = None;
                    for &l2i in &l2_indices {
                        let dist = l2i.abs_diff(li);
                        match best {
                            None => best = Some((dist, l2i)),
                            Some((d, _)) if dist < d => best = Some((dist, l2i)),
                            Some((d, _)) if dist == d && l2i < li => {
                                best = Some((dist, l2i));
                            }
                            _ => {}
                        }
                    }
                    if let Some((_, l2i)) = best
                        && let Some(cid) = parse_contradicted_id(&batch[l2i])
                    {
                        collected.push(cid);
                    }
                }
            }
            let distinct: HashSet<String> = collected.into_iter().collect();
            if distinct.len() == 1 {
                distinct.into_iter().next()
            } else {
                None
            }
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
        let mut linked_by_edge: Vec<(String, String)> = Vec::new();
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
                // Recorded (testdata/lambo/29): inspecting the stale concept carries
                // no "Contradicted" line, only a `Contradicts` edge from the
                // correction in the neighbourhood. Read that edge too.
                linked_by_edge = contradicting_neighbours(&structured);
                for w in &warnings {
                    if let Some(short) = parse_contradicted_id(w) {
                        existing_corr = Some(short);
                        break;
                    }
                }
            }
        }

        if let Some((full, _)) = linked_by_edge
            .iter()
            .find(|(_, label)| strip_type_suffix(label) == correction_text)
        {
            return Ok(self.remember_correction(stale_id, full, correction_text));
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
                return Ok(self.remember_correction(stale_id, &full, correction_text));
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
        // A derive that creates nothing but matches an existing concept (Lambo's
        // de-duplication of a repeated or earlier write, testdata/lambo/26-27)
        // names that concept in `matched`. Re-deriving it with `against` records
        // the contradiction when it was missing (27) and adds nothing when it was
        // already there (21), so the matched concept ends linked either way.
        let first_id = |key: &str| {
            receipt_obj
                .get(key)
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };
        let mut corr = first_id("created");
        if corr.is_empty() {
            corr = first_id("matched");
        }
        if corr.is_empty() {
            return Err(self.backend_error(
                Some(stale_id.to_string()),
                "lambo_stats returned no created node id".to_string(),
            ));
        }
        Ok(self.remember_correction(stale_id, &corr, correction_text))
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
    /// Cache a correction found or made for a stale concept and return its reference.
    fn remember_correction(&self, stale_id: &str, corr_id: &str, text: &str) -> MemoryRef {
        self.content_cache
            .borrow_mut()
            .insert(corr_id.to_string(), text.to_string());
        let scope = self
            .scope_cache
            .borrow()
            .get(stale_id)
            .cloned()
            .unwrap_or_else(|| self.session.clone());
        self.scope_cache
            .borrow_mut()
            .insert(corr_id.to_string(), scope);
        self.supersede_cache.borrow_mut().insert(
            stale_id.to_string(),
            (corr_id.to_string(), text.to_string()),
        );
        MemoryRef {
            adapter: self.name.clone(),
            id: corr_id.to_string(),
        }
    }

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
    #[test]
    fn url_takes_lambo_url_when_set() {
        assert_eq!(
            url_from_env(Some("http://127.0.0.1:9000/mcp/s/other")),
            "http://127.0.0.1:9000/mcp/s/other"
        );
    }

    #[test]
    fn url_defaults_to_doyle_session_when_unset_or_empty() {
        for v in [None, Some(""), Some("  ")] {
            let url = url_from_env(v);
            assert_eq!(url, DEFAULT_URL);
            assert_eq!(session_from_url(&url), "doyle");
        }
        assert_eq!(DEFAULT_URL, "http://127.0.0.1:7700/mcp/s/doyle");
    }

    #[test]
    fn session_from_url_names_the_session() {
        assert_eq!(
            session_from_url("http://127.0.0.1:7700/mcp/s/doyle"),
            "doyle"
        );
        assert_eq!(
            session_from_url("http://127.0.0.1:7700/mcp/s/doyle-probe"),
            "doyle-probe"
        );
        assert_eq!(
            session_from_url("http://127.0.0.1:7700/mcp/s/lambo-dev"),
            "lambo-dev"
        );
        assert_eq!(session_from_url("http://127.0.0.1:7700/mcp"), "doyle");
    }

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
        warnings: Vec<String>,
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
                        let against = c
                            .get("against")
                            .and_then(|a| a.as_array())
                            .cloned()
                            .unwrap_or_default();
                        let is_contradiction = !against.is_empty();
                        st.concepts.push((cid.clone(), content.clone()));
                        if is_contradiction {
                            let corr_short = cid.chars().take(8).collect::<String>();
                            st.warnings.push(format!(
                                "\u{26a0} Contradicted at 2026-10-29T15:07:26Z by doyle-probe: \"{}\" (id {})",
                                content, corr_short
                            ));
                            for a in &against {
                                let stale_short = a.as_str().unwrap_or("").to_string();
                                let stale_content = st
                                    .concepts
                                    .iter()
                                    .find(|(cc, _)| cc.starts_with(&stale_short))
                                    .map(|(_, ct)| ct.clone())
                                    .unwrap_or_default();
                                st.warnings.push(format!(
                                    "\u{26a0} Contradicts \"{}\" (id {})",
                                    stale_content, stale_short
                                ));
                            }
                        }
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
                    let warnings = st.warnings.clone();
                    let msg = serde_json::json!({
                        "jsonrpc":"2.0","id": rid,
                        "result": {
                            "content":[{"type":"text","text":"ctx"}],
                            "structuredContent":{"context":"ctx", "hits": hits, "warnings": warnings},
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

    #[test]
    fn recall_warning_reaches_read() {
        let (transport, _state) = scripted_server();
        let adapter = LamboAdapter::new(
            "http://127.0.0.1:7700/mcp".to_string(),
            "doyle-probe".to_string(),
            Box::new(transport),
        );
        let stale = adapter.write("stale mem", "s").unwrap();
        let corr_text = "corr text";
        let corr = adapter.supersede(&stale.id, corr_text).unwrap();
        // Drop the local supersede cache so the read must use the recall path.
        adapter.supersede_cache.borrow_mut().clear();
        // Populate recall warnings via a recall (list).
        let _ = adapter.list("stale").ok();
        let read = adapter.read(&stale.id).unwrap();
        assert!(
            read.text.contains("\u{26a0} Contradicted"),
            "warning did not reach read output: {}",
            read.text
        );
        assert!(
            read.text.contains(&corr.id[..8]),
            "correction id did not reach read output: {}",
            read.text
        );
    }

    #[test]
    fn two_contradictions_each_own_correction() {
        let (transport, _state) = scripted_server();
        let adapter = LamboAdapter::new(
            "http://127.0.0.1:7700/mcp".to_string(),
            "doyle-probe".to_string(),
            Box::new(transport),
        );
        let stale1 = adapter.write("stale one", "s").unwrap();
        let stale2 = adapter.write("stale two", "s").unwrap();
        let corr1 = adapter.supersede(&stale1.id, "corr one").unwrap();
        let corr2 = adapter.supersede(&stale2.id, "corr two").unwrap();
        adapter.supersede_cache.borrow_mut().clear();
        let _ = adapter.list("stale").ok();
        let r1 = adapter.read(&stale1.id).unwrap();
        assert!(
            r1.text.contains(&corr1.id[..8]),
            "stale1 should attach corr1, got: {}",
            r1.text
        );
        let r2 = adapter.read(&stale2.id).unwrap();
        assert!(
            r2.text.contains(&corr2.id[..8]),
            "stale2 should attach corr2, got: {}",
            r2.text
        );
        // A correction read must not self-attach a warning.
        let rc = adapter.read(&corr1.id).unwrap();
        assert!(
            !rc.text.contains("\u{26a0} Contradicted"),
            "correction must not self-attach warning, got: {}",
            rc.text
        );
    }

    // ---- T3.6: a correction Lambo already holds (recordings 16-31, doyle-probe) ----

    const INV_STALE: &str = "215afe30-c52c-4e1d-b369-2f53cd1704cd";
    const INV_CORR: &str = "de5c37d7-a2ba-4835-9a17-b2c4c3b16040";
    const INV_TEXT: &str =
        "Probe T3.6 B: the invoices endpoint pages at 25 per response and returns a next_cursor";
    const APPT_STALE: &str = "5421710c-ae88-43be-9e37-abaf40a21235";
    const APPT_CORR: &str = "fe162aa7-e59e-483e-9708-462541bb07cc";
    const APPT_TEXT: &str = "Probe T3.6 A: the appointments endpoint pages at 50 per response and returns a next_cursor";

    fn replay_adapter(prefixes: &[&str]) -> LamboAdapter {
        let mut q = vec!["01-initialize", "02-notifications-initialized"];
        q.extend_from_slice(prefixes);
        let resps = q
            .iter()
            .map(|n| crate::mcp::recorded_to_http(&load_recorded(n)))
            .collect();
        LamboAdapter::new(
            "http://127.0.0.1:7700/mcp/s/doyle-probe".to_string(),
            "doyle-probe".to_string(),
            Box::new(FakeTransport::new(resps)),
        )
    }

    fn derive_bodies(adapter: &LamboAdapter) -> Vec<String> {
        adapter
            .recorded_requests()
            .into_iter()
            .filter(|c| c.body.contains("lambo_derive"))
            .map(|c| c.body)
            .collect()
    }

    // The recordings show what Lambo does: the matched receipt names the concept.
    #[test]
    fn recorded_matched_receipts_name_the_concept() {
        let matched = load_recorded("27-stats-wait-matched-contradiction");
        let r = &matched["message"]["result"]["structuredContent"]["receipt"];
        assert_eq!(r["created"].as_array().unwrap().len(), 0);
        assert_eq!(r["matched"][0], INV_CORR);
        assert!(
            r["detail"]
                .as_str()
                .unwrap()
                .contains("1 contradiction(s) recorded")
        );
        let again = load_recorded("21-stats-wait-matched-no-contradiction");
        let r = &again["message"]["result"]["structuredContent"]["receipt"];
        assert_eq!(r["matched"][0], APPT_CORR);
        assert!(!r["detail"].as_str().unwrap().contains("contradiction"));
    }

    #[test]
    fn supersede_matched_derive_returns_existing_and_links() {
        let adapter = replay_adapter(&[
            "10-inspect-memory",
            "26-derive-correction-invoices-matched-against",
            "27-stats-wait-matched-contradiction",
            "28-recall-invoices-linked",
        ]);
        let r = adapter.supersede(INV_STALE, INV_TEXT).unwrap();
        assert_eq!(r.id, INV_CORR);
        let derives = derive_bodies(&adapter);
        assert_eq!(derives.len(), 1);
        assert!(derives[0].contains("against"));
        assert!(
            derives[0].contains("215afe30"),
            "against names the stale concept"
        );
        // Lambo now shows the stale concept contradicted by exactly that correction.
        adapter.list("invoices endpoint pages").unwrap();
        let stale = adapter.read(INV_STALE).unwrap();
        assert!(
            stale.text.contains("de5c37d7"),
            "stale text: {}",
            stale.text
        );
    }

    // flag-1's shape: the correction was created plain, nothing links it.
    #[test]
    fn supersede_unlinked_existing_correction_ends_linked() {
        let adapter = replay_adapter(&[
            "24-derive-correction-invoices-unlinked",
            "25-stats-wait-correction-invoices-unlinked",
            "10-inspect-memory",
            "26-derive-correction-invoices-matched-against",
            "27-stats-wait-matched-contradiction",
            "28-recall-invoices-linked",
        ]);
        let plain = adapter.write(INV_TEXT, "doyle-probe").unwrap();
        assert_eq!(plain.id, INV_CORR);
        let r = adapter.supersede(INV_STALE, INV_TEXT).unwrap();
        assert_eq!(r.id, plain.id);
        let derives = derive_bodies(&adapter);
        assert_eq!(derives.len(), 2);
        assert!(!derives[0].contains("against"));
        assert!(derives[1].contains("against") && derives[1].contains("215afe30"));
        adapter.list("invoices endpoint pages").unwrap();
        assert!(adapter.read(INV_STALE).unwrap().text.contains("de5c37d7"));
    }

    // Already linked: the stale concept's `Contradicts` edge finds the correction;
    // a fresh adapter sends no derive, and a second call sends nothing at all.
    #[test]
    fn repeated_supersede_is_a_no_op() {
        let adapter = replay_adapter(&["29-inspect-stale-appointments-linked"]);
        let first = adapter.supersede(APPT_STALE, APPT_TEXT).unwrap();
        assert_eq!(first.id, APPT_CORR);
        assert!(derive_bodies(&adapter).is_empty());
        let sent = adapter.recorded_requests().len();
        let second = adapter.supersede(APPT_STALE, APPT_TEXT).unwrap();
        assert_eq!(second.id, APPT_CORR);
        assert_eq!(adapter.recorded_requests().len(), sent, "no request at all");
    }

    // The same retry when the edge is not in the inspect result: the derive is
    // matched and, with no new contradiction, still returns the linked concept.
    #[test]
    fn matched_derive_of_linked_correction_is_still_the_correction() {
        let adapter = replay_adapter(&[
            "10-inspect-memory",
            "20-derive-correction-appointments-again-against",
            "21-stats-wait-matched-no-contradiction",
        ]);
        let r = adapter.supersede(APPT_STALE, APPT_TEXT).unwrap();
        assert_eq!(r.id, APPT_CORR);
    }
}
