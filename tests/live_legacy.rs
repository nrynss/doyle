//! Live test for T9.11b: curate and re-check the story's legacy and
//! screen memories against the real models, and record every reply as a
//! replay fixture for the gate.
//!
//! Six curate calls on Muse Spark Open (Baseten): L1–L3 against the v1
//! batch, L4 against v2, S1–S2 against the v1 console. Three re-check
//! calls on Muse Spark (Meta): L1 through the differential run, L2 through
//! the legacy diff, S1 through the screen diff.
//!
//! This test is `#[ignore]`d: it calls real APIs, needs keys, and never
//! runs in CI or the gate. Run it explicitly with the keys in the
//! environment:
//!
//! ```sh
//! BASETEN_API_KEY="$(secret-tool lookup service baseten)" \
//! MODEL_API_KEY="$(secret-tool lookup service meta-model-api)" \
//!     cargo test --test live_legacy -- --ignored live_ --nocapture
//! ```
//!
//! Keys are read from the environment at call time and are never printed,
//! logged, or written to any file. No key enters any recording: requests
//! carry only prompts and schemas. Curate case files land in the temp dir
//! printed as `case_dir`; re-check replies land keyed by request in the
//! temp dir printed as `recheck_dir` (that layout commits directly under
//! `testdata/replies/recheck/`). Copy both into `testdata/replies/` after
//! a green run so the gate replays them offline.
//!
//! Budget: $0.50 TOTAL across both models. Neither API returns a dollar
//! cost, so spend is estimated from input + output tokens on the T9.2/T9.7
//! basis (~$15 per million tokens). The cheapest curate call (L1) runs and
//! is checked first; the total is asserted at the end.
//!
//! To re-run after a prompt change without re-spending the unchanged
//! kind: `DOYLE_LIVE_REUSE_SCREENS=<dir>` reuses that dir's keyed S1/S2
//! replies (same requests, same keys, no model calls) instead of calling
//! Baseten again. `DOYLE_LIVE_SKIP_CURATE=1` runs rechecks only.
//! `DOYLE_LIVE_SKIP_RECHECK=1` runs every recheck input except S1 offline
//! (for runs where only S1's guidance changed). The transcript says when
//! reuse or skipping happens.

use std::cell::Cell;

use doyle_tms::adapters::FakeAdapter;
use doyle_tms::adapters::MemoryAdapter;
use doyle_tms::core::{Citation, ElementId, MemoryRef, flag_citations};
use doyle_tms::evidence::EvidenceSource;
use doyle_tms::evidence::diffrun::{flag_diffrun, run_diffrun_with_paths};
use doyle_tms::evidence::legacy::LegacyIndex;
use doyle_tms::evidence::legacy::ids as legacy_ids;
use doyle_tms::evidence::screens::ScreenIndex;
use doyle_tms::evidence::screens::ids as screen_ids;
use doyle_tms::models::baseten::{
    BasetenModelClient, HttpResponse as BasetenResponse, HttpTransport as BasetenTransport,
    HttpTransportError as BasetenTransportError,
};
use doyle_tms::models::meta::{
    HttpResponse as MetaResponse, HttpTransport as MetaTransport,
    HttpTransportError as MetaTransportError, MetaModelClient,
};
use doyle_tms::models::{ModelClient, ModelRequest, ModelResponse, RecordingClient, request_key};
use doyle_tms::prompts::curator::{self, CuratorVerdict};
use doyle_tms::prompts::recheck;

/// Minimal ureq-backed transport for the live calls, in this test only.
/// Non-2xx statuses are returned as data so the clients' own error logic
/// sees the real wire shape. Nothing sensitive is logged.
struct UreqTransport {
    agent: ureq::Agent,
}

