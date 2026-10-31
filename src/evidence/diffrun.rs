//! Evidence by execution: the differential run.
//!
//! Given the COBOL batch, a rewrite and a set of inputs: run both, compare
//! outputs record by record, and for each mismatch read the paragraph trace
//! (T9.1's `-ftrace` + `COB_SET_TRACE=1` convention) to find which paragraphs
//! ran. A memory citing a paragraph on a mismatched path is flagged, with the
//! inputs and both outputs as its evidence.
//!
//! Running COBOL is live and happens only by hand. The gate runs on the
//! recordings committed under `testdata/legacy/diffrun/` (the mini run's real
//! `-ftrace` output and both rewrites' real reports, plus the full run's
//! per-record paragraph paths derived from its real trace by the recipe in
//! that directory's README). Doyle never writes the rewrite: in tests the
//! rewrite is a fixture (the story's `wonder-bill.rs` / `fixed-bill.rs`); in
//! the demo it is Wonder's.
//!
//! The flag is still computed, never a model's word. The mismatching records
//! blame the paragraphs that ran only on mismatched paths
//! ([`exclusive_paragraphs`]), those name legacy paragraph elements through
//! the T9.6 index, and [`flag_citations`](crate::core::flag_citations) flags
//! the memories citing them — the same path every source flags through. The
//! record-level evidence (the input line, both outputs, the paragraphs that
//! ran) rides alongside each flag in [`DiffrunFlag`].
//!
//! ## The record-path convention
//!
//! A record's path runs from the per-record driver to the per-record write
//! (`PROCESS-ALL` .. `WRITE-DETAIL` for BILLBATCH, taken as parameters by
//! [`split_trace`]). The run prologue, the `READ-NEXT`s between records and
//! the totals epilogue are loop scaffolding: shared by every record or none,
//! they can never discriminate a mismatch, so they belong to no record and
//! are collected in [`TraceSplit::outside`]. A paragraph that runs on both
//! mismatched and clean records likewise implicates no memory: only a branch
//! paragraph exclusive to mismatched paths becomes a change fact. That is why
//! the story's control holds — `CALC-LATE-FEE` ran only on clean records, and
//! `WRITE-DETAIL`, `TALLY-BAND` and the rest ran on both — while `LOG-EXEMPT`
//! alone blames the wrong rewrite.

use std::collections::{BTreeMap, BTreeSet};

use super::legacy::{LegacyIndex, ids as legacy_ids};
use super::{EvidenceChange, EvidenceChangeKind};
use crate::core::{Citation, Flag, Severity, flag_citations};

/// Running the comparison failed: which recording, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffrunError(pub String);

impl std::fmt::Display for DiffrunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "diffrun: {}", self.0)
    }
}

impl std::error::Error for DiffrunError {}

/// One parsed detail line: the account, its money in cents, and the raw line
/// for evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportRow {
    /// The six-digit account number.
    pub acct: String,
    /// The balance in cents.
    pub balance_cents: i64,
    /// The fee in cents.
    pub fee_cents: i64,
    /// The detail line verbatim.
    pub raw: String,
}

/// A parsed `TOTALS` line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Totals {
    /// Records the run wrote detail lines for.
    pub records: usize,
    /// The balance total in cents.
    pub balance_cents: i64,
    /// The fee total in cents.
    pub fee_cents: i64,
}

/// One mismatching record: the inputs and both outputs that prove it, and
/// the paragraphs that ran for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    /// The record's position, zero-based, in input, report and trace order.
    pub index: usize,
    /// The six-digit account number.
    pub acct: String,
    /// The input line (`patients.dat`) verbatim.
    pub input: String,
    /// The batch's detail line verbatim.
    pub cobol: String,
    /// The rewrite's detail line verbatim.
    pub rewrite: String,
    /// The fee the batch printed, in cents.
    pub cobol_fee_cents: i64,
    /// The fee the rewrite printed, in cents.
    pub rewrite_fee_cents: i64,
    /// The record's ordered paragraph path from the trace.
    pub paragraphs: Vec<String>,
}

/// The outcome of one differential run: every compared record's path and the
/// mismatches with their evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffrunOutcome {
    /// Records compared (inputs, both reports and trace paths agreed).
    pub record_count: usize,
    /// The mismatching records, in record order, with their evidence.
    pub mismatches: Vec<Mismatch>,
    /// Every record's ordered paragraph path, in record order.
    pub paths: Vec<Vec<String>>,
    /// The batch report's `TOTALS` line, when the recording holds one.
    pub totals_cobol: Option<Totals>,
    /// The rewrite report's `TOTALS` line, when the recording holds one.
    pub totals_rewrite: Option<Totals>,
}

/// One flag with its record-level evidence: the flag itself (through the same
/// [`flag_citations`] every source uses) plus the paragraphs behind it and
/// the mismatching records that ran them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffrunFlag {
    /// The computed flag: the citing memory and the diffrun change facts.
    pub flag: Flag,
    /// The mismatched-path paragraphs behind this flag, sorted.
    pub paragraphs: Vec<String>,
    /// The mismatching records that ran those paragraphs, in record order.
    pub mismatches: Vec<Mismatch>,
}

