//! Doyle's memory adapters: the plug-in seam between the core and the memory systems.
//!
//! The core calls only [`MemoryAdapter`]. Adding a memory system means writing
//! one new adapter, with no change to the core. See architecture.md,
//! "The plug-in seams".

mod fake;

// Re-exported for the core's tests now, and for the real adapters' conformance
// runs once T1.5 turns the crate into a library; the allow keeps the gate
// quiet until then.
#[allow(unused_imports)]
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

    /// List every memory the adapter can see: id, text and last updated.
    ///
    /// * Muse Code (T3.1) reads the memory folder for the project Doyle is
    ///   told to curate — `MEMORY.md` plus one file per topic — and returns
    ///   each note as a memory whose ID is its file path. Notes carry
    ///   `type`/`description` frontmatter and no timestamp
    ///   (`updated_at` is `None`); see docs/muse-code.md.
    /// * Lambo (T3.3) recalls against its session and returns one memory per
    ///   hit, keyed by the hit's node ID; see docs/lambo.md on `lambo_recall`.
    fn list(&self) -> Result<Vec<Memory>, AdapterError>;

    /// Read one memory by its ID.
    ///
    /// * Muse Code reads the note file; the returned text includes any
    ///   `[STALE ...]` annotation Doyle wrote in place, which is what makes
    ///   a fresh session answer with the corrected value.
    /// * Lambo inspects the concept (`lambo_inspect`) and returns its
    ///   content together with the `Contradicted ...` warning line, matched
    ///   to the hit by ID, when one is present.
    ///
    /// Unknown IDs are [`AdapterError::NotFound`], never a panic.
    fn read(&self, id: &str) -> Result<Memory, AdapterError>;

    /// Write one memory and return its reference. The adapter assigns the ID
    /// in its own system's terms.
    ///
    /// * Muse Code appends the text as a new note (a bullet in `MEMORY.md`
    ///   or a new topic file) in the project's memory folder.
    /// * Lambo derives a concept (`lambo_derive`), waits on the receipt with
    ///   `lambo_stats` plus `wait_ms` so the next read sees it, and returns
    ///   the created node ID.
    fn write(&self, text: &str, scope: &str) -> Result<MemoryRef, AdapterError>;

    /// Put a warning on the memory in place, so it shows on recall. Optional:
    /// the default body returns [`AdapterError::Unsupported`].
    ///
    /// * Muse Code appends a short, plain `[STALE ...]` line to the note.
    ///   The warning quotes nothing from a contract that could read as an
    ///   instruction (product.md, Cautions).
    /// * Lambo has no in-place edit; its adapter leaves this unsupported and
    ///   corrects through [`MemoryAdapter::supersede`] instead (a derive with
    ///   `against`), which is what makes recall warn on both lines.
    fn annotate(&self, id: &str, warning: &str) -> Result<(), AdapterError> {
        let _ = (id, warning);
        Err(AdapterError::unsupported(self.name(), "annotate"))
    }

    /// Mark the stale memory replaced by a correction. Optional: the default
    /// body returns [`AdapterError::Unsupported`].
    ///
    /// * Muse Code points the stale note at the correction (a
    ///   `superseded-by` line naming the correction note).
    /// * Lambo derives the correction concept with `against` naming the
    ///   stale concept's ID, in the same session, and waits on the receipt.
    fn supersede(&self, stale_id: &str, correction_id: &str) -> Result<(), AdapterError> {
        let _ = (stale_id, correction_id);
        Err(AdapterError::unsupported(self.name(), "supersede"))
    }

    /// Notice new memories: the memories first seen since the previous
    /// `watch` call. Each is reported once; a second call with no new
    /// memories returns an empty list. Optional: the default body returns
    /// [`AdapterError::Unsupported`].
    ///
    /// * Muse Code rescans the memory folder for notes it has not reported
    ///   yet (matched by file path).
    /// * Lambo recalls for concepts it has not reported yet (matched by
    ///   node ID), or polls `lambo_stale` for new dependents.
    fn watch(&self) -> Result<Vec<Memory>, AdapterError> {
        Err(AdapterError::unsupported(self.name(), "watch"))
    }
}

/// The shared conformance exercise every adapter must pass: the fake here,
/// the real adapters in P3.
///
/// Each optional operation is exercised when the adapter's [`Capabilities`]
/// claim it; when they do not, the exercise asserts the typed
/// [`AdapterError::Unsupported`] instead, so one test fits adapters with
/// different capabilities. Panics on the first mismatch, with the adapter
/// and memory named in the failure.
pub fn check_conformance(adapter: &dyn MemoryAdapter) {
    let name = adapter.name().to_string();
    assert!(!name.is_empty(), "adapter name must not be empty");

    // Baseline the watch cursor so only memories written below count as new.
    if adapter.supports("watch") {
        adapter.watch().expect("baseline watch must succeed");
    }

    // Write two memories; the adapter assigns IDs in its own terms and the
    // references carry this adapter's name.
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

    // List sees both, with text.
    let listed = adapter.list().expect("list must succeed");
    let ids: Vec<&str> = listed.iter().map(|m| m.reference.id.as_str()).collect();
    assert!(
        ids.contains(&first.id.as_str()),
        "list shows the first write"
    );
    assert!(
        ids.contains(&second.id.as_str()),
        "list shows the second write"
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

    // Supersede: supported means the stale memory still reads; otherwise the
    // typed unsupported error.
    if adapter.supports("supersede") {
        adapter
            .supersede(&first.id, &second.id)
            .expect("supersede must succeed");
        let superseded = adapter.read(&first.id).expect("read must succeed");
        assert_eq!(superseded.reference, first);
    } else {
        match adapter.supersede(&first.id, &second.id) {
            Err(AdapterError::Unsupported { adapter, operation }) => {
                assert_eq!(adapter, name);
                assert_eq!(operation, "supersede");
            }
            other => {
                panic!("supersede without the capability must be Unsupported, got {other:?}")
            }
        }
    }

    // Watch: the writes since the baseline are reported once, then silence.
    if adapter.supports("watch") {
        let noticed = adapter.watch().expect("watch must succeed");
        let noticed_ids: Vec<&str> = noticed.iter().map(|m| m.reference.id.as_str()).collect();
        assert!(
            noticed_ids.contains(&first.id.as_str()),
            "watch reports the first write once"
        );
        assert!(
            noticed_ids.contains(&second.id.as_str()),
            "watch reports the second write once"
        );
        let again = adapter.watch().expect("watch must succeed");
        assert!(
            again.is_empty(),
            "watch reports each memory once, got {}",
            again.len()
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
}
