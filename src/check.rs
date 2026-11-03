//! The `doyle check` command: diff two versions, flag cited memories, write
//! flags, run the re-check, write proposals.
//!
//! The source is picked from the file extensions: two `.cbl` programs diff
//! as legacy COBOL, two `.json` screens files diff as console screens,
//! anything else diffs as API contracts. Every source flags through the
//! same [`flag_citations`](crate::core::flag_citations) and the same store;
//! the re-check runs for contract flags only (the legacy, screen and
//! diffrun re-checks are a later task).

use std::process::ExitCode;

use crate::cli;
use crate::contract::ContractIndex;
use crate::core::Store;
use crate::core::flag_citations;
use crate::core::{Citation, Flag, Proposal, ProposalAction};
use crate::evidence::screens::{RecordingMeta, ScreenIndex};
use crate::evidence::{EvidenceChange, EvidenceSource};
use crate::models::{ModelClient, ModelError, ReplayClient};
use crate::prompts::recheck;
use crate::prompts::vision::{RawElement, RawScreen, RawStep, RawVersion};

/// Which evidence source a `check` run diffs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceKind {
    Contract,
    Legacy,
    Screens,
}

/// Pick the source from what the command is given: two `.cbl` programs are
/// the legacy COBOL source, two `.json` screens files the screen source,
/// anything else an API contract pair. Documented in `doyle check --help`.
fn detect_kind(old_path: &str, new_path: &str) -> SourceKind {
    if old_path.ends_with(".cbl") && new_path.ends_with(".cbl") {
        SourceKind::Legacy
    } else if old_path.ends_with(".json") && new_path.ends_with(".json") {
        SourceKind::Screens
    } else {
        SourceKind::Contract
    }
}

/// One parsed screens version file: the raw screens, steps and recording
/// the review page and the re-check read.
struct ScreenFile {
    screens: Vec<RawScreen>,
    steps: Vec<RawStep>,
    recording: Option<RecordingMeta>,
}

/// Parse one screens version file: the review page's version shape
/// (`console`, `screens`, `steps`, `recording`), shared with the guard so
/// both resolve the same stable IDs.
fn parse_screen_file(path: &str) -> Result<(String, ScreenFile), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
    let console = value
        .get("console")
        .and_then(|v| v.as_str())
        .unwrap_or("fernwhistle");
    let screens_value = value
        .get("screens")
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("{path}: missing screens"))?;
    let mut screens = Vec::with_capacity(screens_value.len());
    for screen in screens_value {
        let name = screen
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("{path}: a screen is missing its name"))?;
        let elements = screen
            .get("elements")
            .and_then(|v| v.as_array())
            .ok_or_else(|| format!("{path}: screen {name} is missing elements"))?;
        let mut raw_elements = Vec::with_capacity(elements.len());
        for element in elements {
            raw_elements.push(RawElement {
                text: element
                    .get("text")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| format!("{path}: an element is missing text"))?
                    .to_string(),
                row: element
                    .get("row")
                    .and_then(|v| v.as_u64())
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or_else(|| format!("{path}: an element is missing row"))?,
                col: element
                    .get("col")
                    .and_then(|v| v.as_u64())
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or_else(|| format!("{path}: an element is missing col"))?,
                kind: element
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                protected: element
                    .get("protected")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
            });
        }
        screens.push(RawScreen {
            name: name.to_string(),
            elements: raw_elements,
        });
    }
    let mut steps = Vec::new();
    if let Some(rows) = value.get("steps").and_then(|v| v.as_array()) {
        for step in rows {
            steps.push(RawStep {
                description: step
                    .get("description")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| format!("{path}: a step is missing description"))?
                    .to_string(),
                start_s: step
                    .get("start_s")
                    .and_then(|v| v.as_f64())
                    .ok_or_else(|| format!("{path}: a step is missing start_s"))?,
                end_s: step
                    .get("end_s")
                    .and_then(|v| v.as_f64())
                    .ok_or_else(|| format!("{path}: a step is missing end_s"))?,
                screen: step
                    .get("screen")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            });
        }
    }
    let recording = value.get("recording").map(|r| RecordingMeta {
        name: r
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("recording")
            .to_string(),
        duration_s: r.get("duration_s").and_then(|v| v.as_f64()).unwrap_or(0.0),
    });
    Ok((
        console.to_string(),
        ScreenFile {
            screens,
            steps,
            recording,
        },
    ))
}

/// Load one screens version file: the review page's version shape
/// (`console`, `screens`, `steps`, `recording`), shared with the guard so
/// both resolve the same stable IDs.
pub fn load_screen_version(path: &str) -> Result<(ScreenIndex, Option<RecordingMeta>), String> {
    let (console, file) = parse_screen_file(path)?;
    let raw = RawVersion {
        screens: file.screens,
        steps: file.steps,
        warnings: Vec::new(),
    };
    let (index, _warnings) = ScreenIndex::from_raw(&console, &raw, file.recording.as_ref());
    Ok((index, file.recording))
}

/// Load one screens version file's frame ground truth: each screen's name
/// with its frame rendered by the re-check, for the screens re-check
/// input. Reads the same version shape [`load_screen_version`] reads.
pub fn load_screen_frames(path: &str) -> Result<Vec<(String, String)>, String> {
    let (_, file) = parse_screen_file(path)?;
    Ok(file
        .screens
        .iter()
        .map(|s| (s.name.clone(), recheck::render_screen_frame(&s.elements)))
        .collect())
}

/// Run the `check` command.
pub fn run(cli: &cli::Cli) -> ExitCode {
    let (format, no_recheck, old_path, new_path) = match &cli.command {
        cli::Command::Check {
            format,
            no_recheck,
            old,
            new,
        } => (format.clone(), *no_recheck, old.clone(), new.clone()),
        _ => (
            String::from("markdown"),
            false,
            String::new(),
            String::new(),
        ),
    };
    if detect_kind(&old_path, &new_path) != SourceKind::Contract {
        return match execute_evidence(cli, &format, no_recheck, &old_path, &new_path) {
            Ok((code, out)) => {
                print!("{out}");
                code
            }
            Err(e) => {
                eprintln!("doyle check: {e}");
                ExitCode::from(2)
            }
        };
    }
    // --live swaps the replay client for the Meta re-check client over ureq;
    // without it the client (and the key requirement) is exactly what it was.
    let client: Box<dyn ModelClient> = match model_for_run(cli.common.live) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("doyle check: {e}");
            return ExitCode::from(2);
        }
    };
    match execute_with_model(cli, &format, no_recheck, client.as_ref()) {
        Ok((code, out)) => {
            print!("{}", out);
            code
        }
        Err(e) => {
            eprintln!("doyle check: {e}");
            ExitCode::from(2)
        }
    }
}

