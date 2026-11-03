//! The `doyle diffrun` command: evidence by execution from the binary.
//!
//! Given the COBOL batch and a recorded run (both reports over the same
//! inputs, with the per-record paragraph paths or a raw trace), compare
//! outputs record by record and flag the cited memories the mismatches
//! blame, through the same [`flag_citations`](crate::core::flag_citations)
//! and the same store every source uses. The flag's old version is the
//! batch program and its new version the run manifest, the way the review
//! page reads them. The re-check runs for contract flags only: diffrun
//! flags get no proposal yet (a later task), noted one line per flag on
//! stderr unless `--no-recheck`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::cli;
use crate::core::Store;
use crate::evidence::diffrun::{
    DiffrunOutcome, Mismatch, flag_diffrun, mismatch_evidence, parse_record_paths, run_diffrun,
    run_diffrun_with_paths, total_overcharge_cents,
};
use crate::evidence::legacy::LegacyIndex;

/// Run the `diffrun` command.
pub fn run(cli: &cli::Cli) -> ExitCode {
    let (format, no_recheck) = match &cli.command {
        cli::Command::Diffrun {
            format, no_recheck, ..
        } => (format.clone(), *no_recheck),
        _ => (String::from("markdown"), false),
    };
    match execute(cli, &format, no_recheck) {
        Ok((code, out)) => {
            print!("{out}");
            code
        }
        Err(e) => {
            eprintln!("doyle diffrun: {e}");
            ExitCode::from(2)
        }
    }
}

/// Run the differential run and return its exit code and rendered report
/// instead of printing. `format` is `"markdown"` or `"json"`.
pub fn execute(
    cli: &cli::Cli,
    format: &str,
    no_recheck: bool,
) -> Result<(ExitCode, String), String> {
    let (program_path, run_path) = match &cli.command {
        cli::Command::Diffrun { program, run, .. } => (program.clone(), run.clone()),
        _ => return Err(String::from("doyle diffrun: expected a Diffrun command")),
    };

    let index = LegacyIndex::load(&program_path).map_err(|e| format!("doyle diffrun: {e}"))?;
    let outcome = load_outcome(&run_path)?;

    let dir = cli
        .common
        .state_dir
        .clone()
        .unwrap_or_else(|| String::from(".doyle"));
    let mut store = Store::open(&dir).map_err(|e| e.to_string())?;

    let citations = store.citations().to_vec();
    let open_flags = store.open_flags();
    let diffrun_flags = flag_diffrun(
        &citations,
        &index,
        &outcome,
        &open_flags,
        &program_path,
        &run_path,
    );

    let before = store.flags().len();
    for f in &diffrun_flags {
        store
            .record_flag(f.flag.clone())
            .map_err(|e| e.to_string())?;
    }
    let written: Vec<_> = store.flags()[before..].to_vec();
    // The store assigns flag IDs on write; carry them onto the rendered
    // flags so the report names the same flags the review page will show.
    let mut diffrun_flags = diffrun_flags;
    for (flag, stored) in diffrun_flags.iter_mut().zip(written.iter()) {
        flag.flag.id.clone_from(&stored.id);
    }

    if !no_recheck {
        for f in &written {
            eprintln!(
                "doyle diffrun: flag {} ({}/{}) got no proposal: re-check for diffrun flags is not built yet (a later task); rerun with --no-recheck to silence",
                f.id, f.memory.adapter, f.memory.id,
            );
        }
    }

    let report = if format == "json" {
        render_json(&program_path, &run_path, &outcome, &diffrun_flags)
    } else {
        render_markdown(&program_path, &run_path, &outcome, &diffrun_flags)
    };

    let code = if written.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    };
    Ok((code, report))
}

/// Load the recorded run a manifest names: the inputs and both reports plus
/// the per-record paths (or a raw trace with its record bounds). Relative
/// paths resolve beside the manifest; any drift between inputs, reports and
/// paths is an error, never a silent pass.
fn load_outcome(manifest_path: &str) -> Result<DiffrunOutcome, String> {
    let manifest = PathBuf::from(manifest_path);
    let text =
        std::fs::read_to_string(&manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let base = manifest
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let resolve = |key: &str| -> Result<String, String> {
        let raw = value
            .get(key)
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("manifest: missing {key}"))?;
        let path = if Path::new(raw).is_absolute() {
            PathBuf::from(raw)
        } else {
            base.join(raw)
        };
        std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))
    };
    let inputs = resolve("inputs")?;
    let cobol = resolve("cobol_report")?;
    let rewrite = resolve("rewrite_report")?;
    if let Some(raw_paths) = value.get("record_paths").and_then(|v| v.as_str()) {
        let path = if Path::new(raw_paths).is_absolute() {
            PathBuf::from(raw_paths)
        } else {
            base.join(raw_paths)
        };
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let paths = parse_record_paths(&text).map_err(|e| e.to_string())?;
        run_diffrun_with_paths(&inputs, &cobol, &rewrite, &paths).map_err(|e| e.to_string())
    } else {
        let trace = resolve("trace")?;
        let start = value
            .get("record_start")
            .and_then(|v| v.as_str())
            .unwrap_or("PROCESS-ALL");
        let end = value
            .get("record_end")
            .and_then(|v| v.as_str())
            .unwrap_or("WRITE-DETAIL");
        run_diffrun(&inputs, &cobol, &rewrite, &trace, start, end).map_err(|e| e.to_string())
    }
}

