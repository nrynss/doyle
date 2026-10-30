//! Apply a person's decision to a proposal: write the correction back
//! through every adapter holding the memory.
//!
//! The core knows only the [`MemoryAdapter`](crate::adapters::MemoryAdapter)
//! trait, never a particular memory system: every adapter is treated
//! uniformly through its [`Capabilities`](crate::adapters::Capabilities).
//! Lambo gets no special path.
//!
//! # Routing: decision x capability -> adapter calls
//!
//! The verdict resolves to one effective action, then each target adapter is
//! written through by its capabilities, with exactly one mutating call per
//! adapter per apply:
//!
//! | Effective action | `supersede` supported | `annotate` only | neither |
//! |---|---|---|
//! | Correct(text) | `supersede(id, text)` | `annotate(id, correction_warning(text))` | `write(text)` in the stale memory's scope; the old memory stays, and the outcome says so |
//! | Retire | `supersede(id, RETIREMENT_TEXT)` | `annotate(id, RETIRED_MARKER)` | `write(RETIREMENT_TEXT)`; the old memory stays, and the outcome says so |
//! | Keep | no call | no call | no call |
//!
//! The verdict wins over the proposal: `Approve` follows the proposal's
//! action, `Retire` retires whatever was proposed, `Keep` keeps whatever was
//! proposed, and `EditThenApprove` corrects with the person's text whatever
//! was proposed.
//!
//! # Partial failure: never half-silent
//!
//! [`apply_decision`] records the [`Decision`] in the [`Store`] FIRST, then
//! writes through the adapters. One adapter's failure never stops the others,
//! and the returned [`DecisionReport`] holds one [`PerAdapterResult`] per
//! target: each is either a success describing what was done (with the new
//! reference where one was minted) or a [`AdapterOutcome::Failed`] carrying
//! the adapter's message. No adapter is left in a state the report does not
//! describe. A failed target is marked for retry
//! ([`AdapterOutcome::needs_retry`]); the caller retries only those targets
//! with [`retry_failed`], which performs no store write.
//!
//! # Retry converges: the stated assumption
//!
//! Retry converges to exactly one correction per adapter under this
//! assumption: **each adapter's `supersede` is idempotent for identical
//! (`stale_id`, `correction_text`)** — it returns the existing correction
//! without creating a duplicate. That is the adapters' own contract (Muse
//! Code points at the identical-text correction or reuses the identical-text
//! bullet; Lambo checks its cache, then inspects for the existing
//! against-link; the fake checks pointer plus text), pinned by their own
//! retry tests. `apply_decision` and [`retry_failed`] always re-issue the
//! identical text for a given decision, so every retry lands on the adapter
//! no-op path.
//!
//! Two disclosed limits: (1) `annotate` is NOT idempotent (Muse Code appends),
//! so the caller must retry only [`AdapterOutcome::Failed`] targets —
//! `retry_failed` takes exactly those. (2) The write-fallback path mints a
//! fresh memory per call, so a retry after a timed-out-but-landed write could
//! duplicate; no adapter in the tree is write-only today (both real adapters
//! support `supersede`), so the path is tested but unexercised in practice.
//!
//! # Seams
//!
//! The caller resolves which (adapter, memory id) pairs hold the logical
//! memory (e.g. M1 lives in Muse Code under one id and in Lambo under
//! another); [`Target`] carries exactly that. Per-adapter outcomes are
//! returned in the [`DecisionReport`], not persisted: the store (frozen at
//! T1.2) has no outcome record, so retry state lives with the caller until
//! the review page owns it. Recording the decision settles the flag in store
//! terms (`open_flags` closes on any decision behind a proposal); surfacing
//! pending retries is the caller's job.

use super::store::StoreError;
use super::{Decision, DecisionVerdict, MemoryRef, Proposal, ProposalAction, Store};
use crate::adapters::{AdapterError, MemoryAdapter};

/// The text written as the correction when a memory is retired through
/// `supersede` or `write`: the link (against / superseded-by / new note)
/// ties it to the stale memory, so the notice itself stays generic.
pub const RETIREMENT_TEXT: &str = "This memory is retired; it no longer applies.";

/// The in-place marker for retiring through `annotate`-only adapters.
pub const RETIRED_MARKER: &str = "[RETIRED: no longer applies]";

/// The in-place warning for correcting through `annotate`-only adapters: the
/// correction rides inside the warning, since no correction memory can be
/// created on that path.
pub fn correction_warning(correction: &str) -> String {
    format!("[STALE correction: {correction}]")
}

/// One memory in one adapter's system that holds the logical memory a
/// proposal answers. The caller resolves these; see the module docs.
pub struct Target<'a> {
    /// The adapter holding this copy.
    pub adapter: &'a dyn MemoryAdapter,
    /// The memory's id in that adapter's own terms.
    pub memory_id: String,
}

/// What one adapter did (or failed to do) for a decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterOutcome {
    /// Correction created and linked via `supersede`, with its reference.
    Superseded { correction: MemoryRef },
    /// Warning placed in situ via `annotate`.
    Annotated,
    /// A fresh memory written because the adapter supports neither
    /// `supersede` nor `annotate`. `stale_remains` is always true today: the
    /// old memory is untouched, and the report says so.
    WrittenNew {
        new_memory: MemoryRef,
        stale_remains: bool,
    },
    /// The verdict kept the memory: no adapter call was made, by design.
    Kept,
    /// The adapter call failed. `retryable` is true for backend failures
    /// (worth retrying: [`retry_failed`] converges via the adapter no-ops)
    /// and false for terminal ones (not found, unsupported).
    Failed { message: String, retryable: bool },
}

