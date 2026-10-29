//! The `doyle check` command: diff two contracts, flag cited memories, write flags.

use std::process::ExitCode;

use crate::cli;
use crate::contract::ContractIndex;
use crate::core::Store;
use crate::core::flag_citations;
use crate::core::{Citation, Flag};

/// Run the `check` command.
pub fn run(cli: &cli::Cli) -> ExitCode {
    match execute(cli, "markdown") {
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
/// printing. `format` is `"markdown"` or `"json"`.
pub fn execute(cli: &cli::Cli, format: &str) -> Result<(ExitCode, String), String> {
    let (old_path, new_path) = match &cli.command {
        cli::Command::Check { old, new } => (old.clone(), new.clone()),
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

    let report = if format == "json" {
        render_json(&old_path, &new_path, &written)
    } else {
        render_markdown(&old_path, &new_path, &written)
    };

    let code = if written.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    };
    Ok((code, report))
}

/// Render a Markdown report for the flags written this run.
pub fn render_markdown(old_path: &str, new_path: &str, flags: &[Flag]) -> String {
    let mut out = String::new();
    out.push_str("# doyle check\n\n");
    out.push_str(&format!("Old: {}\n", old_path));
    out.push_str(&format!("New: {}\n\n", new_path));

    if flags.is_empty() {
        out.push_str("No flags.\n");
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
        out.push('\n');
    }
    out
}

/// Render a JSON report for the flags written this run.
pub fn render_json(old_path: &str, new_path: &str, flags: &[Flag]) -> String {
    let flag_values: Vec<serde_json::Value> = flags
        .iter()
        .map(|f| {
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
            })
        })
        .collect();
    let value = serde_json::json!({
        "old": old_path,
        "new": new_path,
        "flags": flag_values,
    });
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| String::from("{}"))
}

fn severity_word(s: crate::core::Severity) -> &'static str {
    match s {
        crate::core::Severity::Breaking => "breaking",
        crate::core::Severity::Review => "review",
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
            },
            common: cli::CommonOpts {
                state_dir: Some(dir.to_string_lossy().to_string()),
                ..Default::default()
            },
        };
        let (code, out) = execute(&cli, "markdown").unwrap();
        assert_eq!(code, ExitCode::from(1));
        assert!(out.contains("M1"), "report should name M1: {out}");
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
        let json = render_json("old", "new", &[flag]);
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
    }
}
