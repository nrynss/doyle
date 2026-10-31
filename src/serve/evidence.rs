//! Per-kind evidence views for the review page (T9.9).
//!
//! The T6.2 page showed every flag as a contract change. A flag from another
//! evidence source (architecture.md, Evidence sources) reads best in its own
//! terms, so this module renders, for each non-contract flag, the evidence
//! the flag rests on:
//!
//! - **screens**: the old and new frames side by side as 80x24 text grids
//!   with the changed elements marked, the element changes listed, and the
//!   cited training-video span playable in the page;
//! - **legacy**: the changed COBOL paragraph (or copybook) before and after,
//!   with the member layout for a record change;
//! - **differential run**: the compared inputs and both outputs, record by
//!   record, with the paragraphs each mismatching record ran.
//!
//! The decide path is untouched: approve / edit / retire / keep still call
//! T5.3's `apply_decision` through the routes in `super`. This module only
//! adds the evidence section and the one route that streams the cited
//! recording (`GET /flag/{id}/evidence/video`).
//!
//! ## How a flag names its evidence
//!
//! Detection is by ID prefix, the seam's own rule (T9.5): `sc2_`/`se2_` is a
//! screen flag, `lc2_`/`le2_` a legacy or differential-run flag, anything
//! else the contract path the page already had. The flag's recorded version
//! strings name where each source's evidence loads from, the way a contract
//! flag's versions name its contract files:
//!
//! - legacy: `old_version` and `new_version` are `.cbl` program paths
//!   ([`LegacyIndex::load`]; copybooks resolve as siblings);
//! - screens: `old_version` and `new_version` are version JSON files in the
//!   vision reply shape (`{"screens": [...], "steps": [...]}`, see
//!   `src/prompts/vision`), carrying two extra keys of display metadata:
//!   `console` (the console name the stable IDs were derived under) and
//!   `recording` (`{"name", "duration_s", "file"}`; `file` names the
//!   recording beside the version file, streamed by the video route). The
//!   frames below are rebuilt from the version's element lists, so no still
//!   image file is needed to show what changed;
//! - differential run: `old_version` is the batch's `.cbl` path (it resolves
//!   the blamed paragraphs' stable IDs) and `new_version` is a run manifest
//!   JSON (`{"inputs", "cobol_report", "rewrite_report"}` plus either
//!   `record_paths` or a raw `trace` with `record_start`/`record_end`;
//!   relative paths resolve beside the manifest) — the recorded-run form
//!   T9.8's gate runs on, never a live compile.
//!
//! A source that cannot be read from here is said so on the page, with the
//! change IDs listed — the same fail-visible rule as the contract fallback.
//! Nothing here calls a model, a compiler, or the network.

use std::path::{Path, PathBuf};

use crate::core::{Citation, Flag};
use crate::evidence::diffrun::{
    DiffrunOutcome, Mismatch, parse_record_paths, run_diffrun, run_diffrun_with_paths,
    total_overcharge_cents,
};
use crate::evidence::legacy::{LegacyChange, LegacyIndex, LegacyKind, LegacySeverity};
use crate::evidence::screens::{
    ElementChangeKind, RecordingMeta, ScreenDiff, ScreenIndex, ScreenStatus, diff_versions,
};
use crate::evidence::{EvidenceSource, NativeLocation};
use crate::prompts::vision::{RawElement, RawScreen, RawStep, RawVersion};

use super::render::esc;
use super::{Response, ReviewServer};

/// Which evidence source a flag came from, by its change and element IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceKind {
    /// Vimanam contracts: the T6.2 change section, unchanged.
    Contract,
    /// COBOL source, diffed (T9.6).
    Legacy,
    /// Console screens and recordings (T9.7).
    Screens,
    /// A differential run's recorded outputs (T9.8).
    Diffrun,
}

/// Classify a flag: screen IDs win, then legacy/diffrun (the manifest
/// extension of `new_version` separates them), else the contract default.
pub fn evidence_kind(flag: &Flag, citation: Option<&Citation>) -> EvidenceKind {
    let has_prefix = |prefix: &str| {
        flag.changes.iter().any(|c| c.0.starts_with(prefix))
            || citation.is_some_and(|cit| cit.elements.iter().any(|e| e.0.starts_with(prefix)))
    };
    if has_prefix("sc2_") || has_prefix("se2_") {
        return EvidenceKind::Screens;
    }
    if has_prefix("lc2_") || has_prefix("le2_") {
        if flag.new_version.ends_with(".json") {
            return EvidenceKind::Diffrun;
        }
        return EvidenceKind::Legacy;
    }
    EvidenceKind::Contract
}

impl ReviewServer {
    /// The flag page's evidence section: the contract change section for a
    /// contract flag, the per-kind evidence view for any other source.
    pub(super) fn evidence_or_change_section(
        &self,
        flag: &Flag,
        citation: Option<&Citation>,
    ) -> String {
        match evidence_kind(flag, citation) {
            EvidenceKind::Contract => self.change_section(flag, citation),
            EvidenceKind::Legacy => legacy_section(flag, citation),
            EvidenceKind::Screens => screens_section(flag, citation),
            EvidenceKind::Diffrun => diffrun_section(flag, citation),
        }
    }

    /// Stream the recording a screen flag's cited span plays from:
    /// `GET /flag/{id}/evidence/video`, the file the old version's
    /// `recording.file` names beside its version file. Anything else 404s.
    pub(super) fn evidence_video(&self, flag_id: &str) -> Response {
        let Some(flag) = self.store.flag(flag_id) else {
            return Response::html(404, not_found_body());
        };
        let citation = self.citation_for(&flag);
        if evidence_kind(&flag, citation.as_ref()) != EvidenceKind::Screens {
            return Response::html(404, not_found_body());
        }
        match load_screens(&flag) {
            Ok(evidence) => match evidence.video_file() {
                Some(path) => match std::fs::read(&path) {
                    Ok(bytes) => Response::media("video/mp4".to_string(), bytes),
                    Err(e) => Response::html(
                        404,
                        super::render::page(
                            "Recording not readable",
                            &format!(
                                "<section class=\"card\"><h1>Recording not readable</h1><p>{}</p><p><a href=\"/flag/{}\">Back to the flag</a>.</p></section>",
                                esc(&e.to_string()),
                                esc(&flag.id)
                            ),
                        ),
                    ),
                },
                None => Response::html(404, not_found_body()),
            },
            Err(_) => Response::html(404, not_found_body()),
        }
    }
}

fn not_found_body() -> String {
    super::render::page(
        "Not found",
        "<section class=\"card\"><h1>Not found</h1><p><a href=\"/\">Back to the review list</a>.</p></section>",
    )
}

/// Extra styling for the evidence views, self-contained so the shared
/// stylesheet in `render.rs` (owned by the T6.2 page) stays untouched.
const EVIDENCE_STYLE: &str = "<style>\
.diff>div{min-width:0}\
.ev-frame{font-size:.72rem;line-height:1.35;white-space:pre;overflow-x:auto;max-width:100%}\
.ev-frame mark{background:var(--warn-bg);color:inherit;font-weight:700;padding:0 .1rem;border-radius:.2rem}\
video.ev-video{width:100%;max-height:22rem;background:#000;border-radius:.45rem}\
@media (max-width:46rem){.ev-frame{font-size:.48rem}}\
.ev-input{white-space:pre-wrap;word-break:break-all}\
</style>";

fn kind_str(kind: &ElementChangeKind) -> &'static str {
    match kind {
        ElementChangeKind::Removed => "removed",
        ElementChangeKind::Added => "added",
        ElementChangeKind::TextChanged => "text changed",
    }
}

