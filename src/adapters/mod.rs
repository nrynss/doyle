//! Doyle's memory adapters: the plug-in seam between the core and the memory systems.
//!
//! The core calls only [`MemoryAdapter`]. Adding a memory system means writing
//! one new adapter, with no change to the core. See architecture.md,
//! "The plug-in seams".

mod fake;

pub mod lambo;
pub mod muse_code;

pub use fake::FakeAdapter;

use crate::core::{Memory, MemoryRef};

/// Which optional operations an adapter implements.
///
/// The required operations ([`MemoryAdapter::list`], `read`, `write`) are
/// always available. Each optional operation ([`MemoryAdapter::annotate`],
/// `supersede`, `watch`) is declared here; calling one the adapter lacks
/// returns [`AdapterError::Unsupported`], never a panic, so the core can
/// decide up front whether to call it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// In-place warning, so it shows on recall.
    pub annotate: bool,
    /// Mark a memory replaced by a correction.
    pub supersede: bool,
    /// Notice new memories.
    pub watch: bool,
}

impl Capabilities {
    /// No optional operation.
    pub const NONE: Self = Self {
        annotate: false,
        supersede: false,
        watch: false,
    };

    /// Every optional operation. What [`FakeAdapter`] reports.
    pub const ALL: Self = Self {
        annotate: true,
        supersede: true,
        watch: true,
    };

    /// Whether the named optional operation (`"annotate"`, `"supersede"` or
    /// `"watch"`) is available. Anything else is `false`.
    pub fn supports(self, operation: &str) -> bool {
        match operation {
            "annotate" => self.annotate,
            "supersede" => self.supersede,
            "watch" => self.watch,
            _ => false,
        }
    }
}

/// What can go wrong in an adapter. Every variant names the adapter, so a
/// failure in a multi-adapter run traces to its system; the variants that
/// concern one memory name it too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterError {
    /// No memory with that ID in this adapter's system.
    NotFound {
        /// The adapter's declared name ([`MemoryAdapter::name`]).
        adapter: String,
        /// The requested memory ID.
        memory: String,
    },
    /// The adapter does not implement this optional operation.
    Unsupported {
        /// The adapter's declared name.
        adapter: String,
        /// `"annotate"`, `"supersede"` or `"watch"`.
        operation: String,
    },
    /// The underlying system failed (file unreadable, server error, ...).
    Backend {
        /// The adapter's declared name.
        adapter: String,
        /// The memory involved, when the failure concerns one.
        memory: Option<String>,
        /// What happened, in the system's own terms.
        message: String,
    },
}

impl AdapterError {
    /// Build the [`AdapterError::Unsupported`] error for this adapter and
    /// operation. Used by the trait's default method bodies.
    pub fn unsupported(adapter: &str, operation: &str) -> Self {
        Self::Unsupported {
            adapter: adapter.to_string(),
            operation: operation.to_string(),
        }
    }

    /// Build the [`AdapterError::NotFound`] error for this adapter and memory.
    pub fn not_found(adapter: &str, memory: &str) -> Self {
        Self::NotFound {
            adapter: adapter.to_string(),
            memory: memory.to_string(),
        }
    }
}

impl std::fmt::Display for AdapterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound { adapter, memory } => {
                write!(f, "{adapter}: memory not found: {memory}")
            }
            Self::Unsupported { adapter, operation } => {
                write!(f, "{adapter}: unsupported operation: {operation}")
            }
            Self::Backend {
                adapter,
                memory,
                message,
            } => match memory {
                Some(memory) => {
                    write!(f, "{adapter}: backend error for memory {memory}: {message}")
                }
                None => write!(f, "{adapter}: backend error: {message}"),
            },
        }
    }
}

impl std::error::Error for AdapterError {}

/// The contract every memory system implements. The core knows only this
/// trait, never a particular system.
///
/// Adapters are synchronous and blocking: the binary does nothing that needs
/// async. All methods take `&self`; adapters that mutate (caches, poll
/// cursors) hold their state in interior mutability.
pub trait MemoryAdapter {
    /// The name this adapter's memories carry in [`MemoryRef::adapter`].
    /// A plain string, never an enum of known systems, so a new adapter
    /// needs no change to the core.
    fn name(&self) -> &str;

