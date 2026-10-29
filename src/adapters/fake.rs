//! An in-memory [`MemoryAdapter`](super::MemoryAdapter) implementing every
//! capability, for the core's tests and as the reference the real adapters
//! are checked against.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use super::{AdapterError, Capabilities, MemoryAdapter};
use crate::core::{Memory, MemoryRef};

/// What the fake stores per memory: the memory itself, the warnings put on
/// it, and the correction that replaced it, if any.
#[derive(Debug, Clone)]
struct Stored {
    memory: Memory,
    annotations: Vec<String>,
    superseded_by: Option<String>,
}

impl Stored {
    /// The memory as recall sees it: its text, then each warning on its own
    /// line, then the superseded-by pointer, if any. Mirrors the Muse Code
    /// rule that an annotation appended to the note is what a fresh session
    /// reads back.
    fn displayed(&self) -> Memory {
        let mut text = self.memory.text.clone();
        for annotation in &self.annotations {
            text.push('\n');
            text.push_str(annotation);
        }
        if let Some(correction) = &self.superseded_by {
            text.push('\n');
            text.push_str("[Superseded by ");
            text.push_str(correction);
            text.push(']');
        }
        Memory {
            reference: self.memory.reference.clone(),
            text,
            updated_at: self.memory.updated_at.clone(),
            scope: self.memory.scope.clone(),
        }
    }
}

#[derive(Debug, Default)]
struct State {
    memories: HashMap<String, Stored>,
    /// Insertion order, so `list` is deterministic.
    order: Vec<String>,
    /// IDs already reported by `watch`.
    seen: HashSet<String>,
    next: usize,
}

/// An in-memory adapter with every capability, for tests.
///
/// IDs are minted `mem-1`, `mem-2`, ... in write order. Timestamps are
/// deterministic (`updated_at` counts up from a fixed date) so tests can pin
/// them. Cloning rules: none, the fake is used by reference.
///
/// The fake models the Muse Code side of each contract: `list(query)` is a
/// case-insensitive substring filter over the memories as `read` shows them
/// (empty query returns everything, in insertion order); `supersede`
/// creates the correction and points the stale memory at it; `watch` reports
/// from a process-local seen-set that does NOT survive a restart (test-only
/// behavior — real adapters persist their cursors, see the trait docs).
#[derive(Debug)]
pub struct FakeAdapter {
    name: String,
    capabilities: Capabilities,
    state: RefCell<State>,
}

impl FakeAdapter {
    /// A deterministic timestamp for the Nth created memory.
    fn stamp(n: usize) -> String {
        format!("2026-10-29T00:{:02}:{:02}Z", (n / 60) % 60, n % 60)
    }

    /// A fake with every capability, named `name`.
    pub fn new(name: &str) -> Self {
        Self::with_capabilities(name, Capabilities::ALL)
    }

    /// A fake with a reduced capability set, for testing the core's fallback
    /// when an adapter lacks an optional operation.
    pub fn with_capabilities(name: &str, capabilities: Capabilities) -> Self {
        Self {
            name: name.to_string(),
            capabilities,
            state: RefCell::new(State::default()),
        }
    }

    fn unsupported<T>(&self, operation: &str) -> Result<T, AdapterError> {
        Err(AdapterError::unsupported(&self.name, operation))
    }
}

