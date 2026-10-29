//! Muse Code built-in memory adapter: locate and read memory scopes.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use crate::adapters::{AdapterError, Capabilities, MemoryAdapter};
use crate::core::{Memory, MemoryRef};

pub mod locate;
pub mod parse;

pub use locate::{canonical_dir, fnv1a64, locate, slug};
pub use parse::{Frontmatter, Note, collect_notes, read_folder, split_frontmatter};

#[derive(Debug, Default)]
struct State {
    next: usize,
    writes: Vec<Memory>,
}

pub struct MuseCodeAdapter {
    name: String,
    scopes: Vec<(PathBuf, String)>,
    state: RefCell<State>,
}

impl MuseCodeAdapter {
    /// Build an adapter for the memory scopes discovered from `start_or_folder`.
    ///
    /// If `start_or_folder` is a repository root, its private per-project
    /// folder (by hash) and its shared `.agents/memory/` folder are used.
    /// Otherwise the path is treated as a single memory folder containing
    /// `MEMORY.md`.
    pub fn new(start_or_folder: &Path) -> Self {
        let scopes = locate(start_or_folder);
        let scopes = if scopes.is_empty() {
            vec![(
                start_or_folder.to_path_buf(),
                "personal_project".to_string(),
            )]
        } else {
            scopes
        };
        Self {
            name: "muse-code".to_string(),
            scopes,
            state: RefCell::new(State::default()),
        }
    }

    /// Build an adapter from explicit scope folders.
    pub fn from_scopes(scopes: Vec<(PathBuf, String)>) -> Self {
        Self {
            name: "muse-code".to_string(),
            scopes,
            state: RefCell::new(State::default()),
        }
    }

    fn read_files(&self) -> Vec<Memory> {
        let mut out = Vec::new();
        for (folder, scope) in &self.scopes {
            out.extend(parse::read_folder(folder, scope, &self.name));
        }
        out
    }
}

impl MemoryAdapter for MuseCodeAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::NONE
    }

    fn list(&self, query: &str) -> Result<Vec<Memory>, AdapterError> {
        let query = query.to_lowercase();
        let mut out = self.read_files();
        // in-memory writes
        let state = self.state.borrow();
        out.extend(state.writes.clone());
        if query.is_empty() {
            Ok(out)
        } else {
            Ok(out
                .into_iter()
                .filter(|m| m.text.to_lowercase().contains(&query))
                .collect())
        }
    }

    fn read(&self, id: &str) -> Result<Memory, AdapterError> {
        // in-memory writes first
        let state = self.state.borrow();
        if let Some(m) = state.writes.iter().find(|m| m.reference.id == id) {
            return Ok(m.clone());
        }
        drop(state);
        for m in self.read_files() {
            if m.reference.id == id {
                return Ok(m);
            }
        }
        Err(AdapterError::not_found(&self.name, id))
    }

    fn write(&self, text: &str, scope: &str) -> Result<MemoryRef, AdapterError> {
        let mut state = self.state.borrow_mut();
        state.next += 1;
        let n = state.next;
        let reference = MemoryRef {
            adapter: self.name.clone(),
            id: format!("mem-{n}"),
        };
        state.writes.push(Memory {
            reference: reference.clone(),
            text: text.to_string(),
            updated_at: None,
            scope: scope.to_string(),
        });
        Ok(reference)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::check_conformance;
    use std::fs;
    use std::path::PathBuf;

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

    fn temp_fixture() -> PathBuf {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let src = base.join("testdata/memories/muse-code");
        let dst = std::env::temp_dir().join(format!(
            "doyle-muse-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&dst);
        copy_dir(&src, &dst);
        dst
    }

    #[test]
    fn conformance_list_and_read() {
        let dir = temp_fixture();
        let adapter = MuseCodeAdapter::new(&dir);
        check_conformance(&adapter);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_all_four_notes() {
        let dir = temp_fixture();
        let adapter = MuseCodeAdapter::new(&dir);
        let all = adapter.list("").unwrap();
        assert_eq!(
            all.len(),
            4,
            "got {:?}",
            all.iter().map(|m| &m.text).collect::<Vec<_>>()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn stable_refs_on_extra_note() {
        let dir = temp_fixture();
        let adapter = MuseCodeAdapter::new(&dir);
        let before: Vec<String> = adapter
            .list("")
            .unwrap()
            .into_iter()
            .map(|m| m.reference.id)
            .collect();
        let p = dir.join("MEMORY.md");
        let mut s = fs::read_to_string(&p).unwrap();
        s.push_str("- EXTRA NOTE ADDED\n");
        fs::write(&p, s).unwrap();
        let after: Vec<String> = adapter
            .list("")
            .unwrap()
            .into_iter()
            .map(|m| m.reference.id)
            .collect();
        for id in &before {
            assert!(after.contains(id), "missing {}", id);
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
