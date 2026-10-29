//! The `curate` command: read memories through the adapters, have the curator
//! cite each against the current contract, write citations.
//!
//! For each configured adapter, list memories (one search per contract
//! element, so recall-backed adapters stay honest), skip those already cited
//! at their current text, run the curator against the current contract for
//! the rest, and write citations. A memory the curator ties to nothing is
//! recorded as a citation with no elements, so it is not asked again until
//! its text changes. `--dry-run` prints what would be cited and writes
//! nothing.

use std::collections::HashSet;
use std::path::Path;
use std::process::ExitCode;

use crate::adapters::lambo::LamboAdapter;
use crate::adapters::muse_code::MuseCodeAdapter;
use crate::adapters::{FakeAdapter, MemoryAdapter};
use crate::cli;
use crate::contract::ContractIndex;
use crate::core::{Citation, ElementId, Memory, MemoryRef, Store};
use crate::models::ModelClient;
use crate::models::ReplayClient;
use crate::models::baseten::BasetenModelClient;
use crate::prompts::curator::{self, CuratorVerdict};
use vimanam::contract::Entity as ContractEntity;

/// The model the curator runs on (the user's pick: Muse Spark Open on
/// Baseten).
const CURATOR_MODEL: &str = crate::prompts::curator::BASETEN_CURATOR_MODEL;

/// One citation decision made during a curate run.
#[derive(Debug, Clone)]
pub struct CurateItem {
    /// The memory that was considered.
    pub memory: MemoryRef,
    /// The claim the curator recorded (one line).
    pub claim: String,
    /// The elements cited (empty when the memory rests on nothing).
    pub elements: Vec<ElementId>,
    /// Whether the curator tied the memory to nothing in this contract.
    pub rests_on_nothing: bool,
}

/// Alias kept for callers that prefer the `Outcome` spelling.
pub type CurateOutcome = CurateItem;
/// Alias kept for callers that prefer the `Result` spelling.
pub type CurateResult = CurateItem;

/// Run the `curate` command.
pub fn run(cli: &cli::Cli) -> ExitCode {
    let dry_run = cli.common.dry_run;

    let contract_path = "testdata/contracts/patients-v1.yaml";
    let index = match ContractIndex::load(contract_path) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("doyle curate: {e}");
            return ExitCode::from(2);
        }
    };

    let dir = cli
        .common
        .state_dir
        .clone()
        .unwrap_or_else(|| ".doyle".to_string());
    let mut store = match Store::open(&dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("doyle curate: {e}");
            return ExitCode::from(2);
        }
    };

    let names = if cli.common.adapters.is_empty() {
        vec![String::from("muse-code")]
    } else {
        cli.common.adapters.clone()
    };
    let mut adapters: Vec<Box<dyn MemoryAdapter>> = Vec::new();
    for name in &names {
        match name.as_str() {
            "muse-code" | "muse_code" | "musecode" => {
                adapters.push(Box::new(MuseCodeAdapter::new(Path::new("."))));
            }
            "lambo" => {
                adapters.push(Box::new(LamboAdapter::http(
                    "http://127.0.0.1:7700/mcp",
                    "doyle",
                )));
            }
            "fake" => {
                adapters.push(Box::new(FakeAdapter::new("fake")));
            }
            other => {
                eprintln!("doyle curate: unknown adapter: {other}");
                return ExitCode::from(2);
            }
        }
    }

    // Model client: replay from DOYLE_REPLAY_DIR when given, otherwise a
    // Baseten client that will fail without a key (CLI runs are not the
    // offline gate's concern).
    let model: Box<dyn ModelClient> = if let Ok(dir) = std::env::var("DOYLE_REPLAY_DIR") {
        Box::new(ReplayClient::new(dir))
    } else {
        // A transport that always errors; keeps the binary key-free at rest.
        struct FailingTransport;
        impl crate::models::baseten::HttpTransport for FailingTransport {
            fn post(
                &self,
                _url: &str,
                _headers: &[(&str, &str)],
                _body: &str,
            ) -> Result<
                crate::models::baseten::HttpResponse,
                crate::models::baseten::HttpTransportError,
            > {
                Err(crate::models::baseten::HttpTransportError {
                    message: String::from("no transport configured"),
                })
            }
        }
        Box::new(BasetenModelClient::new(Box::new(FailingTransport)))
    };

    let items = match curate(&adapters, model.as_ref(), &index, &mut store, dry_run) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("doyle curate: {e}");
            return ExitCode::from(2);
        }
    };

    for item in &items {
        if dry_run {
            println!(
                "would cite: {}/{} \"{}\" ({} elements)",
                item.memory.adapter,
                item.memory.id,
                item.claim,
                item.elements.len()
            );
        } else {
            println!(
                "cited: {}/{} \"{}\" ({} elements)",
                item.memory.adapter,
                item.memory.id,
                item.claim,
                item.elements.len()
            );
        }
    }

    ExitCode::SUCCESS
}