impl UreqTransport {
    fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(std::time::Duration::from_secs(300)))
            .build();
        Self {
            agent: ureq::Agent::new_with_config(config),
        }
    }

    fn post_json(&self, url: &str, headers: &[(&str, &str)], body: &str) -> (u16, String) {
        let mut request = self.agent.post(url);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let response = request
            .header("Content-Type", "application/json")
            .send(body)
            .expect("live transport must send");
        let status = response.status().as_u16();
        let text = response
            .into_body()
            .read_to_string()
            .expect("live transport must read");
        (status, text)
    }
}

impl BasetenTransport for UreqTransport {
    fn post(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> Result<BasetenResponse, BasetenTransportError> {
        let (status, body) = self.post_json(url, headers, body);
        Ok(BasetenResponse { status, body })
    }
}

impl MetaTransport for UreqTransport {
    fn post(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> Result<MetaResponse, MetaTransportError> {
        let (status, body) = self.post_json(url, headers, body);
        Ok(MetaResponse { status, body })
    }
}

/// A model client that retries once with the fallback when the primary
/// errors or returns a null reply (the T9.3 starvation shape). Counts
/// retries for the transcript.
struct RetryClient<'a> {
    primary: &'a dyn ModelClient,
    fallback: &'a dyn ModelClient,
    retries: Cell<usize>,
}

impl<'a> ModelClient for RetryClient<'a> {
    fn complete(
        &self,
        request: &ModelRequest,
    ) -> Result<ModelResponse, doyle_tms::models::ModelError> {
        match self.primary.complete(request) {
            Ok(response) if !response.value.is_null() => Ok(response),
            _ => {
                self.retries.set(self.retries.get() + 1);
                self.fallback.complete(request)
            }
        }
    }
}

/// Dollar estimate on the T9.2 basis (~$15 per million tokens).
fn estimate_usd(input: Option<u64>, output: Option<u64>) -> f64 {
    (input.unwrap_or(0) + output.unwrap_or(0)) as f64 / 1_000_000.0 * 15.0
}

fn story(rel: &str) -> String {
    format!("{}/testdata/legacy/story/{rel}", env!("CARGO_MANIFEST_DIR"))
}

/// The story's memory texts, read from the fixture (provenance.json holds
/// the same texts the Muse Code notes carry), keyed by memory id.
fn memory_texts() -> std::collections::HashMap<String, String> {
    let raw = std::fs::read_to_string(story("memories/provenance.json")).expect("provenance reads");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("provenance parses");
    value["memories"]
        .as_array()
        .expect("memories")
        .iter()
        .map(|m| {
            (
                m["id"].as_str().expect("id").to_string(),
                m["text"].as_str().expect("text").to_string(),
            )
        })
        .collect()
}

fn para(name: &str) -> ElementId {
    legacy_ids::element_id(&legacy_ids::paragraph_key("BILLBATCH", name))
}

fn field(path: &str) -> ElementId {
    legacy_ids::element_id(&legacy_ids::field_key("BILLBATCH", path))
}

/// The accepted L1 citation: the fee path plus the exemption branch, minus
/// VISIT-CODE. All four live draws missed the field; the user accepted the
/// 3-element set as flag-algebra-perfect (diffrun flags via LOG-EXEMPT,
/// legacy silent). Proof of record is the replayed pin
/// `replayed_l1_cites_fee_path_and_exemption_branch` in
/// src/prompts/curator.rs, not this live gate.
fn accepted_l1_elements() -> Vec<ElementId> {
    vec![
        para("CALC-LATE-FEE"),
        field("PATIENT-RECORD.BALANCE"),
        para("LOG-EXEMPT"),
    ]
}

/// The story's citation element sets (T9.4 README, `owns` the truth):
/// recomputed from the canonical keys, never copied.
fn story_elements(id: &str) -> Vec<ElementId> {
    match id {
        "L1" => vec![
            para("CALC-LATE-FEE"),
            field("PATIENT-RECORD.BALANCE"),
            para("LOG-EXEMPT"),
            field("PATIENT-RECORD.VISIT-CODE"),
        ],
        "L2" => vec![
            legacy_ids::element_id(&legacy_ids::record_key("BILLBATCH", "PATIENT-RECORD")),
            field("PATIENT-RECORD.ACCT-NO"),
            field("PATIENT-RECORD.ACCT-NAME"),
            field("PATIENT-RECORD.BALANCE"),
            field("PATIENT-RECORD.VISIT-CODE"),
        ],
        "L3" => vec![para("CALC-LATE-FEE"), field("PATIENT-RECORD.BALANCE")],
        "L4" => vec![
            para("APPLY-PROMPT-DISCOUNT"),
            field("PATIENT-RECORD.DISCOUNT-FLAG"),
        ],
        _ => panic!("no story set for {id}"),
    }
}

fn cite(id: &str, texts: &std::collections::HashMap<String, String>) -> Citation {
    Citation {
        memory: MemoryRef {
            adapter: "muse-code".to_string(),
            id: id.to_string(),
        },
        elements: story_elements(id),
        claim: texts[id].clone(),
        recorded_by: "live-curator".to_string(),
        confidence: 0.9,
        source_text: texts[id].clone(),
    }
}

/// The screen citations: the screen plus the training steps that
/// demonstrate it, derived from the index (never hand-copied).
fn screen_cite(
    id: &str,
    screen: &str,
    v1: &ScreenIndex,
    texts: &std::collections::HashMap<String, String>,
) -> Citation {
    let screen_id = screen_ids::screen_id("fernwhistle", screen);
    let mut elements = vec![screen_id.clone()];
    let mut steps: Vec<ElementId> = v1
        .elements()
        .into_iter()
        .filter(|e| {
            e.kind == doyle_tms::evidence::EvidenceElementKind::Other("step".to_string())
                && e.references.contains(&screen_id)
        })
        .map(|e| e.id)
        .collect();
    steps.sort_by(|a, b| a.0.cmp(&b.0));
    elements.extend(steps);
    Citation {
        memory: MemoryRef {
            adapter: "muse-code".to_string(),
            id: id.to_string(),
        },
        elements,
        claim: texts[id].clone(),
        recorded_by: "live-curator".to_string(),
        confidence: 0.9,
        source_text: texts[id].clone(),
    }
}

/// Write the combined screens version files the check screens source
/// reads: v1 carries the training steps and recording, v2 is stills
/// only. Built from the fixtures the way the story e2e builds them.
fn write_screen_versions(dir: &std::path::Path) -> (String, String) {
    let screen_doc = |file: &str| -> serde_json::Value {
        let value: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(story(&format!("screens/{file}.json"))).unwrap(),
        )
        .unwrap();
        serde_json::json!({
            "name": value["screen"],
            "elements": value["elements"],
        })
    };
    let steps_value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(story("training-v1-steps.json")).unwrap())
            .unwrap();
    let steps: Vec<serde_json::Value> = steps_value["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let screen = match s["id"].as_str().unwrap() {
                "T2" | "T3" | "T4" => "lookup",
                "T5" | "T6" => "decommission",
                _ => "",
            };
            serde_json::json!({
                "description": s["description"],
                "start_s": s["start_s"],
                "end_s": s["end_s"],
                "screen": screen,
            })
        })
        .collect();
    let v1 = serde_json::json!({
        "console": "fernwhistle",
        "screens": [screen_doc("lookup-v1"), screen_doc("decomm-v1")],
        "steps": steps,
        "recording": {"name": "training-v1", "duration_s": 45.0},
    });
    let v2 = serde_json::json!({
        "console": "fernwhistle",
        "screens": [screen_doc("lookup-v2"), screen_doc("decomm-v2")],
        "steps": [],
    });
    let v1_path = dir.join("screens-v1.json");
    let v2_path = dir.join("screens-v2.json");
    std::fs::write(&v1_path, serde_json::to_string_pretty(&v1).unwrap()).unwrap();
    std::fs::write(&v2_path, serde_json::to_string_pretty(&v2).unwrap()).unwrap();
    (
        v1_path.to_string_lossy().to_string(),
        v2_path.to_string_lossy().to_string(),
    )
}

