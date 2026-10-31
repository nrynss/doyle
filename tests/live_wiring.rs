//! Live wiring for `curate --live` and `check --live` (T5.4).
//!
//! The binary could not run the demo live: `curate` built its Baseten client
//! on a transport that always errors, and `check` re-checked only from
//! recorded replies. This test proves the wiring: from a scratch project
//! directory outside the repository, `curate --live` cites the demo memories
//! through the live Baseten curator, and `check --live` flags M1 with a live
//! Meta re-check proposal.
//!
//! Ignored by default: it needs a network and `BASETEN_API_KEY` plus
//! `MODEL_API_KEY` in the environment. Run it with
//! `cargo test --test live_wiring -- --ignored live_ --nocapture`. It never
//! runs in CI and never gates. Keys are read from the environment at call
//! time (via [`keys`](doyle_tms::models::keys)) and are never printed,
//! logged, or written anywhere: the transcript below holds only models,
//! token counts and proposal text.
//!
//! Budget: $0.50 for the whole run. The probe test makes two calls first
//! (one tiny wire check, one real curator-shaped call whose usage bounds the
//! rest); the pipeline test then makes five (four curator, one re-check).
//! The providers return no cost figures (as in T4.2b/T4.3), so the report
//! bounds spend from measured tokens.

use doyle_tms::cli::{self, ParseResult};
use doyle_tms::contract::ContractIndex;
use doyle_tms::models::baseten::BasetenModelClient;
use doyle_tms::models::http::UreqTransport;
use doyle_tms::models::keys;
use doyle_tms::models::{Budget, ModelClient, ModelRequest};
use doyle_tms::prompts::curator;
use std::process::ExitCode;

fn repo_path(rel: &str) -> String {
    format!("{}/{}", env!("CARGO_MANIFEST_DIR"), rel)
}

/// A scratch project directory holding copies of the Muse Code demo notes.
/// Outside the repository, so `--project` (not the cwd) decides what the
/// adapter reads.
fn scratch_project(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "doyle-live-wiring-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch project");
    for file in ["MEMORY.md", "patients-stats.md"] {
        std::fs::copy(
            repo_path(&format!("testdata/memories/muse-code/{file}")),
            dir.join(file),
        )
        .expect("copy demo note");
    }
    dir
}

fn scratch_state(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "doyle-live-wiring-state-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch state");
    dir
}

fn parse(args: &[&str]) -> cli::Cli {
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    match cli::parse_from(&owned) {
        ParseResult::Run(cli) => cli,
        other => panic!("expected Run for {args:?}, got {other:?}"),
    }
}

fn print_usage(model: &str, usage: &doyle_tms::models::Usage) {
    eprintln!(
        "usage: model={model} input={:?} output={:?} cost_usd={:?}",
        usage.input_tokens, usage.output_tokens, usage.cost_usd
    );
}

/// Probe first: one tiny Baseten call (wire + key check), then one
/// curator-shaped call for M1's text whose usage bounds the pipeline. Stops
/// the run if the sample is absurdly large for the $0.50 budget.
#[test]
#[ignore = "live: calls Baseten; run with cargo test -- --ignored live_"]
fn live_wiring_probe() {
    assert!(
        std::env::var_os(BasetenModelClient::DEFAULT_ENV_VAR).is_some(),
        "live probe needs BASETEN_API_KEY in the environment"
    );
    let key = keys::baseten_key().expect("key must resolve from the environment");
    assert!(!key.is_empty());
    drop(key); // never printed, never logged

    let client = BasetenModelClient::new(Box::new(UreqTransport::new()));

    // 1. Tiny wire check.
    let tiny = ModelRequest {
        model: BasetenModelClient::DEFAULT_MODEL.to_string(),
        system: "Reply with exactly the requested JSON object.".to_string(),
        user: "Return the integer 42 as `answer`.".to_string(),
        schema: serde_json::json!({
            "type": "object",
            "properties": {"answer": {"type": "integer"}},
            "required": ["answer"],
            "additionalProperties": false
        }),
        media: Vec::new(),
        budget: Budget::tokens(2048),
    };
    let resp = client.complete(&tiny).expect("tiny live call must succeed");
    assert_eq!(resp.value.get("answer"), Some(&serde_json::json!(42)));
    print_usage(&tiny.model, &resp.usage);

    // 2. One real curator-shaped call for M1's text: the spend bound. Same
    // marginal-budget flake as the pipeline (content-null when reasoning
    // fills 2048 tokens), so retry bounded at three; each attempt is logged.
    let index =
        ContractIndex::load(repo_path("testdata/contracts/patients-v1.yaml")).expect("load v1");
    let request = curator::build_curator_request(
        "GET /patients returns every patient in one response; one call exports the whole clinic",
        &index,
        curator::BASETEN_CURATOR_MODEL,
    );
    let mut attempts = 0;
    let resp = loop {
        attempts += 1;
        match client.complete(&request) {
            Ok(resp) => {
                eprintln!("probe curator-shaped attempt {attempts}: ok");
                break resp;
            }
            Err(e) if attempts < 3 => {
                eprintln!("probe curator-shaped attempt {attempts}: {e}");
            }
            Err(e) => panic!("curator-shaped call must succeed: {e}"),
        }
    };
    print_usage(&request.model, &resp.usage);
    assert!(resp.value.is_object(), "reply: {}", resp.value);

    let total: u64 = resp.usage.input_tokens.unwrap_or(0) + resp.usage.output_tokens.unwrap_or(0);
    eprintln!("probe: curator-shaped call used ~{total} tokens");
    // Five more calls of this shape follow in the pipeline test. Even at a
    // generous $1/MTok this is cents; stop if the sample ever says otherwise.
    assert!(
        total < 100_000,
        "probe used {total} tokens: over budget, stopping"
    );
}

