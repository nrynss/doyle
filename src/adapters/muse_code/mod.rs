//! Muse Code built-in memory adapter: locate and read memory scopes.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::adapters::{AdapterError, Capabilities, MemoryAdapter};
use crate::core::{Memory, MemoryRef};

pub mod locate;
pub mod parse;
mod write;

pub use locate::{canonical_dir, fnv1a64, locate, slug};
pub use parse::{Frontmatter, Note, collect_notes, read_folder, split_frontmatter};

#[derive(Debug, Default)]
struct State {
    writes: Vec<Memory>,
    scopes: HashMap<String, String>,
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
        Capabilities {
            annotate: true,
            supersede: true,
            watch: false,
        }
    }

    fn list(&self, query: &str) -> Result<Vec<Memory>, AdapterError> {
        let query = query.to_lowercase();
        let mut out = self.read_files();
        // Apply scopes recorded for notes Doyle wrote, so a write's scope
        // round-trips even though Muse Code stores no per-note scope on disk.
        let state = self.state.borrow();
        for m in &mut out {
            if let Some(s) = state.scopes.get(&m.reference.id) {
                m.scope = s.clone();
            }
        }
        out.extend(state.writes.clone());
        drop(state);
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
        // recorded scope for a written note
        let recorded_scope = state.scopes.get(id).cloned();
        drop(state);
        for mut m in self.read_files() {
            if m.reference.id == id {
                if let Some(s) = recorded_scope {
                    m.scope = s;
                }
                return Ok(m);
            }
        }
        Err(AdapterError::not_found(&self.name, id))
    }

    fn write(&self, text: &str, scope: &str) -> Result<MemoryRef, AdapterError> {
        write::write(self, text, scope)
    }

    fn annotate(&self, id: &str, warning: &str) -> Result<(), AdapterError> {
        write::annotate(self, id, warning)
    }

    fn supersede(&self, stale_id: &str, correction_text: &str) -> Result<MemoryRef, AdapterError> {
        write::supersede(self, stale_id, correction_text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::check_conformance;
    use crate::adapters::muse_code::parse;
    use crate::adapters::muse_code::write;
    use std::collections::HashSet;
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::SystemTime;

    static HOME_LOCK: Mutex<()> = Mutex::new(());
    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn temp_dir(label: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!(
            "doyle-muse-{}-{}-{}-{}",
            label,
            std::process::id(),
            n,
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

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
        let dst = temp_dir("fixture");
        copy_dir(&src, &dst);
        dst
    }

    fn set_home(home: &Path) {
        unsafe { std::env::set_var("HOME", home) };
    }

    #[test]
    fn conformance_list_and_read() {
        let _g = HOME_LOCK.lock().unwrap();
        let home = temp_dir("home");
        set_home(&home);
        let dir = temp_fixture();
        let adapter = MuseCodeAdapter::new(&dir);
        check_conformance(&adapter);
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn reads_all_four_notes() {
        let _g = HOME_LOCK.lock().unwrap();
        let home = temp_dir("home");
        set_home(&home);
        let dir = temp_fixture();
        let adapter = MuseCodeAdapter::new(&dir);
        let all = adapter.list("").unwrap();
        assert_eq!(
            all.len(),
            4,
            "got {:?}",
            all.iter().map(|m| &m.text).collect::<Vec<_>>()
        );
        let texts: Vec<&str> = all.iter().map(|m| m.text.as_str()).collect();
        assert!(texts.contains(&"GET /patients returns every patient in one response; one call exports the whole clinic"));
        assert!(texts.contains(&"GET /patients/{id} returns 404 for an unknown patient id"));
        assert!(
            texts.contains(
                &"POST /admin/decommission is permanent once confirmed; there is no undo"
            )
        );
        assert!(texts.contains(&"GET /stats patient_count is the total number of patients"));
        for m in &all {
            assert_eq!(m.scope, "personal_project");
        }
        // frontmatter via parse entry points
        let mem_content = fs::read_to_string(dir.join("MEMORY.md")).unwrap();
        let (fm, _) = parse::split_frontmatter(&mem_content);
        assert_eq!(fm.note_type.as_deref(), Some("reference"));
        assert_eq!(
            fm.description.as_deref(),
            Some("Fernwhistle patient records API")
        );
        let stats_content = fs::read_to_string(dir.join("patients-stats.md")).unwrap();
        let (fm2, _) = parse::split_frontmatter(&stats_content);
        assert_eq!(fm2.note_type.as_deref(), Some("reference"));
        assert_eq!(
            fm2.description.as_deref(),
            Some("Fernwhistle patient records stats endpoint")
        );
        let mut seen = HashSet::new();
        let notes = parse::collect_notes(&dir, "MEMORY.md", &mut seen);
        assert_eq!(notes.len(), 4);
        for n in &notes {
            assert_eq!(n.frontmatter.note_type.as_deref(), Some("reference"));
        }
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn stable_refs_on_extra_note() {
        let _g = HOME_LOCK.lock().unwrap();
        let home = temp_dir("home");
        set_home(&home);
        let dir = temp_fixture();
        let adapter = MuseCodeAdapter::new(&dir);
        let before: Vec<(String, String)> = adapter
            .list("")
            .unwrap()
            .into_iter()
            .map(|m| (m.reference.id, m.text))
            .collect();
        let before_count = before.len();
        let p = dir.join("MEMORY.md");
        let mut s = fs::read_to_string(&p).unwrap();
        s.push_str("- EXTRA NOTE ADDED\n");
        fs::write(&p, s).unwrap();
        let after = adapter.list("").unwrap();
        assert_eq!(after.len(), before_count + 1, "count must be 5");
        for (id, text) in &before {
            let found = after.iter().find(|m| &m.reference.id == id).unwrap();
            assert_eq!(&found.text, text, "old text must be byte-identical");
        }
        assert!(after.iter().any(|m| m.text == "EXTRA NOTE ADDED"));
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&home);
    }

    // LB1: in-place annotation of a note's own line keeps the old id readable
    // with the warning visible.
    #[test]
    fn in_place_annotation_keeps_ref() {
        let _g = HOME_LOCK.lock().unwrap();
        let home = temp_dir("home");
        set_home(&home);
        let dir = temp_fixture();
        let adapter = MuseCodeAdapter::new(&dir);
        let first = adapter.list("").unwrap().into_iter().next().unwrap();
        let id = first.reference.id.clone();
        let old_text = first.text.clone();
        let p = dir.join("MEMORY.md");
        let content = fs::read_to_string(&p).unwrap();
        let new_content = content.replace(&old_text, &format!("{} [STALE marker]", old_text));
        assert_ne!(new_content, content);
        fs::write(&p, new_content).unwrap();
        let read = adapter.read(&id).expect("old id must still read");
        assert!(
            read.text.contains("[STALE marker]"),
            "warning must be visible"
        );
        assert_eq!(read.reference.id, id);
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&home);
    }

    // LB1: two identical bullets get distinct ids.
    #[test]
    fn duplicate_text_distinct_ids() {
        let _g = HOME_LOCK.lock().unwrap();
        let home = temp_dir("home");
        set_home(&home);
        let dir = temp_dir("dup");
        fs::write(
            dir.join("MEMORY.md"),
            "---\ntype: reference\n---\n\n- same note text\n- same note text\n",
        )
        .unwrap();
        let adapter = MuseCodeAdapter::new(&dir);
        let all = adapter.list("").unwrap();
        assert_eq!(all.len(), 2);
        assert_ne!(all[0].reference.id, all[1].reference.id);
        for m in &all {
            let r = adapter.read(&m.reference.id).unwrap();
            assert_eq!(r.text, "same note text");
        }
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&home);
    }

    // LB3: fake HOME locate (private scope).
    #[test]
    fn locate_private_scope_fake_home() {
        let _g = HOME_LOCK.lock().unwrap();
        let home = temp_dir("home");
        set_home(&home);
        assert_eq!(std::env::var("HOME").unwrap(), home.to_string_lossy());
        let start = temp_dir("start");
        let canon = start.canonicalize().unwrap();
        let path_str = canon.to_string_lossy().to_string();
        let hash = fnv1a64(&path_str);
        let slug_str = slug(&path_str);
        let folder_name = format!("{}-{:016x}", slug_str, hash);
        let folder = home
            .join(".local")
            .join("share")
            .join("muse")
            .join("memory")
            .join("projects")
            .join(folder_name);
        fs::create_dir_all(&folder).unwrap();
        fs::write(
            folder.join("MEMORY.md"),
            "---\ntype: reference\n---\n\n- private note\n",
        )
        .unwrap();

        let adapter = MuseCodeAdapter::new(&start);
        let all = adapter.list("").unwrap();
        assert!(all.iter().any(|m| m.text == "private note"));
        assert!(all.iter().any(|m| m.scope == "personal_project"));
        let _ = fs::remove_dir_all(&start);
        let _ = fs::remove_dir_all(&home);
    }

    // LB3: symlink resolution case.
    #[test]
    fn locate_private_scope_symlink() {
        let _g = HOME_LOCK.lock().unwrap();
        let home = temp_dir("home");
        set_home(&home);
        let start = temp_dir("start");
        let canon = start.canonicalize().unwrap();
        let path_str = canon.to_string_lossy().to_string();
        let hash = fnv1a64(&path_str);
        let slug_str = slug(&path_str);
        let folder_name = format!("{}-{:016x}", slug_str, hash);
        let folder = home
            .join(".local")
            .join("share")
            .join("muse")
            .join("memory")
            .join("projects")
            .join(folder_name);
        fs::create_dir_all(&folder).unwrap();
        fs::write(
            folder.join("MEMORY.md"),
            "---\ntype: reference\n---\n\n- private note via symlink\n",
        )
        .unwrap();

        let link_parent = temp_dir("linkparent");
        let link = link_parent.join("link");
        symlink(&start, &link).unwrap();

        let adapter = MuseCodeAdapter::new(&link);
        let all = adapter.list("").unwrap();
        assert!(all.iter().any(|m| m.text == "private note via symlink"));
        let _ = fs::remove_dir_all(&start);
        let _ = fs::remove_dir_all(&link_parent);
        let _ = fs::remove_dir_all(&home);
    }

    // LB3: shared scope.
    #[test]
    fn locate_shared_scope() {
        let _g = HOME_LOCK.lock().unwrap();
        let home = temp_dir("home");
        set_home(&home);
        let start = temp_dir("start");
        let shared = start.join(".agents").join("memory");
        fs::create_dir_all(&shared).unwrap();
        fs::write(
            shared.join("MEMORY.md"),
            "---\ntype: reference\n---\n\n- shared note\n",
        )
        .unwrap();
        let adapter = MuseCodeAdapter::new(&start);
        let all = adapter.list("").unwrap();
        assert!(all.iter().any(|m| m.text == "shared note"));
        assert!(all.iter().any(|m| m.scope == "shared"));
        let _ = fs::remove_dir_all(&start);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn annotate_in_place_keeps_ref_and_text() {
        let _g = HOME_LOCK.lock().unwrap();
        let home = temp_dir("home");
        set_home(&home);
        let dir = temp_fixture();
        let adapter = MuseCodeAdapter::new(&dir);
        let first = adapter.list("").unwrap().into_iter().next().unwrap();
        let id = first.reference.id.clone();
        adapter.annotate(&id, "[STALE test warning]").unwrap();
        let read = adapter.read(&id).unwrap();
        assert!(read.text.contains("[STALE test warning]"));
        assert_eq!(read.reference.id, id);
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn supersede_creates_and_links_and_retry_noop() {
        let _g = HOME_LOCK.lock().unwrap();
        let home = temp_dir("home");
        set_home(&home);
        let dir = temp_fixture();
        let adapter = MuseCodeAdapter::new(&dir);
        let first = adapter.list("").unwrap().into_iter().next().unwrap();
        let id = first.reference.id.clone();
        let correction = adapter
            .supersede(
                &id,
                "GET /patients returns one page of 100 plus next_cursor",
            )
            .unwrap();
        let read = adapter.read(&id).unwrap();
        assert!(read.text.contains(&correction.id));
        let stored = adapter.read(&correction.id).unwrap();
        assert!(
            stored
                .text
                .contains("GET /patients returns one page of 100 plus next_cursor")
        );
        // retry no-op
        let retry = adapter
            .supersede(
                &id,
                "GET /patients returns one page of 100 plus next_cursor",
            )
            .unwrap();
        assert_eq!(retry, correction);
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn interrupted_write_leaves_original_intact() {
        let _g = HOME_LOCK.lock().unwrap();
        let home = temp_dir("home");
        set_home(&home);
        let dir = temp_fixture();
        let adapter = MuseCodeAdapter::new(&dir);
        let path = dir.join("MEMORY.md");
        let before = fs::read_to_string(&path).unwrap();

        // inject failure at the rename seam
        write::set_rename_hook(Some(|_src, _dst| Err(std::io::Error::other("boom"))));
        let res = adapter.write("INTERRUPTED NOTE", "personal_project");
        assert!(res.is_err());
        write::set_rename_hook(None);

        let after = fs::read_to_string(&path).unwrap();
        assert_eq!(before, after, "original file must be unchanged");
        // temp file may remain; clean it
        let tmp = dir.join(".MEMORY.md.doyle-tmp");
        let _ = fs::remove_file(&tmp);
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&home);
    }
}