    /// Which optional operations this adapter implements.
    fn capabilities(&self) -> Capabilities;

    /// Convenience over [`Capabilities::supports`]: whether `operation`
    /// (`"annotate"`, `"supersede"` or `"watch"`) may be called.
    fn supports(&self, operation: &str) -> bool {
        self.capabilities().supports(operation)
    }

    /// Search the memories the adapter can see: id, text and last updated.
    ///
    /// Search, not enumerate: `query` is free text and the adapter returns
    /// the memories matching it, in its own order (insertion for the fake,
    /// folder order for Muse Code, recall score for Lambo).
    ///
    /// Why a query instead of enumerate-everything: Lambo has no list-all
    /// primitive — `lambo_recall` needs a query, `lambo_saints` returns
    /// only canonicals, and `lambo_stale` only dependents of contradicted
    /// concepts — so an enumerate method would be unsatisfiable by Lambo.
    /// One search method keeps the core's curate loop uniform: enumerable
    /// adapters answer it as a substring filter, recall-backed ones as
    /// recall.
    ///
    /// * An empty query means no filter: enumerable adapters (Muse Code,
    ///   the fake) return everything they see. A recall-backed adapter has
    ///   no match-all query to send, so it returns [`AdapterError::Backend`]
    ///   on an empty query; the core always passes a non-empty query when
    ///   it needs Lambo results (curate queries per contract element).
    /// * Muse Code (T3.1) scans the memory folder for the project Doyle is
    ///   told to curate — `MEMORY.md` plus one file per topic — and returns
    ///   each note whose text contains the query (case-insensitive), in
    ///   folder order. Notes carry `type`/`description` frontmatter and no
    ///   timestamp (`updated_at` is `None`); see docs/muse-code.md.
    /// * Lambo (T3.3) recalls the query verbatim (`lambo_recall`, `top_k`
    ///   50) in its session and returns one memory per hit, keyed by the
    ///   hit's node ID; see docs/lambo.md on `lambo_recall`.
    ///
    /// Incompleteness contract: `list` is a search, not an audit. Recall
    /// returns the top hits and can miss low-scoring matches, and an
    /// over-full session is cut at `top_k`; enumerable adapters are
    /// complete for substring matches. The core treats "absent from
    /// `list(query)`" as "not found by this query", never as "does not
    /// exist". The shared conformance asserts inclusion (the matching
    /// writes are present), never exact counts, so recall extras pass.
    fn list(&self, query: &str) -> Result<Vec<Memory>, AdapterError>;

    /// Read one memory by its ID.
    ///
    /// * Muse Code reads the note file; the returned text includes any
    ///   `[STALE ...]` annotation Doyle wrote in place (which is what makes
    ///   a fresh session answer with the corrected value) and the
    ///   `superseded-by` line naming the correction, when one is present.
    ///   The correction itself reads back under the reference
    ///   [`MemoryAdapter::supersede`] returned.
    /// * Lambo inspects the concept (`lambo_inspect`) for its content and
    ///   resolves the ID; the `Contradicted ...` warning line is not on the
    ///   inspect result (transcript 10 carries none) but arrives via recall
    ///   warnings, combined with the local supersede cache populated by
    ///   `supersede` (and by recall). The correction reads back under its
    ///   derived node ID.
    ///
    /// Unknown IDs are [`AdapterError::NotFound`], never a panic.
    fn read(&self, id: &str) -> Result<Memory, AdapterError>;