/// `$1,745.63` to cents. Rejects anything without the `$` the batch writes.
fn parse_money(token: &str) -> Result<i64, DiffrunError> {
    let digits = token
        .strip_prefix('$')
        .ok_or_else(|| DiffrunError(format!("not money: {token:?}")))?;
    if digits.contains('-') {
        return Err(DiffrunError(format!("negative money: {token:?}")));
    }
    let plain: String = digits.chars().filter(|c| *c != ',').collect();
    let (dollars, cents) = match plain.split_once('.') {
        Some((d, c)) => (d, c),
        None => return Err(DiffrunError(format!("no cents: {token:?}"))),
    };
    if dollars.is_empty() || !dollars.chars().all(|c| c.is_ascii_digit()) {
        return Err(DiffrunError(format!("bad dollars: {token:?}")));
    }
    let cents = match cents.len() {
        2 if cents.chars().all(|c| c.is_ascii_digit()) => cents.to_string(),
        1 if cents.chars().all(|c| c.is_ascii_digit()) => format!("{cents}0"),
        _ => return Err(DiffrunError(format!("bad cents: {token:?}"))),
    };
    let dollars: i64 = dollars
        .parse()
        .map_err(|_| DiffrunError(format!("bad dollars: {token:?}")))?;
    let cents: i64 = cents
        .parse()
        .map_err(|_| DiffrunError(format!("bad cents: {token:?}")))?;
    Ok(dollars * 100 + cents)
}

/// Parse a report's detail lines (`ACCT ... BALANCE $... FEE $...`). Headers,
/// `TOTALS`, summary and footer lines are not detail lines and are skipped.
/// A line starting with `ACCT ` that does not parse is an error naming it.
pub fn parse_report(text: &str) -> Result<Vec<ReportRow>, DiffrunError> {
    let mut rows = Vec::new();
    for line in text.lines() {
        if !line.starts_with("ACCT ") {
            continue;
        }
        let excerpt: String = line.chars().take(60).collect();
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let acct = tokens
            .get(1)
            .ok_or_else(|| DiffrunError(format!("detail line without account: {excerpt:?}")))?;
        let balance_token = tokens
            .iter()
            .position(|t| *t == "BALANCE")
            .and_then(|i| tokens.get(i + 1))
            .ok_or_else(|| DiffrunError(format!("detail line without balance: {excerpt:?}")))?;
        let fee_token = tokens
            .iter()
            .position(|t| *t == "FEE")
            .and_then(|i| tokens.get(i + 1))
            .ok_or_else(|| DiffrunError(format!("detail line without fee: {excerpt:?}")))?;
        rows.push(ReportRow {
            acct: (*acct).to_string(),
            balance_cents: parse_money(balance_token)?,
            fee_cents: parse_money(fee_token)?,
            raw: line.to_string(),
        });
    }
    Ok(rows)
}

/// Parse a report's `TOTALS` line. Missing is an error: every recording the
/// gate reads (batch and rewrite alike) carries one.
pub fn parse_totals(text: &str) -> Result<Totals, DiffrunError> {
    for line in text.lines() {
        if !line.starts_with("TOTALS") {
            continue;
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let records = tokens
            .iter()
            .position(|t| *t == "RECORDS")
            .and_then(|i| tokens.get(i + 1))
            .and_then(|n| n.parse::<usize>().ok())
            .ok_or_else(|| DiffrunError(format!("bad TOTALS count: {line:?}")))?;
        let balance_cents = tokens
            .iter()
            .position(|t| *t == "BALANCE")
            .and_then(|i| tokens.get(i + 1))
            .ok_or_else(|| DiffrunError(format!("TOTALS without balance: {line:?}")))?;
        let fee_cents = tokens
            .iter()
            .position(|t| *t == "FEE")
            .and_then(|i| tokens.get(i + 1))
            .ok_or_else(|| DiffrunError(format!("TOTALS without fee: {line:?}")))?;
        return Ok(Totals {
            records,
            balance_cents: parse_money(balance_cents)?,
            fee_cents: parse_money(fee_cents)?,
        });
    }
    Err(DiffrunError("no TOTALS line".to_string()))
}

/// Compare two parsed reports record by record: the indices where the
/// outputs differ. Length or account-order drift is not a fee mismatch but a
/// broken pairing, so it is an error naming the first divergence.
pub fn compare_reports(
    cobol: &[ReportRow],
    rewrite: &[ReportRow],
) -> Result<Vec<usize>, DiffrunError> {
    if cobol.len() != rewrite.len() {
        return Err(DiffrunError(format!(
            "row count drift: batch {} vs rewrite {}",
            cobol.len(),
            rewrite.len()
        )));
    }
    for (index, (left, right)) in cobol.iter().zip(rewrite.iter()).enumerate() {
        if left.acct != right.acct {
            return Err(DiffrunError(format!(
                "account drift at record {index}: batch {} vs rewrite {}",
                left.acct, right.acct
            )));
        }
    }
    Ok(cobol
        .iter()
        .zip(rewrite.iter())
        .enumerate()
        .filter(|(_, (left, right))| {
            left.balance_cents != right.balance_cents || left.fee_cents != right.fee_cents
        })
        .map(|(index, _)| index)
        .collect())
}

/// Parse a `-ftrace` stderr recording into paragraph names in run order: one
/// entry per `Paragraph:` line. Anything else (the `Source` line, the bare
/// `Program-Id` lines, the `Entry` line) carries no paragraph and is skipped.
pub fn parse_trace(text: &str) -> Vec<String> {
    let mut paragraphs = Vec::new();
    for line in text.lines() {
        let Some(start) = line.find("Paragraph:") else {
            continue;
        };
        let after = &line[start + "Paragraph:".len()..];
        let name = match after.find("Line:") {
            Some(end) => after[..end].trim(),
            None => after.trim(),
        };
        if !name.is_empty() {
            paragraphs.push(name.to_string());
        }
    }
    paragraphs
}

/// A split trace: one ordered path per record plus the scaffolding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceSplit {
    /// One ordered paragraph path per record, in run order.
    pub records: Vec<Vec<String>>,
    /// Everything outside any record: the run prologue, the reads between
    /// records and the totals epilogue.
    pub outside: Vec<String>,
}

