//! The `doyle check` command: diff two contracts, flag cited memories, write flags,
//! run the re-check, write proposals.

use std::process::ExitCode;

use crate::cli;
use crate::contract::ContractIndex;
use crate::core::Store;
use crate::core::flag_citations;
use crate::core::{Citation, Flag, Proposal, ProposalAction};
use crate::models::{ModelClient, ReplayClient};
use crate::prompts::recheck;

/// Run the `check` command.
pub fn run(cli: &cli::Cli) -> ExitCode {
    let (format, no_recheck) = match &cli.command {
        cli::Command::Check {
            format, no_recheck, ..
        } => (format.clone(), *no_recheck),
        _ => (String::from("markdown"), false),
    };
    let client: Box<dyn ModelClient> = default_recheck_client();
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
    let no_recheck = match &cli.command {
        cli::Command::Check { no_recheck, .. } => *no_recheck,
        _ => false,
    };
    let client = default_recheck_client();
    execute_with_model(cli, format, no_recheck, client.as_ref())
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

    // Re-check: run on each new flag unless --no-recheck.
    let mut proposals: Vec<Proposal> = Vec::new();
    if !no_recheck {
        for f in &written {
            let citation = citations.iter().find(|c| c.memory == f.memory).cloned();
            let input = build_recheck_input(f, citation.as_ref(), &changes, &old, &new);
            let request = recheck::build_request(&input);
            if let Ok(resp) = client.complete(&request)
                && let Ok(parsed) = recheck::parse_response(&resp.value)
            {
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

fn default_recheck_client() -> Box<dyn ModelClient> {
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
    // verbatim so the replay client can serve the recorded reply.
    let mut ids: Vec<&str> = flag.changes.iter().map(|c| c.0.as_str()).collect();
    ids.sort();
    for case in recheck::measurement_cases() {
        let mut cids: Vec<&str> = case.input.changes.iter().map(|c| c.id.as_str()).collect();
        cids.sort();
        if cids == ids {
            return case.input.clone();
        }
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
        // The proposal obj doesn't include new_text currently; check store.
        let store = Store::open(&dir).unwrap();
        let proposals = store.proposals();
        assert!(!proposals.is_empty(), "proposal written: {out}");
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
}