/// Run the check and return its exit code and rendered report instead of
/// printing. `format` is `"markdown"` or `"json"`. Uses the default (replay)
/// model client and reads `--no-recheck` from `cli`.
pub fn execute(cli: &cli::Cli, format: &str) -> Result<(ExitCode, String), String> {
    let (no_recheck, old_path, new_path) = match &cli.command {
        cli::Command::Check {
            no_recheck,
            old,
            new,
            ..
        } => (no_recheck, old, new),
        _ => return Err(String::from("doyle check: expected a Check command")),
    };
    if detect_kind(old_path, new_path) != SourceKind::Contract {
        return execute_evidence(cli, format, *no_recheck, old_path, new_path);
    }
    let client: Box<dyn ModelClient> = if cli.common.live {
        live_recheck_client()?
    } else {
        default_recheck_client()
    };
    execute_with_model(cli, format, *no_recheck, client.as_ref())
}

/// Run the check with an explicit model client (for tests and injection).
pub fn execute_with_model(
    cli: &cli::Cli,
    format: &str,
    no_recheck: bool,
    client: &dyn ModelClient,
) -> Result<(ExitCode, String), String> {
    let (old_path, new_path) = match &cli.command {
        cli::Command::Check { old, new, .. } => (old.clone(), new.clone()),
        _ => return Err(String::from("doyle check: expected a Check command")),
    };

    let old = ContractIndex::load(&old_path).map_err(|e| e.to_string())?;
    let new = ContractIndex::load(&new_path).map_err(|e| e.to_string())?;
    let changes = old.diff(&new).map_err(|e| e.to_string())?;

    let dir = cli
        .common
        .state_dir
        .clone()
        .unwrap_or_else(|| String::from(".doyle"));
    let mut store = Store::open(&dir).map_err(|e| e.to_string())?;

    let citations: Vec<Citation> = store.citations().to_vec();
    let open_flags: Vec<Flag> = store.open_flags();

    let flags = flag_citations(
        &citations,
        &changes,
        &old,
        &open_flags,
        &old_path,
        &new_path,
    );

    // Write the new flags to the store.
    let before = store.flags().len();
    for f in &flags {
        store.record_flag(f.clone()).map_err(|e| e.to_string())?;
    }
    let written: Vec<Flag> = store.flags()[before..].to_vec();

    // Re-check: run on each open flag that has no proposal yet, unless
    // --no-recheck. Retry policy (L1): a flag that is still open and has no
    // proposal is treated as still needing work, so a rerun re-attempts the
    // re-check for it instead of skipping it. The flag itself is never
    // duplicated: `flag_citations` still dedups by (adapter, memory id) via
    // the store's open flags, so only the re-check step repeats. Once a
    // proposal has been recorded for a flag, it is skipped on subsequent
    // runs (no duplicate proposals). A flag that gets no proposal (replay
    // miss or parse failure) emits a per-flag diagnostic on stderr and stays
    // proposal-less, so the next run retries it.
    let mut proposals: Vec<Proposal> = Vec::new();
    if !no_recheck {
        let targets: Vec<Flag> = store
            .open_flags()
            .into_iter()
            .filter(|f| store.proposals_for_flag(&f.id).is_empty())
            .collect();
        for f in &targets {
            let citation = citations.iter().find(|c| c.memory == f.memory).cloned();
            let input = build_recheck_input(f, citation.as_ref(), &changes, &old, &new);
            let request = recheck::build_request(&input);
            match client.complete(&request) {
                Ok(resp) => match recheck::parse_response(&resp.value) {
                    Ok(parsed) => {
                        let proposal = Proposal {
                            id: String::new(),
                            flag_id: f.id.clone(),
                            action: parsed.action,
                            rationale: parsed.rationale,
                            proposed_by: recheck::MODEL.to_string(),
                        };
                        let pid = store
                            .record_proposal(proposal.clone())
                            .map_err(|e| e.to_string())?;
                        if let Some(p) = store.proposal(&pid) {
                            proposals.push(p);
                        } else {
                            proposals.push(Proposal {
                                id: pid,
                                ..proposal
                            });
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "doyle check: flag {} ({}/{}) got no proposal: parse failure: {} (model {}, changes {})",
                            f.id,
                            f.memory.adapter,
                            f.memory.id,
                            e,
                            recheck::MODEL,
                            f.changes
                                .iter()
                                .map(|c| c.0.as_str())
                                .collect::<Vec<_>>()
                                .join(",")
                        );
                    }
                },
                Err(ModelError::ReplayMiss { key }) => {
                    eprintln!(
                        "doyle check: flag {} ({}/{}) got no proposal: replay miss (key {}) (model {}, changes {})",
                        f.id,
                        f.memory.adapter,
                        f.memory.id,
                        key,
                        recheck::MODEL,
                        f.changes
                            .iter()
                            .map(|c| c.0.as_str())
                            .collect::<Vec<_>>()
                            .join(",")
                    );
                }
                Err(other) => {
                    eprintln!(
                        "doyle check: flag {} ({}/{}) got no proposal: model error: {} (model {}, changes {})",
                        f.id,
                        f.memory.adapter,
                        f.memory.id,
                        other,
                        recheck::MODEL,
                        f.changes
                            .iter()
                            .map(|c| c.0.as_str())
                            .collect::<Vec<_>>()
                            .join(",")
                    );
                }
            }
        }
    }

    let report = if format == "json" {
        render_json(&old_path, &new_path, &written, &proposals)
    } else {
        render_markdown(&old_path, &new_path, &written, &proposals)
    };

    let code = if written.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    };
    Ok((code, report))
}

