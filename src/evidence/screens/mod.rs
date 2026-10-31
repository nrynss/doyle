//! The evidence source for screens and recordings.
//!
//! One Wonder call per console version turns frames and the training
//! recording into elements and steps; everything after that — the diff
//! between two versions and the flag that follows — is computed, never
//! asked of the model. Each screen, element, action and time span gets a
//! stable ID derived from what it is, never from its order (see
//! [`ids`]).
//!
//! How flagging reaches a cited memory: a screen is the analogue of a
//! shared named type ([`EvidenceElementKind::Type`]), and a recording step
//! references the screen it demonstrates the way an operation references
//! the type it returns. A procedure change on a screen therefore flags
//! citers of the screen and of the steps covering it — while a footer-hint
//! relabel on another screen writes no fact at all and stays silent.
//! Cosmetic chrome (the `pfkey` hints) is never a procedure.
//!
//! Span padding follows the T9.2 verdict: cite spans with ±5 s, clamped to
//! the recording.

use std::collections::HashMap;

use crate::contract::ContractError;
use crate::core::ElementId;
use crate::evidence::{
    EvidenceChange, EvidenceChangeKind, EvidenceElement, EvidenceElementKind, EvidenceSource,
    NativeLocation,
};
use crate::prompts::vision::RawVersion;

pub mod ids;

use ids::{change_id, element_id, normalize_text, recording_id, screen_id, step_id};

/// The ±5 s span padding from the T9.2 verdict.
pub const SPAN_PADDING_S: f64 = 5.0;

/// A version's training recording, for span clamping.
#[derive(Debug, Clone)]
pub struct RecordingMeta {
    /// The recording's given name (for example `"training-v1"`).
    pub name: String,
    /// Its duration in seconds.
    pub duration_s: f64,
}

/// One console version, indexed for flagging.
#[derive(Debug, Clone)]
pub struct ScreenIndex {
    console: String,
    elements: Vec<EvidenceElement>,
    by_id: HashMap<String, usize>,
    surface: HashMap<String, ElementSurface>,
    spans: HashMap<String, StepSpan>,
}

/// An indexed text element's matchable surface: what the differ reads.
#[derive(Debug, Clone)]
struct ElementSurface {
    kind: String,
    text: String,
    row: u32,
    col: u32,
}

/// A step's raw span plus its padded citation span.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StepSpan {
    /// The reported span.
    pub raw: (f64, f64),
    /// ±[`SPAN_PADDING_S`], clamped to the recording.
    pub padded: (f64, f64),
}

impl ScreenIndex {
    /// Index one version's reading: screens with their elements, then the
    /// recording with its steps. Steps reference the screen they show;
    /// steps naming no known screen reference nothing, with a warning.
    pub fn from_raw(
        console: &str,
        raw: &RawVersion,
        recording: Option<&RecordingMeta>,
    ) -> (Self, Vec<String>) {
        let mut warnings = Vec::new();
        let mut elements = Vec::new();
        let mut surface = HashMap::new();
        let mut spans = HashMap::new();

        for screen in &raw.screens {
            let screen_element_id = screen_id(console, &screen.name);
            elements.push(EvidenceElement {
                id: screen_element_id.clone(),
                kind: EvidenceElementKind::Type,
                parent: None,
                references: Vec::new(),
                native: NativeLocation {
                    method: None,
                    path: None,
                    operation_id: None,
                    type_name: Some(screen.name.clone()),
                    field_name: None,
                },
            });
            for element in &screen.elements {
                let id = element_id(
                    console,
                    &screen.name,
                    &element.kind,
                    &element.text,
                    element.row,
                    element.col,
                );
                surface.insert(
                    id.0.clone(),
                    ElementSurface {
                        kind: element.kind.clone(),
                        text: element.text.clone(),
                        row: element.row,
                        col: element.col,
                    },
                );
                elements.push(EvidenceElement {
                    id,
                    kind: EvidenceElementKind::Other("screen-element".to_string()),
                    parent: Some(screen_element_id.clone()),
                    references: Vec::new(),
                    native: NativeLocation {
                        method: None,
                        path: None,
                        operation_id: None,
                        type_name: Some(screen.name.clone()),
                        field_name: Some(element.text.clone()),
                    },
                });
            }
        }

        let screen_names: Vec<&str> = raw.screens.iter().map(|s| s.name.as_str()).collect();
        if let Some(meta) = recording {
            let recording_element_id = recording_id(console, &meta.name);
            elements.push(EvidenceElement {
                id: recording_element_id.clone(),
                kind: EvidenceElementKind::Other("recording".to_string()),
                parent: None,
                references: Vec::new(),
                native: NativeLocation {
                    method: None,
                    path: None,
                    operation_id: None,
                    type_name: Some(meta.name.clone()),
                    field_name: None,
                },
            });
            let duration = meta.duration_s.max(0.0);
            for step in &raw.steps {
                let id = step_id(
                    console,
                    &meta.name,
                    &step.description,
                    step.start_s,
                    step.end_s,
                );
                let references = if step.screen.is_empty() {
                    Vec::new()
                } else if screen_names.contains(&step.screen.as_str()) {
                    vec![screen_id(console, &step.screen)]
                } else {
                    warnings.push(format!(
                        "step '{}' names unknown screen '{}'; no reference",
                        step.description, step.screen
                    ));
                    Vec::new()
                };
                elements.push(EvidenceElement {
                    id: id.clone(),
                    kind: EvidenceElementKind::Other("step".to_string()),
                    parent: Some(recording_element_id.clone()),
                    references,
                    native: NativeLocation {
                        method: None,
                        path: None,
                        operation_id: Some(step.description.clone()),
                        type_name: Some(meta.name.clone()),
                        field_name: None,
                    },
                });
                spans.insert(
                    id.0.clone(),
                    StepSpan {
                        raw: (step.start_s, step.end_s),
                        padded: (
                            (step.start_s - SPAN_PADDING_S).max(0.0),
                            (step.end_s + SPAN_PADDING_S).min(duration),
                        ),
                    },
                );
            }
        }

        let by_id = elements
            .iter()
            .enumerate()
            .map(|(i, e)| (e.id.0.clone(), i))
            .collect();
        (
            Self {
                console: console.to_string(),
                elements,
                by_id,
                surface,
                spans,
            },
            warnings,
        )
    }