impl MemoryAdapter for FakeAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    fn list(&self, query: &str) -> Result<Vec<Memory>, AdapterError> {
        let state = self.state.borrow();
        let query = query.to_lowercase();
        Ok(state
            .order
            .iter()
            .filter_map(|id| state.memories.get(id).map(Stored::displayed))
            .filter(|memory| query.is_empty() || memory.text.to_lowercase().contains(&query))
            .collect())
    }

    fn read(&self, id: &str) -> Result<Memory, AdapterError> {
        let state = self.state.borrow();
        state
            .memories
            .get(id)
            .map(Stored::displayed)
            .ok_or_else(|| AdapterError::not_found(&self.name, id))
    }

    fn write(&self, text: &str, scope: &str) -> Result<MemoryRef, AdapterError> {
        let mut state = self.state.borrow_mut();
        state.next += 1;
        let n = state.next;
        let id = format!("mem-{n}");
        let reference = MemoryRef {
            adapter: self.name.clone(),
            id: id.clone(),
        };
        state.memories.insert(
            id.clone(),
            Stored {
                memory: Memory {
                    reference: reference.clone(),
                    text: text.to_string(),
                    updated_at: Some(Self::stamp(n)),
                    scope: scope.to_string(),
                },
                annotations: Vec::new(),
                superseded_by: None,
            },
        );
        state.order.push(id);
        Ok(reference)
    }

    fn annotate(&self, id: &str, warning: &str) -> Result<(), AdapterError> {
        if !self.capabilities.annotate {
            return self.unsupported("annotate");
        }
        let mut state = self.state.borrow_mut();
        match state.memories.get_mut(id) {
            Some(stored) => {
                stored.annotations.push(warning.to_string());
                Ok(())
            }
            None => Err(AdapterError::not_found(&self.name, id)),
        }
    }

    fn supersede(&self, stale_id: &str, correction_text: &str) -> Result<MemoryRef, AdapterError> {
        if !self.capabilities.supersede {
            return self.unsupported("supersede");
        }
        let mut state = self.state.borrow_mut();
        // Idempotent retry: the stale memory already points at a correction
        // with identical text, so create nothing and return it.
        if let Some(stored) = state.memories.get(stale_id)
            && let Some(existing) = &stored.superseded_by
            && let Some(prior) = state.memories.get(existing)
            && prior.memory.text == correction_text
        {
            return Ok(prior.memory.reference.clone());
        }
        let scope = state
            .memories
            .get(stale_id)
            .map(|stored| stored.memory.scope.clone())
            .ok_or_else(|| AdapterError::not_found(&self.name, stale_id))?;
        state.next += 1;
        let n = state.next;
        let id = format!("mem-{n}");
        let reference = MemoryRef {
            adapter: self.name.clone(),
            id: id.clone(),
        };
        state.memories.insert(
            id.clone(),
            Stored {
                memory: Memory {
                    reference: reference.clone(),
                    text: correction_text.to_string(),
                    updated_at: Some(Self::stamp(n)),
                    scope,
                },
                annotations: Vec::new(),
                superseded_by: None,
            },
        );
        state.order.push(id.clone());
        if let Some(stored) = state.memories.get_mut(stale_id) {
            stored.superseded_by = Some(id);
        }
        Ok(reference)
    }

    fn watch(&self) -> Result<Vec<Memory>, AdapterError> {
        if !self.capabilities.watch {
            return self.unsupported("watch");
        }
        let mut state = self.state.borrow_mut();
        let mut fresh = Vec::new();
        let order = state.order.clone();
        for id in &order {
            if state.seen.insert(id.clone())
                && let Some(stored) = state.memories.get(id)
            {
                fresh.push(stored.displayed());
            }
        }
        Ok(fresh)
    }
}

#[cfg(test)]
mod tests {
    use super::super::check_conformance;
    use super::*;

    #[test]
    fn fake_passes_conformance() {
        check_conformance(&FakeAdapter::new("fake"));
    }

    #[test]
    fn reduced_fake_reports_typed_unsupported_never_panics() {
        let adapter = FakeAdapter::with_capabilities("small", Capabilities::NONE);
        assert!(!adapter.supports("annotate"));
        assert!(!adapter.supports("supersede"));
        assert!(!adapter.supports("watch"));

        // The shared exercise covers the unsupported paths too, so a
        // reduced adapter runs the same test as the real ones will.
        check_conformance(&adapter);
    }