fn print_usage(label: &str, usage: &doyle_tms::models::Usage) -> f64 {
    let usd = estimate_usd(usage.input_tokens, usage.output_tokens);
    eprintln!(
        "{label}: input_tokens={:?} output_tokens={:?} cost_usd={:?} (~${usd:.4} on the $15/MTok basis)",
        usage.input_tokens, usage.output_tokens, usage.cost_usd
    );
    usd
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "doyle-live-legacy-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// One curate and one re-check per kind on the fixtures: L1–L4 and S1–S2
/// cite through the Baseten curator, then L1 (diffrun), L2 (legacy) and
/// S1 (screens) propose corrections through Muse Spark.
#[test]
#[ignore]
fn live_legacy_curate_and_recheck() {
    assert!(
        std::env::var(BasetenModelClient::DEFAULT_ENV_VAR).is_ok(),
        "set {} in the environment",
        BasetenModelClient::DEFAULT_ENV_VAR
    );
    assert!(
        std::env::var(MetaModelClient::DEFAULT_ENV_VAR).is_ok(),
        "set {} in the environment",
        MetaModelClient::DEFAULT_ENV_VAR
    );

    let texts = memory_texts();
    let v1 = LegacyIndex::load(story("v1/BILLBATCH.cbl")).expect("v1 loads");
    let v2 = LegacyIndex::load(story("v2/BILLBATCH.cbl")).expect("v2 loads");
    let screens_dir = temp_dir("screens");
    let (screens_v1_path, screens_v2_path) = write_screen_versions(&screens_dir);
    let (screens_v1, _) =
        doyle_tms::check::load_screen_version(&screens_v1_path).expect("screens v1 loads");
    let (screens_v2, _) =
        doyle_tms::check::load_screen_version(&screens_v2_path).expect("screens v2 loads");
    let screens_v1_frames =
        doyle_tms::check::load_screen_frames(&screens_v1_path).expect("frames load");
    let screens_v2_frames =
        doyle_tms::check::load_screen_frames(&screens_v2_path).expect("frames load");

    let rec_dir = temp_dir("replies");
    let case_dir = temp_dir("cases");
    let baseten = BasetenModelClient::new(Box::new(UreqTransport::new()));
    let baseten_low =
        BasetenModelClient::new(Box::new(UreqTransport::new())).with_reasoning_effort("low");
    let recorder = RecordingClient::new(baseten, &rec_dir);
    let recorder_low = RecordingClient::new(baseten_low, &rec_dir);
    let retry = RetryClient {
        primary: &recorder,
        fallback: &recorder_low,
        retries: Cell::new(0),
    };

    // Cheapest story call first (L1 vs v1): compared against the T4.2
    // handoff figures (~1800 in / ~1200 out on Baseten) before the rest
    // of the budget is spent.
    //
    // `DOYLE_LIVE_SKIP_CURATE=1` skips every curate call (and the L1 gate
    // with it) for a rechecks-only run: the rechecks use story citations
    // built in-test, never curate outputs.
    // `DOYLE_LIVE_SKIP_L1=1` skips the L1 curate call and its gate (for
    // re-runs after L1's citation already recorded); L1's case file is
    // assembled offline from that run's recording.
    let mut total_usd = 0.0;
    let mut responses: Vec<(String, ModelRequest, ModelResponse)> = Vec::new();
    let skip_curate = std::env::var("DOYLE_LIVE_SKIP_CURATE").is_ok();
    let skip_l1 = std::env::var("DOYLE_LIVE_SKIP_L1").is_ok();
    if !skip_curate && !skip_l1 {
        let l1_request = curator::build_legacy_curator_request(
            &texts["L1"],
            &v1,
            curator::BASETEN_CURATOR_MODEL,
        );
        let l1_response = retry.complete(&l1_request).expect("L1 curate must succeed");
        total_usd += print_usage("curate L1", &l1_response.usage);
        assert!(
            total_usd < 0.25,
            "stop: L1 alone at ~${total_usd:.4} leaves no room for the rest inside $0.50"
        );

        // Fail fast on the story's load-bearing citation: L1 must cite the
        // exemption branch (LOG-EXEMPT, VISIT-CODE) and never the v2-wired
        // dispatcher (CHARGE-ACCOUNT), or the flag algebra breaks (L1 would
        // flag on the legacy wiring and miss the diffrun blame). Stop here,
        // before spending the rest of the budget, if the model draws
        // otherwise.
        {
            let allowed = curator::allowed_legacy_ids(&v1);
            match curator::parse_curator_value(&l1_response.value, &allowed).expect("parses") {
                CuratorVerdict::Cited {
                    elements, claim, ..
                } => {
                    let set: std::collections::HashSet<String> =
                        elements.into_iter().map(|e| e.0).collect();
                    eprintln!("L1 claim: {claim}");
                    for id in &set {
                        eprintln!("L1 cites {id}");
                    }
                    let dispatcher = legacy_ids::element_id(&legacy_ids::paragraph_key(
                        "BILLBATCH",
                        "CHARGE-ACCOUNT",
                    ))
                    .0;
                    assert!(
                        !set.contains(&dispatcher),
                        "L1 must not cite the dispatcher CHARGE-ACCOUNT"
                    );
                    // The v2 wiring and layout facts must stay silent on
                    // L1: neither wiring paragraph nor the record itself.
                    for dangerous in ["CHARGE-ACCOUNT", "WRITE-SUMMARY"] {
                        let id = legacy_ids::element_id(&legacy_ids::paragraph_key(
                            "BILLBATCH",
                            dangerous,
                        ))
                        .0;
                        assert!(!set.contains(&id), "L1 must not cite {dangerous}");
                    }
                    let record = legacy_ids::element_id(&legacy_ids::record_key(
                        "BILLBATCH",
                        "PATIENT-RECORD",
                    ))
                    .0;
                    assert!(!set.contains(&record), "L1 must not cite PATIENT-RECORD");
                    for want in accepted_l1_elements() {
                        assert!(set.contains(&want.0), "L1 must cite {}", want.0);
                    }
                }
                other => panic!("L1: must cite, got {other:?}"),
            }
        }
        // The L1 response joins the case files below.
        responses.push(("L1".to_string(), l1_request, l1_response));
    }
    if !skip_curate {
        // The remaining curate calls through the real curate path (one memory
        // per adapter so each group cites against its own index).
        let run_group =
            |ids: &[&str],
             index: &LegacyIndex,
             out: &mut Vec<(String, ModelRequest, ModelResponse)>| {
                let adapter = FakeAdapter::new("fake");
                let mut refs = Vec::new();
                for id in ids {
                    refs.push(
                        adapter
                            .write(&texts[*id], "s")
                            .expect("fake adapter takes the memory"),
                    );
                }
                let adapters: Vec<Box<dyn MemoryAdapter>> = vec![Box::new(adapter)];
                let mut store =
                    doyle_tms::core::Store::open(temp_dir("store")).expect("store opens");
                let items =
                    doyle_tms::curate::curate_legacy(&adapters, &retry, index, &mut store, true)
                        .expect("curate runs");
                assert_eq!(items.len(), ids.len(), "every memory cites");
                for (id, item) in ids.iter().map(|id| {
                    let reference =
                        refs[ids.iter().position(|other| other == id).expect("known id")].clone();
                    let item = items
                        .iter()
                        .find(|item| item.memory == reference)
                        .expect("one item per memory");
                    (id, item)
                }) {
                    let request = curator::build_legacy_curator_request(
                        &texts[*id],
                        index,
                        curator::BASETEN_CURATOR_MODEL,
                    );
                    let key = request_key(&request);
                    let response: ModelResponse = serde_json::from_slice(
                        &std::fs::read(rec_dir.join(format!("{key}.json")))
                            .expect("recording written"),
                    )
                    .expect("recording parses");
                    assert_eq!(
                        item.elements.len(),
                        match curator::parse_curator_value(
                            &response.value,
                            &curator::allowed_legacy_ids(index)
                        )
                        .expect("parses")
                        {
                            CuratorVerdict::Cited { elements, .. } => elements.len(),
                            CuratorVerdict::RestsOnNothing { .. } => 0,
                        }
                    );
                    out.push((id.to_string(), request, response));
                }
            };
        run_group(&["L2", "L3"], &v1, &mut responses);
        run_group(&["L4"], &v2, &mut responses);

        // Screens curate through the real screens path — or reuse a previous
        // run's keyed replies when the screens prompt is unchanged (same
        // requests, same keys, no model calls).
        let reuse_dir_opt = std::env::var("DOYLE_LIVE_REUSE_SCREENS").ok();
        if let Some(reuse_dir) = &reuse_dir_opt {
            for id in ["S1", "S2"] {
                let request = curator::build_screens_curator_request(
                    &texts[id],
                    &screens_v1,
                    curator::BASETEN_CURATOR_MODEL,
                );
                let key = request_key(&request);
                let response: ModelResponse = serde_json::from_slice(
                    &std::fs::read(std::path::Path::new(&reuse_dir).join(format!("{key}.json")))
                        .expect("reused recording reads"),
                )
                .expect("recording parses");
                eprintln!("reusing {id} from {reuse_dir} (no spend)");
                responses.push((id.to_string(), request, response));
            }
        } else {
            let screens_adapter = FakeAdapter::new("fake");
            let mut screens_refs = Vec::new();
            for id in ["S1", "S2"] {
                screens_refs.push(
                    screens_adapter
                        .write(&texts[id], "s")
                        .expect("fake adapter takes the memory"),
                );
            }
            let screens_adapters: Vec<Box<dyn MemoryAdapter>> = vec![Box::new(screens_adapter)];
            let mut screens_store =
                doyle_tms::core::Store::open(temp_dir("store-s")).expect("store opens");
            let screens_items = doyle_tms::curate::curate_screens(
                &screens_adapters,
                &retry,
                &screens_v1,
                &mut screens_store,
                true,
            )
            .expect("screens curate runs");
            assert_eq!(screens_items.len(), 2, "S1 and S2 cite");
            for (id, item) in ["S1", "S2"].iter().map(|id| {
                let position = ["S1", "S2"]
                    .iter()
                    .position(|other| other == id)
                    .expect("known id");
                let item = screens_items
                    .iter()
                    .find(|item| item.memory == screens_refs[position])
                    .expect("one item per memory");
                (id, item)
            }) {
                let request = curator::build_screens_curator_request(
                    &texts[*id],
                    &screens_v1,
                    curator::BASETEN_CURATOR_MODEL,
                );
                let key = request_key(&request);
                let response: ModelResponse = serde_json::from_slice(
                    &std::fs::read(rec_dir.join(format!("{key}.json"))).expect("recording written"),
                )
                .expect("recording parses");
                assert_eq!(
                    item.elements.len(),
                    match curator::parse_curator_value(
                        &response.value,
                        &curator::allowed_screen_ids(&screens_v1)
                    )
                    .expect("parses")
                    {
                        CuratorVerdict::Cited { elements, .. } => elements.len(),
                        CuratorVerdict::RestsOnNothing { .. } => 0,
                    }
                );
                responses.push((id.to_string(), request, response));
            }
        }

        // Every curate reply cites the story's sets: expected IDs present,
        // nothing outside the offered list (the parser drops forgeries).
        for (id, _request, response) in &responses {
            let allowed = if id.starts_with('S') {
                curator::allowed_screen_ids(&screens_v1)
            } else if *id == "L4" {
                curator::allowed_legacy_ids(&v2)
            } else {
                curator::allowed_legacy_ids(&v1)
            };
            match curator::parse_curator_value(&response.value, &allowed).expect("parses") {
                CuratorVerdict::Cited {
                    elements, claim, ..
                } => {
                    let set: std::collections::HashSet<String> =
                        elements.into_iter().map(|e| e.0).collect();
                    let expected: std::collections::HashSet<String> = if *id == "L1" {
                        // Accepted set, not the story set: see accepted_l1_elements.
                        accepted_l1_elements().into_iter().map(|e| e.0).collect()
                    } else if *id == "S1" {
                        screen_cite("S1", "decommission", &screens_v1, &texts)
                            .elements
                            .into_iter()
                            .map(|e| e.0)
                            .collect()
                    } else if *id == "S2" {
                        screen_cite("S2", "lookup", &screens_v1, &texts)
                            .elements
                            .into_iter()
                            .map(|e| e.0)
                            .collect()
                    } else {
                        story_elements(id).into_iter().map(|e| e.0).collect()
                    };
                    assert!(
                        expected.is_subset(&set),
                        "{id}: missing {:?} (got {:?})",
                        expected.difference(&set).collect::<Vec<_>>(),
                        set
                    );
                    let extra: Vec<&String> = set.difference(&expected).collect();
                    if !extra.is_empty() {
                        eprintln!("note: {id} cites {extra:?} beyond the story set");
                    }
                    assert!(!claim.trim().is_empty(), "{id}: empty claim");
                    if reuse_dir_opt.is_some() && id.starts_with('S') {
                        eprintln!(
                            "reused {id}: no spend (run-1 usage in={:?} out={:?})",
                            response.usage.input_tokens, response.usage.output_tokens
                        );
                    } else {
                        total_usd += print_usage(&format!("curate {id}"), &response.usage);
                    }
                }
                other => panic!("{id}: must cite, got {other:?}"),
            }
        }
        eprintln!("retries so far: {}", retry.retries.get());
        assert!(
            total_usd < 0.40,
            "stop: curate total ~${total_usd:.4} leaves no room for the re-checks inside $0.50"
        );
    } // end skip-curate gate: rechecks below run in every mode

    // Re-checks on Muse Spark: L1 (diffrun), L2 (legacy), S1 (screens).
    let meta = MetaModelClient::new(Box::new(UreqTransport::new()));
    let meta_recorder = RecordingClient::new(meta, &rec_dir);

    let l1_citation = cite("L1", &texts);
    let l2_citation = cite("L2", &texts);
    let s1_citation = screen_cite("S1", "decommission", &screens_v1, &texts);

    let legacy_facts = v1.diff_evidence(&v2).expect("legacy diff works");
    let screens_facts = screens_v1
        .diff_evidence(&screens_v2)
        .expect("screens diff works");

    let l2_flags = flag_citations(
        std::slice::from_ref(&l2_citation),
        &legacy_facts,
        &v1,
        &[],
        "v1/BILLBATCH.cbl",
        "v2/BILLBATCH.cbl",
    );
    assert_eq!(l2_flags.len(), 1, "L2 flags");
    let s1_flags = flag_citations(
        std::slice::from_ref(&s1_citation),
        &screens_facts,
        &screens_v1,
        &[],
        "screens-v1.json",
        "screens-v2.json",
    );
    assert_eq!(s1_flags.len(), 1, "S1 flags");

    let full_paths = doyle_tms::evidence::diffrun::parse_record_paths(
        &std::fs::read_to_string(format!(
            "{}/testdata/legacy/diffrun/full-record-paths.txt",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("record paths read"),
    )
    .expect("record paths parse");
    let outcome = run_diffrun_with_paths(
        &std::fs::read_to_string(story("v1/patients.dat")).expect("inputs read"),
        &std::fs::read_to_string(story("v1/report.txt")).expect("batch report reads"),
        &std::fs::read_to_string(format!(
            "{}/testdata/legacy/diffrun/full-wrong-report.txt",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("rewrite report reads"),
        &full_paths,
    )
    .expect("full run compares");
    let l1_flags = flag_diffrun(
        std::slice::from_ref(&l1_citation),
        &v1,
        &outcome,
        &[],
        "v1/BILLBATCH.cbl",
        "wrong-rewrite.json",
    );
    assert_eq!(l1_flags.len(), 1, "L1 flags");

    let rechecks: Vec<(&str, recheck::RecheckInput)> = vec![
        (
            "L1",
            recheck::build_diffrun_recheck_input(&l1_flags[0], Some(&l1_citation), &v1),
        ),
        (
            "L2",
            recheck::build_legacy_recheck_input(&l2_flags[0], Some(&l2_citation), &v1, &v2),
        ),
        (
            "S1",
            recheck::build_screens_recheck_input(
                &s1_flags[0],
                Some(&s1_citation),
                &screens_v1,
                &screens_v2,
                &screens_v1_frames,
                &screens_v2_frames,
            ),
        ),
    ];
    for (id, input) in &rechecks {
        // `DOYLE_LIVE_SKIP_RECHECK=1` keeps already-recorded rechecks
        // offline: the L1/L2 recordings stand (their keys are stable), so
        // a guidance-only run spends only on S1 (through the wider
        // screens cap).
        if std::env::var("DOYLE_LIVE_SKIP_RECHECK").is_ok() && *id != "S1" {
            eprintln!("skipping recheck {id} (recording stands)");
            continue;
        }
        let request = if *id == "S1" {
            recheck::build_screens_evidence_request(input)
        } else {
            recheck::build_evidence_request(input)
        };
        let response = meta_recorder
            .complete(&request)
            .expect("re-check must succeed");
        total_usd += print_usage(&format!("recheck {id}"), &response.usage);
        match recheck::parse_response(&response.value).expect("re-check must parse") {
            recheck::RecheckProposal {
                action: doyle_tms::core::ProposalAction::Correct { new_text },
                rationale,
            } => {
                recheck::check_replacement_text(&new_text).expect("guideline 7 clean");
                let lowered = new_text.to_lowercase();
                match *id {
                    "L1" => assert!(
                        lowered.contains("vaccin")
                            || lowered.contains("visit code")
                            || lowered.contains("exempt"),
                        "L1 correction states the exemption: {new_text}"
                    ),
                    "L2" => assert!(
                        lowered.contains("35"),
                        "L2 correction states the 35-char record: {new_text}"
                    ),
                    "S1" => assert!(
                        lowered.contains("prompt")
                            || lowered.contains("warning")
                            || lowered.contains("at once")
                            || lowered.contains("permanence notice")
                            || lowered.contains("entry field")
                            || lowered.contains("different wording"),
                        "S1 correction states the dropped confirmation: {new_text}"
                    ),
                    _ => unreachable!(),
                }
                eprintln!("recheck {id}: correct: {new_text} ({rationale})");
            }
            other => panic!("{id}: must propose correct, got {other:?}"),
        }
    }

    // Curate case files for the gate: the case wrapper T9.3's recordings
    // use, so the replayed tests read them the same way.
    for (id, request, response) in &responses {
        let case = serde_json::json!({
            "case_id": id,
            "memory_text": texts[id.as_str()],
            "request": {
                "model": request.model,
                "system": request.system,
                "user": request.user,
                "schema": request.schema,
                "budget": {
                "max_output_tokens": request.budget.max_output_tokens,
                "max_cost_usd": request.budget.max_cost_usd,
            },
            },
            "response": response,
        });
        std::fs::write(
            case_dir.join(format!("{id}-baseten.json")),
            serde_json::to_string_pretty(&case).expect("serialize"),
        )
        .expect("case file writes");
    }

    eprintln!("retries: {}", retry.retries.get());
    eprintln!("case_dir: {}", case_dir.display());
    eprintln!("recheck_dir: {}", rec_dir.display());
    eprintln!("total: ~${total_usd:.4} on the $15/MTok basis (budget $0.50)");
    assert!(
        total_usd < 0.50,
        "over budget: ~${total_usd:.4} for 9 calls"
    );
}