/// Split paragraph events into per-record paths: a path opens at
/// `record_start` and closes with `record_end` inclusive. For BILLBATCH both
/// ends are `PROCESS-ALL` .. `WRITE-DETAIL`. A trace with no events is the
/// T9.1 footgun — compiled without `-ftrace` or run without
/// `COB_SET_TRACE=1` — and surfaces downstream as a count mismatch, never as
/// a silent clean run.
pub fn split_trace(paragraphs: &[String], record_start: &str, record_end: &str) -> TraceSplit {
    let mut records = Vec::new();
    let mut outside = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut open = false;
    for paragraph in paragraphs {
        if !open {
            if paragraph == record_start {
                open = true;
                current.push(paragraph.clone());
                if record_start == record_end {
                    records.push(std::mem::take(&mut current));
                    open = false;
                }
                continue;
            }
            outside.push(paragraph.clone());
            continue;
        }
        current.push(paragraph.clone());
        if paragraph == record_end {
            records.push(std::mem::take(&mut current));
            open = false;
        }
    }
    outside.extend(current);
    TraceSplit { records, outside }
}

/// Parse the committed form of a full-run trace: one line per record, the
/// account plus its ordered paragraph path (`full-record-paths.txt`). Comment
/// and blank lines are skipped; a line without a path is an error naming it.
pub fn parse_record_paths(text: &str) -> Result<Vec<(String, Vec<String>)>, DiffrunError> {
    let mut paths = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let acct = parts
            .next()
            .ok_or_else(|| DiffrunError(format!("record path without account: {line:?}")))?;
        let path: Vec<String> = parts.map(str::to_string).collect();
        if path.is_empty() {
            return Err(DiffrunError(format!(
                "record path without paragraphs: {line:?}"
            )));
        }
        paths.push((acct.to_string(), path));
    }
    Ok(paths)
}