    #[test]
    fn write_mints_ids_and_timestamps_in_order() {
        let adapter = FakeAdapter::new("fake");
        let first = adapter.write("one", "s").expect("write");
        let second = adapter.write("two", "s").expect("write");
        assert_eq!(first.id, "mem-1");
        assert_eq!(second.id, "mem-2");
        let listed = adapter.list("").expect("list");
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].reference, first);
        assert_eq!(listed[1].reference, second);
        assert!(listed[0].updated_at.is_some());
        assert_ne!(listed[0].updated_at, listed[1].updated_at);
    }

    #[test]
    fn list_filters_by_query_case_insensitively() {
        let adapter = FakeAdapter::new("fake");
        let first = adapter
            .write("GET /patients returns every patient", "s")
            .expect("write");
        let second = adapter
            .write("GET /patients returns one page plus next_cursor", "s")
            .expect("write");
        let all = adapter.list("").expect("list");
        assert_eq!(all.len(), 2);
        let both = adapter.list("GET /patients").expect("list");
        let both_ids: Vec<&str> = both.iter().map(|m| m.reference.id.as_str()).collect();
        assert!(both_ids.contains(&first.id.as_str()));
        assert!(both_ids.contains(&second.id.as_str()));
        // Lower-case query matches upper-case text.
        let lowered = adapter.list("get /patients").expect("list");
        assert_eq!(lowered.len(), 2);
        // The distinctive term matches only the second write.
        let paged = adapter.list("next_cursor").expect("list");
        assert_eq!(paged.len(), 1);
        assert_eq!(paged[0].reference, second);
        // Nothing matches a query naming nothing stored.
        let none = adapter.list("no-such-phrase-zzz").expect("list");
        assert!(none.is_empty());
    }

    #[test]
    fn annotate_and_supersede_show_on_read() {
        let adapter = FakeAdapter::new("fake");
        let stale = adapter.write("old claim", "s").expect("write");
        adapter
            .annotate(&stale.id, "[STALE now X]")
            .expect("annotate");
        let correction = adapter
            .supersede(&stale.id, "new claim")
            .expect("supersede");
        assert_ne!(correction.id, stale.id);
        let read = adapter.read(&stale.id).expect("read");
        assert!(read.text.contains("old claim"));
        assert!(read.text.contains("[STALE now X]"));
        assert!(read.text.contains(&correction.id));
        let stored = adapter.read(&correction.id).expect("read");
        assert_eq!(stored.text, "new claim");
        assert_eq!(stored.reference, correction);
    }

    #[test]
    fn supersede_creates_exactly_one_memory_and_links_it() {
        let adapter = FakeAdapter::new("fake");
        let stale = adapter.write("old claim", "clinic").expect("write");
        assert_eq!(adapter.list("").expect("list").len(), 1);
        let correction = adapter
            .supersede(&stale.id, "new claim")
            .expect("supersede");
        // Exactly one new memory: the correction.
        let listed = adapter.list("").expect("list");
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[1].reference, correction);
        // The correction lives where the stale memory lived.
        let stored = adapter.read(&correction.id).expect("read");
        assert_eq!(stored.text, "new claim");
        assert_eq!(stored.scope, "clinic");
        // The stale memory names the correction.
        let read = adapter.read(&stale.id).expect("read");
        assert!(read.text.contains(&correction.id));
    }

    #[test]
    fn supersede_retry_with_same_text_is_noop() {
        let adapter = FakeAdapter::new("fake");
        let stale = adapter.write("old claim", "s").expect("write");
        let first = adapter
            .supersede(&stale.id, "new claim")
            .expect("supersede");
        let retry = adapter
            .supersede(&stale.id, "new claim")
            .expect("supersede");
        assert_eq!(retry, first);
        assert_eq!(adapter.list("").expect("list").len(), 2);
    }

    #[test]
    fn supersede_with_new_text_replaces_the_link() {
        let adapter = FakeAdapter::new("fake");
        let stale = adapter.write("old claim", "s").expect("write");
        let first = adapter
            .supersede(&stale.id, "new claim")
            .expect("supersede");
        let second = adapter
            .supersede(&stale.id, "newer claim")
            .expect("supersede");
        assert_ne!(second, first);
        assert_eq!(adapter.list("").expect("list").len(), 3);
        let read = adapter.read(&stale.id).expect("read");
        assert!(read.text.contains(&second.id));
    }

    #[test]
    fn unknown_ids_name_adapter_and_memory() {
        let adapter = FakeAdapter::new("fake");
        let failed = adapter.read("mem-9").expect_err("must fail");
        assert_eq!(failed.to_string(), "fake: memory not found: mem-9");
        let failed = adapter
            .supersede("mem-9", "new claim")
            .expect_err("must fail");
        assert!(matches!(failed, AdapterError::NotFound { .. }));
        assert!(failed.to_string().contains("mem-9"));
        let failed = adapter.annotate("mem-9", "[STALE]").expect_err("must fail");
        assert_eq!(failed.to_string(), "fake: memory not found: mem-9");
    }
}