/// Curate through a slice of boxed adapters.
pub fn curate(
    adapters: &[Box<dyn MemoryAdapter>],
    model: &dyn ModelClient,
    index: &ContractIndex,
    store: &mut Store,
    dry_run: bool,
) -> Result<Vec<CurateItem>, String> {
    let refs: Vec<&dyn MemoryAdapter> = adapters.iter().map(|a| a.as_ref()).collect();
    curate_refs(&refs, model, index, store, dry_run)
}

/// Curate through a slice of adapter references.
pub fn curate_refs(
    adapters: &[&dyn MemoryAdapter],
    model: &dyn ModelClient,
    index: &ContractIndex,
    store: &mut Store,
    dry_run: bool,
) -> Result<Vec<CurateItem>, String> {
    let allowed = curator::allowed_ids(index);
    let mut out: Vec<CurateItem> = Vec::new();

    for adapter in adapters {
        // Collect candidate memories: one search per contract element.
        let mut candidates: Vec<Memory> = Vec::new();
        let mut seen: HashSet<MemoryRef> = HashSet::new();
        for entity in index.entities() {
            let q = query_for_entity(entity, index);
            if q.trim().is_empty() {
                continue;
            }
            match adapter.list(&q) {
                Ok(mems) => {
                    for m in mems {
                        if seen.insert(m.reference.clone()) {
                            candidates.push(m);
                        }
                    }
                }
                Err(_) => {
                    // A recall-backed adapter may reject some queries; skip.
                }
            }
        }

        for memory in candidates {
            // Skip memories already cited at their current text.
            let existing = store.citations_for_memory(&memory.reference);
            if existing.iter().any(|c| c.claim == memory.text) {
                continue;
            }

            let request = curator::build_curator_request(&memory.text, index, CURATOR_MODEL);
            let response = model.complete(&request).map_err(|e| e.to_string())?;
            let verdict = curator::parse_curator_value(&response.value, &allowed)
                .map_err(|e| e.to_string())?;

            match verdict {
                CuratorVerdict::Cited {
                    elements,
                    claim,
                    confidence,
                } => {
                    let citation = Citation {
                        memory: memory.reference.clone(),
                        elements: elements.clone(),
                        claim: memory.text.clone(),
                        recorded_by: CURATOR_MODEL.to_string(),
                        confidence,
                    };
                    if !dry_run {
                        store.record_citation(citation).map_err(|e| e.to_string())?;
                    }
                    out.push(CurateItem {
                        memory: memory.reference.clone(),
                        claim,
                        elements,
                        rests_on_nothing: false,
                    });
                }
                CuratorVerdict::RestsOnNothing { claim } => {
                    let citation = Citation {
                        memory: memory.reference.clone(),
                        elements: Vec::new(),
                        claim: memory.text.clone(),
                        recorded_by: CURATOR_MODEL.to_string(),
                        confidence: 0.0,
                    };
                    if !dry_run {
                        store.record_citation(citation).map_err(|e| e.to_string())?;
                    }
                    out.push(CurateItem {
                        memory: memory.reference.clone(),
                        claim,
                        elements: Vec::new(),
                        rests_on_nothing: true,
                    });
                }
            }
        }
    }

    Ok(out)
}

/// Curate through a slice of adapter references (alias).
pub fn curate_slice(
    adapters: &[&dyn MemoryAdapter],
    model: &dyn ModelClient,
    index: &ContractIndex,
    store: &mut Store,
    dry_run: bool,
) -> Result<Vec<CurateItem>, String> {
    curate_refs(adapters, model, index, store, dry_run)
}

/// Curate through a homogeneous slice of adapters.
pub fn curate_all<A: MemoryAdapter>(
    adapters: &[A],
    model: &dyn ModelClient,
    index: &ContractIndex,
    store: &mut Store,
    dry_run: bool,
) -> Result<Vec<CurateItem>, String> {
    let refs: Vec<&dyn MemoryAdapter> = adapters.iter().map(|a| a as &dyn MemoryAdapter).collect();
    curate_refs(&refs, model, index, store, dry_run)
}

/// Curate through a vector of boxed adapters (alias).
pub fn run_curate(
    adapters: &[Box<dyn MemoryAdapter>],
    model: &dyn ModelClient,
    index: &ContractIndex,
    store: &mut Store,
    dry_run: bool,
) -> Result<Vec<CurateItem>, String> {
    curate(adapters, model, index, store, dry_run)
}