fn money(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.unsigned_abs() as i64;
    format!("{sign}${}.{:02}", abs / 100, abs % 100)
}

/// Render the Markdown report: the run's counts and money plus each flag
/// with its blamed paragraphs and first mismatching records.
pub fn render_markdown(
    program_path: &str,
    run_path: &str,
    outcome: &DiffrunOutcome,
    flags: &[crate::evidence::diffrun::DiffrunFlag],
) -> String {
    use crate::core::Severity;
    let mut out = String::new();
    out.push_str("# doyle diffrun\n\n");
    out.push_str(&format!("Program: {program_path}\n"));
    out.push_str(&format!("Run: {run_path}\n\n"));
    out.push_str(&format!(
        "Records compared: {}; mismatches: {}; rewrite overcharge: {}.\n",
        outcome.record_count,
        outcome.mismatches.len(),
        money(total_overcharge_cents(outcome)),
    ));
    if let (Some(cobol), Some(rewrite)) = (&outcome.totals_cobol, &outcome.totals_rewrite) {
        out.push_str(&format!(
            "Totals: batch {} records, fees {}; rewrite {} records, fees {}.\n",
            cobol.records,
            money(cobol.fee_cents),
            rewrite.records,
            money(rewrite.fee_cents),
        ));
    }
    if flags.is_empty() {
        out.push_str("\nNo flags.\n");
        return out;
    }
    out.push_str(&format!("\n## Flags ({})\n\n", flags.len()));
    for f in flags {
        let severity = match f.flag.severity {
            Severity::Breaking => "breaking",
            Severity::Review => "review",
        };
        out.push_str(&format!(
            "### {} — {} ({}) — {}\n",
            f.flag.id, f.flag.memory.id, f.flag.memory.adapter, severity
        ));
        out.push_str(&format!(
            "- Changes: {}\n",
            f.flag
                .changes
                .iter()
                .map(|c| c.0.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        out.push_str(&format!(
            "- Blamed paragraphs: {}\n",
            f.paragraphs.join(", ")
        ));
        out.push_str(&format!("- Mismatching records: {}\n", f.mismatches.len()));
        const SHOWN: usize = 10;
        for m in f.mismatches.iter().take(SHOWN) {
            out.push_str(&format!("- {}\n", mismatch_evidence(m)));
        }
        if f.mismatches.len() > SHOWN {
            out.push_str(&format!(
                "- (showing the first {} of {}; every record is in the run this flag names)\n",
                SHOWN,
                f.mismatches.len()
            ));
        }
        out.push('\n');
    }
    out
}

/// Render the JSON report: the run's counts and money plus each flag with
/// its blamed paragraphs and every mismatching record as evidence.
pub fn render_json(
    program_path: &str,
    run_path: &str,
    outcome: &DiffrunOutcome,
    flags: &[crate::evidence::diffrun::DiffrunFlag],
) -> String {
    let mismatch_value = |m: &Mismatch| {
        serde_json::json!({
            "index": m.index,
            "acct": m.acct,
            "input": m.input,
            "cobol": m.cobol,
            "rewrite": m.rewrite,
            "cobol_fee_cents": m.cobol_fee_cents,
            "rewrite_fee_cents": m.rewrite_fee_cents,
            "paragraphs": m.paragraphs,
        })
    };
    let flag_values: Vec<serde_json::Value> = flags
        .iter()
        .map(|f| {
            serde_json::json!({
                "id": f.flag.id,
                "memory": {
                    "adapter": f.flag.memory.adapter,
                    "id": f.flag.memory.id,
                },
                "changes": f.flag.changes.iter().map(|c| c.0.clone()).collect::<Vec<_>>(),
                "severity": f.flag.severity,
                "old_version": f.flag.old_version,
                "new_version": f.flag.new_version,
                "evidence": f.flag.evidence,
                "paragraphs": f.paragraphs,
                "mismatch_count": f.mismatches.len(),
                "mismatches": f.mismatches.iter().map(mismatch_value).collect::<Vec<_>>(),
            })
        })
        .collect();
    let mut value = serde_json::json!({
        "program": program_path,
        "run": run_path,
        "record_count": outcome.record_count,
        "mismatch_count": outcome.mismatches.len(),
        "overcharge_cents": total_overcharge_cents(outcome),
        "flags": flag_values,
    });
    if let (Some(cobol), Some(rewrite)) = (&outcome.totals_cobol, &outcome.totals_rewrite) {
        value["totals_cobol"] = serde_json::json!({
            "records": cobol.records,
            "balance_cents": cobol.balance_cents,
            "fee_cents": cobol.fee_cents,
        });
        value["totals_rewrite"] = serde_json::json!({
            "records": rewrite.records,
            "balance_cents": rewrite.balance_cents,
            "fee_cents": rewrite.fee_cents,
        });
    }
    let mut s = serde_json::to_string_pretty(&value).unwrap_or_else(|_| String::from("{}"));
    s.push('\n');
    s
}