    /// The console this index was read from.
    pub fn console(&self) -> &str {
        &self.console
    }

    /// The indexed screens, in index order.
    pub fn screens(&self) -> Vec<EvidenceElement> {
        self.elements
            .iter()
            .filter(|e| e.kind == EvidenceElementKind::Type)
            .cloned()
            .collect()
    }

    /// A step's reported span, if the ID is a step of this index.
    pub fn step_span(&self, id: &ElementId) -> Option<(f64, f64)> {
        self.spans.get(id.0.as_str()).map(|s| s.raw)
    }

    /// A step's citation span: ±[`SPAN_PADDING_S`], clamped to the recording.
    pub fn padded_span(&self, id: &ElementId) -> Option<(f64, f64)> {
        self.spans.get(id.0.as_str()).map(|s| s.padded)
    }
}

impl EvidenceSource for ScreenIndex {
    fn element(&self, id: &ElementId) -> Option<EvidenceElement> {
        self.by_id
            .get(id.0.as_str())
            .map(|&i| self.elements[i].clone())
    }

    fn elements(&self) -> Vec<EvidenceElement> {
        self.elements.clone()
    }

    fn parent_chain(&self, id: &ElementId) -> Vec<EvidenceElement> {
        let mut chain = Vec::new();
        let mut next = self.element(id);
        while let Some(current) = next {
            if chain.len() > self.elements.len() {
                break;
            }
            next = current
                .parent
                .as_ref()
                .and_then(|parent| self.element(parent));
            chain.push(current);
        }
        chain
    }

    fn referencers(&self, id: &ElementId) -> Vec<EvidenceElement> {
        self.elements
            .iter()
            .filter(|e| e.references.iter().any(|r| r == id))
            .cloned()
            .collect()
    }

    fn native_location(&self, id: &ElementId) -> Option<NativeLocation> {
        self.element(id).map(|e| e.native)
    }

    fn diff_evidence(&self, new: &Self) -> Result<Vec<EvidenceChange>, ContractError> {
        let mut facts = Vec::new();
        for verdict in diff_versions(self, new).verdicts {
            let (kind, severity) = match verdict.status {
                ScreenStatus::ProcedureChanged => (
                    EvidenceChangeKind::Other("screen-procedure-changed".to_string()),
                    crate::core::Severity::Breaking,
                ),
                ScreenStatus::Removed => (
                    EvidenceChangeKind::Other("screen-removed".to_string()),
                    crate::core::Severity::Breaking,
                ),
                ScreenStatus::Unchanged | ScreenStatus::Cosmetic | ScreenStatus::Added => continue,
            };
            facts.push(EvidenceChange {
                id: change_id(&verdict.screen_id, "screen", &verdict.detail_digest()),
                subject: verdict.screen_id,
                kind,
                severity,
            });
        }
        Ok(facts)
    }
}

/// What one screen's comparison found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenStatus {
    /// Element-for-element identical.
    Unchanged,
    /// Only footer hints moved: chrome, not procedure, writes no fact.
    Cosmetic,
    /// Anything else changed: the cited procedure may be stale.
    ProcedureChanged,
    /// The screen exists only in the new version: additive, no fact.
    Added,
    /// The screen exists only in the old version: citing memories flag.
    Removed,
}

/// One element's before or after view, for display (T9.9 shows old and new
/// frames side by side with the changed element marked).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementView {
    /// The element's stable ID (the old version's for removals).
    pub id: ElementId,
    /// The visible text.
    pub text: String,
    /// 1-indexed row.
    pub row: u32,
    /// 1-indexed column.
    pub col: u32,
    /// The element kind (for example `"pfkey"`).
    pub kind: String,
}

