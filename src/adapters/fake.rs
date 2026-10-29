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
#[derive(Debug)]
pub struct FakeAdapter {
    name: String,
    capabilities: Capabilities,
    state: RefCell<State>,
}

impl FakeAdapter {
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

    fn list(&self) -> Result<Vec<Memory>, AdapterError> {
        let state = self.state.borrow();
        Ok(state
            .order
            .iter()
            .filter_map(|id| state.memories.get(id).map(Stored::displayed))
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
                    updated_at: Some(format!("2026-10-29T00:{:02}:{:02}Z", (n / 60) % 60, n % 60)),
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

    fn supersede(&self, stale_id: &str, correction_id: &str) -> Result<(), AdapterError> {
        if !self.capabilities.supersede {
            return self.unsupported("supersede");
        }
        let mut state = self.state.borrow_mut();
        if !state.memories.contains_key(correction_id) {
            return Err(AdapterError::not_found(&self.name, correction_id));
        }
        match state.memories.get_mut(stale_id) {
            Some(stored) => {
                stored.superseded_by = Some(correction_id.to_string());
                Ok(())
            }
            None => Err(AdapterError::not_found(&self.name, stale_id)),
        }
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
        let listed = adapter.list().expect("list");
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].reference, first);
        assert_eq!(listed[1].reference, second);
        assert!(listed[0].updated_at.is_some());
        assert_ne!(listed[0].updated_at, listed[1].updated_at);
    }

    #[test]
    fn annotate_and_supersede_show_on_read() {
        let adapter = FakeAdapter::new("fake");
        let stale = adapter.write("old claim", "s").expect("write");
        let correction = adapter.write("new claim", "s").expect("write");
        adapter
            .annotate(&stale.id, "[STALE now X]")
            .expect("annotate");
        adapter
            .supersede(&stale.id, &correction.id)
            .expect("supersede");
        let read = adapter.read(&stale.id).expect("read");
        assert!(read.text.contains("old claim"));
        assert!(read.text.contains("[STALE now X]"));
        assert!(read.text.contains(&correction.id));
    }

    #[test]
    fn unknown_ids_name_adapter_and_memory() {
        let adapter = FakeAdapter::new("fake");
        let failed = adapter.read("mem-9").expect_err("must fail");
        assert_eq!(failed.to_string(), "fake: memory not found: mem-9");
        let correction = adapter.write("new claim", "s").expect("write");
        let failed = adapter
            .supersede("mem-9", &correction.id)
            .expect_err("must fail");
        assert!(matches!(failed, AdapterError::NotFound { .. }));
        assert!(failed.to_string().contains("mem-9"));
        let failed = adapter
            .supersede(&correction.id, "mem-9")
            .expect_err("must fail");
        assert!(failed.to_string().contains("mem-9"));
        let failed = adapter.annotate("mem-9", "[STALE]").expect_err("must fail");
        assert_eq!(failed.to_string(), "fake: memory not found: mem-9");
    }
}