impl AdapterOutcome {
    /// Whether this outcome marks its target for retry.
    pub fn needs_retry(&self) -> bool {
        matches!(
            self,
            Self::Failed {
                retryable: true,
                ..
            }
        )
    }
}

/// What one adapter's copy looks like after a decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerAdapterResult {
    /// The adapter's declared name.
    pub adapter: String,
    /// The memory id that was targeted in that adapter.
    pub memory_id: String,
    /// What was done.
    pub outcome: AdapterOutcome,
}

/// The recorded decision plus, per adapter, what was done. The caller owns
/// retry: feed the failed targets back to [`retry_failed`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionReport {
    /// Store-assigned id of the recorded decision.
    pub decision_id: String,
    /// The proposal the decision settles.
    pub proposal_id: String,
    /// One entry per target, in target order.
    pub results: Vec<PerAdapterResult>,
}

impl DecisionReport {
    /// Whether any target is marked for retry.
    pub fn needs_retry(&self) -> bool {
        self.results.iter().any(|r| r.outcome.needs_retry())
    }
}

/// What can go wrong in [`apply_decision`]. Adapter failures never surface
/// here: they are per-target [`AdapterOutcome::Failed`] entries in the
/// report.
#[derive(Debug)]
pub enum DecideError {
    /// Recording the decision failed, before any adapter was touched.
    Store(StoreError),
}

impl std::fmt::Display for DecideError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(e) => write!(f, "decide: {e}"),
        }
    }
}

impl std::error::Error for DecideError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Store(e) => Some(e),
        }
    }
}

impl From<StoreError> for DecideError {
    fn from(e: StoreError) -> Self {
        Self::Store(e)
    }
}

/// The verdict resolved against the proposal: what every adapter must do.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Effective {
    Correct(String),
    Retire,
    Keep,
}

fn resolve(proposal: &Proposal, verdict: &DecisionVerdict) -> Effective {
    match verdict {
        DecisionVerdict::Keep => Effective::Keep,
        DecisionVerdict::Retire => Effective::Retire,
        DecisionVerdict::Approve => match &proposal.action {
            ProposalAction::Correct { new_text } => Effective::Correct(new_text.clone()),
            ProposalAction::Retire => Effective::Retire,
            ProposalAction::Keep => Effective::Keep,
        },
        DecisionVerdict::EditThenApprove { new_text } => Effective::Correct(new_text.clone()),
    }
}

fn failed(error: AdapterError) -> AdapterOutcome {
    let retryable = matches!(error, AdapterError::Backend { .. });
    AdapterOutcome::Failed {
        message: error.to_string(),
        retryable,
    }
}

/// Write the fallback fresh memory for adapters with neither `supersede` nor
/// `annotate`. The stale memory is read first for its scope (so the new note
/// lives where the old one did); a read failure is reported, not worked
/// around.
fn write_fallback(adapter: &dyn MemoryAdapter, memory_id: &str, text: &str) -> AdapterOutcome {
    let scope = match adapter.read(memory_id) {
        Ok(memory) => memory.scope,
        Err(e) => return failed(e),
    };
    match adapter.write(text, &scope) {
        Ok(new_memory) => AdapterOutcome::WrittenNew {
            new_memory,
            stale_remains: true,
        },
        Err(e) => failed(e),
    }
}

fn apply_one(
    adapter: &dyn MemoryAdapter,
    memory_id: &str,
    effective: &Effective,
) -> AdapterOutcome {
    match effective {
        Effective::Keep => AdapterOutcome::Kept,
        Effective::Correct(text) => {
            if adapter.supports("supersede") {
                match adapter.supersede(memory_id, text) {
                    Ok(correction) => AdapterOutcome::Superseded { correction },
                    Err(e) => failed(e),
                }
            } else if adapter.supports("annotate") {
                match adapter.annotate(memory_id, &correction_warning(text)) {
                    Ok(()) => AdapterOutcome::Annotated,
                    Err(e) => failed(e),
                }
            } else {
                write_fallback(adapter, memory_id, text)
            }
        }
        Effective::Retire => {
            if adapter.supports("supersede") {
                match adapter.supersede(memory_id, RETIREMENT_TEXT) {
                    Ok(correction) => AdapterOutcome::Superseded { correction },
                    Err(e) => failed(e),
                }
            } else if adapter.supports("annotate") {
                match adapter.annotate(memory_id, RETIRED_MARKER) {
                    Ok(()) => AdapterOutcome::Annotated,
                    Err(e) => failed(e),
                }
            } else {
                write_fallback(adapter, memory_id, RETIREMENT_TEXT)
            }
        }
    }
}

