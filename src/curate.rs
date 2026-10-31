//! The `curate` command: read memories through the adapters, have the curator
//! cite each against the current contract, write citations.
//!
//! For each configured adapter, list memories (one search per contract
//! element, so recall-backed adapters stay honest), plus one unfiltered
//! (empty-query) pass so token-disjoint memories are not silently skipped;
//! skip those already cited at their current text, run the curator against
//! the current contract for the rest, and write citations. A memory the
//! curator ties to nothing is recorded as a citation with no elements, so it
//! is not asked again until its text changes. `--dry-run` prints what would
//! be cited and writes nothing.
//!
//! Inherited limitation: Lambo's `lambo_recall` truncates at `top_k` 50, so
//! a session holding more than 50 concepts can miss candidates; see the
//! adapter docs (`docs/lambo.md`).
//!
//! Fixture runs are harness-only by default: without `--contract` the binary
//! reads the demo v1 contract, without `--project` the Muse Code adapter
//! roots at the cwd, and without `--live` the model client replays from
//! `DOYLE_REPLAY_DIR` (or fails closed with no transport configured). `--live`
//! swaps in the Baseten curator client over
//! [`crate::models::http::UreqTransport`], resolving the key through
//! [`crate::models::keys`] (environment, then keyring); `--dry-run` still
//! writes nothing either way.

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

/// The demo v1 contract `run` curates against unless `--contract` says
/// otherwise.
pub const DEFAULT_CONTRACT: &str = "testdata/contracts/patients-v1.yaml";