fn query_for_entity(entity: &ContractEntity, index: &ContractIndex) -> String {
    if let Some(loc) = index.native_location(&ElementId(entity.id.clone())) {
        if let Some(path) = &loc.path
            && !path.is_empty()
        {
            return path.clone();
        }
        if let Some(op) = &loc.operation_id
            && !op.is_empty()
        {
            return op.clone();
        }
        if let Some(tn) = &loc.type_name
            && !tn.is_empty()
        {
            if let Some(field) = &loc.field_name
                && !field.is_empty()
            {
                return format!("{tn} {field}");
            }
            return tn.clone();
        }
        if let Some(method) = &loc.method {
            // last resort: method alone is too broad; prefer entity name
            let _ = method;
        }
    }
    entity.name.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::ContractIndex;
    use crate::core::Store;
    use crate::models::ModelClient;
    use crate::models::ReplayClient;
    use std::fs;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn fixture_path(rel: &str) -> String {
        format!("{}/{}", env!("CARGO_MANIFEST_DIR"), rel)
    }

    struct CountingClient {
        inner: ReplayClient,
        calls: Arc<AtomicUsize>,
    }

    impl ModelClient for CountingClient {
        fn complete(
            &self,
            request: &crate::models::ModelRequest,
        ) -> Result<crate::models::ModelResponse, crate::models::ModelError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.inner.complete(request)
        }
    }

    fn replay_dir_for_cases(cases: &[&str]) -> (std::path::PathBuf, Arc<AtomicUsize>) {
        use crate::models::request_key;
        let dir = std::env::temp_dir().join(format!(
            "doyle-cur-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        for case in cases {
            let raw = fs::read_to_string(format!(
                "{}/testdata/replies/curator/{}-baseten.json",
                env!("CARGO_MANIFEST_DIR"),
                case
            ))
            .unwrap();
            let record: serde_json::Value = serde_json::from_str(&raw).unwrap();
            let req = &record["request"];
            let budget = &req["budget"];
            let request = crate::models::ModelRequest {
                model: req["model"].as_str().unwrap().to_string(),
                system: req["system"].as_str().unwrap().to_string(),
                user: req["user"].as_str().unwrap().to_string(),
                schema: req["schema"].clone(),
                budget: crate::models::Budget {
                    max_output_tokens: budget["max_output_tokens"].as_u64().map(|n| n as u32),
                    max_cost_usd: budget["max_cost_usd"].as_f64(),
                },
            };
            let key = request_key(&request);
            let response: crate::models::ModelResponse =
                serde_json::from_value(record["response"].clone()).unwrap();
            fs::write(
                dir.join(format!("{key}.json")),
                serde_json::to_string_pretty(&response).unwrap(),
            )
            .unwrap();
        }
        (dir, Arc::new(AtomicUsize::new(0)))
    }

    #[test]
    fn curates_fixtures_and_second_run_no_calls() {
        use crate::adapters::muse_code::MuseCodeAdapter;
        // Muse Code adapter:
        let tmp = std::env::temp_dir().join(format!("mc-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::copy(
            format!(
                "{}/testdata/memories/muse-code/MEMORY.md",
                env!("CARGO_MANIFEST_DIR")
            ),
            tmp.join("MEMORY.md"),
        )
        .unwrap();
        fs::copy(
            format!(
                "{}/testdata/memories/muse-code/patients-stats.md",
                env!("CARGO_MANIFEST_DIR")
            ),
            tmp.join("patients-stats.md"),
        )
        .unwrap();
        let mc = MuseCodeAdapter::new(&tmp);

        // For this test, only Muse Code adapter.
        let adapters: Vec<Box<dyn MemoryAdapter>> = vec![Box::new(mc)];
        let index =
            ContractIndex::load(fixture_path("testdata/contracts/patients-v1.yaml")).unwrap();
        let store_dir = std::env::temp_dir().join(format!("store-{}", std::process::id()));
        let _ = fs::remove_dir_all(&store_dir);
        let mut store = Store::open(&store_dir).unwrap();

        let (rdir, counter) = replay_dir_for_cases(&["M1", "M2", "M3", "M4"]);
        let replay = ReplayClient::new(&rdir);
        let counting = CountingClient {
            inner: replay,
            calls: counter.clone(),
        };

        let items = curate(&adapters, &counting, &index, &mut store, false).unwrap();
        // Expect 4 memories cited
        assert_eq!(items.len(), 4, "got {} items", items.len());
        // second run
        let calls_before = counter.load(Ordering::SeqCst);
        let items2 = curate(&adapters, &counting, &index, &mut store, false).unwrap();
        assert!(items2.is_empty(), "second run should skip all");
        assert_eq!(
            counter.load(Ordering::SeqCst),
            calls_before,
            "second run made model calls"
        );

        let _ = fs::remove_dir_all(&tmp);
        let _ = fs::remove_dir_all(&store_dir);
        let _ = fs::remove_dir_all(&rdir);
    }

    #[test]
    fn curates_lambo_scripted() {
        use crate::adapters::lambo::LamboAdapter;
        use crate::mcp::{FakeTransport, HttpResponse, TransportError};
        use std::sync::Mutex;

        // scripted server returning 4 concepts for any recall
        let state = Arc::new(Mutex::new((0u32, Vec::<(String, String)>::new())));
        // seed concepts
        {
            let mut st = state.lock().unwrap();
            for (i, text) in [
                "GET /patients returns every patient in one response; one call exports the whole clinic",
                "GET /patients/{id} returns 404 for an unknown patient id",
                "GET /stats patient_count is the total number of patients",
                "POST /admin/decommission is permanent once confirmed; there is no undo",
            ]
            .iter()
            .enumerate()
            {
                st.1.push((format!("{:08x}-0000-0000-0000-000000000000", i + 1), text.to_string()));
            }
        }
        let st = state.clone();
        let transport = FakeTransport::scripted(move |_url: &str, body: &str| {
            let v: serde_json::Value =
                serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
            let method = v.get("method").and_then(|m| m.as_str()).unwrap_or("");
            if method == "initialize" {
                let msg = serde_json::json!({"jsonrpc":"2.0","id": v.get("id"),"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"lambo","version":"0.8.0"},"instructions":"x"}});
                return Ok(HttpResponse {
                    status: 200,
                    headers: vec![("Mcp-Session-Id".to_string(), "<mcp-session-id>".to_string())],
                    body: format!("id: 0\nretry: 3000\ndata: {}\n\n", msg),
                });
            }
            if method == "notifications/initialized" {
                return Ok(HttpResponse {
                    status: 202,
                    headers: vec![],
                    body: String::new(),
                });
            }
            if method != "tools/call" {
                return Err(TransportError {
                    url: _url.to_string(),
                    message: "unknown".to_string(),
                });
            }
            let params = v.get("params").cloned().unwrap_or(serde_json::Value::Null);
            let tool = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let rid = v.get("id").cloned().unwrap_or(serde_json::Value::Null);
            let st = st.lock().unwrap();
            match tool {
                "lambo_recall" => {
                    let hits: Vec<serde_json::Value> = st.1.iter().map(|(cid, content)| serde_json::json!({"node_id": cid, "content": content, "concept_type":"Logic", "score":1.0, "is_canonical":false, "blast_radius":null})).collect();
                    let msg = serde_json::json!({"jsonrpc":"2.0","id": rid, "result": {"content":[{"type":"text","text":"ctx"}], "structuredContent":{"context":"ctx", "hits": hits, "warnings":[]}, "isError": false}});
                    Ok(HttpResponse {
                        status: 200,
                        headers: vec![],
                        body: format!("id: 0\nretry: 3000\ndata: {}\n\n", msg),
                    })
                }
                "lambo_inspect" => {
                    let focus = args.get("focus").and_then(|x| x.as_str()).unwrap_or("");
                    let found =
                        st.1.iter()
                            .find(|(cid, _)| cid == focus || cid.starts_with(focus));
                    let (label, node_id) = match found {
                        Some((cid, content)) => (format!("{} [Logic]", content), cid.clone()),
                        None => (String::new(), String::new()),
                    };
                    let msg = serde_json::json!({"jsonrpc":"2.0","id": rid, "result": {"content":[{"type":"text","text": format!("focus: {}", label)}], "structuredContent":{"focus":{"label": label, "node_id": node_id}, "warnings":[]}, "isError": false}});
                    Ok(HttpResponse {
                        status: 200,
                        headers: vec![],
                        body: format!("id: 0\nretry: 3000\ndata: {}\n\n", msg),
                    })
                }
                _ => Err(TransportError {
                    url: _url.to_string(),
                    message: format!("unknown tool {}", tool),
                }),
            }
        });

        let adapter = LamboAdapter::new(
            "http://127.0.0.1:7700/mcp".to_string(),
            "doyle-probe".to_string(),
            Box::new(transport),
        );
        let adapters: Vec<Box<dyn MemoryAdapter>> = vec![Box::new(adapter)];
        let index =
            ContractIndex::load(fixture_path("testdata/contracts/patients-v1.yaml")).unwrap();
        let store_dir = std::env::temp_dir().join(format!("store-lambo-{}", std::process::id()));
        let _ = fs::remove_dir_all(&store_dir);
        let mut store = Store::open(&store_dir).unwrap();

        let (rdir, _counter) = replay_dir_for_cases(&["M1", "M2", "M3", "M4"]);
        let replay = ReplayClient::new(&rdir);
        // wrap in counting not needed
        let items = curate(&adapters, &replay, &index, &mut store, false).unwrap();
        assert_eq!(items.len(), 4, "got {} items", items.len());

        let _ = fs::remove_dir_all(&store_dir);
        let _ = fs::remove_dir_all(&rdir);
    }
}