/// Run the check over a non-contract source: load both versions through the
/// source's own index, diff into change facts, flag through the same
/// [`flag_citations`] and the same store. The re-check runs for contract
/// flags only: each new-kind flag gets a one-line stderr note naming the
/// later task instead of a proposal, so a default run stays keyless and
/// `--no-recheck` stays silent.
fn execute_evidence(
    cli: &cli::Cli,
    format: &str,
    no_recheck: bool,
    old_path: &str,
    new_path: &str,
) -> Result<(ExitCode, String), String> {
    enum OldNew {
        Legacy(
            crate::evidence::legacy::LegacyIndex,
            crate::evidence::legacy::LegacyIndex,
        ),
        Screens(ScreenIndex, ScreenIndex),
    }
    let kind = detect_kind(old_path, new_path);
    let pair = match kind {
        SourceKind::Legacy => {
            let old = crate::evidence::legacy::LegacyIndex::load(old_path)
                .map_err(|e| format!("doyle check: {e}"))?;
            let new = crate::evidence::legacy::LegacyIndex::load(new_path)
                .map_err(|e| format!("doyle check: {e}"))?;
            OldNew::Legacy(old, new)
        }
        SourceKind::Screens => {
            let (old, _) = load_screen_version(old_path)?;
            let (new, _) = load_screen_version(new_path)?;
            OldNew::Screens(old, new)
        }
        SourceKind::Contract => {
            return Err(String::from(
                "doyle check: expected a legacy or screens pair",
            ));
        }
    };

    let dir = cli
        .common
        .state_dir
        .clone()
        .unwrap_or_else(|| String::from(".doyle"));
    let mut store = Store::open(&dir).map_err(|e| e.to_string())?;

    let citations: Vec<Citation> = store.citations().to_vec();
    let open_flags: Vec<Flag> = store.open_flags();

    let flags: Vec<Flag> = match &pair {
        OldNew::Legacy(old, new) => {
            let changes: Vec<EvidenceChange> = old.diff_evidence(new).map_err(|e| e.to_string())?;
            flag_citations(&citations, &changes, old, &open_flags, old_path, new_path)
        }
        OldNew::Screens(old, new) => {
            let changes: Vec<EvidenceChange> = old.diff_evidence(new).map_err(|e| e.to_string())?;
            flag_citations(&citations, &changes, old, &open_flags, old_path, new_path)
        }
    };

    let before = store.flags().len();
    for f in &flags {
        store.record_flag(f.clone()).map_err(|e| e.to_string())?;
    }
    let written: Vec<Flag> = store.flags()[before..].to_vec();

    // The re-check producer for the new kinds (T9.11b): each open
    // proposal-less flag gets a real proposal from Muse Spark through the
    // evidence input builders, the way contract flags propose. The client
    // is the replay dir by default (keyless, model-free) and the live Meta
    // client with --live; --no-recheck stays silent.
    let mut proposals: Vec<Proposal> = Vec::new();
    if !no_recheck {
        let client: Box<dyn ModelClient> = if cli.common.live {
            live_recheck_client()?
        } else {
            default_recheck_client()
        };
        proposals = match &pair {
            OldNew::Legacy(old, new) => {
                propose_legacy_flags(&mut store, &citations, client.as_ref(), old, new)
            }
            OldNew::Screens(old, new) => {
                let old_frames = load_screen_frames(old_path)?;
                let new_frames = load_screen_frames(new_path)?;
                propose_screen_flags(
                    &mut store,
                    &citations,
                    client.as_ref(),
                    old,
                    new,
                    &old_frames,
                    &new_frames,
                )
            }
        };
    }

    let report = if format == "json" {
        render_json(old_path, new_path, &written, &proposals)
    } else {
        render_markdown(old_path, new_path, &written, &proposals)
    };

    let code = if written.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    };
    Ok((code, report))
}