/// Run the `curate` command.
pub fn run(cli: &cli::Cli) -> ExitCode {
    let dry_run = cli.common.dry_run;
    let live = cli.common.live;

    let contract_path = cli
        .common
        .contract
        .clone()
        .unwrap_or_else(|| DEFAULT_CONTRACT.to_string());
    let index = match ContractIndex::load(&contract_path) {
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
    // The Muse Code adapter roots at --project (default: the cwd), so the
    // binary can run from anywhere while curating another checkout's notes.
    let project = cli
        .common
        .project
        .clone()
        .unwrap_or_else(|| String::from("."));
    for name in &names {
        match name.as_str() {
            "muse-code" | "muse_code" | "musecode" => {
                adapters.push(Box::new(MuseCodeAdapter::new(Path::new(&project))));
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

    // Model client: live Baseten over ureq with --live, otherwise replay
    // from DOYLE_REPLAY_DIR when given, otherwise a Baseten client that
    // will fail without a key (CLI runs are not the offline gate's concern).
    // Without --live the behaviour is exactly what it was: no key needed.
    let model: Box<dyn ModelClient> = match model_for_run(live) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("doyle curate: {e}");
            return ExitCode::from(2);
        }
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

/// Build the model client for a `run`: live Baseten over ureq with `--live`,
/// otherwise the replay/failing transports exactly as before (no key needed,
/// no behaviour change).
fn model_for_run(live: bool) -> Result<Box<dyn ModelClient>, String> {
    if live {
        return live_client();
    }
    if let Ok(dir) = std::env::var("DOYLE_REPLAY_DIR") {
        return Ok(Box::new(ReplayClient::new(dir)));
    }
    // A transport that always errors; keeps the binary key-free at rest.
    struct FailingTransport;
    impl crate::models::baseten::HttpTransport for FailingTransport {
        fn post(
            &self,
            _url: &str,
            _headers: &[(&str, &str)],
            _body: &str,
        ) -> Result<crate::models::baseten::HttpResponse, crate::models::baseten::HttpTransportError>
        {
            Err(crate::models::baseten::HttpTransportError {
                message: String::from("no transport configured"),
            })
        }
    }
    Ok(Box::new(BasetenModelClient::new(Box::new(
        FailingTransport,
    ))))
}

/// A live Baseten curator client. The key resolves through
/// [`crate::models::keys`] (environment, then keyring); the client seam
/// reads its key from the environment at call time, so a keyring-resolved
/// key is planted in the process environment here — the only bridge that
/// leaves `meta.rs`/`baseten.rs` untouched. Planting a key that already came
/// from the environment is a no-op value-wise.
fn live_client() -> Result<Box<dyn ModelClient>, String> {
    if std::env::var(BasetenModelClient::DEFAULT_ENV_VAR).is_err() {
        let key = crate::models::keys::baseten_key().map_err(|e| e.to_string())?;
        // SAFETY: the CLI's live path runs before any thread spawns, and the
        // client reads the process environment at call time.
        unsafe {
            std::env::set_var(BasetenModelClient::DEFAULT_ENV_VAR, key);
        }
    }
    Ok(Box::new(BasetenModelClient::new(Box::new(
        crate::models::http::UreqTransport::new(),
    ))))
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
        // Collect candidate memories: one search per contract element, plus one
        // unfiltered pass so enumerable adapters surface token-disjoint
        // memories (empty query returns everything per the adapter contract).
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
        // Unfiltered pass: enumerable adapters return everything; recall-backed
        // adapters error on empty query and are skipped.
        match adapter.list("") {
            Ok(mems) => {
                for m in mems {
                    if seen.insert(m.reference.clone()) {
                        candidates.push(m);
                    }
                }
            }
            Err(_) => {
                // Recall-backed adapter has no match-all query; skip.
            }
        }

        for memory in candidates {
            // Skip memories already cited at their current text.
            let existing = store.citations_for_memory(&memory.reference);
            if existing.iter().any(|c| c.source_text == memory.text) {
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
                        claim: claim.clone(),
                        recorded_by: CURATOR_MODEL.to_string(),
                        confidence,
                        source_text: memory.text.clone(),
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
                        claim: claim.clone(),
                        recorded_by: CURATOR_MODEL.to_string(),
                        confidence: 0.0,
                        source_text: memory.text.clone(),
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
    use std::sync::Mutex;
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

    struct FixedClient {
        calls: Arc<AtomicUsize>,
    }

    impl ModelClient for FixedClient {
        fn complete(
            &self,
            _request: &crate::models::ModelRequest,
        ) -> Result<crate::models::ModelResponse, crate::models::ModelError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(crate::models::ModelResponse {
                value: serde_json::json!({
                    "element_ids": [],
                    "claim": "rests on nothing",
                    "confidence": 0.0,
                    "rests_on_nothing": true
                }),
                usage: crate::models::Usage::NONE,
            })
        }
    }

    struct MutableAdapter {
        name: String,
        text: Arc<Mutex<String>>,
        id: String,
    }

    impl MemoryAdapter for MutableAdapter {
        fn name(&self) -> &str {
            &self.name
        }
        fn capabilities(&self) -> crate::adapters::Capabilities {
            crate::adapters::Capabilities::NONE
        }
        fn list(&self, query: &str) -> Result<Vec<Memory>, crate::adapters::AdapterError> {
            let text = self.text.lock().unwrap().clone();
            let mem = Memory {
                reference: MemoryRef {
                    adapter: self.name.clone(),
                    id: self.id.clone(),
                },
                text: text.clone(),
                updated_at: None,
                scope: "s".to_string(),
            };
            if query.trim().is_empty() || text.to_lowercase().contains(&query.to_lowercase()) {
                Ok(vec![mem])
            } else {
                Ok(vec![])
            }
        }
        fn read(&self, id: &str) -> Result<Memory, crate::adapters::AdapterError> {
            if id != self.id {
                return Err(crate::adapters::AdapterError::not_found(&self.name, id));
            }
            let text = self.text.lock().unwrap().clone();
            Ok(Memory {
                reference: MemoryRef {
                    adapter: self.name.clone(),
                    id: self.id.clone(),
                },
                text,
                updated_at: None,
                scope: "s".to_string(),
            })
        }
        fn write(
            &self,
            _text: &str,
            _scope: &str,
        ) -> Result<MemoryRef, crate::adapters::AdapterError> {
            Err(crate::adapters::AdapterError::unsupported(
                &self.name, "write",
            ))
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
                media: Vec::new(),
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
        // First run must have called the model.
        assert!(
            counter.load(Ordering::SeqCst) > 0,
            "first run made no model calls"
        );
        // Per-memory element sets: exact for M1/M2, subset of
        // expected ∪ also_accepted for M3/M4.
        let list_op = "ve2_c81e37b7c42c484a1859c09e95bf8cec89a360db6743a691afab818f177c7bc6";
        let list_200 = "ve2_86d3cdb7ce3a7f0b1a2fefa696f0acbbcf2d5406c717bb1f1f1cb5f4f62667ba";
        let get_op = "ve2_c69ae2bf6960e6f28e01bf913a93aa0a325f6d3ee49d445a68b1f580f27af2b5";
        let get_404 = "ve2_6add08d53def059f4e973257dc9e174a5398208bce2fa4d719e0418ca523349e";
        let stats_op = "ve2_88f1296b143cdad99952319ae783095b3386438f0c8932538d9bfbe1b7fcfb0c";
        let patient_count = "ve2_99aaf77708e08db6ee0c7bd151e3044ef33e089e1bbe915fe889e7330b79058f";
        let stats_200 = "ve2_6c3962f4033a5832c062e5d2bce8458ec018d67d8ff15cdbf109543d63662c5c";
        let stats_type = "ve2_c09c07ec8983f09afc5dafed67ee0e3e0d271de3b8f8f1bd61daa1111f063809";
        let decom_op = "ve2_9e9663295485ddf808651f117c8ca42ab58e33fbe49a27309e0547f77c866846";
        let m3_union: HashSet<String> = [stats_op, patient_count, stats_200, stats_type]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let m4_union: HashSet<String> = [
            decom_op,
            "ve2_961aa3ef3e4dcfb88a106954f24e7b98bbeac77cbbd840ad30138558aa607c9c",
            "ve2_7b2fe814702fee516a298fa7d2dde87c41638d860c96e14885faf599fc26ec24",
            "ve2_e9959823b68116d2498f7d823c14b316b045df638495fe9d496c0f7430538355",
            "ve2_eaa9caa44e2da53743bb372db1b7511946f9903fd9255609357a85e6ab932bfd",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        for item in &items {
            let set: HashSet<String> = item.elements.iter().map(|e| e.0.clone()).collect();
            match item.memory.id.as_str() {
                "MEMORY.md#1" => {
                    let expected: HashSet<String> =
                        [list_op, list_200].iter().map(|s| s.to_string()).collect();
                    assert_eq!(set, expected, "M1 element set mismatch");
                }
                "MEMORY.md#2" => {
                    let expected: HashSet<String> =
                        [get_op, get_404].iter().map(|s| s.to_string()).collect();
                    assert_eq!(set, expected, "M2 element set mismatch");
                }
                "patients-stats.md#1" => {
                    assert!(set.is_subset(&m3_union), "M3 not subset: {set:?}");
                    assert!(!set.is_empty(), "M3 empty");
                }
                "MEMORY.md#3" => {
                    assert!(set.is_subset(&m4_union), "M4 not subset: {set:?}");
                    assert!(!set.is_empty(), "M4 empty");
                }
                other => panic!("unexpected memory id {other}"),
            }
        }
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

    fn temp_store_dir(label: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "doyle-cur-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn token_disjoint_note_recorded_as_rests_on_nothing() {
        use crate::adapters::FakeAdapter;
        let adapter = FakeAdapter::new("fake");
        adapter
            .write("zzqq unique phrase unrelated to any contract", "scope")
            .unwrap();
        let adapters: Vec<Box<dyn MemoryAdapter>> = vec![Box::new(adapter)];
        let index =
            ContractIndex::load(fixture_path("testdata/contracts/patients-v1.yaml")).unwrap();
        let store_dir = temp_store_dir("disjoint");
        let mut store = Store::open(&store_dir).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let fixed = FixedClient {
            calls: calls.clone(),
        };
        let items = curate(&adapters, &fixed, &index, &mut store, false).unwrap();
        assert_eq!(items.len(), 1, "disjoint note not curated");
        assert!(items[0].rests_on_nothing);
        assert_eq!(store.citations().len(), 1);
        assert!(store.citations()[0].elements.is_empty());
        let _ = fs::remove_dir_all(&store_dir);
    }

    #[test]
    fn restart_skip_and_text_change_retrigger() {
        let text = Arc::new(Mutex::new(String::from("zzqq unique unrelated note")));
        let adapter = MutableAdapter {
            name: "fake".to_string(),
            text: text.clone(),
            id: "mem-1".to_string(),
        };
        let adapters: Vec<Box<dyn MemoryAdapter>> = vec![Box::new(adapter)];
        let index =
            ContractIndex::load(fixture_path("testdata/contracts/patients-v1.yaml")).unwrap();
        let store_dir = temp_store_dir("restart");
        let calls = Arc::new(AtomicUsize::new(0));
        let fixed = FixedClient {
            calls: calls.clone(),
        };

        {
            let mut store = Store::open(&store_dir).unwrap();
            let items = curate(&adapters, &fixed, &index, &mut store, false).unwrap();
            assert_eq!(items.len(), 1);
        }
        // restart, same text -> skip, no calls
        {
            let mut store = Store::open(&store_dir).unwrap();
            let before = calls.load(Ordering::SeqCst);
            let items = curate(&adapters, &fixed, &index, &mut store, false).unwrap();
            assert!(items.is_empty(), "restart should skip");
            assert_eq!(calls.load(Ordering::SeqCst), before, "restart made calls");
        }
        // text change -> re-trigger
        *text.lock().unwrap() = String::from("changed unrelated note text");
        {
            let mut store = Store::open(&store_dir).unwrap();
            let before = calls.load(Ordering::SeqCst);
            let items = curate(&adapters, &fixed, &index, &mut store, false).unwrap();
            assert_eq!(items.len(), 1, "text change should re-curate");
            assert!(
                calls.load(Ordering::SeqCst) > before,
                "text change made no call"
            );
        }
        let _ = fs::remove_dir_all(&store_dir);
    }

    #[test]
    fn rests_on_nothing_skip_writes_and_contract_change() {
        let text = Arc::new(Mutex::new(String::from("zzqq unique unrelated note")));
        let adapter = MutableAdapter {
            name: "fake".to_string(),
            text: text.clone(),
            id: "mem-1".to_string(),
        };
        let adapters: Vec<Box<dyn MemoryAdapter>> = vec![Box::new(adapter)];
        let index =
            ContractIndex::load(fixture_path("testdata/contracts/patients-v1.yaml")).unwrap();
        let store_dir = temp_store_dir("ron");
        let mut store = Store::open(&store_dir).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let fixed = FixedClient {
            calls: calls.clone(),
        };

        // (a) empty-elements citation recorded
        let items = curate(&adapters, &fixed, &index, &mut store, false).unwrap();
        assert_eq!(items.len(), 1);
        assert!(items[0].rests_on_nothing);
        assert_eq!(store.citations().len(), 1);
        assert!(store.citations()[0].elements.is_empty());

        // (b) second run skips with zero calls and zero new writes (store files)
        let calls_before = calls.load(Ordering::SeqCst);
        let cit_before = store.citations().len();
        let file_before = fs::read_to_string(store_dir.join("citations.jsonl"))
            .unwrap()
            .lines()
            .count();
        let items2 = curate(&adapters, &fixed, &index, &mut store, false).unwrap();
        assert!(items2.is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), calls_before);
        assert_eq!(store.citations().len(), cit_before);
        let file_after = fs::read_to_string(store_dir.join("citations.jsonl"))
            .unwrap()
            .lines()
            .count();
        assert_eq!(file_after, file_before);

        // (c) text change re-triggers curation
        *text.lock().unwrap() = String::from("changed unrelated note text");
        let calls_before2 = calls.load(Ordering::SeqCst);
        let items3 = curate(&adapters, &fixed, &index, &mut store, false).unwrap();
        assert_eq!(items3.len(), 1);
        assert!(calls.load(Ordering::SeqCst) > calls_before2);

        // (d) unrelated contract change does NOT re-trigger (no version key in skip)
        let index2 =
            ContractIndex::load(fixture_path("testdata/contracts/patients-v2.yaml")).unwrap();
        let calls_before3 = calls.load(Ordering::SeqCst);
        let cit_before3 = store.citations().len();
        let file_before3 = fs::read_to_string(store_dir.join("citations.jsonl"))
            .unwrap()
            .lines()
            .count();
        let items4 = curate(&adapters, &fixed, &index2, &mut store, false).unwrap();
        assert!(items4.is_empty(), "contract change should not re-trigger");
        assert_eq!(calls.load(Ordering::SeqCst), calls_before3);
        assert_eq!(store.citations().len(), cit_before3);
        let file_after3 = fs::read_to_string(store_dir.join("citations.jsonl"))
            .unwrap()
            .lines()
            .count();
        assert_eq!(file_after3, file_before3);

        let _ = fs::remove_dir_all(&store_dir);
    }

    #[test]
    fn dry_run_writes_nothing() {
        use crate::adapters::muse_code::MuseCodeAdapter;
        let tmp = std::env::temp_dir().join(format!("mc-dry-{}", std::process::id()));
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
        let adapters: Vec<Box<dyn MemoryAdapter>> = vec![Box::new(mc)];
        let index =
            ContractIndex::load(fixture_path("testdata/contracts/patients-v1.yaml")).unwrap();
        let store_dir = temp_store_dir("dry");
        let mut store = Store::open(&store_dir).unwrap();

        let (rdir, counter) = replay_dir_for_cases(&["M1", "M2", "M3", "M4"]);
        let replay = ReplayClient::new(&rdir);
        let counting = CountingClient {
            inner: replay,
            calls: counter.clone(),
        };

        let items = curate(&adapters, &counting, &index, &mut store, true).unwrap();
        assert_eq!(items.len(), 4);
        assert!(!store_dir.join("citations.jsonl").exists());
        assert!(store.citations().is_empty());

        let _ = fs::remove_dir_all(&tmp);
        let _ = fs::remove_dir_all(&store_dir);
        let _ = fs::remove_dir_all(&rdir);
    }

    #[test]
    fn cli_curate_live_contract_project_parse() {
        let args = vec![
            "curate".to_string(),
            "--live".to_string(),
            "--contract".to_string(),
            "x.yaml".to_string(),
            "--project".to_string(),
            "/tmp/p".to_string(),
        ];
        match crate::cli::parse_from(&args) {
            crate::cli::ParseResult::Run(cli) => {
                assert!(cli.common.live, "live must be set");
                assert_eq!(cli.common.contract.as_deref(), Some("x.yaml"));
                assert_eq!(cli.common.project.as_deref(), Some("/tmp/p"));
            }
            other => panic!("expected Run, got {other:?}"),
        }
    }

    #[test]
    fn cli_curate_contract_equals_form_and_defaults_are_replay() {
        let args = vec!["curate".to_string(), "--contract=x.yaml".to_string()];
        match crate::cli::parse_from(&args) {
            crate::cli::ParseResult::Run(cli) => {
                assert!(!cli.common.live, "live defaults off");
                assert_eq!(cli.common.contract.as_deref(), Some("x.yaml"));
                assert_eq!(cli.common.project, None);
            }
            other => panic!("expected Run, got {other:?}"),
        }
        let args = vec!["curate".to_string()];
        match crate::cli::parse_from(&args) {
            crate::cli::ParseResult::Run(cli) => {
                assert!(!cli.common.live, "live defaults off");
                assert_eq!(cli.common.contract, None);
                assert_eq!(cli.common.project, None);
            }
            other => panic!("expected Run, got {other:?}"),
        }
    }

    #[test]
    fn cli_live_and_contract_are_scoped_to_their_commands() {
        for args in [
            vec!["serve".to_string(), "--live".to_string()],
            vec![
                "check".to_string(),
                "a".to_string(),
                "b".to_string(),
                "--contract".to_string(),
                "x".to_string(),
            ],
            vec![
                "serve".to_string(),
                "--contract".to_string(),
                "x".to_string(),
            ],
            vec!["curate".to_string(), "--contract".to_string()],
            vec![
                "check".to_string(),
                "a".to_string(),
                "b".to_string(),
                "--project".to_string(),
            ],
        ] {
            match crate::cli::parse_from(&args) {
                crate::cli::ParseResult::Error(_) => {}
                other => panic!("expected Error for {args:?}, got {other:?}"),
            }
        }
        // --project is shared: serve and check accept it.
        for args in [
            vec![
                "serve".to_string(),
                "--project".to_string(),
                "/tmp/p".to_string(),
            ],
            vec![
                "check".to_string(),
                "a".to_string(),
                "b".to_string(),
                "--project=/tmp/p".to_string(),
            ],
        ] {
            match crate::cli::parse_from(&args) {
                crate::cli::ParseResult::Run(cli) => {
                    assert_eq!(cli.common.project.as_deref(), Some("/tmp/p"));
                }
                other => panic!("expected Run for {args:?}, got {other:?}"),
            }
        }
    }

    #[test]
    fn replay_default_needs_no_key_and_serves_replay() {
        // Without --live, model_for_run builds the replay client when the
        // replay dir is set (a miss proves it is ReplayClient, not a live
        // client and not the failing transport), and the failing transport
        // otherwise — all with no key in the environment.
        let saved = std::env::var("DOYLE_REPLAY_DIR").ok();
        // SAFETY: no other test reads DOYLE_REPLAY_DIR in-process.
        unsafe {
            std::env::remove_var("DOYLE_REPLAY_DIR");
        }
        let failing = model_for_run(false).expect("no-live without dir must build");
        let request = crate::models::ModelRequest {
            model: "probe".to_string(),
            system: "s".to_string(),
            user: "u".to_string(),
            schema: serde_json::json!({}),
            media: Vec::new(),
            budget: crate::models::Budget::tokens(8),
        };
        // The pre-existing offline failure, unchanged by --live: with no key
        // the env-reading client refuses before touching the transport; with
        // a key in the environment the failing transport errors. Either way
        // no network, no keyring, no replay.
        let err = failing.complete(&request).expect_err("must fail");
        match err {
            crate::models::ModelError::InvalidRequest { message } => {
                assert!(
                    message.contains("BASETEN_API_KEY"),
                    "must name the variable: {message}"
                );
            }
            crate::models::ModelError::Backend { message } => {
                assert!(
                    message.contains("no transport configured"),
                    "unexpected: {message}"
                );
            }
            other => panic!("unexpected error kind: {other:?}"),
        }

        let dir = temp_store_dir("replay-default");
        // SAFETY: no other test reads DOYLE_REPLAY_DIR in-process.
        unsafe {
            std::env::set_var("DOYLE_REPLAY_DIR", &dir);
        }
        let replay = model_for_run(false).expect("no-live with dir must build");
        let err = replay.complete(&request).expect_err("empty dir must miss");
        match err {
            crate::models::ModelError::ReplayMiss { .. } => {}
            other => panic!("expected ReplayMiss, got {other:?}"),
        }

        // Restore.
        // SAFETY: restoring the variable this test owns for its duration.
        unsafe {
            match saved {
                Some(v) => std::env::set_var("DOYLE_REPLAY_DIR", v),
                None => std::env::remove_var("DOYLE_REPLAY_DIR"),
            }
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