    /// Write one fresh memory and return its reference. The adapter assigns
    /// the ID in its own system's terms. Corrections are never written here:
    /// they are created by [`MemoryAdapter::supersede`], which links them to
    /// the stale memory in the same call — Lambo can only record the
    /// contradiction inside the deriving call (`against`), so a
    /// write-then-link flow would derive twice (see `supersede`).
    ///
    /// * Muse Code appends the text as a new note (a bullet in `MEMORY.md`
    ///   or a new topic file) in the project's memory folder.
    /// * Lambo derives a concept (`lambo_derive`), without `against`, waits
    ///   on the receipt with `lambo_stats` plus `wait_ms` so the next read
    ///   sees it, and returns the created node ID.
    fn write(&self, text: &str, scope: &str) -> Result<MemoryRef, AdapterError>;

    /// Put a warning on the memory in place, so it shows on recall. Optional:
    /// the default body returns [`AdapterError::Unsupported`].
    ///
    /// * Muse Code appends a short, plain `[STALE ...]` line to the note.
    ///   The warning quotes nothing from a contract that could read as an
    ///   instruction (product.md, Cautions).
    /// * Lambo has no in-place edit; its adapter leaves this unsupported and
    ///   corrects through [`MemoryAdapter::supersede`] instead (a derive with
    ///   `against`), which is what makes recall warn on both lines. Leaving
    ///   `annotate` unsupported while `supersede` is the correcting path is
    ///   the compliant reading of this trait (frozen trait governs over loose
    ///   phase prose).
    fn annotate(&self, id: &str, warning: &str) -> Result<(), AdapterError> {
        let _ = (id, warning);
        Err(AdapterError::unsupported(self.name(), "annotate"))
    }

    /// Correct a memory: create the correction AND link it to the stale
    /// memory, in this one call, and return the correction's reference.
    /// Optional: the default body returns [`AdapterError::Unsupported`].
    ///
    /// The write/supersede division, so no correction is ever noted or
    /// derived twice (`write` creates fresh memories only, never with a
    /// link; `supersede` is the sole creator of corrections, exactly one
    /// new memory per call):
    ///
    /// * Muse Code points the stale note at the correction (a
    ///   `superseded-by` line naming the correction note) and creates the
    ///   correction as a new note in the same call. Retrying with identical
    ///   text is a no-op returning the existing correction's reference (the
    ///   stale note already points at a note with that text), so a lost
    ///   acknowledgement cannot duplicate the note.
    /// * Lambo derives the correction concept WITH `against` naming the
    ///   stale concept's ID, in the same session, and waits on the receipt
    ///   (`lambo_stats` plus `wait_ms`) so the next recall warns on both
    ///   lines — one derive, one concept. Lambo has no post-hoc link
    ///   primitive (`against` is set at derive time only; deriving the same
    ///   text a second time would double-write), so `supersede` MUST NOT
    ///   derive when the link already exists: it inspects first, and when
    ///   the correction already contradicts the stale memory (a retry whose
    ///   first derive applied, or a repeated decision) it derives nothing
    ///   and returns the existing correction's reference. P3 verifies the
    ///   retry path against the running server (T3.3b) rather than assuming
    ///   it.
    ///
    /// The fake models the Muse rule (pointer plus identical-text no-op);
    /// the shared conformance pins it: the correction reads back with its
    /// text, and the stale memory reads back naming the correction's ID —
    /// a no-op `supersede` fails both.
    fn supersede(&self, stale_id: &str, correction_text: &str) -> Result<MemoryRef, AdapterError> {
        let _ = (stale_id, correction_text);
        Err(AdapterError::unsupported(self.name(), "supersede"))
    }