/// Apply `verdict` on `proposal`: record the [`Decision`] in `store`, then
/// write it through each target adapter by its capabilities.
///
/// The decision is recorded FIRST so a partial failure still leaves the
/// decision recorded with each failed adapter marked for retry in the
/// report. If recording itself fails, no adapter is touched.
pub fn apply_decision(
    store: &mut Store,
    proposal: &Proposal,
    verdict: &DecisionVerdict,
    decided_by: &str,
    decided_at: &str,
    targets: &[Target<'_>],
) -> Result<DecisionReport, DecideError> {
    let decision_id = store.record_decision(Decision {
        id: String::new(),
        proposal_id: proposal.id.clone(),
        verdict: verdict.clone(),
        decided_by: decided_by.to_string(),
        decided_at: decided_at.to_string(),
    })?;
    let effective = resolve(proposal, verdict);
    let results = targets
        .iter()
        .map(|t| PerAdapterResult {
            adapter: t.adapter.name().to_string(),
            memory_id: t.memory_id.clone(),
            outcome: apply_one(t.adapter, &t.memory_id, &effective),
        })
        .collect();
    Ok(DecisionReport {
        decision_id,
        proposal_id: proposal.id.clone(),
        results,
    })
}

/// Retry the failed targets of a decision: no store write, just the adapter
/// calls, with the identical text so the adapters' retry no-ops make a
/// repeated retry converge. Pass only targets whose outcome
/// [`AdapterOutcome::needs_retry`] (or, harmlessly, any target: successes
/// re-applied with identical text converge to the same references on the
/// `supersede` path, but `annotate` re-application would append a duplicate
/// warning).
pub fn retry_failed(
    proposal: &Proposal,
    verdict: &DecisionVerdict,
    targets: &[Target<'_>],
) -> Vec<PerAdapterResult> {
    let effective = resolve(proposal, verdict);
    targets
        .iter()
        .map(|t| PerAdapterResult {
            adapter: t.adapter.name().to_string(),
            memory_id: t.memory_id.clone(),
            outcome: apply_one(t.adapter, &t.memory_id, &effective),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{Capabilities, FakeAdapter, MemoryAdapter, muse_code::MuseCodeAdapter};
    use crate::core::model::ProposalAction;
    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn temp_dir(label: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let p =
            std::env::temp_dir().join(format!("doyle-decide-{label}-{n}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn temp_store() -> (PathBuf, Store) {
        let dir = temp_dir("store");
        let state = dir.join(".doyle");
        let store = Store::open(&state).unwrap();
        (dir, store)
    }

    fn proposal(action: ProposalAction) -> Proposal {
        Proposal {
            id: "proposal-1".to_string(),
            flag_id: "flag-1".to_string(),
            action,
            rationale: "v2 pages at 100".to_string(),
            proposed_by: "muse-spark-1.3".to_string(),
        }
    }

    const CORRECTION: &str =
        "GET /patients returns one page of 100 plus next_cursor; follow cursor to the end";
    const STALE: &str =
        "GET /patients returns every patient in one response; one call exports the whole clinic";

    fn seed_fake(adapter: &FakeAdapter) -> String {
        adapter.write(STALE, "test-scope").unwrap().id
    }

    // ---- verdict x proposal resolution, through the full fake ----

    #[test]
    fn approve_correct_supersedes_and_records() {
        let fake = FakeAdapter::new("fake");
        let stale_id = seed_fake(&fake);
        let (_dir, mut store) = temp_store();
        let proposal = proposal(ProposalAction::Correct {
            new_text: CORRECTION.to_string(),
        });

        let report = apply_decision(
            &mut store,
            &proposal,
            &DecisionVerdict::Approve,
            "reviewer",
            "2026-10-29T12:00:00Z",
            &[Target {
                adapter: &fake,
                memory_id: stale_id.clone(),
            }],
        )
        .unwrap();

        // Decision recorded with its verdict, author and time.
        let decision = store.decision_for_proposal("proposal-1").unwrap();
        assert_eq!(report.decision_id, decision.id);
        assert_eq!(report.proposal_id, "proposal-1");
        assert_eq!(decision.verdict, DecisionVerdict::Approve);
        assert_eq!(decision.decided_by, "reviewer");
        assert_eq!(decision.decided_at, "2026-10-29T12:00:00Z");
        // One entry, describing the adapter's state (mirror of
        // check_conformance's supersede assertions).
        assert_eq!(report.results.len(), 1);
        let correction = match &report.results[0].outcome {
            AdapterOutcome::Superseded { correction } => correction.clone(),
            other => panic!("expected superseded, got {other:?}"),
        };
        assert_eq!(report.results[0].adapter, "fake");
        assert!(!report.needs_retry());
        let stale = fake.read(&stale_id).unwrap();
        assert!(
            stale.text.contains(&correction.id),
            "stale memory names its correction, got: {}",
            stale.text
        );
        let stored = fake.read(&correction.id).unwrap();
        assert!(
            stored.text.contains(CORRECTION),
            "correction reads back with its text, got: {}",
            stored.text
        );
    }

    #[test]
    fn edit_then_approve_writes_edited_text() {
        let fake = FakeAdapter::new("fake");
        let stale_id = seed_fake(&fake);
        let (_dir, mut store) = temp_store();
        let proposal = proposal(ProposalAction::Correct {
            new_text: "proposed wording".to_string(),
        });

        let report = apply_decision(
            &mut store,
            &proposal,
            &DecisionVerdict::EditThenApprove {
                new_text: CORRECTION.to_string(),
            },
            "reviewer",
            "2026-10-29T12:00:00Z",
            &[Target {
                adapter: &fake,
                memory_id: stale_id.clone(),
            }],
        )
        .unwrap();

        let correction = match &report.results[0].outcome {
            AdapterOutcome::Superseded { correction } => correction.clone(),
            other => panic!("expected superseded, got {other:?}"),
        };
        let stored = fake.read(&correction.id).unwrap();
        assert!(stored.text.contains(CORRECTION));
        assert!(!stored.text.contains("proposed wording"));
    }

    #[test]
    fn approve_retire_proposal_retires_through_supersede() {
        let fake = FakeAdapter::new("fake");
        let stale_id = seed_fake(&fake);
        let (_dir, mut store) = temp_store();
        let proposal = proposal(ProposalAction::Retire);

        let report = apply_decision(
            &mut store,
            &proposal,
            &DecisionVerdict::Approve,
            "reviewer",
            "2026-10-29T12:00:00Z",
            &[Target {
                adapter: &fake,
                memory_id: stale_id.clone(),
            }],
        )
        .unwrap();

        let correction = match &report.results[0].outcome {
            AdapterOutcome::Superseded { correction } => correction.clone(),
            other => panic!("expected superseded, got {other:?}"),
        };
        let stored = fake.read(&correction.id).unwrap();
        assert_eq!(stored.text, RETIREMENT_TEXT);
        let stale = fake.read(&stale_id).unwrap();
        assert!(stale.text.contains(&correction.id));
    }

    #[test]
    fn retire_verdict_overrides_correct_proposal() {
        let fake = FakeAdapter::new("fake");
        let stale_id = seed_fake(&fake);
        let (_dir, mut store) = temp_store();
        let proposal = proposal(ProposalAction::Correct {
            new_text: CORRECTION.to_string(),
        });

        let report = apply_decision(
            &mut store,
            &proposal,
            &DecisionVerdict::Retire,
            "reviewer",
            "2026-10-29T12:00:00Z",
            &[Target {
                adapter: &fake,
                memory_id: stale_id.clone(),
            }],
        )
        .unwrap();

        let correction = match &report.results[0].outcome {
            AdapterOutcome::Superseded { correction } => correction.clone(),
            other => panic!("expected superseded, got {other:?}"),
        };
        assert_eq!(fake.read(&correction.id).unwrap().text, RETIREMENT_TEXT);
    }

    #[test]
    fn keep_verdict_writes_nothing() {
        let fake = FakeAdapter::new("fake");
        let stale_id = seed_fake(&fake);
        let before = fake.read(&stale_id).unwrap();
        let (_dir, mut store) = temp_store();
        let proposal = proposal(ProposalAction::Correct {
            new_text: CORRECTION.to_string(),
        });

        let report = apply_decision(
            &mut store,
            &proposal,
            &DecisionVerdict::Keep,
            "reviewer",
            "2026-10-29T12:00:00Z",
            &[Target {
                adapter: &fake,
                memory_id: stale_id.clone(),
            }],
        )
        .unwrap();

        assert_eq!(report.results[0].outcome, AdapterOutcome::Kept);
        assert!(!report.needs_retry());
        // Untouched: byte-identical text, no new memory.
        assert_eq!(fake.read(&stale_id).unwrap(), before);
        assert_eq!(fake.list("").unwrap().len(), 1);
        // The decision is still recorded: keep is a verdict too.
        assert!(store.decision_for_proposal("proposal-1").is_some());
    }

    #[test]
    fn approve_keep_proposal_keeps() {
        let fake = FakeAdapter::new("fake");
        let stale_id = seed_fake(&fake);
        let (_dir, mut store) = temp_store();
        let proposal = proposal(ProposalAction::Keep);

        let report = apply_decision(
            &mut store,
            &proposal,
            &DecisionVerdict::Approve,
            "reviewer",
            "2026-10-29T12:00:00Z",
            &[Target {
                adapter: &fake,
                memory_id: stale_id.clone(),
            }],
        )
        .unwrap();

        assert_eq!(report.results[0].outcome, AdapterOutcome::Kept);
        assert_eq!(fake.list("").unwrap().len(), 1);
    }

    // ---- capability routing through reduced fakes ----

    fn annotate_only() -> FakeAdapter {
        FakeAdapter::with_capabilities(
            "annotate-only",
            Capabilities {
                annotate: true,
                supersede: false,
                watch: false,
            },
        )
    }

    fn write_only() -> FakeAdapter {
        FakeAdapter::with_capabilities("write-only", Capabilities::NONE)
    }

    #[test]
    fn annotate_fallback_carries_correction_text() {
        let adapter = annotate_only();
        let stale_id = seed_fake(&adapter);
        let (_dir, mut store) = temp_store();
        let proposal = proposal(ProposalAction::Correct {
            new_text: CORRECTION.to_string(),
        });

        let report = apply_decision(
            &mut store,
            &proposal,
            &DecisionVerdict::Approve,
            "reviewer",
            "2026-10-29T12:00:00Z",
            &[Target {
                adapter: &adapter,
                memory_id: stale_id.clone(),
            }],
        )
        .unwrap();

        assert_eq!(report.results[0].outcome, AdapterOutcome::Annotated);
        let stale = adapter.read(&stale_id).unwrap();
        assert!(
            stale.text.contains(&correction_warning(CORRECTION)),
            "annotation carries the correction, got: {}",
            stale.text
        );
        // Annotated, not superseded: no new memory.
        assert_eq!(adapter.list("").unwrap().len(), 1);
    }

    #[test]
    fn retire_through_annotate_marks_retired() {
        let adapter = annotate_only();
        let stale_id = seed_fake(&adapter);
        let (_dir, mut store) = temp_store();
        let proposal = proposal(ProposalAction::Retire);

        let report = apply_decision(
            &mut store,
            &proposal,
            &DecisionVerdict::Approve,
            "reviewer",
            "2026-10-29T12:00:00Z",
            &[Target {
                adapter: &adapter,
                memory_id: stale_id.clone(),
            }],
        )
        .unwrap();

        assert_eq!(report.results[0].outcome, AdapterOutcome::Annotated);
        assert!(
            adapter
                .read(&stale_id)
                .unwrap()
                .text
                .contains(RETIRED_MARKER)
        );
    }

    #[test]
    fn write_fallback_reports_old_stays() {
        let adapter = write_only();
        let stale_id = seed_fake(&adapter);
        let stale_before = adapter.read(&stale_id).unwrap();
        let (_dir, mut store) = temp_store();
        let proposal = proposal(ProposalAction::Correct {
            new_text: CORRECTION.to_string(),
        });

        let report = apply_decision(
            &mut store,
            &proposal,
            &DecisionVerdict::Approve,
            "reviewer",
            "2026-10-29T12:00:00Z",
            &[Target {
                adapter: &adapter,
                memory_id: stale_id.clone(),
            }],
        )
        .unwrap();

        let new_memory = match &report.results[0].outcome {
            AdapterOutcome::WrittenNew {
                new_memory,
                stale_remains,
            } => {
                assert!(stale_remains, "the old memory stays");
                new_memory.clone()
            }
            other => panic!("expected written-new, got {other:?}"),
        };
        // The old memory is byte-identical: nothing points at the new note.
        assert_eq!(adapter.read(&stale_id).unwrap(), stale_before);
        let stored = adapter.read(&new_memory.id).unwrap();
        assert_eq!(stored.text, CORRECTION);
        assert_eq!(stored.scope, "test-scope");
    }

    #[test]
    fn missing_memory_is_terminal_failure() {
        let fake = FakeAdapter::new("fake");
        let (_dir, mut store) = temp_store();
        let proposal = proposal(ProposalAction::Correct {
            new_text: CORRECTION.to_string(),
        });

        let report = apply_decision(
            &mut store,
            &proposal,
            &DecisionVerdict::Approve,
            "reviewer",
            "2026-10-29T12:00:00Z",
            &[Target {
                adapter: &fake,
                memory_id: "mem-9".to_string(),
            }],
        )
        .unwrap();

        match &report.results[0].outcome {
            AdapterOutcome::Failed { message, retryable } => {
                assert!(message.contains("mem-9"), "names the memory: {message}");
                assert!(!retryable, "not-found never converges by retry");
            }
            other => panic!("expected failed, got {other:?}"),
        }
        assert!(
            !report.needs_retry(),
            "terminal failures are described, not marked for retry"
        );
        // Recorded even so: never half-silent.
        assert!(store.decision_for_proposal("proposal-1").is_some());
    }

    // ---- the real Muse Code adapter on a temp fixture copy ----

    fn copy_dir(src: &Path, dst: &Path) {
        fs::create_dir_all(dst).unwrap();
        for entry in fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let ft = entry.file_type().unwrap();
            let to = dst.join(entry.file_name());
            if ft.is_dir() {
                copy_dir(&entry.path(), &to);
            } else {
                fs::copy(entry.path(), to).unwrap();
            }
        }
    }

    fn temp_muse_fixture() -> (PathBuf, MuseCodeAdapter) {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let src = base.join("testdata/memories/muse-code");
        let dst = temp_dir("muse");
        copy_dir(&src, &dst);
        // Explicit scopes: no HOME lookup, no environment mutation.
        let adapter =
            MuseCodeAdapter::from_scopes(vec![(dst.clone(), "personal_project".to_string())]);
        (dst, adapter)
    }

    #[test]
    fn muse_code_demo_approve_supersedes_m1() {
        let (_dir, adapter) = temp_muse_fixture();
        let m1 = adapter
            .list("")
            .unwrap()
            .into_iter()
            .find(|m| m.text == STALE)
            .expect("M1 must be in the fixture")
            .reference
            .id;
        let (_stored, mut store) = temp_store();
        let proposal = proposal(ProposalAction::Correct {
            new_text: CORRECTION.to_string(),
        });

        let report = apply_decision(
            &mut store,
            &proposal,
            &DecisionVerdict::Approve,
            "reviewer",
            "2026-10-29T12:00:00Z",
            &[Target {
                adapter: &adapter,
                memory_id: m1.clone(),
            }],
        )
        .unwrap();

        let correction = match &report.results[0].outcome {
            AdapterOutcome::Superseded { correction } => correction.clone(),
            other => panic!("expected superseded, got {other:?}"),
        };
        // The fixture note names its correction (the stale-marking), and the
        // correction reads back with the approved text.
        let stale = adapter.read(&m1).unwrap();
        assert!(
            stale
                .text
                .contains(&format!("[Superseded by {}]", correction.id)),
            "stale note names its correction, got: {}",
            stale.text
        );
        let stored = adapter.read(&correction.id).unwrap();
        assert!(
            stored.text.contains(CORRECTION),
            "correction reads back with its text, got: {}",
            stored.text
        );
    }

    // ---- scripted Lambo server (shaped by testdata/lambo transcripts) ----

    #[derive(Default)]
    struct Srv {
        concepts: Vec<(String, String)>,
        links: Vec<(String, String)>,
        warnings: Vec<String>,
        derives: usize,
        receipts: HashMap<String, Vec<String>>,
        next: u32,
    }

    fn sse(message: serde_json::Value) -> crate::mcp::HttpResponse {
        crate::mcp::HttpResponse {
            status: 200,
            headers: vec![("Mcp-Session-Id".to_string(), "<mcp-session-id>".to_string())],
            body: format!("id: 0\nretry: 3000\ndata: {message}\n\n"),
        }
    }

    fn scripted_lambo(seed: Vec<(String, String)>) -> (crate::mcp::FakeTransport, Arc<Mutex<Srv>>) {
        let state = Arc::new(Mutex::new(Srv {
            concepts: seed,
            ..Srv::default()
        }));
        let st = state.clone();
        let transport = crate::mcp::FakeTransport::scripted(move |url: &str, body: &str| {
            let v: serde_json::Value =
                serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
            let method = v.get("method").and_then(|m| m.as_str()).unwrap_or("");
            if method == "initialize" {
                let msg = serde_json::json!({
                    "jsonrpc": "2.0", "id": v.get("id"),
                    "result": {
                        "protocolVersion": "2025-06-18",
                        "capabilities": {"tools": {}},
                        "serverInfo": {"name": "lambo", "version": "0.8.0"},
                        "instructions": "x"
                    }
                });
                return Ok(sse(msg));
            }
            if method == "notifications/initialized" {
                return Ok(crate::mcp::HttpResponse {
                    status: 202,
                    headers: vec![],
                    body: String::new(),
                });
            }
            if method != "tools/call" {
                return Err(crate::mcp::TransportError {
                    url: url.to_string(),
                    message: "unknown method".to_string(),
                });
            }
            let params = v.get("params").cloned().unwrap_or(serde_json::Value::Null);
            let tool = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let rid = v.get("id").cloned().unwrap_or(serde_json::Value::Null);
            let mut st = st.lock().unwrap_or_else(|e| e.into_inner());
            match tool {
                "lambo_derive" => {
                    let concepts = args
                        .get("concepts")
                        .and_then(|c| c.as_array())
                        .cloned()
                        .unwrap_or_default();
                    let mut created = Vec::new();
                    for c in &concepts {
                        st.next += 1;
                        let cid = format!("{:08x}-decide-0000-000000000000", st.next);
                        let content = c
                            .get("content")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string();
                        let against = c
                            .get("against")
                            .and_then(|a| a.as_array())
                            .cloned()
                            .unwrap_or_default();
                        for a in &against {
                            let stale_short = a.as_str().unwrap_or("").to_string();
                            let corr_short: String = cid.chars().take(8).collect();
                            st.links.push((stale_short.clone(), cid.clone()));
                            st.warnings.push(format!(
                                "\u{26a0} Contradicted at 2026-10-29T15:07:26Z by doyle-probe: \"{content}\" (id {corr_short})"
                            ));
                            let stale_content = st
                                .concepts
                                .iter()
                                .find(|(id, _)| id.starts_with(&stale_short))
                                .map(|(_, t)| t.clone())
                                .unwrap_or_default();
                            st.warnings.push(format!(
                                "\u{26a0} Contradicts \"{stale_content}\" (id {stale_short})"
                            ));
                        }
                        st.concepts.push((cid.clone(), content));
                        created.push(cid);
                    }
                    st.derives += 1;
                    let receipt = format!("receipt-{}", st.derives);
                    st.receipts.insert(receipt.clone(), created.clone());
                    let msg = serde_json::json!({
                        "jsonrpc": "2.0", "id": rid,
                        "result": {
                            "content": [{"type": "text", "text": "accepted"}],
                            "structuredContent": {
                                "agent_id": args.get("agent_id"),
                                "receipt": receipt,
                                "receipt_state": "pending",
                                "warnings": []
                            },
                            "isError": false
                        }
                    });
                    Ok(sse(msg))
                }
                "lambo_stats" => {
                    let receipt = args.get("receipt").and_then(|x| x.as_str()).unwrap_or("");
                    let created = st.receipts.get(receipt).cloned().unwrap_or_default();
                    let msg = serde_json::json!({
                        "jsonrpc": "2.0", "id": rid,
                        "result": {
                            "content": [{"type": "text", "text": "stats"}],
                            "structuredContent": {
                                "receipt": {"created": created, "state": "applied"}
                            },
                            "isError": false
                        }
                    });
                    Ok(sse(msg))
                }
                "lambo_recall" => {
                    let hits: Vec<serde_json::Value> = st
                        .concepts
                        .iter()
                        .map(|(cid, content)| {
                            serde_json::json!({
                                "node_id": cid, "content": content,
                                "concept_type": "Logic", "score": 1.0,
                                "is_canonical": false, "blast_radius": null
                            })
                        })
                        .collect();
                    let warnings = st.warnings.clone();
                    let msg = serde_json::json!({
                        "jsonrpc": "2.0", "id": rid,
                        "result": {
                            "content": [{"type": "text", "text": "ctx"}],
                            "structuredContent": {
                                "context": "ctx", "hits": hits, "warnings": warnings
                            },
                            "isError": false
                        }
                    });
                    Ok(sse(msg))
                }
                "lambo_inspect" => {
                    let focus = args.get("focus").and_then(|x| x.as_str()).unwrap_or("");
                    let found = st
                        .concepts
                        .iter()
                        .find(|(cid, _)| cid == focus || cid.starts_with(focus));
                    let (label, node_id) = match found {
                        Some((cid, content)) => (format!("{content} [Logic]"), cid.clone()),
                        None => (String::new(), String::new()),
                    };
                    let focus_short: String = focus.chars().take(8).collect();
                    let warnings: Vec<String> = st
                        .links
                        .iter()
                        .filter(|(stale_short, _)| *stale_short == focus_short)
                        .map(|(_, corr_full)| {
                            let corr_short: String = corr_full.chars().take(8).collect();
                            format!(
                                "\u{26a0} Contradicted at 2026-10-29T15:07:26Z by doyle-probe: \"...\" (id {corr_short})"
                            )
                        })
                        .collect();
                    let msg = serde_json::json!({
                        "jsonrpc": "2.0", "id": rid,
                        "result": {
                            "content": [{"type": "text", "text": format!("focus: {label}")}],
                            "structuredContent": {
                                "focus": {"label": label, "node_id": node_id},
                                "warnings": warnings
                            },
                            "isError": false
                        }
                    });
                    Ok(sse(msg))
                }
                _ => Err(crate::mcp::TransportError {
                    url: url.to_string(),
                    message: format!("unknown tool {tool}"),
                }),
            }
        });
        (transport, state)
    }

    fn lambo_adapter(transport: crate::mcp::FakeTransport) -> crate::adapters::lambo::LamboAdapter {
        crate::adapters::lambo::LamboAdapter::new(
            "http://127.0.0.1:7700/mcp".to_string(),
            "doyle-probe".to_string(),
            Box::new(transport),
        )
    }

    /// M1 as the scripted server holds it: the demo's stale text under a
    /// full node id whose short form is the fixture's `5e1d0a73`.
    fn m1_seed() -> Vec<(String, String)> {
        vec![(
            "5e1d0a73-0000-0000-0000-000000000000".to_string(),
            STALE.to_string(),
        )]
    }

    #[test]
    fn lambo_demo_approve_links_against_m1() {
        let (transport, _state) = scripted_lambo(m1_seed());
        let adapter = lambo_adapter(transport);
        let stale_id = adapter
            .list("GET /patients returns every patient")
            .unwrap()
            .into_iter()
            .find(|m| m.text == STALE)
            .expect("M1 must recall")
            .reference
            .id;
        let (_dir, mut store) = temp_store();
        let proposal = proposal(ProposalAction::Correct {
            new_text: CORRECTION.to_string(),
        });

        let report = apply_decision(
            &mut store,
            &proposal,
            &DecisionVerdict::Approve,
            "reviewer",
            "2026-10-29T12:00:00Z",
            &[Target {
                adapter: &adapter,
                memory_id: stale_id.clone(),
            }],
        )
        .unwrap();

        let correction = match &report.results[0].outcome {
            AdapterOutcome::Superseded { correction } => correction.clone(),
            other => panic!("expected superseded, got {other:?}"),
        };
        // The stale memory warns with its correction's id ...
        let stale = adapter.read(&stale_id).unwrap();
        let corr_short: String = correction.id.chars().take(8).collect();
        assert!(
            stale.text.contains("\u{26a0} Contradicted") && stale.text.contains(&corr_short),
            "stale memory warns with its correction, got: {}",
            stale.text
        );
        // ... the correction reads back with the approved text ...
        let stored = adapter.read(&correction.id).unwrap();
        assert!(
            stored.text.contains(CORRECTION),
            "correction reads back with its text, got: {}",
            stored.text
        );
        // ... and the transcript shows the derive was against-linked to M1.
        let derive = adapter
            .recorded_requests()
            .into_iter()
            .find(|c| c.body.contains("lambo_derive") && c.body.contains("against"))
            .expect("one against-linked derive");
        assert!(
            derive.body.contains("5e1d0a73"),
            "derive names M1's short id, got: {}",
            derive.body
        );
        assert!(
            derive.body.contains(CORRECTION),
            "derive carries the correction, got: {}",
            derive.body
        );
    }

    // ---- partial failure, then retry convergence ----

    fn failing_transport() -> crate::mcp::FakeTransport {
        crate::mcp::FakeTransport::scripted(|url: &str, _body: &str| {
            Err(crate::mcp::TransportError {
                url: url.to_string(),
                message: "connection refused (test)".to_string(),
            })
        })
    }

    #[test]
    fn partial_failure_records_decision_and_retry_converges() {
        // Lambo down, Muse Code fine.
        let (_muse_dir, muse) = temp_muse_fixture();
        let m1_muse = muse
            .list("")
            .unwrap()
            .into_iter()
            .find(|m| m.text == STALE)
            .expect("M1 must be in the fixture")
            .reference
            .id;
        let down = lambo_adapter(failing_transport());
        let (_dir, mut store) = temp_store();
        let proposal = proposal(ProposalAction::Correct {
            new_text: CORRECTION.to_string(),
        });
        let verdict = DecisionVerdict::Approve;

        let report = apply_decision(
            &mut store,
            &proposal,
            &verdict,
            "reviewer",
            "2026-10-29T12:00:00Z",
            &[
                Target {
                    adapter: &muse,
                    memory_id: m1_muse.clone(),
                },
                Target {
                    adapter: &down,
                    memory_id: "5e1d0a73-0000-0000-0000-000000000000".to_string(),
                },
            ],
        )
        .unwrap();

        // The decision is recorded; the report describes both adapters.
        assert_eq!(report.results.len(), 2);
        let muse_correction = match &report.results[0].outcome {
            AdapterOutcome::Superseded { correction } => correction.clone(),
            other => panic!("muse must succeed, got {other:?}"),
        };
        match &report.results[1].outcome {
            AdapterOutcome::Failed { message, retryable } => {
                assert!(
                    message.contains("127.0.0.1:7700"),
                    "failure names the system: {message}"
                );
                assert!(retryable, "a down server is marked for retry");
            }
            other => panic!("lambo must fail, got {other:?}"),
        }
        assert!(report.needs_retry());
        assert!(store.decision_for_proposal("proposal-1").is_some());

        // Retry with a healed transport, seeded with the same M1.
        let (healed_transport, state) = scripted_lambo(m1_seed());
        let healed = lambo_adapter(healed_transport);
        let lambo_target = Target {
            adapter: &healed,
            memory_id: "5e1d0a73-0000-0000-0000-000000000000".to_string(),
        };
        let retried = retry_failed(&proposal, &verdict, &[lambo_target]);
        let lambo_correction = match &retried[0].outcome {
            AdapterOutcome::Superseded { correction } => correction.clone(),
            other => panic!("retry must succeed, got {other:?}"),
        };
        // Exactly one correction exists on the healed server.
        let corrections = state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .concepts
            .iter()
            .filter(|(_, text)| text == CORRECTION)
            .count();
        assert_eq!(corrections, 1);

        // A second retry on the same adapter is a cache no-op ...
        let again = retry_failed(
            &proposal,
            &verdict,
            &[Target {
                adapter: &healed,
                memory_id: "5e1d0a73-0000-0000-0000-000000000000".to_string(),
            }],
        );
        match &again[0].outcome {
            AdapterOutcome::Superseded { correction } => assert_eq!(*correction, lambo_correction),
            other => panic!("second retry must succeed, got {other:?}"),
        };
        // ... and a fresh adapter (empty caches, as after a restart) finds
        // the existing against-link by inspect and derives nothing.
        let (fresh_transport, fresh_state) = scripted_lambo(vec![
            (
                "5e1d0a73-0000-0000-0000-000000000000".to_string(),
                STALE.to_string(),
            ),
            (lambo_correction.id.clone(), CORRECTION.to_string()),
        ]);
        // Replay the link the first derive recorded.
        fresh_state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .links
            .push(("5e1d0a73".to_string(), lambo_correction.id.clone()));
        let fresh = lambo_adapter(fresh_transport);
        let derives_before = fresh_state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .derives;
        let resumed = retry_failed(
            &proposal,
            &verdict,
            &[Target {
                adapter: &fresh,
                memory_id: "5e1d0a73-0000-0000-0000-000000000000".to_string(),
            }],
        );
        match &resumed[0].outcome {
            AdapterOutcome::Superseded { correction } => assert_eq!(*correction, lambo_correction),
            other => panic!("resumed retry must succeed, got {other:?}"),
        };
        assert_eq!(
            fresh_state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .derives,
            derives_before,
            "inspect-first retry must not derive"
        );

        // Muse Code converges too: re-applying returns the same correction
        // and leaves exactly one correction bullet.
        let muse_again = retry_failed(
            &proposal,
            &verdict,
            &[Target {
                adapter: &muse,
                memory_id: m1_muse.clone(),
            }],
        );
        match &muse_again[0].outcome {
            AdapterOutcome::Superseded { correction } => assert_eq!(*correction, muse_correction),
            other => panic!("muse re-apply must converge, got {other:?}"),
        };
        let count = muse
            .list("")
            .unwrap()
            .into_iter()
            .filter(|m| m.text == CORRECTION)
            .count();
        assert_eq!(count, 1, "exactly one correction in Muse Code");
    }

    #[test]
    fn error_message_shapes() {
        assert_eq!(correction_warning("t"), "[STALE correction: t]");
        assert_eq!(RETIRED_MARKER, "[RETIRED: no longer applies]");
        let outcome = AdapterOutcome::Failed {
            message: "x".to_string(),
            retryable: true,
        };
        assert!(outcome.needs_retry());
        let done = AdapterOutcome::Kept;
        assert!(!done.needs_retry());
    }
}