fn severity_word(severity: crate::core::Severity) -> &'static str {
    match severity {
        crate::core::Severity::Breaking => "breaking",
        crate::core::Severity::Review => "review",
    }
}

fn location_label(native: &NativeLocation) -> String {
    let mut parts = Vec::new();
    if let (Some(m), Some(p)) = (&native.method, &native.path) {
        parts.push(format!("{m} {p}"));
    }
    if let Some(op) = &native.operation_id {
        parts.push(op.clone());
    }
    if let Some(t) = &native.type_name {
        parts.push(match &native.field_name {
            Some(f) => format!("{t}.{f}"),
            None => t.clone(),
        });
    }
    parts.join(" · ")
}

fn unreadable_section(title: &str, flag: &Flag, reason: &str) -> String {
    let mut out = format!(
        "{EVIDENCE_STYLE}<section class=\"card wide\"><h2>{title}</h2><p class=\"failed\">The evidence files for this flag cannot be read from here: {}</p><p>The change facts behind this flag, by ID:</p><ul>",
        esc(reason)
    );
    for cid in &flag.changes {
        out.push_str(&format!(
            "<li><span class=\"mono\">{}</span></li>",
            esc(&cid.0)
        ));
    }
    out.push_str("</ul></section>");
    out
}

// ---------------------------------------------------------------------------
// Legacy evidence: the paragraph (or copybook) before and after.
// ---------------------------------------------------------------------------

fn load_legacy(flag: &Flag) -> Result<(LegacyIndex, LegacyIndex, Vec<LegacyChange>), String> {
    let old = LegacyIndex::load(&flag.old_version).map_err(|e| e.to_string())?;
    let new = LegacyIndex::load(&flag.new_version).map_err(|e| e.to_string())?;
    let all = old.diff(&new);
    let mine: Vec<LegacyChange> = all
        .iter()
        .filter(|c| flag.changes.contains(&c.id))
        .cloned()
        .collect();
    Ok((old, new, mine))
}

/// The raw source lines of one paragraph: its `NAME.` header up to the next
/// header. Display only — the differ's normalized bodies decide the facts.
fn paragraph_source(source: &str, name: &str) -> Option<String> {
    let is_header = |line: &str| -> Option<String> {
        let trimmed = line.trim();
        let core = trimmed.strip_suffix('.')?;
        if core.is_empty()
            || !core.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            || !core.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        {
            return None;
        }
        Some(core.to_uppercase())
    };
    let mut lines = Vec::new();
    let mut inside = false;
    for line in source.lines() {
        if let Some(header) = is_header(line) {
            if inside {
                break;
            }
            inside = header == name;
            if !inside {
                continue;
            }
        }
        if inside {
            lines.push(line.trim_end());
        }
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

fn legacy_kind_word(kind: LegacyKind) -> &'static str {
    match kind {
        LegacyKind::Program => "program",
        LegacyKind::Paragraph => "paragraph",
        LegacyKind::Section => "section",
        LegacyKind::Record => "copybook record",
        LegacyKind::Field => "copybook field",
        LegacyKind::File => "file",
    }
}

fn legacy_severity_word(severity: LegacySeverity) -> &'static str {
    match severity {
        LegacySeverity::Breaking => "breaking",
        LegacySeverity::Review => "review",
        LegacySeverity::NonBreaking => "non-breaking",
    }
}

fn legacy_section(flag: &Flag, citation: Option<&Citation>) -> String {
    let (old, new, changes) = match load_legacy(flag) {
        Ok(loaded) => loaded,
        Err(reason) => {
            return unreadable_section("Evidence: the legacy code", flag, &reason);
        }
    };
    let old_src = std::fs::read_to_string(&flag.old_version).unwrap_or_default();
    let new_src = std::fs::read_to_string(&flag.new_version).unwrap_or_default();

    let mut out = String::new();
    out.push_str(EVIDENCE_STYLE);
    out.push_str(
        "<section class=\"card wide\"><h2>Evidence: the legacy code, before and after</h2>",
    );
    out.push_str(&format!(
        "<p class=\"small\">Program <span class=\"mono\">{}</span> · <span class=\"mono\">{}</span> → <span class=\"mono\">{}</span> · {} change(s) behind this flag.</p>",
        esc(old.program()),
        esc(&super::short_version(&flag.old_version)),
        esc(&super::short_version(&flag.new_version)),
        changes.len()
    ));

    if changes.is_empty() {
        out.push_str("<p>No recorded change fact for this flag is in the diff of the two program files as recorded; the facts are listed by ID above in Doyle's log.</p>");
    }
    for change in &changes {
        let subject_old = old.legacy_element(&change.subject);
        let subject_new = new.legacy_element(&change.subject);
        let name = subject_old
            .or(subject_new)
            .map(|e| {
                if e.path.is_empty() {
                    e.name.clone()
                } else {
                    e.path.clone()
                }
            })
            .unwrap_or_else(|| change.subject.0.clone());
        let kind = subject_old.or(subject_new).map(|e| e.kind);
        out.push_str(&format!(
            "<h3 class=\"small\">{} <span class=\"mono muted\">{}</span></h3><p class=\"small\">{} · {} · change <span class=\"mono\">{}</span></p>",
            esc(&name),
            esc(&change.subject.0),
            esc(change.kind.kind_str()),
            legacy_severity_word(change.severity),
            esc(&change.id.0),
        ));
        match kind {
            Some(LegacyKind::Paragraph) | Some(LegacyKind::Section) => {
                let before = paragraph_source(&old_src, &name)
                    .or_else(|| subject_old.map(|e| e.body.clone()))
                    .unwrap_or_else(|| "(absent in the old version)".to_string());
                let after = paragraph_source(&new_src, &name)
                    .or_else(|| subject_new.map(|e| e.body.clone()))
                    .unwrap_or_else(|| "(removed in the new version)".to_string());
                out.push_str(&format!(
                    "<div class=\"diff\"><div class=\"old\"><h3>Before — {}</h3><pre>{}</pre></div><div class=\"new\"><h3>After — {}</h3><pre>{}</pre></div></div>",
                    esc(&name),
                    esc(&before),
                    esc(&name),
                    esc(&after),
                ));
            }
            Some(LegacyKind::Record) | Some(LegacyKind::Field) => {
                if let (Some(o), Some(n)) = (subject_old, subject_new)
                    && o.kind == LegacyKind::Record
                {
                    out.push_str(&format!(
                        "<p class=\"small\">Record length {} → {} characters. Members (offset, length):</p>",
                        o.length, n.length
                    ));
                    out.push_str("<table><tr><th>Member</th><th>Old (offset, len)</th><th>New (offset, len)</th></tr>");
                    let mut names: Vec<String> = o.members.iter().map(|m| m.0.clone()).collect();
                    for m in &n.members {
                        if !names.contains(&m.0) {
                            names.push(m.0.clone());
                        }
                    }
                    for member in &names {
                        let cell = |members: &[(String, usize, usize)]| {
                            members
                                .iter()
                                .find(|m| &m.0 == member)
                                .map(|m| format!("{}, {}", m.1, m.2))
                                .unwrap_or_else(|| "—".to_string())
                        };
                        out.push_str(&format!(
                            "<tr><td><span class=\"mono\">{}</span></td><td>{}</td><td>{}</td></tr>",
                            esc(member),
                            esc(&cell(&o.members)),
                            esc(&cell(&n.members)),
                        ));
                    }
                    out.push_str("</table>");
                } else if !change.old_attr.is_empty() || !change.new_attr.is_empty() {
                    out.push_str(&format!(
                        "<p class=\"small\">{} → {}</p>",
                        esc(&change.old_attr),
                        esc(&change.new_attr)
                    ));
                }
                // The copybook itself, before and after.
                let book = subject_old
                    .or(subject_new)
                    .map(|e| e.source.clone())
                    .unwrap_or_default();
                let old_book = Path::new(&flag.old_version)
                    .parent()
                    .map(|d| d.join(&book))
                    .and_then(|p| std::fs::read_to_string(p).ok());
                let new_book = Path::new(&flag.new_version)
                    .parent()
                    .map(|d| d.join(&book))
                    .and_then(|p| std::fs::read_to_string(p).ok());
                if let (Some(o), Some(n)) = (old_book, new_book) {
                    out.push_str(&format!(
                        "<div class=\"diff\"><div class=\"old\"><h3>Before — {}</h3><pre>{}</pre></div><div class=\"new\"><h3>After — {}</h3><pre>{}</pre></div></div>",
                        esc(&book),
                        esc(&o),
                        esc(&book),
                        esc(&n),
                    ));
                }
            }
            _ => {
                if !change.old_attr.is_empty() || !change.new_attr.is_empty() {
                    out.push_str(&format!(
                        "<div class=\"diff\"><div class=\"old\"><h3>Before</h3><pre>{}</pre></div><div class=\"new\"><h3>After</h3><pre>{}</pre></div></div>",
                        esc(&change.old_attr),
                        esc(&change.new_attr),
                    ));
                } else {
                    out.push_str(
                        "<p class=\"small muted\">The element is present on one side only; see its name above.</p>",
                    );
                }
            }
        }
    }

    if let Some(citation) = citation
        && !citation.elements.is_empty()
    {
        out.push_str("<h3 class=\"small\">What the memory cites</h3><ul>");
        for element in &citation.elements {
            let label = old
                .legacy_element(element)
                .map(|e| {
                    let name = if e.path.is_empty() {
                        e.name.clone()
                    } else {
                        e.path.clone()
                    };
                    format!("{} ({})", name, legacy_kind_word(e.kind))
                })
                .or_else(|| old.element(element).map(|e| location_label(&e.native)))
                .unwrap_or_else(|| element.0.clone());
            out.push_str(&format!(
                "<li>{} <span class=\"mono muted\">{}</span></li>",
                esc(&label),
                esc(&element.0)
            ));
        }
        out.push_str("</ul>");
    }
    out.push_str("</section>");
    out
}