/// How one element changed between versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElementChangeKind {
    /// Present in old, absent in new.
    Removed,
    /// Absent in old, present in new.
    Added,
    /// Same grid position and kind, different text.
    TextChanged,
}

/// One element's change, with both views for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementChange {
    /// The screen holding the element.
    pub screen: String,
    /// How it changed.
    pub kind: ElementChangeKind,
    /// The old view (absent for additions).
    pub before: Option<ElementView>,
    /// The new view (absent for removals).
    pub after: Option<ElementView>,
}

impl ElementChange {
    /// Footer-hint churn is chrome, never procedure: a relabelled `pfkey`
    /// hint alone writes no fact.
    fn is_chrome(&self) -> bool {
        let kind = match &self.kind {
            ElementChangeKind::Removed => self.before.as_ref().map(|v| v.kind.as_str()),
            ElementChangeKind::Added => self.after.as_ref().map(|v| v.kind.as_str()),
            ElementChangeKind::TextChanged => self.before.as_ref().map(|v| v.kind.as_str()),
        };
        kind == Some("pfkey")
    }
}

/// One screen's verdict: its status plus the element changes behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenVerdict {
    /// The screen's name.
    pub screen: String,
    /// The screen's stable ID (the old version's scope).
    pub screen_id: ElementId,
    /// The verdict.
    pub status: ScreenStatus,
    /// The element changes behind the verdict, in old-index order.
    pub changes: Vec<ElementChange>,
}

impl ScreenVerdict {
    /// The order-free digest one flaggable fact derives its ID from.
    fn detail_digest(&self) -> String {
        let mut parts: Vec<String> = self
            .changes
            .iter()
            .map(|c| {
                format!(
                    "{:?}:{}:{}",
                    c.kind,
                    c.before.as_ref().map(|v| v.id.0.as_str()).unwrap_or("-"),
                    c.after.as_ref().map(|v| v.id.0.as_str()).unwrap_or("-"),
                )
            })
            .collect();
        parts.sort();
        ids::sha256_hex(parts.join("|").as_bytes())
    }
}

/// The computed diff between two console versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenDiff {
    /// One verdict per screen: old screens in index order, then additions.
    pub verdicts: Vec<ScreenVerdict>,
}

/// Diff two indexed versions element by element. Matching is
/// position-anchored on the fixed 80x24 grid with the T9.2 scoring
/// tolerances (row within 1, column within 2): the same text at a drifted
/// column is the same element, not a change.
pub fn diff_versions(old: &ScreenIndex, new: &ScreenIndex) -> ScreenDiff {
    let old_screens = old.screens();
    let new_screens = new.screens();
    let mut verdicts = Vec::new();
    for old_screen in &old_screens {
        let name = old_screen.native.type_name.as_deref().unwrap_or_default();
        let new_screen = new_screens
            .iter()
            .find(|s| s.native.type_name.as_deref() == Some(name));
        let Some(new_screen) = new_screen else {
            verdicts.push(ScreenVerdict {
                screen: name.to_string(),
                screen_id: old_screen.id.clone(),
                status: ScreenStatus::Removed,
                changes: Vec::new(),
            });
            continue;
        };
        let changes = diff_elements(
            name,
            &screen_surfaces(old, &old_screen.id),
            &screen_surfaces(new, &new_screen.id),
        );
        let status = if changes.is_empty() {
            ScreenStatus::Unchanged
        } else if changes.iter().all(ElementChange::is_chrome) {
            ScreenStatus::Cosmetic
        } else {
            ScreenStatus::ProcedureChanged
        };
        verdicts.push(ScreenVerdict {
            screen: name.to_string(),
            screen_id: old_screen.id.clone(),
            status,
            changes,
        });
    }
    for new_screen in &new_screens {
        let name = new_screen.native.type_name.as_deref().unwrap_or_default();
        if !old_screens
            .iter()
            .any(|s| s.native.type_name.as_deref() == Some(name))
        {
            verdicts.push(ScreenVerdict {
                screen: name.to_string(),
                screen_id: new_screen.id.clone(),
                status: ScreenStatus::Added,
                changes: Vec::new(),
            });
        }
    }
    ScreenDiff { verdicts }
}

/// One indexed text element's surface for matching, with its stable ID.
#[derive(Debug, Clone)]
struct SurfacedElement {
    id: ElementId,
    kind: String,
    text: String,
    norm_text: String,
    row: u32,
    col: u32,
}

fn screen_surfaces(index: &ScreenIndex, screen: &ElementId) -> Vec<SurfacedElement> {
    index
        .elements
        .iter()
        .filter(|e| e.parent.as_ref() == Some(screen))
        .filter_map(|e| {
            index.surface.get(e.id.0.as_str()).map(|s| SurfacedElement {
                id: e.id.clone(),
                kind: s.kind.clone(),
                text: s.text.clone(),
                norm_text: normalize_text(&s.text),
                row: s.row,
                col: s.col,
            })
        })
        .collect()
}