    /// Notice new memories: the memories first seen since the previous
    /// `watch` call. Each is reported once; a second call with no new
    /// memories returns an empty list. Optional: the default body returns
    /// [`AdapterError::Unsupported`].
    ///
    /// First call and baselining: the cursor starts empty when the adapter
    /// is built, so the FIRST `watch` call reports every memory visible at
    /// that time — the adapter never silently baselines. The CALLER
    /// baselines when it wants only-new afterwards: it calls `watch` once
    /// and discards the result. The shared conformance instead writes first
    /// and asserts the first call reports those writes, pinning that the
    /// adapter never auto-baselines.
    ///
    /// Cursor durability: real adapters persist the reported set across
    /// restarts (or a restart re-reports everything); the fake deliberately
    /// does not (its seen-set is process-local `RefCell` state, for tests
    /// only).
    ///
    /// * Muse Code rescans the memory folder for notes it has not reported
    ///   yet (matched by file path) and persists the reported set in
    ///   Doyle's state dir.
    /// * Lambo recalls a broad query (`top_k` 50) for concepts it has not
    ///   reported yet (matched by node ID), keeping the reported IDs in an
    ///   in-memory `HashSet` (process-local; a restart re-reports
    ///   everything). `lambo_stale` is NOT the watch source: it lists direct
    ///   dependents of contradicted concepts (docs/lambo.md), not new
    ///   memories.
    ///   The broad query is `BROAD_WATCH_QUERY` (`"*"`); its match-all
    ///   behavior is unrecorded (no transcript shows a server response to
    ///   `*`) and live verification is deferred to T3.3b.
    ///   Restart-loss disclosure: the supersede and scope caches are
    ///   process-local, so after a restart reads lose recorded warnings and
    ///   scopes until the next in-process supersede.
    fn watch(&self) -> Result<Vec<Memory>, AdapterError> {
        Err(AdapterError::unsupported(self.name(), "watch"))
    }
}

