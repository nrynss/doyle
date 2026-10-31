//! Live test for the screen source on the T9.4 console fixtures (T9.7b,
//! sibling of T9.7's recorded run).
//!
//! Two Wonder calls on `muse-wonder-1.0` through [`MetaModelClient`]: the v2
//! console (screens only, the cheaper call) first, then v1 (screens plus the
//! 45 s training video). The diff between the two live reads is compared
//! FUZZY with the recorded expectations: the same changed procedure
//! (decommission screen-procedure change), no invented procedure changes
//! (lookup stays procedure-silent), S1 and the span-only citation flag while
//! S2 stays silent at procedure level — all through the same `diff_versions`
//! / `diff_evidence` / `flag_citations` path the gate tests use, with the
//! positional tolerances the code already applies. Token counts and exact
//! element text are never asserted.
//!
//! The one deliberate fuzz beyond the recorded run: separator rules are
//! chrome, not procedure. T9.2 §1 excludes a reported dashes line from
//! scoring ("not scored"): the story PNGs render a `----` rule at row 2
//! that the ground truth does not list, and a live read reports it only
//! sometimes. The differ has no separator concept (only pfkey chrome), so a
//! read that reports the rule in one version promotes it to a lookup fact.
//! The test treats a dashes-only footer add/remove as chrome and asserts that
//! any lookup fact is wobble-only (its verdict holds no procedure change);
//! the transcript always prints the raw verdicts, facts and flags, so a raw
//! S2 flag from separator churn is visible, never silent.
//!
//! This test is `#[ignore]`d: it calls the real API, needs a key, and never
//! runs in CI or the gate. Run it explicitly with the key in the
//! environment:
//!
//! ```sh
//! MODEL_API_KEY="$(secret-tool lookup service meta-model-api)" \
//!     cargo test --test live_screens -- --ignored live_ --nocapture
//! ```
//!
//! The key is read from the environment at call time and is never printed,
//! logged, or written to any file. No recording is written; the transcript
//! goes to stderr (reply shapes, verdicts, facts, flags, usage).
//!
//! Budget: $2 TOTAL for both calls. The v2 usage is printed and checked
//! before the v1 call is spent; the total is asserted at the end.

use doyle_tms::core::{Citation, ElementId, MemoryRef, Severity, flag_citations};
use doyle_tms::evidence::screens::ids::screen_id;
use doyle_tms::evidence::screens::{
    ElementChange, ElementChangeKind, ScreenDiff, ScreenStatus, diff_versions,
};
use doyle_tms::evidence::{EvidenceElement, EvidenceElementKind, EvidenceSource};
use doyle_tms::models::meta::{HttpResponse, HttpTransport, HttpTransportError, MetaModelClient};
use doyle_tms::models::vision::{self, RecordingInput, ScreenInput, VersionSpec};
use std::time::Duration;

/// Minimal [`HttpTransport`] over ureq, for the live calls only.
struct UreqTransport {
    agent: ureq::Agent,
}

impl UreqTransport {
    fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(300)))
            .build();
        Self {
            agent: ureq::Agent::new_with_config(config),
        }
    }
}