/// Run the evidence re-check for one flag: complete the request, parse
/// the reply, record the proposal. Diagnostics mirror the contract path
/// (same `doyle check: flag ... got no proposal` shape) so operators see
/// one failure vocabulary for every source.
pub(crate) fn run_evidence_recheck(
    store: &mut Store,
    flag: &Flag,
    request: &crate::models::ModelRequest,
    client: &dyn ModelClient,
) -> Option<Proposal> {
    match client.complete(request) {
        Ok(resp) => match recheck::parse_response(&resp.value) {
            Ok(parsed) => {
                let proposal = Proposal {
                    id: String::new(),
                    flag_id: flag.id.clone(),
                    action: parsed.action,
                    rationale: parsed.rationale,
                    proposed_by: recheck::MODEL.to_string(),
                };
                let pid = store
                    .record_proposal(proposal.clone())
                    .map_err(|e| e.to_string())
                    .ok()?;
                if let Some(p) = store.proposal(&pid) {
                    Some(p)
                } else {
                    Some(Proposal {
                        id: pid,
                        ..proposal
                    })
                }
            }
            Err(e) => {
                eprintln!(
                    "doyle check: flag {} ({}/{}) got no proposal: parse failure: {} (model {}, changes {})",
                    flag.id,
                    flag.memory.adapter,
                    flag.memory.id,
                    e,
                    recheck::MODEL,
                    flag.changes
                        .iter()
                        .map(|c| c.0.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                );
                None
            }
        },
        Err(ModelError::ReplayMiss { key }) => {
            eprintln!(
                "doyle check: flag {} ({}/{}) got no proposal: replay miss (key {}) (model {}, changes {})",
                flag.id,
                flag.memory.adapter,
                flag.memory.id,
                key,
                recheck::MODEL,
                flag.changes
                    .iter()
                    .map(|c| c.0.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            );
            None
        }
        Err(other) => {
            eprintln!(
                "doyle check: flag {} ({}/{}) got no proposal: model error: {} (model {}, changes {})",
                flag.id,
                flag.memory.adapter,
                flag.memory.id,
                other,
                recheck::MODEL,
                flag.changes
                    .iter()
                    .map(|c| c.0.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            );
            None
        }
    }
}

/// Propose for legacy-diff flags: every open proposal-less flag is
/// re-checked against both program versions. Once a proposal is recorded
/// it is skipped on reruns; a flag that gets no proposal stays
/// proposal-less so the next run retries it.
fn propose_legacy_flags(
    store: &mut Store,
    citations: &[Citation],
    client: &dyn ModelClient,
    old: &crate::evidence::legacy::LegacyIndex,
    new: &crate::evidence::legacy::LegacyIndex,
) -> Vec<Proposal> {
    let targets: Vec<Flag> = store
        .open_flags()
        .into_iter()
        .filter(|f| store.proposals_for_flag(&f.id).is_empty())
        .collect();
    let mut proposals = Vec::new();
    for f in &targets {
        let citation = citations.iter().find(|c| c.memory == f.memory).cloned();
        let input = recheck::build_legacy_recheck_input(f, citation.as_ref(), old, new);
        let request = recheck::build_evidence_request(&input);
        if let Some(p) = run_evidence_recheck(store, f, &request, client) {
            proposals.push(p);
        }
    }
    proposals
}

/// Propose for screen-diff flags: the same loop over both console
/// versions, with each version's frame ground truth beside the verdict.
fn propose_screen_flags(
    store: &mut Store,
    citations: &[Citation],
    client: &dyn ModelClient,
    old: &ScreenIndex,
    new: &ScreenIndex,
    old_frames: &[(String, String)],
    new_frames: &[(String, String)],
) -> Vec<Proposal> {
    let targets: Vec<Flag> = store
        .open_flags()
        .into_iter()
        .filter(|f| store.proposals_for_flag(&f.id).is_empty())
        .collect();
    let mut proposals = Vec::new();
    for f in &targets {
        let citation = citations.iter().find(|c| c.memory == f.memory).cloned();
        let input = recheck::build_screens_recheck_input(
            f,
            citation.as_ref(),
            old,
            new,
            old_frames,
            new_frames,
        );
        let request = recheck::build_screens_evidence_request(&input);
        if let Some(p) = run_evidence_recheck(store, f, &request, client) {
            proposals.push(p);
        }
    }
    proposals
}

fn model_for_run(live: bool) -> Result<Box<dyn ModelClient>, String> {
    if live {
        live_recheck_client()
    } else {
        Ok(default_recheck_client())
    }
}

/// A live Meta re-check client over ureq. The key resolves through
/// [`crate::models::keys`] (environment, then keyring); the client seam
/// reads its key from the environment at call time, so a keyring-resolved
/// key is planted in the process environment here — the only bridge that
/// leaves `meta.rs`/`baseten.rs` untouched.
pub(crate) fn live_recheck_client() -> Result<Box<dyn ModelClient>, String> {
    use crate::models::meta::MetaModelClient;
    if std::env::var(MetaModelClient::DEFAULT_ENV_VAR).is_err() {
        let key = crate::models::keys::meta_key().map_err(|e| e.to_string())?;
        // SAFETY: the CLI's live path runs before any thread spawns, and the
        // client reads the process environment at call time.
        unsafe {
            std::env::set_var(MetaModelClient::DEFAULT_ENV_VAR, key);
        }
    }
    Ok(Box::new(MetaModelClient::new(Box::new(
        crate::models::http::UreqTransport::new(),
    ))))
}

pub(crate) fn default_recheck_client() -> Box<dyn ModelClient> {
    let dir = std::env::var("DOYLE_RECHECK_REPLAY_DIR")
        .unwrap_or_else(|_| String::from("testdata/replies/recheck"));
    Box::new(ReplayClient::new(dir))
}

fn build_recheck_input(
    flag: &Flag,
    citation: Option<&Citation>,
    changes: &[vimanam::contract::Change],
    old: &ContractIndex,
    new: &ContractIndex,
) -> recheck::RecheckInput {
    // If a committed measurement case matches this flag's change set, reuse it
    // verbatim so the replay client can serve the recorded reply. The reuse
    // is gated on more than the change-id set (L2): it also requires the
    // cited memory (adapter + id) and the claim to match the case, so a
    // different memory that happens to share the change set falls through to
    // the generic live-built request instead of receiving another memory's
    // verbatim input.
    let mut ids: Vec<&str> = flag.changes.iter().map(|c| c.0.as_str()).collect();
    ids.sort();
    let claim_matches = |case_claim: &str| {
        if let Some(cit) = citation {
            cit.claim == case_claim
        } else {
            flag.evidence.as_deref() == Some(case_claim)
        }
    };
    // The committed cases are pinned to specific memories; only those
    // memories are eligible for verbatim reuse (the recorded replies are
    // for them). A case with no pinned memory never reuses.
    let case_memory = |name: &str| -> Option<(&'static str, &'static str)> {
        match name {
            "m1_correct" => Some(("muse-code", "M1")),
            _ => None,
        }
    };
    for case in recheck::measurement_cases() {
        let mut cids: Vec<&str> = case.input.changes.iter().map(|c| c.id.as_str()).collect();
        cids.sort();
        if cids != ids {
            continue;
        }
        if !claim_matches(&case.input.claim) {
            continue;
        }
        match case_memory(case.name) {
            Some((adapter, id)) if flag.memory.adapter == adapter && flag.memory.id == id => {}
            _ => continue,
        }
        return case.input.clone();
    }

    // Generic fallback.
    let claim = citation
        .map(|c| c.claim.clone())
        .or_else(|| flag.evidence.clone())
        .unwrap_or_default();
    let memory_text = claim.clone();

    let mut change_facts = Vec::new();
    for cid in &flag.changes {
        let change = changes.iter().find(|c| c.id == cid.0);
        let (kind, severity, description) = match change {
            Some(c) => (
                serde_kind(&c.kind),
                serde_severity(&c.severity),
                c.native_kind.clone(),
            ),
            None => (
                String::from("unknown"),
                String::from("review"),
                String::new(),
            ),
        };
        change_facts.push(recheck::ChangeFact {
            id: cid.0.clone(),
            kind,
            severity,
            description,
        });
    }

    let mut elements = Vec::new();
    if let Some(cit) = citation {
        for eid in &cit.elements {
            let e_old = old.entity(eid);
            let e_new = new.entity(eid);
            let name = e_old
                .or(e_new)
                .map(|e| render_name(e, eid))
                .unwrap_or_else(|| eid.0.clone());
            let old_text = e_old
                .map(|e| serde_json::to_string(&e.attributes).unwrap_or_default())
                .unwrap_or_else(|| String::from("(absent in old)"));
            let new_text = e_new
                .map(|e| serde_json::to_string(&e.attributes).unwrap_or_default())
                .unwrap_or_else(|| String::from("(removed in new)"));
            elements.push(recheck::CitedElement {
                element_id: eid.0.clone(),
                name,
                old: old_text,
                new: new_text,
            });
        }
    }

    recheck::RecheckInput {
        memory_text,
        claim,
        changes: change_facts,
        elements,
        old_version: flag.old_version.clone(),
        new_version: flag.new_version.clone(),
        guidance: String::new(),
    }
}

fn serde_kind(k: &vimanam::contract::ChangeKind) -> String {
    // ChangeKind serializes to snake_case via serde; mirror that.
    serde_json::to_value(k)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| format!("{:?}", k).to_lowercase())
}

fn serde_severity(s: &vimanam::contract::Severity) -> String {
    serde_json::to_value(s)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| format!("{:?}", s).to_lowercase())
}

fn render_name(e: &vimanam::contract::Entity, _eid: &crate::core::ElementId) -> String {
    use vimanam::contract::EntityKind;
    let base = e.name.clone();
    let loc = match e.kind {
        EntityKind::Operation => {
            // method + path
            if let vimanam::contract::NativeLocation::Openapi { method, path, .. } = &e.native {
                match (method, path) {
                    (Some(m), Some(p)) => format!(" ({} {})", m, p),
                    _ => String::new(),
                }
            } else {
                String::new()
            }
        }
        _ => String::new(),
    };
    format!("{}{}", base, loc)
}

/// Render a Markdown report for the flags written this run.
pub fn render_markdown(
    old_path: &str,
    new_path: &str,
    flags: &[Flag],
    proposals: &[Proposal],
) -> String {
    let mut out = String::new();
    out.push_str("# doyle check\n\n");
    out.push_str(&format!("Old: {}\n", old_path));
    out.push_str(&format!("New: {}\n\n", new_path));

    if flags.is_empty() {
        out.push_str("No flags.\n");
        // Still list proposals if any (shouldn't happen, but keep symmetry).
        if !proposals.is_empty() {
            out.push_str("\n## Proposals\n\n");
            for p in proposals {
                out.push_str(&format!("- {} — {:?}\n", p.id, p.action));
            }
        }
        return out;
    }

    out.push_str(&format!("## Flags ({})\n\n", flags.len()));
    for f in flags {
        out.push_str(&format!(
            "### {} — {} ({}) — {}\n",
            f.id,
            f.memory.id,
            f.memory.adapter,
            severity_word(f.severity)
        ));
        out.push_str(&format!(
            "- Changes: {}\n",
            f.changes
                .iter()
                .map(|c| c.0.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        if let Some(evidence) = &f.evidence {
            out.push_str(&format!("- Evidence: {}\n", evidence));
        }
        // Attach proposal if present.
        if let Some(p) = proposals.iter().find(|p| p.flag_id == f.id) {
            out.push_str(&format!(
                "- Proposal: {} — {}\n",
                p.id,
                action_word(&p.action)
            ));
            out.push_str(&format!("  - Rationale: {}\n", p.rationale));
            if let ProposalAction::Correct { new_text } = &p.action {
                out.push_str(&format!("  - New text: {}\n", new_text));
            }
        }
        out.push('\n');
    }

    if !proposals.is_empty() {
        out.push_str(&format!("## Proposals ({})\n\n", proposals.len()));
        for p in proposals {
            out.push_str(&format!(
                "- {} — {} — {}\n",
                p.id,
                p.flag_id,
                action_word(&p.action)
            ));
            out.push_str(&format!("  - Rationale: {}\n", p.rationale));
            if let ProposalAction::Correct { new_text } = &p.action {
                out.push_str(&format!("  - New text: {}\n", new_text));
            }
        }
    }
    out
}

/// Render a JSON report for the flags written this run.
pub fn render_json(
    old_path: &str,
    new_path: &str,
    flags: &[Flag],
    proposals: &[Proposal],
) -> String {
    let flag_values: Vec<serde_json::Value> = flags
        .iter()
        .map(|f| {
            let proposal = proposals.iter().find(|p| p.flag_id == f.id);
            let proposal_value = match proposal {
                Some(p) => {
                    let mut v = serde_json::json!({
                        "id": p.id,
                        "flag_id": p.flag_id,
                        "action": action_word(&p.action),
                        "rationale": p.rationale,
                        "proposed_by": p.proposed_by,
                    });
                    if let ProposalAction::Correct { new_text } = &p.action {
                        v["new_text"] = serde_json::json!(new_text);
                    }
                    v
                }
                None => serde_json::Value::Null,
            };
            serde_json::json!({
                "id": f.id,
                "memory": {
                    "adapter": f.memory.adapter,
                    "id": f.memory.id,
                },
                "changes": f.changes.iter().map(|c| c.0.clone()).collect::<Vec<_>>(),
                "severity": f.severity,
                "old_version": f.old_version,
                "new_version": f.new_version,
                "evidence": f.evidence,
                "proposal": proposal_value,
            })
        })
        .collect();
    let mut value = serde_json::json!({
        "old": old_path,
        "new": new_path,
        "flags": flag_values,
    });
    if !proposals.is_empty() {
        let proposal_values: Vec<serde_json::Value> = proposals
            .iter()
            .map(|p| {
                let mut v = serde_json::json!({
                    "id": p.id,
                    "flag_id": p.flag_id,
                    "action": action_word(&p.action),
                    "rationale": p.rationale,
                    "proposed_by": p.proposed_by,
                });
                if let ProposalAction::Correct { new_text } = &p.action {
                    v["new_text"] = serde_json::json!(new_text);
                }
                v
            })
            .collect();
        value["proposals"] = serde_json::json!(proposal_values);
    }
    let mut s = serde_json::to_string_pretty(&value).unwrap_or_else(|_| String::from("{}"));
    s.push('\n');
    s
}

fn severity_word(s: crate::core::Severity) -> &'static str {
    match s {
        crate::core::Severity::Breaking => "breaking",
        crate::core::Severity::Review => "review",
    }
}

fn action_word(a: &ProposalAction) -> &'static str {
    match a {
        ProposalAction::Correct { .. } => "correct",
        ProposalAction::Retire => "retire",
        ProposalAction::Keep => "keep",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli;
    use crate::core::ElementId;
    use std::path::PathBuf;

    const LIST_PATIENTS: &str =
        "ve2_c81e37b7c42c484a1859c09e95bf8cec89a360db6743a691afab818f177c7bc6";
    const LIST_PATIENTS_200: &str =
        "ve2_86d3cdb7ce3a7f0b1a2fefa696f0acbbcf2d5406c717bb1f1f1cb5f4f62667ba";

    fn fixture(name: &str) -> String {
        format!("{}/testdata/{name}", env!("CARGO_MANIFEST_DIR"))
    }

    fn temp_state_dir(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static CTR: AtomicU64 = AtomicU64::new(0);
        let n = CTR.fetch_add(1, Ordering::Relaxed);
        let mut p = std::env::temp_dir();
        p.push(format!(
            "doyle-check-test-{}-{}-{}",
            std::process::id(),
            tag,
            n
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn seed_citations(dir: &PathBuf) {
        let mut store = Store::open(dir).unwrap();
        let citation = crate::core::Citation {
            memory: crate::core::MemoryRef {
                adapter: "muse-code".to_string(),
                id: "M1".to_string(),
            },
            elements: vec![
                ElementId(LIST_PATIENTS.to_string()),
                ElementId(LIST_PATIENTS_200.to_string()),
            ],
            claim: "GET /patients returns every patient in one response".to_string(),
            recorded_by: "curator".to_string(),
            confidence: 0.9,
            source_text: "GET /patients returns every patient in one response".to_string(),
        };
        store.record_citation(citation).unwrap();
    }

    #[test]
    fn demo_flags_m1_exits_one() {
        let dir = temp_state_dir("demo");
        seed_citations(&dir);
        let cli = cli::Cli {
            command: cli::Command::Check {
                old: fixture("contracts/patients-v1.yaml"),
                new: fixture("contracts/patients-v2.yaml"),
                format: "markdown".to_string(),
                no_recheck: false,
            },
            common: cli::CommonOpts {
                state_dir: Some(dir.to_string_lossy().to_string()),
                ..Default::default()
            },
        };
        let (code, out) = execute(&cli, "markdown").unwrap();
        assert_eq!(code, ExitCode::from(1));
        assert!(out.contains("M1"), "report should name M1: {out}");
        assert!(
            out.contains("vc2_9bedbbb47825fa87ea41aa38342d0492948286a7dd4a5ecbb378310261df6841"),
            "report should contain breaking change id: {out}"
        );
        assert!(
            out.contains("vc2_d7e1b2acbebe45b089106951b949710e3112d1586ed74cfe5fab47d702987277"),
            "report should contain constraint change id: {out}"
        );
        assert!(
            out.contains("vc2_d82adaee7e47799b8027d3c1b9f458e16be8a20df1696186f351369f07e011e0"),
            "report should contain pagination change id: {out}"
        );
        assert!(
            out.contains("breaking"),
            "report should say breaking: {out}"
        );
        assert!(
            out.contains("GET /patients returns every patient in one response"),
            "report should contain evidence: {out}"
        );
        // Re-check proposal present.
        assert!(
            out.contains("Proposal:") || out.contains("proposal"),
            "report should include a proposal: {out}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn demo_flags_m1_json() {
        let dir = temp_state_dir("demo-json");
        seed_citations(&dir);
        let old = fixture("contracts/patients-v1.yaml");
        let new = fixture("contracts/patients-v2.yaml");
        let cli = cli::Cli {
            command: cli::Command::Check {
                old: old.clone(),
                new: new.clone(),
                format: "json".to_string(),
                no_recheck: false,
            },
            common: cli::CommonOpts {
                state_dir: Some(dir.to_string_lossy().to_string()),
                ..Default::default()
            },
        };
        let (code, out) = execute(&cli, "json").unwrap();
        assert_eq!(code, ExitCode::from(1));
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let f = &v["flags"][0];
        assert_eq!(f["memory"]["id"], "M1");
        let changes = f["changes"].as_array().unwrap();
        assert_eq!(changes.len(), 3);
        assert!(
            changes.iter().any(
                |c| c == "vc2_9bedbbb47825fa87ea41aa38342d0492948286a7dd4a5ecbb378310261df6841"
            ),
            "breaking change id missing: {out}"
        );
        assert!(
            changes.iter().any(
                |c| c == "vc2_d7e1b2acbebe45b089106951b949710e3112d1586ed74cfe5fab47d702987277"
            ),
            "constraint change id missing: {out}"
        );
        assert!(
            changes.iter().any(
                |c| c == "vc2_d82adaee7e47799b8027d3c1b9f458e16be8a20df1696186f351369f07e011e0"
            ),
            "pagination change id missing: {out}"
        );
        assert_eq!(f["severity"], "breaking");
        assert_eq!(f["old_version"], old);
        assert_eq!(f["new_version"], new);
        assert_eq!(
            f["evidence"],
            "GET /patients returns every patient in one response"
        );
        // Proposal present.
        assert!(f["proposal"].is_object() || v["proposals"].is_array());
        if let Some(p) = f["proposal"].as_object() {
            assert_eq!(p["action"], "correct");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_recheck_matches_t23_output() {
        let dir = temp_state_dir("norecheck");
        seed_citations(&dir);
        // First run: no-recheck.
        let cli_nr = cli::Cli {
            command: cli::Command::Check {
                old: fixture("contracts/patients-v1.yaml"),
                new: fixture("contracts/patients-v2.yaml"),
                format: "markdown".to_string(),
                no_recheck: true,
            },
            common: cli::CommonOpts {
                state_dir: Some(dir.to_string_lossy().to_string()),
                ..Default::default()
            },
        };
        let (code_nr, out_nr) = execute(&cli_nr, "markdown").unwrap();
        assert_eq!(code_nr, ExitCode::from(1));
        // Second state dir for a "recheck" run that we ignore; just ensure no-recheck
        // produces no proposal text.
        assert!(
            !out_nr.contains("Proposal:"),
            "no-recheck must not propose: {out_nr}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rerun_exits_zero_and_store_holds_one_flag() {
        let dir = temp_state_dir("rerun");
        seed_citations(&dir);
        let cli = cli::Cli {
            command: cli::Command::Check {
                old: fixture("contracts/patients-v1.yaml"),
                new: fixture("contracts/patients-v2.yaml"),
                format: "markdown".to_string(),
                no_recheck: true,
            },
            common: cli::CommonOpts {
                state_dir: Some(dir.to_string_lossy().to_string()),
                ..Default::default()
            },
        };
        let (code1, _) = execute(&cli, "markdown").unwrap();
        assert_eq!(code1, ExitCode::from(1));
        let (code2, _) = execute(&cli, "markdown").unwrap();
        assert_eq!(code2, ExitCode::SUCCESS);
        let store = Store::open(&dir).unwrap();
        assert_eq!(store.flags().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn identical_contracts_exit_zero() {
        let dir = temp_state_dir("ident");
        seed_citations(&dir);
        let v1 = fixture("contracts/patients-v1.yaml");
        let cli = cli::Cli {
            command: cli::Command::Check {
                old: v1.clone(),
                new: v1,
                format: "markdown".to_string(),
                no_recheck: true,
            },
            common: cli::CommonOpts {
                state_dir: Some(dir.to_string_lossy().to_string()),
                ..Default::default()
            },
        };
        let (code, _out) = execute(&cli, "markdown").unwrap();
        assert_eq!(code, ExitCode::SUCCESS);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn json_shape_pinned() {
        let flag = crate::core::Flag {
            id: "flag-1".to_string(),
            memory: crate::core::MemoryRef {
                adapter: "muse-code".to_string(),
                id: "M1".to_string(),
            },
            changes: vec![crate::core::ChangeId(
                "vc2_9bedbbb47825fa87ea41aa38342d0492948286a7dd4a5ecbb378310261df6841".to_string(),
            )],
            severity: crate::core::Severity::Breaking,
            old_version: "a".to_string(),
            new_version: "b".to_string(),
            evidence: Some("claim".to_string()),
        };
        let proposal = crate::core::Proposal {
            id: "proposal-1".to_string(),
            flag_id: "flag-1".to_string(),
            action: crate::core::ProposalAction::Correct {
                new_text: "GET /patients returns one page".to_string(),
            },
            rationale: "r".to_string(),
            proposed_by: "muse-spark-1.3".to_string(),
        };
        let json = render_json("old", "new", &[flag], &[proposal]);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["old"], "old");
        assert_eq!(v["new"], "new");
        assert!(v["flags"].is_array());
        let f = &v["flags"][0];
        assert_eq!(f["id"], "flag-1");
        assert_eq!(f["memory"]["adapter"], "muse-code");
        assert_eq!(f["memory"]["id"], "M1");
        assert!(f["changes"].is_array());
        assert_eq!(f["severity"], "breaking");
        assert_eq!(f["old_version"], "a");
        assert_eq!(f["new_version"], "b");
        assert_eq!(f["evidence"], "claim");
        assert!(f["proposal"].is_object());
        assert_eq!(f["proposal"]["action"], "correct");
        assert!(v["proposals"].is_array());
    }

    #[test]
    fn demo_flag_carries_correct_proposal() {
        let dir = temp_state_dir("demo-proposal");
        seed_citations(&dir);
        let cli = cli::Cli {
            command: cli::Command::Check {
                old: fixture("contracts/patients-v1.yaml"),
                new: fixture("contracts/patients-v2.yaml"),
                format: "json".to_string(),
                no_recheck: false,
            },
            common: cli::CommonOpts {
                state_dir: Some(dir.to_string_lossy().to_string()),
                ..Default::default()
            },
        };
        let (code, out) = execute(&cli, "json").unwrap();
        assert_eq!(code, ExitCode::from(1));
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let f = &v["flags"][0];
        let p = &f["proposal"];
        assert!(p.is_object(), "flag must carry proposal: {out}");
        assert_eq!(p["action"], "correct", "must be correct: {out}");
        // JSON proposal carries new_text too; the store is authoritative.
        let store = Store::open(&dir).unwrap();
        let proposals = store.proposals();
        assert!(!proposals.is_empty(), "proposal written: {out}");
        assert_eq!(
            proposals[0].flag_id,
            store.flags()[0].id,
            "proposal.flag_id must equal the written flag's store id"
        );
        match &proposals[0].action {
            crate::core::ProposalAction::Correct { new_text } => {
                crate::prompts::recheck::check_replacement_text(new_text)
                    .expect("guideline 7 clean");
                let lowered = new_text.to_lowercase();
                assert!(
                    lowered.contains("100") || lowered.contains("page"),
                    "recorded text must state paging truth: {new_text}"
                );
            }
            other => panic!("expected correct, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // L1: replay miss emits a per-flag diagnostic on stderr (not stdout),
    // and the exit code is preserved (flags written -> 1).
    #[test]
    fn l1_replay_miss_diagnostic_on_stderr() {
        use std::process::Command;
        let dir = temp_state_dir("l1-miss");
        seed_citations(&dir);
        // Empty replay dir forces a replay miss for every flag.
        let empty_replay = temp_state_dir("l1-empty-replay");
        let bin = format!("{}/target/debug/doyle", env!("CARGO_MANIFEST_DIR"));
        let out = Command::new(&bin)
            .args([
                "check",
                fixture("contracts/patients-v1.yaml").as_str(),
                fixture("contracts/patients-v2.yaml").as_str(),
                "--state-dir",
                dir.to_str().unwrap(),
                "--format",
                "json",
            ])
            .env("DOYLE_RECHECK_REPLAY_DIR", empty_replay.to_str().unwrap())
            .output()
            .expect("spawn doyle");
        // Exit code preserved: flags written -> 1.
        assert_eq!(
            out.status.code(),
            Some(1),
            "stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        // Diagnostic on stderr, not stdout.
        assert!(
            stderr.contains("got no proposal") && stderr.contains("replay miss"),
            "stderr must name flag + replay miss: {stderr}"
        );
        assert!(
            !stdout.contains("got no proposal"),
            "diagnostic must not appear on stdout: {stdout}"
        );
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&empty_replay);
    }

    // L1 retry policy: a proposal-less open flag is re-attempted on rerun.
    #[test]
    fn l1_retry_rechecks_proposal_less_flag() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct FlakyClient {
            calls: AtomicUsize,
        }
        impl crate::models::ModelClient for FlakyClient {
            fn complete(
                &self,
                _request: &crate::models::ModelRequest,
            ) -> Result<crate::models::ModelResponse, crate::models::ModelError> {
                let n = self.calls.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    Err(crate::models::ModelError::ReplayMiss {
                        key: "first-miss".to_string(),
                    })
                } else {
                    Ok(crate::models::ModelResponse {
                        value: serde_json::json!({
                            "action": "correct",
                            "new_text": "GET /patients returns one page of up to 100 plus next_cursor",
                            "rationale": "v2 pages at 100 with a cursor"
                        }),
                        usage: crate::models::Usage::NONE,
                    })
                }
            }
        }
        let dir = temp_state_dir("l1-retry");
        seed_citations(&dir);
        let cli = cli::Cli {
            command: cli::Command::Check {
                old: fixture("contracts/patients-v1.yaml"),
                new: fixture("contracts/patients-v2.yaml"),
                format: "markdown".to_string(),
                no_recheck: false,
            },
            common: cli::CommonOpts {
                state_dir: Some(dir.to_string_lossy().to_string()),
                ..Default::default()
            },
        };
        let client = FlakyClient {
            calls: AtomicUsize::new(0),
        };
        // First run: replay miss -> no proposal.
        let (code1, _) = execute_with_model(&cli, "markdown", false, &client).unwrap();
        assert_eq!(code1, ExitCode::from(1));
        let store = Store::open(&dir).unwrap();
        assert!(store.proposals().is_empty(), "first run must not propose");
        // Second run: re-attempt succeeds -> proposal recorded.
        let (code2, _) = execute_with_model(&cli, "markdown", false, &client).unwrap();
        let _ = code2;
        let store = Store::open(&dir).unwrap();
        assert!(
            !store.proposals().is_empty(),
            "retry must propose once the reply exists"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // L2: two flags sharing a change-id set but citing different memories:
    // the second must not receive the first's verbatim input (fallback path).
    #[test]
    fn l2_shared_set_different_memory_no_verbatim_reuse() {
        let dir = temp_state_dir("l2-shared");
        // Seed two citations: M1 (demo claim, reuses case) and M2 (different
        // memory, same cited elements -> same change set).
        {
            let mut store = Store::open(&dir).unwrap();
            let citation_m1 = crate::core::Citation {
                memory: crate::core::MemoryRef {
                    adapter: "muse-code".to_string(),
                    id: "M1".to_string(),
                },
                elements: vec![
                    ElementId(LIST_PATIENTS.to_string()),
                    ElementId(LIST_PATIENTS_200.to_string()),
                ],
                claim: "GET /patients returns every patient in one response".to_string(),
                source_text: "GET /patients returns every patient in one response; one call exports the whole clinic".to_string(),
                recorded_by: "curator".to_string(),
                confidence: 0.9,
            };
            let citation_m2 = crate::core::Citation {
                memory: crate::core::MemoryRef {
                    adapter: "muse-code".to_string(),
                    id: "M2".to_string(),
                },
                elements: vec![
                    ElementId(LIST_PATIENTS.to_string()),
                    ElementId(LIST_PATIENTS_200.to_string()),
                ],
                claim: "M2 remembers patients differently".to_string(),
                source_text: "M2 remembers patients differently".to_string(),
                recorded_by: "curator".to_string(),
                confidence: 0.9,
            };
            store.record_citation(citation_m1).unwrap();
            store.record_citation(citation_m2).unwrap();
        }
        let cli = cli::Cli {
            command: cli::Command::Check {
                old: fixture("contracts/patients-v1.yaml"),
                new: fixture("contracts/patients-v2.yaml"),
                format: "json".to_string(),
                no_recheck: false,
            },
            common: cli::CommonOpts {
                state_dir: Some(dir.to_string_lossy().to_string()),
                ..Default::default()
            },
        };
        let (code, out) = execute(&cli, "json").unwrap();
        assert_eq!(code, ExitCode::from(1));
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let _flags = v["flags"].as_array().unwrap();
        // Find proposals per flag via store.
        let store = Store::open(&dir).unwrap();
        let proposals = store.proposals();
        // M1 (demo) should have a proposal (reuse -> replay hit).
        let m1_proposals: Vec<_> = proposals
            .iter()
            .filter(|p| {
                store
                    .flags()
                    .iter()
                    .any(|f| f.id == p.flag_id && f.memory.id == "M1")
            })
            .collect();
        assert!(
            !m1_proposals.is_empty(),
            "M1 should propose via reuse: {out}"
        );
        // M2 must NOT receive the verbatim case input -> fallback -> replay
        // miss -> no proposal.
        let m2_proposals: Vec<_> = proposals
            .iter()
            .filter(|p| {
                store
                    .flags()
                    .iter()
                    .any(|f| f.id == p.flag_id && f.memory.id == "M2")
            })
            .collect();
        assert!(
            m2_proposals.is_empty(),
            "M2 must not reuse M1's verbatim input: {out}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // N4: --no-recheck makes zero model calls.
    #[test]
    fn n4_no_recheck_zero_model_calls() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct CountingClient {
            calls: AtomicUsize,
        }
        impl crate::models::ModelClient for CountingClient {
            fn complete(
                &self,
                _request: &crate::models::ModelRequest,
            ) -> Result<crate::models::ModelResponse, crate::models::ModelError> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                Ok(crate::models::ModelResponse {
                    value: serde_json::json!({"action":"keep","rationale":"x"}),
                    usage: crate::models::Usage::NONE,
                })
            }
        }
        let dir = temp_state_dir("n4-norecheck");
        seed_citations(&dir);
        let cli = cli::Cli {
            command: cli::Command::Check {
                old: fixture("contracts/patients-v1.yaml"),
                new: fixture("contracts/patients-v2.yaml"),
                format: "markdown".to_string(),
                no_recheck: true,
            },
            common: cli::CommonOpts {
                state_dir: Some(dir.to_string_lossy().to_string()),
                ..Default::default()
            },
        };
        let client = CountingClient {
            calls: AtomicUsize::new(0),
        };
        let (_code, _out) = execute_with_model(&cli, "markdown", true, &client).unwrap();
        assert_eq!(
            client.calls.load(Ordering::SeqCst),
            0,
            "no-recheck must make zero model calls"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // T5.4: --live parses on check, and the default stays on replay.
    #[test]
    fn live_parses_on_check_and_defaults_off() {
        for (argv, live) in [
            (
                vec![
                    "check".to_string(),
                    "a".to_string(),
                    "b".to_string(),
                    "--live".to_string(),
                ],
                true,
            ),
            (
                vec!["check".to_string(), "a".to_string(), "b".to_string()],
                false,
            ),
        ] {
            match crate::cli::parse_from(&argv) {
                crate::cli::ParseResult::Run(cli) => {
                    assert_eq!(cli.common.live, live, "argv: {argv:?}");
                }
                other => panic!("expected Run for {argv:?}, got {other:?}"),
            }
        }
    }

    // T5.4: without --live the check path builds the replay client, never a
    // live one: it needs no key.
    #[test]
    fn replay_default_needs_no_key() {
        let client = model_for_run(false).expect("no-live must build");
        let request = crate::models::ModelRequest {
            model: "probe".to_string(),
            system: "s".to_string(),
            user: "u".to_string(),
            schema: serde_json::json!({"action": "string"}),
            media: Vec::new(),
            budget: crate::models::Budget::tokens(8),
        };
        // The default replay dir holds no recording for this probe request,
        // so a miss proves the client is the replay client.
        let err = client.complete(&request).expect_err("must miss");
        match err {
            crate::models::ModelError::ReplayMiss { .. } => {}
            other => panic!("expected ReplayMiss, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod frames_tests {
    use super::*;

    fn write_screens_version(dir: &std::path::Path) -> (String, String) {
        let screen_doc = |file: &str| -> serde_json::Value {
            let value: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(format!(
                    "{}/testdata/legacy/story/screens/{file}.json",
                    env!("CARGO_MANIFEST_DIR")
                ))
                .unwrap(),
            )
            .unwrap();
            serde_json::json!({
                "name": value["screen"],
                "elements": value["elements"],
            })
        };
        let v1 = serde_json::json!({
            "console": "fernwhistle",
            "screens": [screen_doc("lookup-v1"), screen_doc("decomm-v1")],
            "steps": [],
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

    #[test]
    fn screen_frames_render_both_versions_ground_truth() {
        let dir = std::env::temp_dir().join(format!(
            "doyle-frames-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (v1_path, v2_path) = write_screens_version(&dir);
        let old_frames = load_screen_frames(&v1_path).expect("frames load");
        let new_frames = load_screen_frames(&v2_path).expect("frames load");
        assert_eq!(old_frames.len(), 2);
        let decomm_old = old_frames
            .iter()
            .find(|(name, _)| name == "decommission")
            .expect("decomm frame");
        assert!(decomm_old.1.contains("CONFIRM"), "{}", decomm_old.1);
        let decomm_new = new_frames
            .iter()
            .find(|(name, _)| name == "decommission")
            .expect("decomm frame");
        assert!(decomm_new.1.contains("DECOMMISSION"), "{}", decomm_new.1);
        let lookup_new = new_frames
            .iter()
            .find(|(name, _)| name == "lookup")
            .expect("lookup frame");
        assert!(lookup_new.1.contains("SAVE"), "{}", lookup_new.1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