/// The shared conformance exercise every adapter must pass: the fake here,
/// the real adapters in P3 (run against a scratch session or an empty memory
/// folder, so the first `watch` reports exactly the test writes).
///
/// Each optional operation is exercised when the adapter's [`Capabilities`]
/// claim it; when they do not, the exercise asserts the typed
/// [`AdapterError::Unsupported`] instead, so one test fits adapters with
/// different capabilities. `list` asserts inclusion (the matching writes are
/// present), never exact counts, so recall extras pass. Panics on the first
/// mismatch, with the adapter and memory named in the failure.
pub fn check_conformance(adapter: &dyn MemoryAdapter) {
    let name = adapter.name().to_string();
    assert!(!name.is_empty(), "adapter name must not be empty");

    // Write two fresh memories; the adapter assigns IDs in its own terms and
    // the references carry this adapter's name. Corrections are NOT written
    // here: they are created by `supersede` (see below).
    let first = adapter
        .write(
            "GET /patients returns every patient in one response",
            "test-scope",
        )
        .expect("write must succeed");
    let second = adapter
        .write(
            "GET /patients returns one page of 100 plus next_cursor",
            "test-scope",
        )
        .expect("write must succeed");
    assert_eq!(first.adapter, name);
    assert_eq!(second.adapter, name);
    assert_ne!(first.id, second.id, "writes must mint distinct IDs");

    // List is a search: the shared-vocabulary query finds both writes, and
    // the distinctive query finds the page-size write. Inclusion only —
    // recall-backed adapters may return extras.
    let listed = adapter.list("GET /patients").expect("list must succeed");
    let ids: Vec<&str> = listed.iter().map(|m| m.reference.id.as_str()).collect();
    assert!(
        ids.contains(&first.id.as_str()),
        "list shows the first write"
    );
    assert!(
        ids.contains(&second.id.as_str()),
        "list shows the second write"
    );
    let paged = adapter.list("next_cursor").expect("list must succeed");
    let paged_ids: Vec<&str> = paged.iter().map(|m| m.reference.id.as_str()).collect();
    assert!(
        paged_ids.contains(&second.id.as_str()),
        "list finds the page-size write by its distinctive term"
    );

    // Read round-trips text and scope.
    let read = adapter.read(&first.id).expect("read must succeed");
    assert_eq!(read.reference, first);
    assert_eq!(
        read.text,
        "GET /patients returns every patient in one response"
    );
    assert_eq!(read.scope, "test-scope");

    // Unknown IDs are NotFound, naming the adapter and the memory.
    let missing = adapter.read("no-such-memory");
    match missing {
        Err(AdapterError::NotFound { adapter, memory }) => {
            assert_eq!(adapter, name);
            assert_eq!(memory, "no-such-memory");
        }
        other => panic!("read of an unknown ID must be NotFound, got {other:?}"),
    }

    // Annotate: supported means the warning shows on read; otherwise the
    // typed unsupported error, naming the adapter and the operation.
    if adapter.supports("annotate") {
        adapter
            .annotate(&first.id, "[STALE test warning]")
            .expect("annotate must succeed");
        let annotated = adapter.read(&first.id).expect("read must succeed");
        assert!(
            annotated.text.contains("[STALE test warning]"),
            "annotation shows on read"
        );
    } else {
        match adapter.annotate(&first.id, "[STALE test warning]") {
            Err(AdapterError::Unsupported { adapter, operation }) => {
                assert_eq!(adapter, name);
                assert_eq!(operation, "annotate");
            }
            other => panic!("annotate without the capability must be Unsupported, got {other:?}"),
        }
    }

    // Supersede CREATES the correction and links it: one new memory per
    // call, no pre-written correction. The stale memory still reads, now
    // naming the correction's ID; the correction reads back with its text.
    // A no-op supersede fails the naming assertion. Otherwise the typed
    // unsupported error, naming the adapter and the operation.
    let mut correction_ids: Vec<String> = Vec::new();
    if adapter.supports("supersede") {
        let correction_text =
            "GET /patients returns one page of 100 plus next_cursor; follow cursor to the end";
        let correction = adapter
            .supersede(&first.id, correction_text)
            .expect("supersede must succeed");
        assert_eq!(correction.adapter, name);
        assert_ne!(correction.id, first.id, "supersede must mint a new ID");
        assert_ne!(correction.id, second.id, "supersede must mint a new ID");
        let superseded = adapter.read(&first.id).expect("read must succeed");
        assert_eq!(superseded.reference, first);
        assert!(
            superseded.text.contains(&correction.id),
            "stale memory names its correction, got: {}",
            superseded.text
        );
        let stored = adapter
            .read(&correction.id)
            .expect("correction must read back");
        assert_eq!(stored.reference, correction);
        assert!(
            stored.text.contains(correction_text),
            "correction reads back with its text, got: {}",
            stored.text
        );
        correction_ids.push(correction.id.clone());
    } else {
        match adapter.supersede(&first.id, "GET /patients pages") {
            Err(AdapterError::Unsupported { adapter, operation }) => {
                assert_eq!(adapter, name);
                assert_eq!(operation, "supersede");
            }
            other => {
                panic!("supersede without the capability must be Unsupported, got {other:?}")
            }
        }
    }

    // Watch: the cursor starts empty, so the FIRST call reports every
    // memory visible — the two writes plus any correction — and each is
    // reported once; a later write is reported once, then silence again.
    // (A first call that comes back empty means the adapter silently
    // baselined, and fails here.)
    if adapter.supports("watch") {
        let noticed = adapter.watch().expect("watch must succeed");
        let noticed_ids: Vec<&str> = noticed.iter().map(|m| m.reference.id.as_str()).collect();
        assert!(
            noticed_ids.contains(&first.id.as_str()),
            "first watch reports the first write"
        );
        assert!(
            noticed_ids.contains(&second.id.as_str()),
            "first watch reports the second write"
        );
        for correction_id in &correction_ids {
            assert!(
                noticed_ids.contains(&correction_id.as_str()),
                "first watch reports the correction"
            );
        }
        let again = adapter.watch().expect("watch must succeed");
        assert!(
            again.is_empty(),
            "watch reports each memory once, got {}",
            again.len()
        );
        let third = adapter
            .write("GET /patients filter needs an index", "test-scope")
            .expect("write must succeed");
        let later = adapter.watch().expect("watch must succeed");
        let later_ids: Vec<&str> = later.iter().map(|m| m.reference.id.as_str()).collect();
        assert!(
            later_ids.contains(&third.id.as_str()),
            "watch reports the later write"
        );
        assert!(
            !later_ids.contains(&first.id.as_str()) && !later_ids.contains(&second.id.as_str()),
            "watch never re-reports"
        );
        for correction_id in &correction_ids {
            assert!(
                !later_ids.contains(&correction_id.as_str()),
                "watch never re-reports"
            );
        }
        let silent = adapter.watch().expect("watch must succeed");
        assert!(
            silent.is_empty(),
            "watch falls silent again, got {}",
            silent.len()
        );
    } else {
        match adapter.watch() {
            Err(AdapterError::Unsupported { adapter, operation }) => {
                assert_eq!(adapter, name);
                assert_eq!(operation, "watch");
            }
            other => panic!("watch without the capability must be Unsupported, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_map_operation_names() {
        let caps = Capabilities {
            annotate: true,
            supersede: false,
            watch: true,
        };
        assert!(caps.supports("annotate"));
        assert!(!caps.supports("supersede"));
        assert!(caps.supports("watch"));
        assert!(!caps.supports("migrate"));
        assert!(!caps.supports(""));
        assert!(Capabilities::ALL.supports("annotate"));
        assert!(Capabilities::ALL.supports("supersede"));
        assert!(Capabilities::ALL.supports("watch"));
        assert!(!Capabilities::NONE.supports("annotate"));
        assert!(!Capabilities::NONE.supports("supersede"));
        assert!(!Capabilities::NONE.supports("watch"));
    }

    #[test]
    fn errors_name_adapter_and_memory() {
        let missing = AdapterError::not_found("muse-code", "MEMORY.md");
        assert_eq!(
            missing.to_string(),
            "muse-code: memory not found: MEMORY.md"
        );
        let unsupported = AdapterError::unsupported("lambo", "annotate");
        assert_eq!(
            unsupported.to_string(),
            "lambo: unsupported operation: annotate"
        );
        let backend = AdapterError::Backend {
            adapter: "lambo".to_string(),
            memory: Some("810c1cbf".to_string()),
            message: "connection refused".to_string(),
        };
        assert_eq!(
            backend.to_string(),
            "lambo: backend error for memory 810c1cbf: connection refused"
        );
        let backend_fault = AdapterError::Backend {
            adapter: "muse-code".to_string(),
            memory: None,
            message: "folder missing".to_string(),
        };
        assert_eq!(
            backend_fault.to_string(),
            "muse-code: backend error: folder missing"
        );
    }

    /// Delegates everything to a [`FakeAdapter`] except `supersede`, which
    /// claims success without creating or linking anything. Exists so the
    /// conformance's supersede assertion has a no-op to catch (LB3).
    struct NoopSupersede<'a> {
        inner: &'a FakeAdapter,
    }

    impl MemoryAdapter for NoopSupersede<'_> {
        fn name(&self) -> &str {
            self.inner.name()
        }

        fn capabilities(&self) -> Capabilities {
            self.inner.capabilities()
        }

        fn list(&self, query: &str) -> Result<Vec<Memory>, AdapterError> {
            self.inner.list(query)
        }

        fn read(&self, id: &str) -> Result<Memory, AdapterError> {
            self.inner.read(id)
        }

        fn write(&self, text: &str, scope: &str) -> Result<MemoryRef, AdapterError> {
            self.inner.write(text, scope)
        }

        fn annotate(&self, id: &str, warning: &str) -> Result<(), AdapterError> {
            self.inner.annotate(id, warning)
        }

        fn supersede(
            &self,
            stale_id: &str,
            correction_text: &str,
        ) -> Result<MemoryRef, AdapterError> {
            let _ = (stale_id, correction_text);
            Ok(MemoryRef {
                adapter: self.name().to_string(),
                id: "mem-noop".to_string(),
            })
        }

        fn watch(&self) -> Result<Vec<Memory>, AdapterError> {
            self.inner.watch()
        }
    }

    #[test]
    #[should_panic(expected = "stale memory names its correction")]
    fn conformance_catches_noop_supersede() {
        let fake = FakeAdapter::new("fake");
        check_conformance(&NoopSupersede { inner: &fake });
    }
}