/// The pipeline, live: `curate --live` then `check --live` on the demo
/// fixtures, rooted at a scratch project outside the repository.
#[test]
#[ignore = "live: calls Baseten and Meta; run with cargo test -- --ignored live_"]
fn live_wiring_curate_then_check() {
    assert!(
        std::env::var_os(BasetenModelClient::DEFAULT_ENV_VAR).is_some(),
        "live run needs BASETEN_API_KEY in the environment"
    );
    assert!(
        std::env::var_os(doyle_tms::models::meta::MetaModelClient::DEFAULT_ENV_VAR).is_some(),
        "live run needs MODEL_API_KEY in the environment"
    );

    let project = scratch_project("proj");
    let state = scratch_state("state");
    let v1 = repo_path("testdata/contracts/patients-v1.yaml");
    let v2 = repo_path("testdata/contracts/patients-v2.yaml");

    // curate --live: four memories, four curator calls.
    let curate_cli = parse(&[
        "curate",
        "--live",
        "--adapters",
        "muse-code",
        "--project",
        project.to_str().unwrap(),
        "--contract",
        &v1,
        "--state-dir",
        state.to_str().unwrap(),
    ]);
    assert!(curate_cli.common.live);
    // The curator budget (2048 tokens, T4.3) is marginal for the live model's
    // reasoning: some memories come back content-null (see the T5.4 report).
    // Curate skips already-cited memories, so retrying the run re-attempts
    // only what is missing. Bounded at three attempts; each attempt is
    // logged, nothing is hidden.
    let mut attempts = 0;
    let code = loop {
        attempts += 1;
        let code = doyle_tms::curate::run(&curate_cli);
        eprintln!("curate --live attempt {attempts}: exit {code:?}");
        if code == ExitCode::SUCCESS || attempts >= 3 {
            break code;
        }
    };
    assert_eq!(code, ExitCode::SUCCESS, "curate --live must succeed");

    let store = doyle_tms::core::Store::open(&state).expect("open state");
    assert_eq!(
        store.citations().len(),
        4,
        "four memories must be cited, got {}",
        store.citations().len()
    );
    for id in [
        "MEMORY.md#1",
        "MEMORY.md#2",
        "patients-stats.md#1",
        "MEMORY.md#3",
    ] {
        assert!(
            store
                .citations()
                .iter()
                .any(|c| c.memory.id == id && c.memory.adapter == "muse-code"),
            "missing citation for muse-code/{id}"
        );
    }

    // check --live: one re-check call through the Meta client.
    let check_cli = parse(&[
        "check",
        &v1,
        &v2,
        "--live",
        "--format",
        "json",
        "--adapters",
        "muse-code",
        "--project",
        project.to_str().unwrap(),
        "--state-dir",
        state.to_str().unwrap(),
    ]);
    assert!(check_cli.common.live);
    let (code, out) = doyle_tms::check::execute(&check_cli, "json").expect("check --live must run");
    assert_eq!(code, ExitCode::from(1), "check must flag M1: {out}");
    let report: serde_json::Value = serde_json::from_str(&out).expect("valid json");
    // Live curate cites real adapter refs (MEMORY.md#1 is M1's note), not the
    // replay fixtures' logical M1 id.
    assert_eq!(
        report["flags"][0]["memory"]["adapter"], "muse-code",
        "{out}"
    );
    assert_eq!(report["flags"][0]["memory"]["id"], "MEMORY.md#1", "{out}");
    let action = report["flags"][0]["proposal"]["action"]
        .as_str()
        .unwrap_or("");
    eprintln!("live proposal action: {action}");
    assert_eq!(action, "correct", "proposal must correct: {out}");

    let _ = std::fs::remove_dir_all(&project);
    // State dir kept for the report if needed; comment out removal to inspect.
    let _ = std::fs::remove_dir_all(&state);
}