fn view_of(element: &SurfacedElement) -> ElementView {
    ElementView {
        id: element.id.clone(),
        text: element.text.clone(),
        row: element.row,
        col: element.col,
        kind: element.kind.clone(),
    }
}

/// Match old elements to new ones in deterministic passes: exact grid
/// position and text first, then the same text at a drifted column (the
/// T9.2 column wobble), then the nearest same-kind neighbour within
/// tolerance. Leftovers are removals and additions. Old-index order decides
/// every tie.
fn diff_elements(
    screen: &str,
    old: &[SurfacedElement],
    new: &[SurfacedElement],
) -> Vec<ElementChange> {
    let mut unmatched: Vec<usize> = (0..new.len()).collect();
    let mut paired: Vec<Option<usize>> = vec![None; old.len()];

    let mut take = |predicate: &dyn Fn(&SurfacedElement, &SurfacedElement) -> bool| {
        let mut used = vec![false; unmatched.len()];
        for (old_idx, old_element) in old.iter().enumerate() {
            if paired[old_idx].is_some() {
                continue;
            }
            for (slot, &new_idx) in unmatched.iter().enumerate() {
                if used[slot] {
                    continue;
                }
                if predicate(old_element, &new[new_idx]) {
                    paired[old_idx] = Some(new_idx);
                    used[slot] = true;
                    break;
                }
            }
        }
        let mut remaining = Vec::new();
        for (slot, &new_idx) in unmatched.iter().enumerate() {
            if !used[slot] {
                remaining.push(new_idx);
            }
        }
        unmatched = remaining;
    };

    // Pass 1: exact position, kind and text.
    take(&|a, b| {
        a.row == b.row && a.col == b.col && a.kind == b.kind && a.norm_text == b.norm_text
    });
    // Pass 2: the same text of the same kind on the same row at a drifted
    // column (footer reflow, OCR wobble).
    take(&|a, b| a.row == b.row && a.kind == b.kind && a.norm_text == b.norm_text);
    // Pass 3: the nearest same-kind neighbour within tolerance.
    {
        let mut used = vec![false; unmatched.len()];
        for (old_idx, old_element) in old.iter().enumerate() {
            if paired[old_idx].is_some() {
                continue;
            }
            let mut best: Option<(i64, usize)> = None;
            for (slot, &new_idx) in unmatched.iter().enumerate() {
                if used[slot] {
                    continue;
                }
                let candidate = &new[new_idx];
                if candidate.kind != old_element.kind {
                    continue;
                }
                let row_gap = (candidate.row as i64 - old_element.row as i64).abs();
                let col_gap = (candidate.col as i64 - old_element.col as i64).abs();
                if row_gap > 1 || col_gap > 2 {
                    continue;
                }
                let distance = row_gap * 100 + col_gap;
                if best.is_none_or(|(d, _)| distance < d) {
                    best = Some((distance, slot));
                }
            }
            if let Some((_, slot)) = best {
                paired[old_idx] = Some(unmatched[slot]);
                used[slot] = true;
            }
        }
        let mut remaining = Vec::new();
        for (slot, &new_idx) in unmatched.iter().enumerate() {
            if !used[slot] {
                remaining.push(new_idx);
            }
        }
        unmatched = remaining;
    }

    let mut changes = Vec::new();
    for (old_idx, old_element) in old.iter().enumerate() {
        match paired[old_idx] {
            None => changes.push(ElementChange {
                screen: screen.to_string(),
                kind: ElementChangeKind::Removed,
                before: Some(view_of(old_element)),
                after: None,
            }),
            Some(new_idx) => {
                let new_element = &new[new_idx];
                if old_element.norm_text != new_element.norm_text {
                    changes.push(ElementChange {
                        screen: screen.to_string(),
                        kind: ElementChangeKind::TextChanged,
                        before: Some(view_of(old_element)),
                        after: Some(view_of(new_element)),
                    });
                }
            }
        }
    }
    let mut added: Vec<&SurfacedElement> = unmatched.iter().map(|&i| &new[i]).collect();
    added.sort_by_key(|e| (e.row, e.col));
    for new_element in added {
        changes.push(ElementChange {
            screen: screen.to_string(),
            kind: ElementChangeKind::Added,
            before: None,
            after: Some(view_of(new_element)),
        });
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::ids::{element_id, screen_id, step_id};
    use super::*;
    use crate::core::{Citation, MemoryRef, Severity, flag_citations};
    use crate::models::{
        ModelClient, ModelError, ModelRequest, ModelResponse, ReplayClient, Usage,
    };
    use crate::models::{RecordingClient, vision};
    use crate::prompts::vision::{RawElement, RawScreen, RawStep};

    const CONSOLE: &str = "fernwhistle";
    const RECORDING: &str = "training-v1";

    fn story_path(name: &str) -> String {
        format!(
            "{}/testdata/legacy/story/{name}",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    /// Screens the ground-truth steps show, by step ID. T1 shows the menu,
    /// which the console versions do not index, so it names no screen.
    fn step_screen(id: &str) -> &'static str {
        match id {
            "T1" => "",
            "T2" | "T3" | "T4" => "lookup",
            "T5" | "T6" => "decommission",
            _ => panic!("unknown story step {id}"),
        }
    }

    fn raw_screen(file: &str) -> RawScreen {
        let raw = std::fs::read_to_string(story_path(&format!("screens/{file}.json")))
            .expect("screen reads");
        let value: serde_json::Value = serde_json::from_str(&raw).expect("parses");
        RawScreen {
            name: value["screen"].as_str().expect("screen name").to_string(),
            elements: value["elements"]
                .as_array()
                .expect("elements")
                .iter()
                .map(|e| RawElement {
                    text: e["text"].as_str().expect("text").to_string(),
                    row: u32::try_from(e["row"].as_u64().expect("row")).expect("row fits"),
                    col: u32::try_from(e["col"].as_u64().expect("col")).expect("col fits"),
                    kind: e["kind"].as_str().expect("kind").to_string(),
                    protected: e["protected"].as_bool().expect("protected"),
                })
                .collect(),
        }
    }

    fn raw_steps() -> Vec<RawStep> {
        let raw =
            std::fs::read_to_string(story_path("training-v1-steps.json")).expect("steps read");
        let value: serde_json::Value = serde_json::from_str(&raw).expect("parses");
        value["steps"]
            .as_array()
            .expect("steps")
            .iter()
            .map(|s| {
                let id = s["id"].as_str().expect("step id");
                RawStep {
                    description: s["description"].as_str().expect("description").to_string(),
                    start_s: s["start_s"].as_f64().expect("start"),
                    end_s: s["end_s"].as_f64().expect("end"),
                    screen: step_screen(id).to_string(),
                }
            })
            .collect()
    }

    /// Screens by file stem (`lookup-v1`); the screen name comes from the
    /// file's own `screen` field.
    fn raw_version(files: &[&str], with_steps: bool) -> RawVersion {
        RawVersion {
            screens: files.iter().map(|file| raw_screen(file)).collect(),
            steps: if with_steps { raw_steps() } else { Vec::new() },
            warnings: Vec::new(),
        }
    }

    fn recording_meta() -> RecordingMeta {
        RecordingMeta {
            name: RECORDING.to_string(),
            duration_s: 45.0,
        }
    }

    fn index_v1() -> ScreenIndex {
        let raw = raw_version(&["lookup-v1", "decomm-v1"], true);
        let (index, warnings) = ScreenIndex::from_raw(CONSOLE, &raw, Some(&recording_meta()));
        assert!(warnings.is_empty(), "{warnings:?}");
        index
    }

    fn index_v2() -> ScreenIndex {
        // v2 is never trained, only re-watched: stills, no recording.
        let raw = raw_version(&["lookup-v2", "decomm-v2"], false);
        let (index, warnings) = ScreenIndex::from_raw(CONSOLE, &raw, None);
        assert!(warnings.is_empty(), "{warnings:?}");
        index
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

    fn decomm_id() -> ElementId {
        screen_id(CONSOLE, "decommission")
    }

    fn lookup_id() -> ElementId {
        screen_id(CONSOLE, "lookup")
    }

    /// The step IDs demonstrating one screen (or none, for the menu step),
    /// derived from the ground-truth steps so no description is hand-copied.
    fn step_ids(screen: &str) -> Vec<ElementId> {
        raw_steps()
            .into_iter()
            .filter(|s| s.screen == screen)
            .map(|s| step_id(CONSOLE, RECORDING, &s.description, s.start_s, s.end_s))
            .collect()
    }

    #[test]
    fn v1_indexes_two_screens_and_the_training_video() {
        let index = index_v1();
        assert_eq!(index.screens().len(), 2);
        // 2 screens + 37 lookup + 21 decommission elements + recording + 6 steps.
        assert_eq!(index.elements().len(), 67);
        assert_eq!(
            index.element(&decomm_id()).expect("decommission").kind,
            EvidenceElementKind::Type
        );
        let warning = index
            .element(&element_id(
                CONSOLE,
                "decommission",
                "message",
                "WARNING: DECOMMISSION IS PERMANENT.",
                9,
                4,
            ))
            .expect("warning element");
        assert_eq!(warning.parent, Some(decomm_id()));
        // Steps reference the screen they demonstrate.
        let decomm_steps = step_ids("decommission");
        assert_eq!(decomm_steps.len(), 2);
        let step = index.element(&decomm_steps[0]).expect("T5");
        assert_eq!(step.references, vec![decomm_id()]);
        let menu_steps = step_ids("");
        assert_eq!(menu_steps.len(), 1);
        assert!(
            index
                .element(&menu_steps[0])
                .expect("T1")
                .references
                .is_empty()
        );
        // Native locations serve the guard's fields.
        let native = index.native_location(&decomm_id()).expect("native");
        assert_eq!(native.type_name.as_deref(), Some("decommission"));
        let step_native = index
            .native_location(&decomm_steps[0])
            .expect("step native");
        assert!(step_native.operation_id.is_some());
    }

    #[test]
    fn ids_ignore_reply_order() {
        let raw = raw_version(&["lookup-v1", "decomm-v1"], true);
        let (index, _) = ScreenIndex::from_raw(CONSOLE, &raw, Some(&recording_meta()));
        let mut shuffled = raw.clone();
        shuffled.screens.reverse();
        for screen in &mut shuffled.screens {
            screen.elements.reverse();
        }
        shuffled.steps.reverse();
        let (reordered, _) = ScreenIndex::from_raw(CONSOLE, &shuffled, Some(&recording_meta()));
        let before_elements = index.elements();
        let after_elements = reordered.elements();
        let mut before: Vec<&str> = before_elements.iter().map(|e| e.id.0.as_str()).collect();
        let mut after: Vec<&str> = after_elements.iter().map(|e| e.id.0.as_str()).collect();
        before.sort();
        after.sort();
        assert_eq!(before, after);
    }

    #[test]
    fn identical_versions_diff_to_nothing() {
        let old = index_v1();
        let diff = diff_versions(&old, &old);
        assert!(
            diff.verdicts
                .iter()
                .all(|v| v.status == ScreenStatus::Unchanged)
        );
        assert!(diff.verdicts.iter().all(|v| v.changes.is_empty()));
        assert!(old.diff_evidence(&old).expect("diff works").is_empty());
        // The fact ID is deterministic across runs.
        let new = index_v2();
        let first = old.diff_evidence(&new).expect("diff works");
        let second = old.diff_evidence(&new).expect("diff works");
        assert_eq!(first, second);
    }

    #[test]
    fn lookup_footer_relabel_is_cosmetic() {
        let (old, new) = (index_v1(), index_v2());
        let diff = diff_versions(&old, &new);
        let lookup = diff
            .verdicts
            .iter()
            .find(|v| v.screen == "lookup")
            .expect("lookup verdict");
        assert_eq!(lookup.status, ScreenStatus::Cosmetic);
        assert_eq!(lookup.changes.len(), 1);
        let change = &lookup.changes[0];
        assert_eq!(change.kind, ElementChangeKind::TextChanged);
        assert_eq!(change.before.as_ref().expect("before").text, "PF5 UPDATE");
        assert_eq!(change.after.as_ref().expect("after").text, "PF5 SAVE");
        // The column-drifted hints are the same elements, not changes:
        // only the PF5 text itself differs.
    }

    #[test]
    fn decomm_diff_gives_the_changed_procedure() {
        let (old, new) = (index_v1(), index_v2());
        let diff = diff_versions(&old, &new);
        let decomm = diff
            .verdicts
            .iter()
            .find(|v| v.screen == "decommission")
            .expect("decommission verdict");
        assert_eq!(decomm.status, ScreenStatus::ProcedureChanged);
        let removed: Vec<&str> = decomm
            .changes
            .iter()
            .filter(|c| c.kind == ElementChangeKind::Removed)
            .map(|c| c.before.as_ref().expect("before").text.as_str())
            .collect();
        assert_eq!(
            removed,
            [
                "WARNING: DECOMMISSION IS PERMANENT.",
                "TYPE CLINIC NAME TO CONFIRM:",
                "________________",
            ]
        );
        let changed: Vec<(&str, &str)> = decomm
            .changes
            .iter()
            .filter(|c| c.kind == ElementChangeKind::TextChanged)
            .map(|c| {
                (
                    c.before.as_ref().expect("before").text.as_str(),
                    c.after.as_ref().expect("after").text.as_str(),
                )
            })
            .collect();
        assert_eq!(changed, [("PF12 CONFIRM", "PF12 DECOMMISSION")]);
        assert!(
            decomm.changes.iter().all(|c| c.screen == "decommission"),
            "{:?}",
            decomm.changes
        );

        let facts = old.diff_evidence(&new).expect("diff works");
        assert_eq!(facts.len(), 1, "{facts:?}");
        let fact = &facts[0];
        assert_eq!(fact.subject, decomm_id());
        assert_eq!(fact.severity, Severity::Breaking);
        assert!(
            matches!(fact.kind, EvidenceChangeKind::Other(ref name) if name == "screen-procedure-changed")
        );
        assert!(fact.id.0.starts_with("sc2_"));
    }

    #[test]
    fn citing_the_training_span_flags_only_the_changed_procedure() {
        let old = index_v1();
        let new = index_v2();
        let facts = old.diff_evidence(&new).expect("diff works");
        let decomm_steps = step_ids("decommission");
        let lookup_steps = step_ids("lookup");
        assert_eq!(decomm_steps.len(), 2);
        assert!(lookup_steps.len() >= 2);
        // S1 as the story cites it: the decommission screen plus T5–T6.
        let s1_screen = citation("S1", &[&[decomm_id()], &decomm_steps[..]].concat());
        // The span alone, without the screen, flags too.
        let s1_span = citation("S1-span", &decomm_steps[..1]);
        // S2 as the story cites it: the lookup screen plus T2–T3.
        let s2 = citation("S2", &[&[lookup_id()], &lookup_steps[..2]].concat());
        let flags = flag_citations(&[s1_screen, s1_span, s2], &facts, &old, &[], "v1", "v2");
        let flagged: Vec<&str> = flags.iter().map(|f| f.memory.id.as_str()).collect();
        assert_eq!(flagged, ["S1", "S1-span"], "{flags:?}");
        for flag in &flags {
            assert_eq!(flag.severity, Severity::Breaking);
            assert_eq!(flag.changes.len(), 1);
        }
    }

    #[test]
    fn step_spans_pad_five_seconds_clamped_to_the_recording() {
        let index = index_v1();
        let menu_steps = step_ids("");
        let decomm_steps = step_ids("decommission");
        let (t1, t5, t6) = (&menu_steps[0], &decomm_steps[0], &decomm_steps[1]);
        assert_eq!(index.step_span(t5), Some((28.0, 33.0)));
        assert_eq!(index.padded_span(t1), Some((0.0, 11.0)));
        assert_eq!(index.padded_span(t5), Some((23.0, 38.0)));
        assert_eq!(index.padded_span(t6), Some((28.0, 45.0)));
    }

    #[test]
    fn unknown_step_screen_detaches_with_a_warning() {
        let mut raw = raw_version(&["lookup-v1"], false);
        raw.steps.push(RawStep {
            description: "Open the menu".to_string(),
            start_s: 0.0,
            end_s: 6.0,
            screen: "menu".to_string(),
        });
        let meta = recording_meta();
        let (index, warnings) = ScreenIndex::from_raw(CONSOLE, &raw, Some(&meta));
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("menu"));
        let step = index
            .elements()
            .into_iter()
            .find(|e| e.native.operation_id.as_deref() == Some("Open the menu"))
            .expect("step indexed");
        assert!(step.references.is_empty());
    }

    #[test]
    fn added_screen_is_silent_removed_screen_flags() {
        let full = index_v1();
        let lookup_only_raw = raw_version(&["lookup-v1"], false);
        let (lookup_only, _) = ScreenIndex::from_raw(CONSOLE, &lookup_only_raw, None);
        // Removing the decommission screen flags its citers.
        let removed = full.diff_evidence(&lookup_only).expect("diff works");
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].subject, decomm_id());
        let flags = flag_citations(
            &[citation("S1", &[decomm_id()])],
            &removed,
            &full,
            &[],
            "v1",
            "v2",
        );
        assert_eq!(flags.len(), 1);
        // Adding it back writes no fact: additive changes flag nothing.
        let added = lookup_only.diff_evidence(&full).expect("diff works");
        assert!(added.is_empty(), "{added:?}");
    }

    /// The Done-when run: from committed PNG/MP4 bytes and recorded replies,
    /// one Wonder call per version, the diff gives the changed procedure and
    /// no invented change, and the S1 citations (screen and span) flag while
    /// S2 stays silent. No network, no key: the replay client serves the
    /// recorded replies keyed on the media's hash.
    #[test]
    fn recorded_console_run_flags_the_changed_procedure() {
        let dir = std::env::temp_dir().join(format!(
            "doyle-t97-recorded-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");

        let lookup_png = std::fs::read(story_path("screens/lookup-v1.png")).expect("lookup art");
        let decomm_png = std::fs::read(story_path("screens/decomm-v1.png")).expect("decomm art");
        let training_mp4 = std::fs::read(story_path("training-v1.mp4")).expect("training video");
        // Usage magnitudes mirror the T9.2 measurements (§5/§7): ~940 input
        // tokens per still, ~230 per video second, a few thousand out. Live
        // values land with T9.7b.
        let v1_spec = vision::VersionSpec {
            console: CONSOLE.to_string(),
            version: "v1".to_string(),
            screens: vec![
                vision::ScreenInput {
                    name: "lookup".to_string(),
                    png: lookup_png.clone(),
                },
                vision::ScreenInput {
                    name: "decommission".to_string(),
                    png: decomm_png.clone(),
                },
            ],
            recording: Some(vision::RecordingInput {
                name: RECORDING.to_string(),
                duration_s: 45.0,
                mp4: training_mp4,
            }),
        };
        let lookup_v2_png =
            std::fs::read(story_path("screens/lookup-v2.png")).expect("lookup v2 art");
        let decomm_v2_png =
            std::fs::read(story_path("screens/decomm-v2.png")).expect("decomm v2 art");
        let v2_spec = vision::VersionSpec {
            console: CONSOLE.to_string(),
            version: "v2".to_string(),
            screens: vec![
                vision::ScreenInput {
                    name: "lookup".to_string(),
                    png: lookup_v2_png,
                },
                vision::ScreenInput {
                    name: "decommission".to_string(),
                    png: decomm_v2_png,
                },
            ],
            recording: None,
        };

        let v1_reply = reply_from_ground_truth(&["lookup-v1", "decomm-v1"], true);
        let v2_reply = reply_from_ground_truth(&["lookup-v2", "decomm-v2"], false);
        let v1_usage = Usage {
            input_tokens: Some(12_200),
            output_tokens: Some(3_900),
            cost_usd: None,
        };
        let v2_usage = Usage {
            input_tokens: Some(1_900),
            output_tokens: Some(2_100),
            cost_usd: None,
        };

        let v1_outcome = recorded_read(&dir, &v1_spec, v1_reply, v1_usage.clone());
        let v2_outcome = recorded_read(&dir, &v2_spec, v2_reply, v2_usage.clone());
        // Each call is recorded with its usage.
        assert_eq!(v1_outcome.usage, v1_usage);
        assert_eq!(v2_outcome.usage, v2_usage);
        assert!(v1_outcome.warnings.is_empty(), "{:?}", v1_outcome.warnings);
        assert!(v2_outcome.warnings.is_empty(), "{:?}", v2_outcome.warnings);
        // The media moves the key: the same reply under different bytes
        // replays under a different key.
        assert_ne!(v1_outcome.request_key, v2_outcome.request_key);

        let facts = v1_outcome
            .index
            .diff_evidence(&v2_outcome.index)
            .expect("diff works");
        assert_eq!(facts.len(), 1, "{facts:?}");
        assert_eq!(facts[0].subject, decomm_id());

        let decomm_steps = step_ids("decommission");
        let lookup_steps = step_ids("lookup");
        let citations = vec![
            citation("S1", &[&[decomm_id()], &decomm_steps[..]].concat()),
            citation("S1-span", &decomm_steps[..1]),
            citation("S2", &[&[lookup_id()], &lookup_steps[..2]].concat()),
        ];
        let flags = flag_citations(&citations, &facts, &v1_outcome.index, &[], "v1", "v2");
        let flagged: Vec<&str> = flags.iter().map(|f| f.memory.id.as_str()).collect();
        assert_eq!(flagged, ["S1", "S1-span"], "{flags:?}");

        std::fs::remove_dir_all(&dir).ok();
    }

    struct Canned {
        response: ModelResponse,
    }

    impl ModelClient for Canned {
        fn complete(&self, _request: &ModelRequest) -> Result<ModelResponse, ModelError> {
            Ok(self.response.clone())
        }
    }

    /// Record one canned reply, then replay it: the replayed index equals
    /// the recorded one, proving the media-keyed round trip.
    fn recorded_read(
        dir: &std::path::Path,
        spec: &vision::VersionSpec,
        reply: serde_json::Value,
        usage: Usage,
    ) -> vision::ReadOutcome {
        let response = ModelResponse {
            value: reply,
            usage,
        };
        let recorder = RecordingClient::new(Canned { response }, dir);
        let recorded = vision::read_version(&recorder, spec).expect("records");
        let replayer = ReplayClient::new(dir);
        let replayed = vision::read_version(&replayer, spec).expect("replays");
        assert_eq!(
            recorded.request_key, replayed.request_key,
            "same media replays under the same key"
        );
        assert_eq!(
            recorded.index.elements(),
            replayed.index.elements(),
            "replay restores the same index"
        );
        replayed
    }

    /// A recorded reply built from the committed ground truth: the screens'
    /// element lists and the training video's steps, with the step-screen
    /// links from [`step_screen`]. Screens are named by file stem
    /// (`lookup-v1`); the reply carries the file's own screen name.
    fn reply_from_ground_truth(files: &[&str], with_steps: bool) -> serde_json::Value {
        let screens_json: Vec<serde_json::Value> = files
            .iter()
            .map(|file| {
                let raw = std::fs::read_to_string(story_path(&format!("screens/{file}.json")))
                    .expect("story screen reads");
                let value: serde_json::Value = serde_json::from_str(&raw).expect("parses");
                let elements: Vec<serde_json::Value> = value["elements"]
                    .as_array()
                    .expect("elements")
                    .iter()
                    .map(|e| {
                        serde_json::json!({
                            "text": e["text"],
                            "row": e["row"],
                            "col": e["col"],
                            "kind": e["kind"],
                            "protected": e["protected"],
                        })
                    })
                    .collect();
                serde_json::json!({"name": value["screen"], "elements": elements})
            })
            .collect();
        let steps_json: Vec<serde_json::Value> = if with_steps {
            let raw =
                std::fs::read_to_string(story_path("training-v1-steps.json")).expect("steps read");
            let value: serde_json::Value = serde_json::from_str(&raw).expect("parses");
            value["steps"]
                .as_array()
                .expect("steps")
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "description": s["description"],
                        "start_s": s["start_s"],
                        "end_s": s["end_s"],
                        "screen": step_screen(s["id"].as_str().expect("step id")),
                    })
                })
                .collect()
        } else {
            Vec::new()
        };
        serde_json::json!({"screens": screens_json, "steps": steps_json})
    }
}