impl HttpTransport for UreqTransport {
    fn post(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> Result<HttpResponse, HttpTransportError> {
        let mut request = self.agent.post(url);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let response = request
            .header("Content-Type", "application/json")
            .send(body)
            .map_err(|e| HttpTransportError {
                message: format!("live transport: {e}"),
            })?;
        let status = response.status().as_u16();
        let text = response
            .into_body()
            .read_to_string()
            .map_err(|e| HttpTransportError {
                message: format!("live transport: read body: {e}"),
            })?;
        Ok(HttpResponse { status, body: text })
    }
}

const CONSOLE: &str = "fernwhistle";
const RECORDING: &str = "training-v1";

fn story_path(name: &str) -> String {
    format!(
        "{}/testdata/legacy/story/{name}",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// Dollar estimate on the T9.2 basis (~$15 per million tokens): the API
/// returns no cost field, so spend is estimated from input + output tokens.
fn estimate_usd(input: Option<u64>, output: Option<u64>) -> f64 {
    (input.unwrap_or(0) + output.unwrap_or(0)) as f64 / 1_000_000.0 * 15.0
}

fn citation(memory: &str, elements: &[ElementId]) -> Citation {
    Citation {
        memory: MemoryRef {
            adapter: "muse-code".to_string(),
            id: memory.to_string(),
        },
        elements: elements.to_vec(),
        claim: format!("memory {memory}"),
        recorded_by: "curator".to_string(),
        confidence: 0.9,
        source_text: format!("memory {memory}"),
    }
}

/// Footer-hint churn is chrome, never procedure (the differ's own rule).
fn is_pfkey_chrome(change: &ElementChange) -> bool {
    change_kind(change) == Some("pfkey")
}

/// The element kind on the change's informative side: before for removals
/// and relabels, after for additions.
fn change_kind(change: &ElementChange) -> Option<&str> {
    match &change.kind {
        ElementChangeKind::Removed => change.before.as_ref().map(|v| v.kind.as_str()),
        ElementChangeKind::Added => change.after.as_ref().map(|v| v.kind.as_str()),
        ElementChangeKind::TextChanged => change.before.as_ref().map(|v| v.kind.as_str()),
    }
}

/// A dashes-only footer add/remove is separator chrome, not procedure
/// (T9.2 §1: a reported separator rule is not scored). Narrow: only
/// add/remove of a footer element whose text is all dashes and spaces.
fn is_separator_chrome(change: &ElementChange) -> bool {
    let view = match &change.kind {
        ElementChangeKind::Removed => change.before.as_ref(),
        ElementChangeKind::Added => change.after.as_ref(),
        ElementChangeKind::TextChanged => None,
    };
    view.is_some_and(|v| {
        v.kind == "footer"
            && !v.text.is_empty()
            && v.text.chars().all(|c| c == '-' || c == '─' || c == ' ')
    })
}

/// The verdict's procedure changes: everything but pfkey and separator
/// chrome. Empty means the screen is procedure-silent, whatever the raw
/// status says.
fn procedure_changes<'a>(diff: &'a ScreenDiff, screen: &str) -> Vec<&'a ElementChange> {
    let verdict = diff
        .verdicts
        .iter()
        .find(|v| v.screen == screen)
        .unwrap_or_else(|| panic!("no verdict for screen '{screen}'"));
    verdict
        .changes
        .iter()
        .filter(|c| !is_pfkey_chrome(c) && !is_separator_chrome(c))
        .collect()
}

/// A footer relabel seen through column wobble: either one `TextChanged`
/// (columns stable) or a `Removed` + `Added` pair (columns drifted past the
/// ±2 tolerance, as live reads do — T9.2's column wobble). Empty is allowed
/// too: two identical misreads would pair silently. Anything else is footer
/// churn beyond the one recorded relabel.
fn assert_single_relabel(changes: &[&ElementChange], screen: &str) {
    let text = changes
        .iter()
        .filter(|c| c.kind == ElementChangeKind::TextChanged)
        .count();
    let removed = changes
        .iter()
        .filter(|c| c.kind == ElementChangeKind::Removed)
        .count();
    let added = changes
        .iter()
        .filter(|c| c.kind == ElementChangeKind::Added)
        .count();
    assert!(
        (text == 0 && removed == 0 && added == 0)
            || (text == 1 && removed == 0 && added == 0)
            || (text == 0 && removed == 1 && added == 1),
        "{screen} footer must hold a single relabel at most, got text={text} removed={removed} added={added}"
    );
}
/// The step IDs of the live v1 index that reference `screen`: the S-memory
/// span citations, derived from the reply itself, never hand-copied.
fn steps_referencing(index: &vision::ReadOutcome, screen: &ElementId) -> Vec<ElementId> {
    index
        .index
        .elements()
        .into_iter()
        .filter(|e: &EvidenceElement| {
            e.kind == EvidenceElementKind::Other("step".to_string())
                && e.references.iter().any(|r| r == screen)
        })
        .map(|e| e.id)
        .collect()
}

fn print_version(label: &str, outcome: &vision::ReadOutcome) {
    let elements = outcome.index.elements();
    for screen in outcome.index.screens() {
        let name = screen.native.type_name.as_deref().unwrap_or("?");
        let count = elements
            .iter()
            .filter(|e| e.parent.as_ref() == Some(&screen.id))
            .count();
        eprintln!("{label} screen '{name}': {count} elements");
    }
    for element in &elements {
        if element.kind == EvidenceElementKind::Other("step".to_string()) {
            let span = outcome
                .index
                .step_span(&element.id)
                .map(|(a, b)| format!("{a}-{b}s"))
                .unwrap_or_else(|| "?".to_string());
            let screen = element.references.first().map(|r| r.0.as_str());
            eprintln!(
                "{label} step '{}' span={span} screen={screen:?}",
                element.native.operation_id.as_deref().unwrap_or("?"),
            );
        }
    }
    eprintln!("{label} warnings={:?}", outcome.warnings);
}

fn print_change(change: &ElementChange) -> String {
    let show = |v: Option<&doyle_tms::evidence::screens::ElementView>| {
        v.map(|v| format!("'{}' {}r{}c kind={}", v.text, v.row, v.col, v.kind))
            .unwrap_or_else(|| "-".to_string())
    };
    format!(
        "{:?} before={} after={}",
        change.kind,
        show(change.before.as_ref()),
        show(change.after.as_ref())
    )
}

/// Cheapest call first (v2: two stills, no recording), then v1 (stills plus
/// the 45 s training video); the fuzzy comparison runs on both live reads.
#[test]
#[ignore]
fn live_screens_v1_v2_fuzzy_comparison() {
    assert!(
        std::env::var(MetaModelClient::DEFAULT_ENV_VAR).is_ok(),
        "set {} in the environment (see module docs)",
        MetaModelClient::DEFAULT_ENV_VAR
    );
    let client = MetaModelClient::new(Box::new(UreqTransport::new())).with_reasoning_effort("low");

    // v2 first: screens only, the cheaper call.
    let v2_spec = VersionSpec {
        console: CONSOLE.to_string(),
        version: "v2".to_string(),
        screens: vec![
            ScreenInput {
                name: "lookup".to_string(),
                png: std::fs::read(story_path("screens/lookup-v2.png")).expect("lookup v2 art"),
            },
            ScreenInput {
                name: "decommission".to_string(),
                png: std::fs::read(story_path("screens/decomm-v2.png")).expect("decomm v2 art"),
            },
        ],
        recording: None,
    };
    let v2 = vision::read_version(&client, &v2_spec).expect("v2 read must succeed");
    let v2_usd = estimate_usd(v2.usage.input_tokens, v2.usage.output_tokens);
    print_version("v2", &v2);
    eprintln!(
        "v2 usage: input_tokens={:?} output_tokens={:?} cost_usd={:?} (~${v2_usd:.4} on the $15/MTok basis)",
        v2.usage.input_tokens, v2.usage.output_tokens, v2.usage.cost_usd
    );
    assert!(
        v2_usd < 1.0,
        "stop: v2 alone at ~${v2_usd:.4} leaves no room for v1 inside the $2 budget"
    );

    // v1: screens plus the 45 s training recording.
    let v1_spec = VersionSpec {
        console: CONSOLE.to_string(),
        version: "v1".to_string(),
        screens: vec![
            ScreenInput {
                name: "lookup".to_string(),
                png: std::fs::read(story_path("screens/lookup-v1.png")).expect("lookup v1 art"),
            },
            ScreenInput {
                name: "decommission".to_string(),
                png: std::fs::read(story_path("screens/decomm-v1.png")).expect("decomm v1 art"),
            },
        ],
        recording: Some(RecordingInput {
            name: RECORDING.to_string(),
            duration_s: 45.0,
            mp4: std::fs::read(story_path("training-v1.mp4")).expect("training video"),
        }),
    };
    let v1 = vision::read_version(&client, &v1_spec).expect("v1 read must succeed");
    let v1_usd = estimate_usd(v1.usage.input_tokens, v1.usage.output_tokens);
    print_version("v1", &v1);
    eprintln!(
        "v1 usage: input_tokens={:?} output_tokens={:?} cost_usd={:?} (~${v1_usd:.4} on the $15/MTok basis)",
        v1.usage.input_tokens, v1.usage.output_tokens, v1.usage.cost_usd
    );

    // The fuzzy comparison, through the same flag path the gate tests use.
    // Raw verdicts print in full: the transcript shows separator churn when
    // the reads disagree about the row-2 rule.
    let diff = diff_versions(&v1.index, &v2.index);
    for verdict in &diff.verdicts {
        eprintln!(
            "verdict: screen={} raw_status={:?} changes={}",
            verdict.screen,
            verdict.status,
            verdict.changes.len()
        );
        for change in &verdict.changes {
            eprintln!("  change: {}", print_change(change));
        }
    }

    // No invented procedure change: lookup holds no procedure change, only
    // the recorded PF5 relabel and possible separator churn.
    let lookup_procedure = procedure_changes(&diff, "lookup");
    assert!(
        lookup_procedure.is_empty(),
        "lookup invents a procedure change: {:?}",
        lookup_procedure
            .iter()
            .map(|c| print_change(c))
            .collect::<Vec<_>>()
    );
    let lookup_pfkey: Vec<&ElementChange> = diff
        .verdicts
        .iter()
        .find(|v| v.screen == "lookup")
        .expect("lookup verdict")
        .changes
        .iter()
        .filter(|c| is_pfkey_chrome(c))
        .collect();
    assert_single_relabel(&lookup_pfkey, "lookup");

    // The same changed procedure: decommission drops procedure elements and
    // relabels one, exactly the recorded shape (modulo separator churn).
    let decomm_verdict = diff
        .verdicts
        .iter()
        .find(|v| v.screen == "decommission")
        .expect("decommission verdict");
    assert_eq!(
        decomm_verdict.status,
        ScreenStatus::ProcedureChanged,
        "decommission must show the changed procedure"
    );
    let decomm_procedure = procedure_changes(&diff, "decommission");
    assert!(
        decomm_procedure.iter().any(|c| {
            c.kind == ElementChangeKind::Removed
                && c.before.as_ref().is_some_and(|v| v.kind != "pfkey")
        }),
        "decommission must drop procedure elements"
    );
    assert!(
        decomm_procedure
            .iter()
            .all(|c| c.kind != ElementChangeKind::Added),
        "decommission must not add procedure elements: {:?}",
        decomm_procedure
            .iter()
            .map(|c| print_change(c))
            .collect::<Vec<_>>()
    );
    // The PF12 relabel, seen through column wobble as a TextChanged or a
    // remove+add pair; the footer holds nothing else.
    let decomm_pfkey: Vec<&ElementChange> = decomm_verdict
        .changes
        .iter()
        .filter(|c| is_pfkey_chrome(c))
        .collect();
    assert_single_relabel(&decomm_pfkey, "decommission");
    assert!(
        !decomm_pfkey.is_empty(),
        "decommission must relabel its PF12 hint"
    );

    // Facts and flags are computed and printed BEFORE any comparison
    // assert: even a run that diverges on lookup still leaves live
    // flag-path evidence (S1 flagged? S2 flagged from what?) in the
    // transcript.
    let facts = v1.index.diff_evidence(&v2.index).expect("diff works");
    let decomm_id = screen_id(CONSOLE, "decommission");
    let lookup_id = screen_id(CONSOLE, "lookup");
    for fact in &facts {
        eprintln!(
            "fact: id={} subject={} severity={:?} kind={:?}",
            fact.id.0, fact.subject.0, fact.severity, fact.kind
        );
    }
    let decomm_steps = steps_referencing(&v1, &decomm_id);
    let lookup_steps = steps_referencing(&v1, &lookup_id);
    eprintln!(
        "citations: {} decommission steps, {} lookup steps",
        decomm_steps.len(),
        lookup_steps.len()
    );
    assert!(
        !decomm_steps.is_empty(),
        "live v1 must show decommission steps, or the span path is untested"
    );
    assert!(
        !lookup_steps.is_empty(),
        "live v1 must show lookup steps, or S2's silence is vacuous"
    );
    let citations = vec![
        citation(
            "S1",
            &[std::slice::from_ref(&decomm_id), &decomm_steps[..]].concat(),
        ),
        citation("S1-span", &decomm_steps[..1]),
        citation(
            "S2",
            &[std::slice::from_ref(&lookup_id), &lookup_steps[..]].concat(),
        ),
    ];
    let flags = flag_citations(&citations, &facts, &v1.index, &[], "v1", "v2");
    for flag in &flags {
        eprintln!(
            "flag: memory={} severity={:?} changes={:?}",
            flag.memory.id,
            flag.severity,
            flag.changes
                .iter()
                .map(|c| c.0.as_str())
                .collect::<Vec<_>>()
        );
    }

    // Facts: the decommission breaking fact, plus at most wobble-only
    // lookup facts (a lookup fact whose verdict holds no procedure change).
    let decomm_facts: Vec<_> = facts.iter().filter(|f| f.subject == decomm_id).collect();
    assert_eq!(decomm_facts.len(), 1, "one decommission fact: {facts:?}");
    assert_eq!(decomm_facts[0].severity, Severity::Breaking);
    for fact in facts.iter().filter(|f| f.subject == lookup_id) {
        eprintln!(
            "note: wobble-only lookup fact {} (no procedure change)",
            fact.id.0
        );
    }
    assert!(
        facts
            .iter()
            .all(|f| f.subject == decomm_id || f.subject == lookup_id),
        "every fact is decommission or wobble-only lookup: {facts:?}"
    );

    let decomm_fact_id = &decomm_facts[0].id;
    for memory in ["S1", "S1-span"] {
        let flag = flags
            .iter()
            .find(|f| f.memory.id == memory)
            .unwrap_or_else(|| panic!("{memory} must flag: {flags:?}"));
        assert_eq!(flag.severity, Severity::Breaking);
        assert_eq!(
            flag.changes,
            vec![decomm_fact_id.clone()],
            "{memory} must flag from the decommission fact alone"
        );
    }
    let lookup_fact_ids: Vec<_> = facts
        .iter()
        .filter(|f| f.subject == lookup_id)
        .map(|f| &f.id)
        .collect();
    for flag in flags.iter().filter(|f| f.memory.id == "S2") {
        assert!(
            !flag.changes.is_empty() && flag.changes.iter().all(|c| lookup_fact_ids.contains(&c)),
            "S2 must stay procedure-silent: {flag:?}"
        );
        eprintln!(
            "note: S2 flags from a wobble-only lookup fact ({:?}), not procedure",
            flag.changes
                .iter()
                .map(|c| c.0.as_str())
                .collect::<Vec<_>>()
        );
    }

    let total_usd = v2_usd + v1_usd;
    eprintln!("total: ~${total_usd:.4} on the $15/MTok basis (budget $2)");
    assert!(
        total_usd < 2.0,
        "over budget: ~${total_usd:.4} for both calls"
    );
}