// ---------------------------------------------------------------------------
// Screen evidence: frames side by side, changed elements marked, cited span.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct ScreenElementData {
    text: String,
    row: u32,
    col: u32,
    kind: String,
    protected: bool,
}

#[derive(Debug, Clone)]
struct ScreenData {
    name: String,
    elements: Vec<ScreenElementData>,
}

#[derive(Debug, Clone)]
struct StepData {
    description: String,
    start_s: f64,
    end_s: f64,
    screen: String,
}

#[derive(Debug, Clone)]
struct RecordingData {
    name: String,
    duration_s: f64,
    file: Option<String>,
}

/// One version's reading as loaded from its version JSON file.
#[derive(Debug, Clone)]
struct ScreenVersion {
    console: String,
    screens: Vec<ScreenData>,
    steps: Vec<StepData>,
    recording: Option<RecordingData>,
    base: PathBuf,
    index: ScreenIndex,
}

fn parse_screen_version(path: &str) -> Result<ScreenVersion, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
    let console = value
        .get("console")
        .and_then(|v| v.as_str())
        .unwrap_or("fernwhistle")
        .to_string();
    let screens_value = value
        .get("screens")
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("{path}: missing screens"))?;
    let mut screens = Vec::new();
    for screen in screens_value {
        let name = screen
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let elements = screen
            .get("elements")
            .and_then(|v| v.as_array())
            .map(|rows| {
                rows.iter()
                    .filter_map(|e| {
                        Some(ScreenElementData {
                            text: e.get("text")?.as_str()?.to_string(),
                            row: u32::try_from(e.get("row")?.as_u64()?).ok()?,
                            col: u32::try_from(e.get("col")?.as_u64()?).ok()?,
                            kind: e
                                .get("kind")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .to_string(),
                            protected: e.get("protected").and_then(|v| v.as_bool()).unwrap_or(true),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        screens.push(ScreenData { name, elements });
    }
    let steps: Vec<StepData> = value
        .get("steps")
        .and_then(|v| v.as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|s| {
                    Some(StepData {
                        description: s.get("description")?.as_str()?.to_string(),
                        start_s: s.get("start_s")?.as_f64()?,
                        end_s: s.get("end_s")?.as_f64()?,
                        screen: s
                            .get("screen")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let recording = value.get("recording").map(|r| RecordingData {
        name: r
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        duration_s: r.get("duration_s").and_then(|v| v.as_f64()).unwrap_or(0.0),
        file: r.get("file").and_then(|v| v.as_str()).map(str::to_string),
    });

    let raw = RawVersion {
        screens: screens
            .iter()
            .map(|s| RawScreen {
                name: s.name.clone(),
                elements: s
                    .elements
                    .iter()
                    .map(|e| RawElement {
                        text: e.text.clone(),
                        row: e.row,
                        col: e.col,
                        kind: e.kind.clone(),
                        protected: e.protected,
                    })
                    .collect(),
            })
            .collect(),
        steps: steps
            .iter()
            .map(|s| RawStep {
                description: s.description.clone(),
                start_s: s.start_s,
                end_s: s.end_s,
                screen: s.screen.clone(),
            })
            .collect(),
        warnings: Vec::new(),
    };
    let meta = recording.as_ref().map(|r| RecordingMeta {
        name: r.name.clone(),
        duration_s: r.duration_s,
    });
    let (index, _warnings) = ScreenIndex::from_raw(&console, &raw, meta.as_ref());
    Ok(ScreenVersion {
        console,
        screens,
        steps,
        recording,
        base: Path::new(path)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
        index,
    })
}

struct ScreensEvidence {
    old: ScreenVersion,
    new: ScreenVersion,
    diff: ScreenDiff,
}

impl ScreensEvidence {
    /// The recording file the video route streams (old version first).
    fn video_file(&self) -> Option<PathBuf> {
        for version in [&self.old, &self.new] {
            if let Some(recording) = &version.recording
                && let Some(file) = &recording.file
            {
                let name = Path::new(file).file_name()?;
                return Some(version.base.join(name));
            }
        }
        None
    }
}

fn load_screens(flag: &Flag) -> Result<ScreensEvidence, String> {
    let old = parse_screen_version(&flag.old_version)?;
    let new = parse_screen_version(&flag.new_version)?;
    let diff = diff_versions(&old.index, &new.index);
    Ok(ScreensEvidence { old, new, diff })
}

/// One 80x24 frame as an HTML text grid, with the changed elements' texts
/// wrapped in `<mark>` at their grid positions.
fn frame_html(screen: &ScreenData, changed: &[crate::evidence::screens::ElementView]) -> String {
    let mut out = String::from("<pre class=\"ev-frame\">");
    for row in 1..=24u32 {
        let mut elements: Vec<&ScreenElementData> =
            screen.elements.iter().filter(|e| e.row == row).collect();
        elements.sort_by_key(|e| e.col);
        let mut line = String::new();
        let mut col = 1u32;
        for element in elements {
            if element.col > col {
                line.push_str(&" ".repeat((element.col - col) as usize));
                col = element.col;
            }
            let marked = changed
                .iter()
                .any(|c| c.row == element.row && c.col == element.col && c.text == element.text);
            if marked {
                line.push_str(&format!("<mark>{}</mark>", esc(&element.text)));
            } else {
                line.push_str(&esc(&element.text));
            }
            col += element.text.chars().count() as u32;
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out.push_str("</pre>");
    out
}

fn status_word(status: ScreenStatus) -> &'static str {
    match status {
        ScreenStatus::Unchanged => "unchanged",
        ScreenStatus::Cosmetic => "cosmetic only",
        ScreenStatus::ProcedureChanged => "procedure changed",
        ScreenStatus::Added => "added",
        ScreenStatus::Removed => "removed",
    }
}

fn fmt_seconds(seconds: f64) -> String {
    if (seconds.fract()).abs() < f64::EPSILON {
        format!("{}", seconds as i64)
    } else {
        format!("{seconds}")
    }
}

fn screens_section(flag: &Flag, _citation: Option<&Citation>) -> String {
    let evidence = match load_screens(flag) {
        Ok(evidence) => evidence,
        Err(reason) => {
            return unreadable_section("Evidence: the screens", flag, &reason);
        }
    };
    let mut out = String::new();
    out.push_str(EVIDENCE_STYLE);
    out.push_str("<section class=\"card wide\"><h2>Evidence: the screens, before and after</h2>");
    out.push_str(&format!(
        "<p class=\"small\">Console <span class=\"mono\">{}</span> · <span class=\"mono\">{}</span> → <span class=\"mono\">{}</span> · {} change(s) behind this flag. Changed elements are <mark>marked</mark> in both frames.</p>",
        esc(&evidence.old.console),
        esc(&super::short_version(&flag.old_version)),
        esc(&super::short_version(&flag.new_version)),
        flag.changes.len()
    ));
    for cid in &flag.changes {
        out.push_str(&format!(
            "<p class=\"small muted\">Change <span class=\"mono\">{}</span> · {}</p>",
            esc(&cid.0),
            esc(severity_word(flag.severity))
        ));
    }

    // Every screen's verdict, so the silent screens are visible too.
    out.push_str("<table><tr><th>Screen</th><th>Verdict</th><th>Element changes</th></tr>");
    for verdict in &evidence.diff.verdicts {
        out.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
            esc(&verdict.screen),
            esc(status_word(verdict.status)),
            verdict.changes.len()
        ));
    }
    out.push_str("</table>");

    for verdict in &evidence.diff.verdicts {
        if !matches!(
            verdict.status,
            ScreenStatus::ProcedureChanged | ScreenStatus::Removed
        ) {
            continue;
        }
        let old_screen = evidence
            .old
            .screens
            .iter()
            .find(|s| s.name == verdict.screen);
        let new_screen = evidence
            .new
            .screens
            .iter()
            .find(|s| s.name == verdict.screen);
        let before_views: Vec<_> = verdict
            .changes
            .iter()
            .filter_map(|c| c.before.clone())
            .collect();
        let after_views: Vec<_> = verdict
            .changes
            .iter()
            .filter_map(|c| c.after.clone())
            .collect();
        out.push_str(&format!(
            "<h3 class=\"small\">Screen {}</h3><div class=\"diff\">",
            esc(&verdict.screen)
        ));
        match old_screen {
            Some(screen) => out.push_str(&format!(
                "<div class=\"old\"><h3>Old — {}</h3>{}</div>",
                esc(&super::short_version(&flag.old_version)),
                frame_html(screen, &before_views)
            )),
            None => out.push_str("<div class=\"old\"><h3>Old</h3><p>(absent)</p></div>"),
        }
        match new_screen {
            Some(screen) => out.push_str(&format!(
                "<div class=\"new\"><h3>New — {}</h3>{}</div>",
                esc(&super::short_version(&flag.new_version)),
                frame_html(screen, &after_views)
            )),
            None => out.push_str("<div class=\"new\"><h3>New</h3><p>(removed)</p></div>"),
        }
        out.push_str("</div>");
        if !verdict.changes.is_empty() {
            out.push_str(
                "<table><tr><th>Change</th><th>Row</th><th>Before</th><th>After</th></tr>",
            );
            for change in &verdict.changes {
                let row = change
                    .before
                    .as_ref()
                    .or(change.after.as_ref())
                    .map(|v| v.row.to_string())
                    .unwrap_or_default();
                out.push_str(&format!(
                    "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                    esc(kind_str(&change.kind)),
                    esc(&row),
                    esc(change
                        .before
                        .as_ref()
                        .map(|v| v.text.as_str())
                        .unwrap_or("(absent)")),
                    esc(change
                        .after
                        .as_ref()
                        .map(|v| v.text.as_str())
                        .unwrap_or("(absent)")),
                ));
            }
            out.push_str("</table>");
        }
    }

    // The cited span, playable.
    let flagged_screens: Vec<&str> = evidence
        .diff
        .verdicts
        .iter()
        .filter(|v| {
            matches!(
                v.status,
                ScreenStatus::ProcedureChanged | ScreenStatus::Removed
            )
        })
        .map(|v| v.screen.as_str())
        .collect();
    let cited_steps: Vec<&StepData> = evidence
        .old
        .steps
        .iter()
        .filter(|s| flagged_screens.contains(&s.screen.as_str()))
        .collect();
    if !cited_steps.is_empty() {
        let duration = evidence
            .old
            .recording
            .as_ref()
            .map(|r| r.duration_s)
            .unwrap_or(0.0);
        let pad = crate::evidence::screens::SPAN_PADDING_S;
        let union_start = cited_steps
            .iter()
            .map(|s| (s.start_s - pad).max(0.0))
            .fold(f64::INFINITY, f64::min);
        let union_end = cited_steps
            .iter()
            .map(|s| {
                if duration > 0.0 {
                    (s.end_s + pad).min(duration)
                } else {
                    s.end_s + pad
                }
            })
            .fold(0.0, f64::max);
        out.push_str("<h3 class=\"small\">The cited training span</h3>");
        out.push_str("<table><tr><th>Step</th><th>Screen</th><th>Recorded span</th><th>Cited span (±5s)</th></tr>");
        for step in &cited_steps {
            let padded_start = (step.start_s - pad).max(0.0);
            let padded_end = if duration > 0.0 {
                (step.end_s + pad).min(duration)
            } else {
                step.end_s + pad
            };
            out.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}s–{}s</td><td>{}s–{}s</td></tr>",
                esc(&step.description),
                esc(&step.screen),
                fmt_seconds(step.start_s),
                fmt_seconds(step.end_s),
                fmt_seconds(padded_start),
                fmt_seconds(padded_end),
            ));
        }
        out.push_str("</table>");
        if evidence.video_file().is_some() {
            out.push_str(&format!(
                "<video class=\"ev-video\" controls preload=\"metadata\" src=\"/flag/{}/evidence/video#t={},{}\"></video><p class=\"small muted\">The training recording, cued to the cited span ({}s–{}s). <a href=\"/flag/{}/evidence/video\">Open the recording</a>.</p>",
                esc(&flag.id),
                fmt_seconds(union_start),
                fmt_seconds(union_end),
                fmt_seconds(union_start),
                fmt_seconds(union_end),
                esc(&flag.id),
            ));
        } else {
            out.push_str("<p class=\"small muted\">No recording file is named by the old version, so the span is listed but not playable from here.</p>");
        }
    }
    out.push_str("</section>");
    out
}

// ---------------------------------------------------------------------------
// Differential-run evidence: the inputs and both outputs.
// ---------------------------------------------------------------------------

fn money(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.abs();
    let dollars = abs / 100;
    let mut grouped = String::new();
    let digits = dollars.to_string();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(c);
    }
    format!("{sign}${grouped}.{:02}", abs % 100)
}

fn load_diffrun(flag: &Flag) -> Result<DiffrunOutcome, String> {
    let manifest_path = PathBuf::from(&flag.new_version);
    let text = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let base = manifest_path
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
    if let Some(raw_paths) = value.get("record_paths") {
        let raw = raw_paths
            .as_str()
            .ok_or_else(|| "manifest: record_paths is not a path".to_string())?;
        let path = if Path::new(raw).is_absolute() {
            PathBuf::from(raw)
        } else {
            base.join(raw)
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

fn mismatch_row(mismatch: &Mismatch) -> String {
    format!(
        "<tr><td><span class=\"mono\">{}</span></td><td class=\"ev-input\">{}</td><td class=\"ev-input\">{}</td><td class=\"ev-input\">{}</td><td>{}</td></tr>",
        esc(&mismatch.acct),
        esc(&mismatch.input),
        esc(&mismatch.cobol),
        esc(&mismatch.rewrite),
        esc(&mismatch.paragraphs.join(" → ")),
    )
}

fn diffrun_section(flag: &Flag, _citation: Option<&Citation>) -> String {
    let outcome = match load_diffrun(flag) {
        Ok(outcome) => outcome,
        Err(reason) => {
            return unreadable_section("Evidence: the differential run", flag, &reason);
        }
    };
    let blamed = crate::evidence::diffrun::exclusive_paragraphs(&outcome);
    let overcharge = total_overcharge_cents(&outcome);
    let mut out = String::new();
    out.push_str(EVIDENCE_STYLE);
    out.push_str("<section class=\"card wide\"><h2>Evidence: running both, record by record</h2>");
    out.push_str(&format!(
        "<p class=\"small\">Batch <span class=\"mono\">{}</span> against the rewrite run <span class=\"mono\">{}</span>: {} records compared, {} mismatch(es), rewrite overcharge {}.</p>",
        esc(&super::short_version(&flag.old_version)),
        esc(&super::short_version(&flag.new_version)),
        outcome.record_count,
        outcome.mismatches.len(),
        money(overcharge),
    ));
    if let (Some(cobol), Some(rewrite)) = (&outcome.totals_cobol, &outcome.totals_rewrite) {
        out.push_str(&format!(
            "<p class=\"small\">Totals: batch {} records, fees {}; rewrite {} records, fees {}.</p>",
            cobol.records,
            money(cobol.fee_cents),
            rewrite.records,
            money(rewrite.fee_cents),
        ));
    }
    if blamed.is_empty() {
        out.push_str(
            "<p>No paragraph ran only on mismatching records, so this run blames no memory.</p>",
        );
    } else {
        out.push_str(&format!(
            "<p>Paragraphs that ran only on mismatching records: <strong>{}</strong>. A memory citing one is flagged with these records as its evidence.</p>",
            esc(&blamed.join(", "))
        ));
    }
    for cid in &flag.changes {
        out.push_str(&format!(
            "<p class=\"small muted\">Change <span class=\"mono\">{}</span> · diffrun-mismatch · {}</p>",
            esc(&cid.0),
            esc(severity_word(flag.severity))
        ));
    }
    const SHOWN: usize = 25;
    if !outcome.mismatches.is_empty() {
        out.push_str("<table><tr><th>Account</th><th>Input</th><th>Batch output</th><th>Rewrite output</th><th>Paragraphs that ran</th></tr>");
        for mismatch in outcome.mismatches.iter().take(SHOWN) {
            out.push_str(&mismatch_row(mismatch));
        }
        out.push_str("</table>");
        if outcome.mismatches.len() > SHOWN {
            out.push_str(&format!(
                "<p class=\"small muted\">Showing the first {} of {} mismatching records; every one is in the run manifest this flag names.</p>",
                SHOWN,
                outcome.mismatches.len()
            ));
        }
    }
    out.push_str("</section>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::muse_code::MuseCodeAdapter;
    use crate::core::flag_citations;
    use crate::core::{
        ChangeId, Citation, ElementId, Flag, MemoryRef, Proposal, ProposalAction, Severity, Store,
    };
    use crate::evidence::legacy::ids as legacy_ids;
    use crate::evidence::screens::ids as screen_ids;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn temp_dir(label: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!(
            "doyle-evidence-test-{label}-{}-{n}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn fixture(rel: &str) -> String {
        format!("{}/{rel}", env!("CARGO_MANIFEST_DIR"))
    }

    fn story(rel: &str) -> PathBuf {
        PathBuf::from(fixture(&format!("testdata/legacy/story/{rel}")))
    }

    const L1_TEXT: &str =
        "BILLBATCH charges a 1.5% late fee on the whole balance, no exceptions; every account pays";
    const L2_TEXT: &str = "Each patient account record is 34 characters: a 6-digit account number, a 20-character name, a 7-digit balance with an implied decimal point, and a 1-character visit code";
    const L3_TEXT: &str = "CALC-LATE-FEE computes the 1.5% fee from BALANCE, rounded to the cent";
    const L4_TEXT: &str = "V2 prompt-payment accounts (DISCOUNT-FLAG Y) keep 10% of the fee, rounded; APPLY-PROMPT-DISCOUNT applies it after CALC-LATE-FEE";
    const S1_TEXT: &str = "To retire the console: open DECOMMISSION, type the clinic name in the confirmation prompt, press PF12 CONFIRM. The warning means it is permanent";
    const S2_TEXT: &str = "Look up a patient: enter the ID on the lookup screen; the results list the matches; PF5 edits the record";

    /// The story memories as a Muse Code scope: an index plus the two
    /// topic files, copied so a decision can write into them.
    fn muse_scope(root: &Path) -> PathBuf {
        let dir = root.join("muse-memory");
        fs::create_dir_all(&dir).unwrap();
        fs::copy(
            story("memories/billing-rules.md"),
            dir.join("billing-rules.md"),
        )
        .unwrap();
        fs::copy(
            story("memories/console-procedures.md"),
            dir.join("console-procedures.md"),
        )
        .unwrap();
        fs::write(
            dir.join("MEMORY.md"),
            "---\ntype: index\ndescription: story memories\n---\n\n- [billing](billing-rules.md)\n- [console](console-procedures.md)\n",
        )
        .unwrap();
        dir
    }

    fn muse_adapter(dir: &Path) -> MuseCodeAdapter {
        MuseCodeAdapter::from_scopes(vec![(dir.to_path_buf(), "personal_project".to_string())])
    }

    fn citation(memory: &str, claim: &str, source_text: &str, elements: &[ElementId]) -> Citation {
        Citation {
            memory: MemoryRef {
                adapter: "muse-code".to_string(),
                id: memory.to_string(),
            },
            elements: elements.to_vec(),
            claim: claim.to_string(),
            recorded_by: "test-curator".to_string(),
            confidence: 0.9,
            source_text: source_text.to_string(),
        }
    }

    fn legacy_citations() -> Vec<Citation> {
        let el = legacy_ids::element_id;
        let para = |name: &str| el(&legacy_ids::paragraph_key("BILLBATCH", name));
        let field = |path: &str| el(&legacy_ids::field_key("BILLBATCH", path));
        vec![
            citation(
                "L1",
                "1.5% late fee on the whole balance, no exceptions",
                L1_TEXT,
                &[
                    para("CALC-LATE-FEE"),
                    field("PATIENT-RECORD.BALANCE"),
                    para("LOG-EXEMPT"),
                    field("PATIENT-RECORD.VISIT-CODE"),
                ],
            ),
            citation(
                "L2",
                "Each patient account record is 34 characters",
                L2_TEXT,
                &[
                    el(&legacy_ids::record_key("BILLBATCH", "PATIENT-RECORD")),
                    field("PATIENT-RECORD.ACCT-NO"),
                    field("PATIENT-RECORD.ACCT-NAME"),
                    field("PATIENT-RECORD.BALANCE"),
                    field("PATIENT-RECORD.VISIT-CODE"),
                ],
            ),
            citation(
                "L3",
                "CALC-LATE-FEE computes the 1.5% fee from BALANCE",
                L3_TEXT,
                &[para("CALC-LATE-FEE"), field("PATIENT-RECORD.BALANCE")],
            ),
            citation(
                "L4",
                "V2 prompt-payment accounts keep 10% of the fee",
                L4_TEXT,
                &[
                    para("APPLY-PROMPT-DISCOUNT"),
                    field("PATIENT-RECORD.DISCOUNT-FLAG"),
                ],
            ),
        ]
    }

    fn seed_store(root: &Path) -> (PathBuf, Store) {
        let state = root.join(".doyle");
        let store = Store::open(&state).unwrap();
        (state, store)
    }

    fn record_flag_and_proposal(
        store: &mut Store,
        flag: Flag,
        new_text: &str,
        rationale: &str,
    ) -> String {
        let flag_id = store.record_flag(flag).unwrap();
        store
            .record_proposal(Proposal {
                id: String::new(),
                flag_id: flag_id.clone(),
                action: ProposalAction::Correct {
                    new_text: new_text.to_string(),
                },
                rationale: rationale.to_string(),
                proposed_by: "test-recheck".to_string(),
            })
            .unwrap();
        flag_id
    }

    // ----- seeding the three kinds through the real flagging paths -----

    struct Seeded {
        root: PathBuf,
        muse_dir: PathBuf,
        server: ReviewServer,
        flag_id: String,
    }

    fn seed_legacy() -> Seeded {
        let root = temp_dir("legacy");
        let muse_dir = muse_scope(&root);
        let (_state, mut store) = seed_store(&root);
        let old_path = story("v1/BILLBATCH.cbl").to_string_lossy().to_string();
        let new_path = story("v2/BILLBATCH.cbl").to_string_lossy().to_string();
        let old = LegacyIndex::load(&old_path).unwrap();
        let new = LegacyIndex::load(&new_path).unwrap();
        let facts = old.diff_evidence(&new).unwrap();
        let citations = legacy_citations();
        for cit in &citations {
            store.record_citation(cit.clone()).unwrap();
        }
        let flags = flag_citations(
            &citations,
            &facts,
            &old,
            &[],
            &old_path.clone(),
            &new_path.clone(),
        );
        assert_eq!(flags.len(), 1, "only L2 flags: {flags:?}");
        assert_eq!(flags[0].memory.id, "L2");
        let flag_id = record_flag_and_proposal(
            &mut store,
            flags.into_iter().next().unwrap(),
            "Each patient account record is 35 characters in v2: a 6-digit account number, a 20-character name, a 7-digit balance with an implied decimal point, a 1-character visit code, and a 1-character discount flag",
            "the v2 copybook adds DISCOUNT-FLAG; the record grows 34 to 35",
        );
        let server = ReviewServer::new(store, vec![Box::new(muse_adapter(&muse_dir))]);
        Seeded {
            root,
            muse_dir,
            server,
            flag_id,
        }
    }

    fn screen_version_json(files: &[&str], with_recording: bool, root: &Path) -> PathBuf {
        let screens: Vec<serde_json::Value> = files
            .iter()
            .map(|file| {
                let value: serde_json::Value = serde_json::from_str(
                    &fs::read_to_string(story(&format!("screens/{file}.json"))).unwrap(),
                )
                .unwrap();
                serde_json::json!({
                    "name": value["screen"],
                    "elements": value["elements"],
                    "screen_text": value["screen_text"],
                })
            })
            .collect();
        let mut steps = Vec::new();
        if with_recording {
            let value: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(story("training-v1-steps.json")).unwrap())
                    .unwrap();
            for step in value["steps"].as_array().unwrap() {
                let id = step["id"].as_str().unwrap();
                let screen = match id {
                    "T2" | "T3" | "T4" => "lookup",
                    "T5" | "T6" => "decommission",
                    _ => "",
                };
                steps.push(serde_json::json!({
                    "description": step["description"],
                    "start_s": step["start_s"],
                    "end_s": step["end_s"],
                    "screen": screen,
                }));
            }
            fs::copy(story("training-v1.mp4"), root.join("training-v1.mp4")).unwrap();
        }
        let mut doc = serde_json::json!({
            "console": "fernwhistle",
            "screens": screens,
            "steps": steps,
        });
        if with_recording {
            doc["recording"] = serde_json::json!({
                "name": "training-v1", "duration_s": 45.0, "file": "training-v1.mp4"
            });
        }
        let name = if with_recording {
            "screens-v1.json"
        } else {
            "screens-v2.json"
        };
        let path = root.join(name);
        fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
        path
    }

    fn seed_screens() -> Seeded {
        let root = temp_dir("screens");
        let muse_dir = muse_scope(&root);
        let v1 = screen_version_json(&["lookup-v1", "decomm-v1"], true, &root);
        let v2 = screen_version_json(&["lookup-v2", "decomm-v2"], false, &root);
        let old_path = v1.to_string_lossy().to_string();
        let new_path = v2.to_string_lossy().to_string();

        let probe = Flag {
            id: String::new(),
            memory: MemoryRef {
                adapter: "muse-code".to_string(),
                id: "probe".to_string(),
            },
            changes: Vec::new(),
            severity: Severity::Breaking,
            old_version: old_path.clone(),
            new_version: new_path.clone(),
            evidence: None,
        };
        let evidence = load_screens(&probe).expect("story screens load");
        let facts = evidence
            .old
            .index
            .diff_evidence(&evidence.new.index)
            .unwrap();
        assert_eq!(facts.len(), 1, "one screen fact: {facts:?}");

        let decomm = screen_ids::screen_id("fernwhistle", "decommission");
        let lookup = screen_ids::screen_id("fernwhistle", "lookup");
        let step = |desc: &str, a: f64, b: f64| {
            screen_ids::step_id("fernwhistle", "training-v1", desc, a, b)
        };
        let citations = vec![
            citation(
                "S1",
                "Retire: type the clinic name, PF12 CONFIRM",
                S1_TEXT,
                &[
                    decomm.clone(),
                    step("Open the decommission confirmation screen", 28.0, 33.0),
                    step(
                        "Type the clinic name MAPLE ST CLINIC to confirm",
                        33.0,
                        45.0,
                    ),
                ],
            ),
            citation(
                "S2",
                "Look up: enter the ID; PF5 edits",
                S2_TEXT,
                &[
                    lookup,
                    step(
                        "Type the patient ID FWV-004812 on the lookup screen",
                        6.0,
                        16.0,
                    ),
                    step("Submit the lookup; results list appears", 16.0, 20.0),
                ],
            ),
        ];
        let (_state, mut store) = seed_store(&root);
        for cit in &citations {
            store.record_citation(cit.clone()).unwrap();
        }
        let flags = flag_citations(
            &citations,
            &facts,
            &evidence.old.index,
            &[],
            &old_path.clone(),
            &new_path.clone(),
        );
        assert_eq!(flags.len(), 1, "only S1 flags: {flags:?}");
        assert_eq!(flags[0].memory.id, "S1");
        let flag_id = record_flag_and_proposal(
            &mut store,
            flags.into_iter().next().unwrap(),
            "To retire the console on v2: open DECOMMISSION and press PF12 DECOMMISSION; v2 has no confirmation prompt and no warning, so check the export count first",
            "the v2 decommission screen dropped the warning, the clinic-name prompt and PF12 CONFIRM",
        );
        let server = ReviewServer::new(store, vec![Box::new(muse_adapter(&muse_dir))]);
        Seeded {
            root,
            muse_dir,
            server,
            flag_id,
        }
    }

    fn seed_diffrun() -> Seeded {
        let root = temp_dir("diffrun");
        let muse_dir = muse_scope(&root);
        let manifest = root.join("run-manifest.json");
        let d = |name: &str| fixture(&format!("testdata/legacy/diffrun/{name}"));
        fs::write(
            &manifest,
            serde_json::json!({
                "inputs": d("mini-inputs.dat"),
                "cobol_report": d("mini-cobol-report.txt"),
                "rewrite_report": d("mini-wrong-report.txt"),
                "trace": d("mini-trace.txt"),
            })
            .to_string(),
        )
        .unwrap();
        let cbl = story("v1/BILLBATCH.cbl").to_string_lossy().to_string();
        let probe = Flag {
            id: String::new(),
            memory: MemoryRef {
                adapter: "muse-code".to_string(),
                id: "probe".to_string(),
            },
            changes: Vec::new(),
            severity: Severity::Breaking,
            old_version: cbl.clone(),
            new_version: manifest.to_string_lossy().to_string(),
            evidence: None,
        };
        let outcome = load_diffrun(&probe).expect("mini diffrun loads");
        let index = LegacyIndex::load(&cbl).unwrap();
        let citations = legacy_citations();
        let manifest_str = manifest.to_string_lossy().to_string();
        let flags = crate::evidence::diffrun::flag_diffrun(
            &citations,
            &index,
            &outcome,
            &[],
            &cbl.clone(),
            &manifest_str,
        );
        assert_eq!(flags.len(), 1, "only L1 flags: {flags:?}");
        assert_eq!(flags[0].flag.memory.id, "L1");
        let (_state, mut store) = seed_store(&root);
        for cit in &citations {
            store.record_citation(cit.clone()).unwrap();
        }
        let flag_id = record_flag_and_proposal(
            &mut store,
            flags.into_iter().next().unwrap().flag,
            "BILLBATCH charges a 1.5% late fee on the whole balance except vaccination visits (VISIT-CODE V), which pay no fee",
            "the differential run mismatches exactly the vaccination records the rewrite charged",
        );
        let server = ReviewServer::new(store, vec![Box::new(muse_adapter(&muse_dir))]);
        Seeded {
            root,
            muse_dir,
            server,
            flag_id,
        }
    }

    // ----- kind detection -----

    #[test]
    fn kind_detection_reads_id_prefixes() {
        let flag = |changes: Vec<&str>, new_version: &str| Flag {
            id: "flag-1".to_string(),
            memory: MemoryRef {
                adapter: "muse-code".to_string(),
                id: "X".to_string(),
            },
            changes: changes
                .into_iter()
                .map(|c| ChangeId(c.to_string()))
                .collect(),
            severity: Severity::Breaking,
            old_version: "a".to_string(),
            new_version: new_version.to_string(),
            evidence: None,
        };
        assert_eq!(
            evidence_kind(&flag(vec!["vc2_x"], "v2.yaml"), None),
            EvidenceKind::Contract
        );
        assert_eq!(
            evidence_kind(&flag(vec!["lc2_x"], "v2/BILLBATCH.cbl"), None),
            EvidenceKind::Legacy
        );
        assert_eq!(
            evidence_kind(&flag(vec!["lc2_x"], "run.json"), None),
            EvidenceKind::Diffrun
        );
        assert_eq!(
            evidence_kind(&flag(vec!["sc2_x"], "screens-v2.json"), None),
            EvidenceKind::Screens
        );
        let cit = citation(
            "S1",
            "claim",
            "text",
            &[screen_ids::screen_id("fernwhistle", "decommission")],
        );
        assert_eq!(
            evidence_kind(&flag(vec![], "screens-v2.json"), Some(&cit)),
            EvidenceKind::Screens
        );
    }

    // ----- rendering through serve -> HTTP handle -----

    #[test]
    fn legacy_flag_page_shows_the_copybook_before_and_after() {
        let mut seeded = seed_legacy();
        let response = seeded
            .server
            .handle("GET", &format!("/flag/{}", seeded.flag_id), "");
        assert_eq!(response.status, 200);
        let body = &response.body;
        assert!(
            body.contains("Evidence: the legacy code, before and after"),
            "{body}"
        );
        assert!(body.contains("PATIENT-RECORD"), "subject named");
        assert!(body.contains("record-layout-changed"), "change kind");
        assert!(
            body.contains("Record length 34 → 35"),
            "layout before/after: {body}"
        );
        assert!(body.contains("DISCOUNT-FLAG"), "the v2 field shows");
        assert!(body.contains("patient.cpy"), "copybook shown");
        // The four decide actions are the existing T5.3 form, untouched.
        for verdict in ["approve", "edit", "retire", "keep"] {
            assert!(
                body.contains(&format!("value=\"{verdict}\"")),
                "action {verdict}"
            );
        }
        let _ = fs::remove_dir_all(&seeded.root);
    }

    #[test]
    fn legacy_paragraph_change_shows_the_paragraph_before_and_after() {
        // CHARGE-ACCOUNT gained the discount wiring in v2: seed its own flag
        // through flag_citations so the paragraph view renders.
        let root = temp_dir("legacy-para");
        let muse_dir = muse_scope(&root);
        let old_path = story("v1/BILLBATCH.cbl").to_string_lossy().to_string();
        let new_path = story("v2/BILLBATCH.cbl").to_string_lossy().to_string();
        let old = LegacyIndex::load(&old_path).unwrap();
        let new = LegacyIndex::load(&new_path).unwrap();
        let facts = old.diff_evidence(&new).unwrap();
        let charge =
            legacy_ids::element_id(&legacy_ids::paragraph_key("BILLBATCH", "CHARGE-ACCOUNT"));
        let cit = citation(
            "LX",
            "CHARGE-ACCOUNT charges each account",
            "CHARGE-ACCOUNT charges each account",
            &[charge],
        );
        let flags = flag_citations(&[cit], &facts, &old, &[], &old_path, &new_path);
        assert_eq!(flags.len(), 1);
        let (_state, mut store) = seed_store(&root);
        let flag_id = record_flag_and_proposal(
            &mut store,
            flags.into_iter().next().unwrap(),
            "corrected",
            "wiring changed",
        );
        let mut server = ReviewServer::new(store, vec![Box::new(muse_adapter(&muse_dir))]);
        let response = server.handle("GET", &format!("/flag/{flag_id}"), "");
        assert_eq!(response.status, 200);
        assert!(
            response.body.contains("CHARGE-ACCOUNT"),
            "{}",
            response.body
        );
        assert!(
            response.body.contains("APPLY-PROMPT-DISCOUNT"),
            "new paragraph carries the wiring"
        );
        assert!(
            response.body.contains("Before — CHARGE-ACCOUNT")
                && response.body.contains("After — CHARGE-ACCOUNT"),
            "before and after both shown"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn screen_flag_page_shows_frames_marked_and_the_span_playable() {
        let mut seeded = seed_screens();
        let response = seeded
            .server
            .handle("GET", &format!("/flag/{}", seeded.flag_id), "");
        assert_eq!(response.status, 200);
        let body = &response.body;
        assert!(
            body.contains("Evidence: the screens, before and after"),
            "{body}"
        );
        // Both frames, changed elements marked in them.
        assert!(
            body.contains("<mark>WARNING: DECOMMISSION IS PERMANENT.</mark>"),
            "old frame marks the warning: {body}"
        );
        assert!(
            body.contains("<mark>PF12 CONFIRM</mark>"),
            "old frame marks CONFIRM"
        );
        assert!(
            body.contains("<mark>PF12 DECOMMISSION</mark>"),
            "new frame marks DECOMMISSION"
        );
        assert!(
            body.contains("TYPE CLINIC NAME TO CONFIRM:"),
            "dropped prompt listed"
        );
        assert!(body.contains("procedure changed"), "decomm verdict");
        assert!(body.contains("cosmetic only"), "lookup stays cosmetic");
        // The cited span: steps, padded span, and a playable video cued to it.
        assert!(body.contains("Type the clinic name MAPLE ST CLINIC to confirm"));
        assert!(body.contains("23s–45s"), "padded union span: {body}");
        assert!(
            body.contains(&format!("/flag/{}/evidence/video", seeded.flag_id)),
            "video route named"
        );
        assert!(body.contains("<video"), "span is playable in the page");
        for verdict in ["approve", "edit", "retire", "keep"] {
            assert!(
                body.contains(&format!("value=\"{verdict}\"")),
                "action {verdict}"
            );
        }

        // The video route streams the fixture recording.
        let video = seeded.server.handle(
            "GET",
            &format!("/flag/{}/evidence/video", seeded.flag_id),
            "",
        );
        assert_eq!(video.status, 200);
        let (content_type, bytes) = video.media.expect("media response");
        assert_eq!(content_type, "video/mp4");
        assert_eq!(bytes, fs::read(story("training-v1.mp4")).unwrap());
        // A flag with no recording (and an unknown flag) 404s on the route.
        let mut legacy = seed_legacy();
        let missing = legacy.server.handle(
            "GET",
            &format!("/flag/{}/evidence/video", legacy.flag_id),
            "",
        );
        assert_eq!(missing.status, 404);
        let _ = fs::remove_dir_all(&legacy.root);
        let _ = fs::remove_dir_all(&seeded.root);
    }

    #[test]
    fn diffrun_flag_page_shows_inputs_and_both_outputs() {
        let mut seeded = seed_diffrun();
        let response = seeded
            .server
            .handle("GET", &format!("/flag/{}", seeded.flag_id), "");
        assert_eq!(response.status, 200);
        let body = &response.body;
        assert!(
            body.contains("Evidence: running both, record by record"),
            "{body}"
        );
        assert!(
            body.contains("10 records compared, 2 mismatch(es)"),
            "{body}"
        );
        assert!(body.contains("LOG-EXEMPT"), "blamed paragraph named");
        assert!(body.contains("000003"), "first mismatching account");
        assert!(body.contains("000009"), "second mismatching account");
        assert!(body.contains("$6.94"), "overcharge totalled: {body}");
        assert!(body.contains("FEE      $0.00"), "batch output shown");
        assert!(body.contains("FEE      $3.75"), "rewrite output shown");
        let _ = fs::remove_dir_all(&seeded.root);
    }

    #[test]
    fn contract_flag_still_uses_the_contract_section() {
        // The dispatch leaves the T6.2 view alone for a contract flag.
        let root = temp_dir("contract-kind");
        let (_state, mut store) = seed_store(&root);
        let flag_id = record_flag_and_proposal(
            &mut store,
            Flag {
                id: String::new(),
                memory: MemoryRef {
                    adapter: "fake".to_string(),
                    id: "M1".to_string(),
                },
                changes: vec![ChangeId("vc2_test".to_string())],
                severity: Severity::Review,
                old_version: "v1".to_string(),
                new_version: "v2".to_string(),
                evidence: None,
            },
            "paged",
            "it pages now",
        );
        let mut server = ReviewServer::new(store, vec![]);
        let response = server.handle("GET", &format!("/flag/{flag_id}"), "");
        assert_eq!(response.status, 200);
        assert!(response.body.contains("What changed"), "{}", response.body);
        assert!(!response.body.contains("Evidence: the screens"));
        assert!(!response.body.contains("Evidence: the legacy code"));
        let _ = fs::remove_dir_all(&root);
    }

    // ----- settling through the real decide path -----

    #[test]
    fn settling_a_screen_flag_writes_the_correction_where_it_lived() {
        let mut seeded = seed_screens();
        let flag_path = format!("/flag/{}", seeded.flag_id);
        let response = seeded.server.handle(
            "POST",
            &format!("{flag_path}/decide"),
            "verdict=approve&decided_by=operator",
        );
        assert_eq!(response.status, 303, "decide redirects: {}", response.body);
        // Doyle's store: decision recorded by the operator, flag settled.
        let proposals = seeded.server.store.proposals();
        let decision = seeded
            .server
            .store
            .decision_for_proposal(&proposals[0].id)
            .expect("decision recorded");
        assert_eq!(decision.decided_by, "operator");
        assert!(seeded.server.store.open_flags().is_empty(), "flag settled");
        // Muse Code: the stale procedure names its correction, which reads.
        let procedures = fs::read_to_string(seeded.muse_dir.join("console-procedures.md")).unwrap();
        assert!(
            procedures.contains("[Superseded by MEMORY.md#"),
            "{procedures}"
        );
        let index = fs::read_to_string(seeded.muse_dir.join("MEMORY.md")).unwrap();
        assert!(
            index.contains("press PF12 DECOMMISSION; v2 has no confirmation prompt"),
            "correction landed in the memory index: {index}"
        );
        // The settled page says what was written where.
        let settled = seeded.server.handle("GET", &flag_path, "");
        assert_eq!(settled.status, 200);
        assert!(settled.body.contains("Settled — what was written where"));
        assert!(settled.body.contains("Correction written"));
        let _ = fs::remove_dir_all(&seeded.root);
    }

    #[test]
    fn settling_a_legacy_flag_with_an_edit_writes_the_edited_text() {
        let mut seeded = seed_legacy();
        let flag_path = format!("/flag/{}", seeded.flag_id);
        let edited = "Each patient account record is 35 characters in v2, discount flag included";
        let body = format!(
            "verdict=edit&decided_by=operator&new_text={}",
            edited.replace(' ', "+")
        );
        let response = seeded
            .server
            .handle("POST", &format!("{flag_path}/decide"), &body);
        assert_eq!(response.status, 303, "decide redirects: {}", response.body);
        let proposals = seeded.server.store.proposals();
        let decision = seeded
            .server
            .store
            .decision_for_proposal(&proposals[0].id)
            .expect("decision recorded");
        assert_eq!(decision.decided_by, "operator");
        assert!(seeded.server.store.open_flags().is_empty());
        let rules = fs::read_to_string(seeded.muse_dir.join("billing-rules.md")).unwrap();
        assert!(rules.contains("[Superseded by MEMORY.md#"), "{rules}");
        let index = fs::read_to_string(seeded.muse_dir.join("MEMORY.md")).unwrap();
        assert!(index.contains(edited), "edited correction landed: {index}");
        // The other billing rules are untouched.
        assert!(rules.contains(L3_TEXT), "L3 stays as written");
        let settled = seeded.server.handle("GET", &flag_path, "");
        assert!(settled.body.contains("Settled — what was written where"));
        assert!(settled.body.contains(edited), "settled view shows the edit");
        let _ = fs::remove_dir_all(&seeded.root);
    }

    #[test]
    fn unreadable_evidence_is_named_not_silent() {
        let root = temp_dir("unreadable");
        let (_state, mut store) = seed_store(&root);
        let flag_id = record_flag_and_proposal(
            &mut store,
            Flag {
                id: String::new(),
                memory: MemoryRef {
                    adapter: "muse-code".to_string(),
                    id: "L2".to_string(),
                },
                changes: vec![ChangeId(
                    "lc2_test0000000000000000000000000000000000000000000000000000000001"
                        .to_string(),
                )],
                severity: Severity::Breaking,
                old_version: root.join("nope-v1.cbl").to_string_lossy().to_string(),
                new_version: root.join("nope-v2.cbl").to_string_lossy().to_string(),
                evidence: None,
            },
            "corrected",
            "rationale",
        );
        let mut server = ReviewServer::new(store, vec![]);
        let response = server.handle("GET", &format!("/flag/{flag_id}"), "");
        assert_eq!(response.status, 200);
        assert!(
            response.body.contains("cannot be read from here"),
            "{}",
            response.body
        );
        assert!(response.body.contains("lc2_test"));
        let _ = fs::remove_dir_all(&root);
    }
}