/// Run the comparison on parsed inputs: detail rows from both reports plus
/// every record's paragraph path. Inputs, both reports and paths must agree
/// record for record; any drift is an error, never a silent pass.
pub fn run_diffrun_with_paths(
    inputs: &str,
    cobol_report: &str,
    rewrite_report: &str,
    paths: &[(String, Vec<String>)],
) -> Result<DiffrunOutcome, DiffrunError> {
    let input_lines: Vec<String> = inputs
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .map(str::to_string)
        .collect();
    let cobol_rows = parse_report(cobol_report)?;
    let rewrite_rows = parse_report(rewrite_report)?;
    if input_lines.len() != cobol_rows.len() {
        return Err(DiffrunError(format!(
            "inputs vs batch report: {} vs {}",
            input_lines.len(),
            cobol_rows.len()
        )));
    }
    if paths.len() != cobol_rows.len() {
        return Err(DiffrunError(format!(
            "trace paths vs batch report: {} vs {}",
            paths.len(),
            cobol_rows.len()
        )));
    }
    let mismatched = compare_reports(&cobol_rows, &rewrite_rows)?;
    let mismatches = mismatched
        .into_iter()
        .map(|index| {
            let path = &paths[index];
            if path.0 != cobol_rows[index].acct {
                return Err(DiffrunError(format!(
                    "trace path drift at record {index}: {} vs {}",
                    path.0, cobol_rows[index].acct
                )));
            }
            Ok(Mismatch {
                index,
                acct: cobol_rows[index].acct.clone(),
                input: input_lines[index].clone(),
                cobol: cobol_rows[index].raw.clone(),
                rewrite: rewrite_rows[index].raw.clone(),
                cobol_fee_cents: cobol_rows[index].fee_cents,
                rewrite_fee_cents: rewrite_rows[index].fee_cents,
                paragraphs: path.1.clone(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let totals_cobol = parse_totals(cobol_report).ok();
    let totals_rewrite = parse_totals(rewrite_report).ok();
    for (label, totals) in [("batch", &totals_cobol), ("rewrite", &totals_rewrite)] {
        if let Some(totals) = totals
            && totals.records != cobol_rows.len()
        {
            return Err(DiffrunError(format!(
                "{label} TOTALS says {} records but the report holds {}",
                totals.records,
                cobol_rows.len()
            )));
        }
    }
    Ok(DiffrunOutcome {
        record_count: cobol_rows.len(),
        mismatches,
        paths: paths.iter().map(|(_, path)| path.clone()).collect(),
        totals_cobol,
        totals_rewrite,
    })
}

/// Run the comparison on a raw trace recording: parse, split and compare.
pub fn run_diffrun(
    inputs: &str,
    cobol_report: &str,
    rewrite_report: &str,
    trace: &str,
    record_start: &str,
    record_end: &str,
) -> Result<DiffrunOutcome, DiffrunError> {
    let paragraphs = parse_trace(trace);
    if paragraphs.is_empty() {
        return Err(DiffrunError(
            "no paragraph events: compile with -ftrace and run with COB_SET_TRACE=1".to_string(),
        ));
    }
    let split = split_trace(&paragraphs, record_start, record_end);
    let accts = parse_report(cobol_report)?
        .into_iter()
        .map(|row| row.acct)
        .collect::<Vec<_>>();
    if split.records.len() != accts.len() {
        return Err(DiffrunError(format!(
            "trace paths vs batch report: {} vs {}",
            split.records.len(),
            accts.len()
        )));
    }
    let paths: Vec<(String, Vec<String>)> = accts.into_iter().zip(split.records).collect();
    run_diffrun_with_paths(inputs, cobol_report, rewrite_report, &paths)
}

/// The blame: paragraphs that ran on at least one mismatched path and on no
/// clean path — the branch paragraphs where the runs diverge. Sorted, so the
/// change facts derived from them are deterministic.
pub fn exclusive_paragraphs(outcome: &DiffrunOutcome) -> Vec<String> {
    let mismatched: BTreeSet<&str> = outcome
        .mismatches
        .iter()
        .flat_map(|m| m.paragraphs.iter().map(String::as_str))
        .collect();
    let clean: BTreeSet<&str> = outcome
        .paths
        .iter()
        .enumerate()
        .filter(|(index, _)| !outcome.mismatches.iter().any(|m| m.index == *index))
        .flat_map(|(_, path)| path.iter().map(String::as_str))
        .collect();
    mismatched
        .difference(&clean)
        .map(|s| (*s).to_string())
        .collect()
}

/// The change facts behind a run: one breaking fact per exclusive paragraph,
/// on the paragraph's stable `le2_` ID through the legacy index's program.
/// The kind is the run's own (`diffrun-mismatch`): the code never changed, so
/// no statement-change fact exists — execution is the evidence.
pub fn diffrun_changes(index: &LegacyIndex, outcome: &DiffrunOutcome) -> Vec<EvidenceChange> {
    exclusive_paragraphs(outcome)
        .into_iter()
        .map(|paragraph| {
            let subject =
                legacy_ids::element_id(&legacy_ids::paragraph_key(index.program(), &paragraph));
            EvidenceChange {
                id: legacy_ids::change_id(&subject, "diffrun-mismatch", "", ""),
                subject,
                kind: EvidenceChangeKind::Other("diffrun-mismatch".to_string()),
                severity: Severity::Breaking,
            }
        })
        .collect()
}

/// Flag the cited memories a run has made stale: the exclusive paragraphs
/// become change facts and [`flag_citations`] flags the memories citing
/// them. Each flag carries the paragraphs behind it and the mismatching
/// records that ran them — the inputs and both outputs as evidence.
pub fn flag_diffrun(
    citations: &[Citation],
    index: &LegacyIndex,
    outcome: &DiffrunOutcome,
    open_flags: &[Flag],
    old_version: &str,
    new_version: &str,
) -> Vec<DiffrunFlag> {
    let changes = diffrun_changes(index, outcome);
    // Which paragraph each change fact blames, by change ID.
    let paragraph_of: BTreeMap<String, String> = exclusive_paragraphs(outcome)
        .into_iter()
        .map(|paragraph| {
            let subject =
                legacy_ids::element_id(&legacy_ids::paragraph_key(index.program(), &paragraph));
            let id = legacy_ids::change_id(&subject, "diffrun-mismatch", "", "");
            (id.0, paragraph)
        })
        .collect();
    flag_citations(
        citations,
        &changes,
        index,
        open_flags,
        old_version,
        new_version,
    )
    .into_iter()
    .map(|flag| {
        let paragraphs: BTreeSet<String> = flag
            .changes
            .iter()
            .filter_map(|id| paragraph_of.get(&id.0))
            .cloned()
            .collect();
        let mismatches = outcome
            .mismatches
            .iter()
            .filter(|m| m.paragraphs.iter().any(|p| paragraphs.contains(p)))
            .cloned()
            .collect();
        DiffrunFlag {
            flag,
            paragraphs: paragraphs.into_iter().collect(),
            mismatches,
        }
    })
    .collect()
}

/// The net overcharge across the mismatching records: rewrite minus batch, in
/// cents. Positive means the rewrite bills more.
pub fn total_overcharge_cents(outcome: &DiffrunOutcome) -> i64 {
    outcome
        .mismatches
        .iter()
        .map(|m| m.rewrite_fee_cents - m.cobol_fee_cents)
        .sum()
}

/// One mismatching record as evidence: the input line, both outputs and the
/// paragraphs that ran.
pub fn mismatch_evidence(mismatch: &Mismatch) -> String {
    format!(
        "ACCT {} input `{}` COBOL `{}` rewrite `{}` ran {}",
        mismatch.acct,
        mismatch.input,
        mismatch.cobol,
        mismatch.rewrite,
        mismatch.paragraphs.join(" ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Citation, ElementId, MemoryRef};
    use crate::evidence::legacy::LegacyIndex;

    fn diffrun_fixture(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/testdata/legacy/diffrun/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("diffrun fixture reads")
    }

    fn story_fixture(path: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/testdata/legacy/story/{path}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("story fixture reads")
    }

    fn story_v1_index() -> LegacyIndex {
        LegacyIndex::load(format!(
            "{}/testdata/legacy/story/v1/BILLBATCH.cbl",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("v1 loads")
    }

    fn story_citation(memory: &str, claim: &str, elements: &[ElementId]) -> Citation {
        Citation {
            memory: MemoryRef {
                adapter: "muse-code".to_string(),
                id: memory.to_string(),
            },
            elements: elements.to_vec(),
            claim: claim.to_string(),
            recorded_by: "curator".to_string(),
            confidence: 0.9,
            source_text: claim.to_string(),
        }
    }

    /// The story's L-memories through the T9.6 citations: L1 cites the fee
    /// path including the exemption, L2 the record layout, L3 the fee
    /// paragraph alone, L4 the v2-only discount elements.
    fn story_citations() -> Vec<Citation> {
        let el = legacy_ids::element_id;
        let para = |name: &str| el(&legacy_ids::paragraph_key("BILLBATCH", name));
        let field = |path: &str| el(&legacy_ids::field_key("BILLBATCH", path));
        vec![
            story_citation(
                "L1",
                "1.5% late fee on the whole balance",
                &[
                    para("CALC-LATE-FEE"),
                    field("PATIENT-RECORD.BALANCE"),
                    para("LOG-EXEMPT"),
                    field("PATIENT-RECORD.VISIT-CODE"),
                ],
            ),
            story_citation(
                "L2",
                "each record is 34 chars",
                &[
                    el(&legacy_ids::record_key("BILLBATCH", "PATIENT-RECORD")),
                    field("PATIENT-RECORD.ACCT-NO"),
                    field("PATIENT-RECORD.ACCT-NAME"),
                    field("PATIENT-RECORD.BALANCE"),
                    field("PATIENT-RECORD.VISIT-CODE"),
                ],
            ),
            story_citation(
                "L3",
                "CALC-LATE-FEE computes 1.5% of BALANCE",
                &[para("CALC-LATE-FEE"), field("PATIENT-RECORD.BALANCE")],
            ),
            story_citation(
                "L4",
                "prompt-payment accounts keep 10% of the fee",
                &[
                    para("APPLY-PROMPT-DISCOUNT"),
                    field("PATIENT-RECORD.DISCOUNT-FLAG"),
                ],
            ),
        ]
    }

    /// The Done-when run on the mini recordings: the wrong rewrite mismatches
    /// exactly the two exempt records, each blamed on LOG-EXEMPT, and L1
    /// alone flags with both records as evidence.
    #[test]
    fn mini_wrong_rewrite_flags_with_record_evidence() {
        let outcome = run_diffrun(
            &diffrun_fixture("mini-inputs.dat"),
            &diffrun_fixture("mini-cobol-report.txt"),
            &diffrun_fixture("mini-wrong-report.txt"),
            &diffrun_fixture("mini-trace.txt"),
            "PROCESS-ALL",
            "WRITE-DETAIL",
        )
        .expect("mini run compares");
        assert_eq!(outcome.record_count, 10);
        assert_eq!(outcome.paths.len(), 10);
        assert_eq!(outcome.mismatches.len(), 2, "{:?}", outcome.mismatches);
        let accts: Vec<&str> = outcome.mismatches.iter().map(|m| m.acct.as_str()).collect();
        assert_eq!(accts, ["000003", "000009"]);
        // Both mismatches ran the exemption branch and never the fee path.
        for mismatch in &outcome.mismatches {
            assert_eq!(
                mismatch.paragraphs,
                [
                    "PROCESS-ALL",
                    "VALIDATE-RECORD",
                    "CHARGE-ACCOUNT",
                    "LOG-EXEMPT",
                    "TALLY-BAND",
                    "WRITE-DETAIL",
                ]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>(),
                "record {}",
                mismatch.acct
            );
            // The inputs are vaccination visits; the batch printed no fee.
            assert_eq!(
                mismatch.input.as_bytes()[33],
                b'V',
                "record {}",
                mismatch.acct
            );
            assert_eq!(mismatch.cobol_fee_cents, 0);
            assert!(mismatch.rewrite_fee_cents > 0);
            assert!(mismatch.cobol.contains("FEE      $0.00"));
        }
        // The surrounding records ran the fee paragraph.
        assert!(outcome.paths[0].contains(&"CALC-LATE-FEE".to_string()));
        assert!(outcome.paths[9].contains(&"CALC-LATE-FEE".to_string()));
        // Totals cross-check the recordings: 62.02 batch, 68.96 wrong.
        assert_eq!(
            outcome
                .totals_cobol
                .as_ref()
                .expect("batch totals")
                .fee_cents,
            6202
        );
        assert_eq!(
            outcome
                .totals_rewrite
                .as_ref()
                .expect("rewrite totals")
                .fee_cents,
            6896
        );
        assert_eq!(total_overcharge_cents(&outcome), 694);

        // Blame is exactly the exemption paragraph.
        assert_eq!(exclusive_paragraphs(&outcome), ["LOG-EXEMPT"]);
        let index = story_v1_index();
        let changes = diffrun_changes(&index, &outcome);
        assert_eq!(changes.len(), 1);
        let log_exempt =
            legacy_ids::element_id(&legacy_ids::paragraph_key("BILLBATCH", "LOG-EXEMPT"));
        assert_eq!(changes[0].subject, log_exempt);
        assert_eq!(changes[0].severity, Severity::Breaking);
        assert!(
            matches!(changes[0].kind, EvidenceChangeKind::Other(ref k) if k == "diffrun-mismatch")
        );
        assert_eq!(changes[0].id.0.len(), 4 + 64);

        // L1 alone flags, with both records as evidence.
        let flags = flag_diffrun(
            &story_citations(),
            &index,
            &outcome,
            &[],
            "batch-v1",
            "rewrite-wrong",
        );
        assert_eq!(flags.len(), 1, "{flags:?}");
        assert_eq!(flags[0].flag.memory.id, "L1");
        assert_eq!(flags[0].flag.severity, Severity::Breaking);
        assert_eq!(flags[0].flag.changes, vec![changes[0].id.clone()]);
        assert_eq!(flags[0].paragraphs, ["LOG-EXEMPT"]);
        assert_eq!(flags[0].mismatches.len(), 2);
        let evidence = mismatch_evidence(&flags[0].mismatches[0]);
        assert!(
            evidence.starts_with("ACCT 000003 input `000003Patient 00003"),
            "{evidence}"
        );
        assert!(evidence.contains("FEE      $0.00"));
        assert!(evidence.contains("FEE      $3.75"));
        assert!(
            evidence.ends_with(
                "ran PROCESS-ALL VALIDATE-RECORD CHARGE-ACCOUNT LOG-EXEMPT TALLY-BAND WRITE-DETAIL"
            ),
            "{evidence}"
        );
    }

    /// The control on the mini recordings: the correct rewrite mismatches
    /// nothing, writes no fact, and flags nothing.
    #[test]
    fn mini_fixed_rewrite_flags_nothing() {
        let outcome = run_diffrun(
            &diffrun_fixture("mini-inputs.dat"),
            &diffrun_fixture("mini-cobol-report.txt"),
            &diffrun_fixture("mini-fixed-report.txt"),
            &diffrun_fixture("mini-trace.txt"),
            "PROCESS-ALL",
            "WRITE-DETAIL",
        )
        .expect("mini fixed run compares");
        assert_eq!(outcome.record_count, 10);
        assert!(outcome.mismatches.is_empty());
        assert!(exclusive_paragraphs(&outcome).is_empty());
        let index = story_v1_index();
        assert!(diffrun_changes(&index, &outcome).is_empty());
        let flags = flag_diffrun(
            &story_citations(),
            &index,
            &outcome,
            &[],
            "batch-v1",
            "rewrite-fixed",
        );
        assert!(flags.is_empty(), "{flags:?}");
        assert_eq!(outcome.totals_rewrite, outcome.totals_cobol);
    }

    /// The trace split on the real mini trace: ten record paths plus the
    /// loop scaffolding outside any record.
    #[test]
    fn mini_trace_splits_into_ten_paths() {
        let paragraphs = parse_trace(&diffrun_fixture("mini-trace.txt"));
        assert_eq!(paragraphs.len(), 78);
        let split = split_trace(&paragraphs, "PROCESS-ALL", "WRITE-DETAIL");
        assert_eq!(split.records.len(), 10);
        assert!(split.records.iter().all(|path| path.len() == 6));
        assert_eq!(split.records[2][3], "LOG-EXEMPT");
        assert_eq!(split.records[8][3], "LOG-EXEMPT");
        assert!(
            split
                .records
                .iter()
                .enumerate()
                .all(|(i, path)| { (i == 2 || i == 8) == (path[3] == "LOG-EXEMPT") })
        );
        // Prologue, the reads between records, and the totals epilogue.
        assert_eq!(split.outside.len(), 18);
        assert_eq!(
            &split.outside[..4],
            ["MAIN-PARA", "OPEN-FILES", "WRITE-HEADER", "READ-NEXT"]
        );
        assert_eq!(
            &split.outside[split.outside.len() - 5..],
            [
                "READ-NEXT",
                "WRITE-TOTALS",
                "WRITE-SUMMARY",
                "VERIFY-TOTALS",
                "CLOSE-FILES"
            ]
        );
        assert!(
            split.outside[4..split.outside.len() - 5]
                .iter()
                .all(|p| p == "READ-NEXT")
        );
    }

    /// The Done-when run at story scale: 535 mismatching vaccination visits
    /// blame LOG-EXEMPT, L1 alone flags with every record as evidence, and
    /// the fixed side converges.
    #[test]
    fn full_story_run_blames_the_exemption_path() {
        let inputs = story_fixture("v1/patients.dat");
        let cobol = story_fixture("v1/report.txt");
        let rewrite = diffrun_fixture("full-wrong-report.txt");
        let paths = parse_record_paths(&diffrun_fixture("full-record-paths.txt"))
            .expect("record paths parse");
        assert_eq!(paths.len(), 4812);
        let outcome =
            run_diffrun_with_paths(&inputs, &cobol, &rewrite, &paths).expect("full run compares");
        assert_eq!(outcome.record_count, 4812);
        assert_eq!(outcome.mismatches.len(), 535);

        // The mismatch accounts are exactly the story's recorded 535.
        let recorded: Vec<String> = story_fixture("rewrite/mismatches.txt")
            .lines()
            .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
            .map(|line| line.split_whitespace().next().expect("acct").to_string())
            .collect();
        assert_eq!(recorded.len(), 535);
        let mismatched: Vec<String> = outcome.mismatches.iter().map(|m| m.acct.clone()).collect();
        assert_eq!(mismatched, recorded);

        // Every mismatch is a vaccination visit the batch exempted, charged
        // 1.5% rounded by the wrong rewrite; every other record ran the fee.
        let input_of: std::collections::HashMap<&str, &str> = inputs
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| (&line[0..6], line))
            .collect();
        for mismatch in &outcome.mismatches {
            assert!(mismatch.paragraphs.contains(&"LOG-EXEMPT".to_string()));
            assert!(!mismatch.paragraphs.contains(&"CALC-LATE-FEE".to_string()));
            assert_eq!(mismatch.input.as_bytes()[33], b'V');
            assert_eq!(mismatch.cobol_fee_cents, 0);
            assert_eq!(mismatch.input, input_of[mismatch.acct.as_str()]);
            let balance: i64 = mismatch.input[26..33].parse().expect("balance digits");
            assert_eq!((balance * 15 + 500) / 1000, mismatch.rewrite_fee_cents);
        }
        let clean_branches = outcome
            .paths
            .iter()
            .enumerate()
            .filter(|(index, _)| !outcome.mismatches.iter().any(|m| m.index == *index))
            .count();
        assert_eq!(clean_branches, 4277);
        for (index, path) in outcome.paths.iter().enumerate() {
            if outcome.mismatches.iter().any(|m| m.index == index) {
                continue;
            }
            assert!(
                path.contains(&"CALC-LATE-FEE".to_string()),
                "record {index}"
            );
        }
        // Totals and overcharge match the recorded evidence: 63,904.43 vs
        // 72,213.23, $8,308.80 across 535 statements.
        assert_eq!(
            outcome
                .totals_cobol
                .as_ref()
                .expect("batch totals")
                .fee_cents,
            6_390_443
        );
        assert_eq!(
            outcome
                .totals_rewrite
                .as_ref()
                .expect("rewrite totals")
                .fee_cents,
            7_221_323
        );
        assert_eq!(total_overcharge_cents(&outcome), 830_880);

        // L1 alone flags, with all 535 records as evidence.
        assert_eq!(exclusive_paragraphs(&outcome), ["LOG-EXEMPT"]);
        let index = story_v1_index();
        let flags = flag_diffrun(
            &story_citations(),
            &index,
            &outcome,
            &[],
            "batch-v1",
            "rewrite-wrong",
        );
        assert_eq!(flags.len(), 1, "{flags:?}");
        assert_eq!(flags[0].flag.memory.id, "L1");
        assert_eq!(flags[0].mismatches.len(), 535);
        assert_eq!(flags[0].paragraphs, ["LOG-EXEMPT"]);
    }

    /// The full-scale control: the fixed rewrite's output is byte-identical
    /// to the batch's (the recorded `cmp`-clean claim in `evidence.txt`),
    /// and identical outputs mismatch nothing and flag nothing.
    #[test]
    fn full_fixed_rewrite_converges_to_zero_mismatches() {
        let cobol = story_fixture("v1/report.txt");
        let detail: Vec<&str> = cobol
            .lines()
            .filter(|line| line.starts_with("ACCT "))
            .collect();
        assert_eq!(detail.len(), 4812);
        let totals = cobol
            .lines()
            .find(|line| line.starts_with("TOTALS"))
            .expect("TOTALS");
        let fixed = format!("{}\n{totals}\n", detail.join("\n"));
        let cobol_rows = parse_report(&cobol).expect("batch parses");
        let fixed_rows = parse_report(&fixed).expect("fixed parses");
        assert!(
            compare_reports(&cobol_rows, &fixed_rows)
                .expect("compares")
                .is_empty()
        );
        let empty = DiffrunOutcome {
            record_count: 4812,
            mismatches: Vec::new(),
            paths: Vec::new(),
            totals_cobol: parse_totals(&cobol).ok(),
            totals_rewrite: parse_totals(&fixed).ok(),
        };
        assert!(exclusive_paragraphs(&empty).is_empty());
        let index = story_v1_index();
        assert!(diffrun_changes(&index, &empty).is_empty());
        assert!(
            flag_diffrun(
                &story_citations(),
                &index,
                &empty,
                &[],
                "batch-v1",
                "rewrite-fixed"
            )
            .is_empty()
        );
    }

    #[test]
    fn money_parses_the_batch_format() {
        assert_eq!(parse_money("$1,745.63").expect("parses"), 174_563);
        assert_eq!(parse_money("$0.00").expect("parses"), 0);
        assert_eq!(parse_money("$100.00").expect("parses"), 10_000);
        assert_eq!(parse_money("$4,814,213.63").expect("parses"), 481_421_363);
        for bad in ["1745.63", "$1,74X.63", "$12.345", "$-1.00", "$.00", ""] {
            assert!(parse_money(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn reports_skip_headers_and_require_totals() {
        let rows = parse_report(&diffrun_fixture("mini-cobol-report.txt")).expect("parses");
        assert_eq!(rows.len(), 10);
        assert_eq!(rows[0].acct, "000001");
        assert_eq!((rows[0].balance_cents, rows[0].fee_cents), (10_000, 150));
        let totals = parse_totals(&diffrun_fixture("mini-cobol-report.txt")).expect("totals");
        assert_eq!(
            (totals.records, totals.balance_cents, totals.fee_cents),
            (10, 459_803, 6202)
        );
        assert!(parse_totals("no totals here").is_err());
        assert!(parse_report("ACCT 000001 NAME X BALANCE $1.00\n").is_err());
    }

    #[test]
    fn pairing_drift_is_an_error_not_a_silent_pass() {
        let rows = parse_report(&diffrun_fixture("mini-cobol-report.txt")).expect("parses");
        assert!(compare_reports(&rows, &rows[..9]).is_err());
        let mut reordered = rows.clone();
        reordered.swap(0, 1);
        assert!(compare_reports(&rows, &reordered).is_err());
    }

    #[test]
    fn trace_parsing_skips_non_paragraph_lines() {
        let lines = [
            "Source: 'BILLBATCH.cbl'",
            "Program-Id:  BILLBATCH",
            "Program-Id:  BILLBATCH            Entry: BILLBATCH                       Line:     96",
            "Program-Id:  BILLBATCH        Paragraph: CALC-LATE-FEE  Line:  144",
            "",
        ];
        assert_eq!(parse_trace(&lines.join("\n")), ["CALC-LATE-FEE"]);
        assert!(parse_trace("nothing here\nat all\n").is_empty());
    }

    #[test]
    fn empty_trace_fails_loudly_naming_the_footgun() {
        let error = run_diffrun(
            &diffrun_fixture("mini-inputs.dat"),
            &diffrun_fixture("mini-cobol-report.txt"),
            &diffrun_fixture("mini-wrong-report.txt"),
            "Source: 'BILLBATCH.cbl'\n",
            "PROCESS-ALL",
            "WRITE-DETAIL",
        )
        .expect_err("empty trace must fail");
        assert!(error.0.contains("COB_SET_TRACE=1"), "{}", error.0);
    }

    #[test]
    fn count_drift_between_inputs_report_and_trace_fails() {
        let trace = "Program-Id:  BILLBATCH        Paragraph: PROCESS-ALL  Line:  1\n";
        let error = run_diffrun(
            "only-one-input-line-here",
            &diffrun_fixture("mini-cobol-report.txt"),
            &diffrun_fixture("mini-wrong-report.txt"),
            trace,
            "PROCESS-ALL",
            "WRITE-DETAIL",
        )
        .expect_err("drift must fail");
        assert!(error.0.contains("vs batch report"), "{}", error.0);
    }

    #[test]
    fn record_paths_file_skips_comments_and_blanks() {
        let text = "# a comment\n\n000001 PROCESS-ALL WRITE-DETAIL\n";
        let paths = parse_record_paths(text).expect("parses");
        assert_eq!(paths.len(), 1);
        assert!(parse_record_paths("000001\n").is_err());
    }

    #[test]
    fn shared_path_paragraphs_implicate_no_memory() {
        // Every record ran PROCESS-ALL, mismatched or not: shared paths are
        // never exclusive, so a paragraph on both sides writes no fact even
        // when it also ran on mismatches.
        let outcome = DiffrunOutcome {
            record_count: 2,
            mismatches: vec![Mismatch {
                index: 0,
                acct: "000003".to_string(),
                input: "input".to_string(),
                cobol: "cobol".to_string(),
                rewrite: "rewrite".to_string(),
                cobol_fee_cents: 0,
                rewrite_fee_cents: 375,
                paragraphs: vec!["SHARED".to_string(), "BRANCH-A".to_string()],
            }],
            paths: vec![
                vec!["SHARED".to_string(), "BRANCH-A".to_string()],
                vec!["SHARED".to_string(), "BRANCH-B".to_string()],
            ],
            totals_cobol: None,
            totals_rewrite: None,
        };
        assert_eq!(exclusive_paragraphs(&outcome), ["BRANCH-A"]);
    }
}
