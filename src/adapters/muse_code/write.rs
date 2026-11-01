//! Muse Code write side: new notes, in-place annotation, supersede.
//!
//! Bounds on Doyle's writes. There is no "index limit" recorded for Muse Code
//! memory: T0.4 settled only the folder hash, hook registration, and wire
//! format, and `docs/muse-code.md` specifies none. Doyle stays within bounds by
//! construction instead: annotations are short plain in-place markers (one marker
//! per note), corrections are single notes, and new notes are single bullets.
//! These are small deltas, so a write never rewrites or truncates a memory file
//! beyond the one bullet it touches (plus, for supersede, one appended bullet).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::adapters::muse_code::MuseCodeAdapter;
use crate::adapters::muse_code::parse::{
    BODY_ORDINAL, has_frontmatter, split_frontmatter, topic_files,
};
use crate::adapters::{AdapterError, MemoryAdapter};
use crate::core::MemoryRef;

#[allow(clippy::type_complexity)]
static RENAME_HOOK: Mutex<Option<fn(&Path, &Path) -> std::io::Result<()>>> = Mutex::new(None);

/// Test seam: override the rename used by atomic writes. Pass `None` to
/// restore the real `std::fs::rename`.
#[allow(dead_code)]
pub fn set_rename_hook(hook: Option<fn(&Path, &Path) -> std::io::Result<()>>) {
    *RENAME_HOOK.lock().unwrap_or_else(|e| e.into_inner()) = hook;
}

fn rename_file(src: &Path, dst: &Path) -> std::io::Result<()> {
    let hook = *RENAME_HOOK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(h) = hook {
        h(src, dst)
    } else {
        fs::rename(src, dst)
    }
}

fn process_alive(pid: i32) -> bool {
    Path::new("/proc").join(pid.to_string()).is_dir()
}

/// Muse Code never specifies a lock format in `docs/muse-code.md`; Doyle assumes
/// a `pid=<n>` line (one integer pid). Fail-open is deliberate: a missing,
/// unreadable, or malformed lock never blocks a write, and only a lock naming a
/// pid that is currently alive refuses the write.
fn check_lock(dir: &Path, adapter: &str) -> Result<(), AdapterError> {
    let lock = dir.join(".muse-memory.lock");
    if lock.is_file()
        && let Ok(s) = fs::read_to_string(&lock)
    {
        let s = s.trim();
        if let Some(pid_str) = s.strip_prefix("pid=")
            && let Ok(pid) = pid_str.parse::<i32>()
            && process_alive(pid)
        {
            return Err(AdapterError::Backend {
                adapter: adapter.to_string(),
                memory: None,
                message: format!("memory locked by pid {pid}"),
            });
        }
    }
    Ok(())
}

fn scope_folder(adapter: &MuseCodeAdapter, scope: &str) -> Option<(PathBuf, String)> {
    for (folder, label) in &adapter.scopes {
        if label == scope {
            return Some((folder.clone(), label.clone()));
        }
    }
    adapter.scopes.first().cloned()
}

fn find_file_folder(adapter: &MuseCodeAdapter, source: &str) -> Option<PathBuf> {
    for (folder, _) in &adapter.scopes {
        if folder.join(source).is_file() {
            return Some(folder.clone());
        }
    }
    None
}

fn parse_id(id: &str) -> Option<(String, usize)> {
    let (file, ord) = id.rsplit_once('#')?;
    let ord: usize = ord.parse().ok()?;
    Some((file.to_string(), ord))
}

fn atomic_write(path: &Path, content: &str, adapter: &str) -> Result<(), AdapterError> {
    if let Some(parent) = path.parent() {
        check_lock(parent, adapter)?;
    }
    let tmp = path.with_file_name(format!(
        ".{}.doyle-tmp",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));
    fs::write(&tmp, content).map_err(|e| AdapterError::Backend {
        adapter: adapter.to_string(),
        memory: None,
        message: format!("write temp file: {e}"),
    })?;
    match rename_file(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(AdapterError::Backend {
                adapter: adapter.to_string(),
                memory: None,
                message: format!("rename temp file: {e}"),
            })
        }
    }
}

/// Append `suffix` to the note at `ordinal` in `content`, leaving everything
/// else (frontmatter, other notes, line endings) untouched. Ordinal 0 is a
/// topic file's whole-body note: the suffix goes after the last body text.
fn mark_note(content: &str, ordinal: usize, suffix: &str) -> Option<String> {
    if ordinal == BODY_ORDINAL {
        if !has_frontmatter(content) {
            return None;
        }
        let (_, body) = split_frontmatter(content);
        if body.trim().is_empty() {
            return None;
        }
        let kept = content.trim_end();
        let tail = &content[kept.len()..];
        return Some(format!("{kept} {suffix}{tail}"));
    }
    let mut lines: Vec<String> = content.split('\n').map(String::from).collect();
    let mut count = 0usize;
    let mut found = false;
    for line in lines.iter_mut() {
        if let Some(rest) = line.trim().strip_prefix("- ") {
            if rest.trim().is_empty() {
                continue;
            }
            count += 1;
            if count == ordinal {
                let trimmed = line.trim_end().to_string();
                *line = format!("{trimmed} {suffix}");
                found = true;
                break;
            }
        }
    }
    found.then(|| lines.join("\n"))
}

/// Starting content for a `MEMORY.md` Doyle has to create: frontmatter, then
/// an index line for every topic file already in the folder, so creating the
/// index never drops a topic file out of what Muse Code reads.
fn new_index_content(folder: &Path) -> String {
    let mut out = String::from("---\ntype: reference\n---\n\n");
    for name in topic_files(folder) {
        let desc = fs::read_to_string(folder.join(&name))
            .ok()
            .and_then(|c| split_frontmatter(&c).0.description)
            .filter(|d| !d.is_empty());
        let stem = name.strip_suffix(".md").unwrap_or(&name);
        match desc {
            Some(d) => out.push_str(&format!("- [{stem}]({name}) — {d}\n")),
            None => out.push_str(&format!("- [{stem}]({name})\n")),
        }
    }
    out
}

fn note_text_at(folder: &Path, source: &str, ordinal: usize) -> Option<String> {
    let full = folder.join(source);
    let content = fs::read_to_string(&full).ok()?;
    if ordinal == BODY_ORDINAL {
        return has_frontmatter(&content)
            .then(|| split_frontmatter(&content).1.trim().to_string())
            .filter(|t| !t.is_empty());
    }
    // split frontmatter
    let body = if content.starts_with("---") {
        let after = &content[content.find('\n')? + 1..];
        if let Some(idx) = after.find("\n---") {
            let body = &after[idx + 4..];
            body.strip_prefix('\n').unwrap_or(body)
        } else {
            &content
        }
    } else {
        &content
    };
    let mut count = 0usize;
    for line in body.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("- ") {
            if rest.trim().is_empty() {
                continue;
            }
            count += 1;
            if count == ordinal {
                let trimmed = line.trim();
                let bullet = trimmed.strip_prefix("- ").unwrap_or("").trim();
                return Some(bullet.to_string());
            }
        }
    }
    None
}

fn count_bullets(content: &str) -> usize {
    let body = if content.starts_with("---") {
        let after = match content.find('\n') {
            Some(i) => &content[i + 1..],
            None => return 0,
        };
        match after.find("\n---") {
            Some(idx) => {
                let body = &after[idx + 4..];
                body.strip_prefix('\n').unwrap_or(body)
            }
            None => content,
        }
    } else {
        content
    };
    body.lines()
        .filter(|l| {
            let t = l.trim();
            if let Some(rest) = t.strip_prefix("- ") {
                !rest.trim().is_empty()
            } else {
                false
            }
        })
        .count()
}

pub fn write(
    adapter: &MuseCodeAdapter,
    text: &str,
    scope: &str,
) -> Result<MemoryRef, AdapterError> {
    let (folder, _) = scope_folder(adapter, scope).ok_or_else(|| AdapterError::Backend {
        adapter: adapter.name().to_string(),
        memory: None,
        message: "no memory folder".to_string(),
    })?;
    let path = folder.join("MEMORY.md");
    // Ensure file exists
    if !path.is_file() {
        let fm = new_index_content(&folder);
        atomic_write(&path, &fm, adapter.name())?;
    }
    let content = fs::read_to_string(&path).map_err(|e| AdapterError::Backend {
        adapter: adapter.name().to_string(),
        memory: None,
        message: format!("read memory file: {e}"),
    })?;
    let n = count_bullets(&content) + 1;
    let mut new_content = content;
    if !new_content.ends_with('\n') && !new_content.is_empty() {
        new_content.push('\n');
    }
    new_content.push_str(&format!("- {}\n", text));
    atomic_write(&path, &new_content, adapter.name())?;

    let id = format!("MEMORY.md#{}", n);
    adapter
        .state
        .borrow_mut()
        .scopes
        .insert(id.clone(), scope.to_string());
    Ok(MemoryRef {
        adapter: adapter.name().to_string(),
        id,
    })
}

pub fn annotate(adapter: &MuseCodeAdapter, id: &str, warning: &str) -> Result<(), AdapterError> {
    let (source, ordinal) =
        parse_id(id).ok_or_else(|| AdapterError::not_found(adapter.name(), id))?;
    let folder = find_file_folder(adapter, &source)
        .ok_or_else(|| AdapterError::not_found(adapter.name(), id))?;
    let path = folder.join(&source);
    let content = fs::read_to_string(&path).map_err(|e| AdapterError::Backend {
        adapter: adapter.name().to_string(),
        memory: Some(id.to_string()),
        message: format!("read memory file: {e}"),
    })?;
    let new_content = mark_note(&content, ordinal, warning)
        .ok_or_else(|| AdapterError::not_found(adapter.name(), id))?;
    atomic_write(&path, &new_content, adapter.name())?;
    Ok(())
}

pub fn supersede(
    adapter: &MuseCodeAdapter,
    stale_id: &str,
    correction_text: &str,
) -> Result<MemoryRef, AdapterError> {
    let (source, ordinal) =
        parse_id(stale_id).ok_or_else(|| AdapterError::not_found(adapter.name(), stale_id))?;
    let folder = find_file_folder(adapter, &source)
        .ok_or_else(|| AdapterError::not_found(adapter.name(), stale_id))?;

    // Retry no-op: stale note already points at a correction with identical text.
    let stale_text = note_text_at(&folder, &source, ordinal).unwrap_or_default();
    if let Some(start) = stale_text.find("[Superseded by ") {
        let rest = &stale_text[start + "[Superseded by ".len()..];
        if let Some(end) = rest.find(']') {
            let existing_id = rest[..end].to_string();
            if let Some((cf, co)) = parse_id(&existing_id)
                && let Some(cfolder) = find_file_folder(adapter, &cf)
                && let Some(ctext) = note_text_at(&cfolder, &cf, co)
                && (ctext == correction_text || ctext.starts_with(correction_text))
            {
                let scope = adapter
                    .state
                    .borrow()
                    .scopes
                    .get(stale_id)
                    .cloned()
                    .unwrap_or_default();
                adapter
                    .state
                    .borrow_mut()
                    .scopes
                    .insert(existing_id.clone(), scope);
                return Ok(MemoryRef {
                    adapter: adapter.name().to_string(),
                    id: existing_id,
                });
            }
        }
    }

    let stale_scope = adapter
        .state
        .borrow()
        .scopes
        .get(stale_id)
        .cloned()
        .unwrap_or_else(|| {
            adapter
                .scopes
                .iter()
                .find(|(f, _)| f == &folder)
                .map(|(_, l)| l.clone())
                .unwrap_or_default()
        });

    // Correction lands in MEMORY.md of the first scope.
    let (first_folder, _) =
        adapter
            .scopes
            .first()
            .cloned()
            .ok_or_else(|| AdapterError::Backend {
                adapter: adapter.name().to_string(),
                memory: None,
                message: "no memory folder".to_string(),
            })?;
    let correction_path = first_folder.join("MEMORY.md");
    if !correction_path.is_file() {
        let fm = new_index_content(&first_folder);
        atomic_write(&correction_path, &fm, adapter.name())?;
    }

    let stale_path = folder.join(&source);
    let same_file = correction_path == stale_path;

    // Recovery scan: if a previous (possibly crashed) attempt already appended a
    // correction bullet with identical text, reuse it instead of appending a
    // duplicate. This makes a retry after a split-file crash converge.
    let corr_content = fs::read_to_string(&correction_path).map_err(|e| AdapterError::Backend {
        adapter: adapter.name().to_string(),
        memory: None,
        message: format!("read memory file: {e}"),
    })?;
    let mut reuse_id: Option<String> = None;
    {
        let body = if corr_content.starts_with("---") {
            let after = &corr_content[corr_content.find('\n').unwrap_or(0) + 1..];
            if let Some(idx) = after.find("\n---") {
                let body = &after[idx + 4..];
                body.strip_prefix('\n').unwrap_or(body)
            } else {
                &corr_content
            }
        } else {
            &corr_content
        };
        let mut count = 0usize;
        for line in body.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("- ") {
                if rest.trim().is_empty() {
                    continue;
                }
                count += 1;
                if rest.trim() == correction_text {
                    reuse_id = Some(format!("MEMORY.md#{}", count));
                    break;
                }
            }
        }
    }

    let correction_id = reuse_id
        .clone()
        .unwrap_or_else(|| format!("MEMORY.md#{}", count_bullets(&corr_content) + 1));

    if same_file {
        // Single atomic write: annotate the stale bullet AND append the correction
        // in one rename, so an interruption cannot orphan one half.
        let content = fs::read_to_string(&stale_path).map_err(|e| AdapterError::Backend {
            adapter: adapter.name().to_string(),
            memory: Some(stale_id.to_string()),
            message: format!("read memory file: {e}"),
        })?;
        let mut new_content = mark_note(
            &content,
            ordinal,
            &format!("[Superseded by {}]", correction_id),
        )
        .ok_or_else(|| AdapterError::not_found(adapter.name(), stale_id))?;
        if reuse_id.is_none() {
            if !new_content.ends_with('\n') && !new_content.is_empty() {
                new_content.push('\n');
            }
            new_content.push_str(&format!("- {}\n", correction_text));
        }
        atomic_write(&stale_path, &new_content, adapter.name())?;
    } else {
        // Split-file case: correction and stale note live in different files, so
        // two atomic writes are unavoidable. Recovery rule: retry reuses an
        // existing correction bullet with identical text (see recovery scan
        // above) and then links the stale note, converging to one correction.
        if reuse_id.is_none() {
            let mut new_content = corr_content;
            if !new_content.ends_with('\n') && !new_content.is_empty() {
                new_content.push('\n');
            }
            new_content.push_str(&format!("- {}\n", correction_text));
            atomic_write(&correction_path, &new_content, adapter.name())?;
        }
        let stale_content = fs::read_to_string(&stale_path).map_err(|e| AdapterError::Backend {
            adapter: adapter.name().to_string(),
            memory: Some(stale_id.to_string()),
            message: format!("read memory file: {e}"),
        })?;
        let new_stale = mark_note(
            &stale_content,
            ordinal,
            &format!("[Superseded by {}]", correction_id),
        )
        .ok_or_else(|| AdapterError::not_found(adapter.name(), stale_id))?;
        atomic_write(&stale_path, &new_stale, adapter.name())?;
    }

    adapter
        .state
        .borrow_mut()
        .scopes
        .insert(correction_id.clone(), stale_scope);

    Ok(MemoryRef {
        adapter: adapter.name().to_string(),
        id: correction_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::muse_code::MuseCodeAdapter;
    use crate::adapters::muse_code::parse;
    use crate::adapters::muse_code::tests::HOME_LOCK;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::SystemTime;

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn temp_dir(label: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!(
            "doyle-musew-{}-{}-{}-{}",
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

    fn copy_dir(src: &std::path::Path, dst: &std::path::Path) {
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
    fn lock_live_pid_refuses_write() {
        let _g = HOME_LOCK.lock().unwrap();
        let home = temp_dir("home");
        set_home(&home);
        let dir = temp_dir("locklive");
        fs::write(dir.join("MEMORY.md"), "---\ntype: reference\n---\n\n").unwrap();
        let adapter =
            MuseCodeAdapter::from_scopes(vec![(dir.clone(), "personal_project".to_string())]);
        fs::write(
            dir.join(".muse-memory.lock"),
            format!("pid={}\n", std::process::id()),
        )
        .unwrap();
        let res = adapter.write("note", "personal_project");
        assert!(res.is_err(), "live pid should refuse write");
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn lock_dead_pid_ignored() {
        let _g = HOME_LOCK.lock().unwrap();
        let home = temp_dir("home");
        set_home(&home);
        let dir = temp_dir("lockdead");
        fs::write(dir.join("MEMORY.md"), "---\ntype: reference\n---\n\n").unwrap();
        let adapter =
            MuseCodeAdapter::from_scopes(vec![(dir.clone(), "personal_project".to_string())]);
        let mut child = Command::new("true").spawn().unwrap();
        let pid = child.id() as i32;
        child.wait().unwrap();
        fs::write(dir.join(".muse-memory.lock"), format!("pid={}\n", pid)).unwrap();
        let res = adapter.write("note", "personal_project");
        assert!(res.is_ok(), "dead pid should be ignored: {:?}", res.err());
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn lock_malformed_or_absent_ignored() {
        let _g = HOME_LOCK.lock().unwrap();
        let home = temp_dir("home");
        set_home(&home);
        let dir = temp_dir("lockmal");
        fs::write(dir.join("MEMORY.md"), "---\ntype: reference\n---\n\n").unwrap();
        let adapter =
            MuseCodeAdapter::from_scopes(vec![(dir.clone(), "personal_project".to_string())]);
        fs::write(dir.join(".muse-memory.lock"), "garbage\n").unwrap();
        assert!(adapter.write("note1", "personal_project").is_ok());
        let _ = fs::remove_file(dir.join(".muse-memory.lock"));
        assert!(adapter.write("note2", "personal_project").is_ok());
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn empty_bullet_does_not_desync_annotate() {
        let _g = HOME_LOCK.lock().unwrap();
        let home = temp_dir("home");
        set_home(&home);
        let dir = temp_dir("emptybullet");
        fs::write(
            dir.join("MEMORY.md"),
            "---\ntype: reference\n---\n\n- target note A\n- \n- target note B\n",
        )
        .unwrap();
        let adapter =
            MuseCodeAdapter::from_scopes(vec![(dir.clone(), "personal_project".to_string())]);
        let all = adapter.list("").unwrap();
        let b = all.iter().find(|m| m.text == "target note B").unwrap();
        let id = b.reference.id.clone();
        assert_eq!(id, "MEMORY.md#2");
        adapter.annotate(&id, "[STALE marker]").unwrap();
        let read = adapter.read(&id).unwrap();
        assert!(read.text.contains("[STALE marker]"));
        let a = adapter.read("MEMORY.md#1").unwrap();
        assert!(!a.text.contains("[STALE marker]"));
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn empty_bullet_does_not_desync_supersede() {
        let _g = HOME_LOCK.lock().unwrap();
        let home = temp_dir("home");
        set_home(&home);
        let dir = temp_dir("emptybullet2");
        fs::write(
            dir.join("MEMORY.md"),
            "---\ntype: reference\n---\n\n- target note A\n- \n- target note B\n",
        )
        .unwrap();
        let adapter =
            MuseCodeAdapter::from_scopes(vec![(dir.clone(), "personal_project".to_string())]);
        let correction = adapter
            .supersede("MEMORY.md#1", "replacement text")
            .unwrap();
        let stale = adapter.read("MEMORY.md#1").unwrap();
        assert!(stale.text.contains(&correction.id));
        let corr = adapter.read(&correction.id).unwrap();
        assert!(corr.text.contains("replacement text"));
        let b = adapter.read("MEMORY.md#2").unwrap();
        assert_eq!(b.text, "target note B");
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn write_pins_other_notes() {
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
        let topic_before = adapter.read("patients-stats.md#1").unwrap().text.clone();
        let mem_before = fs::read_to_string(dir.join("MEMORY.md")).unwrap();
        let (fm_before, _) = parse::split_frontmatter(&mem_before);

        adapter.write("NEW NOTE", "personal_project").unwrap();

        let after = adapter.list("").unwrap();
        for (id, text) in &before {
            let found = after.iter().find(|m| &m.reference.id == id).unwrap();
            assert_eq!(&found.text, text);
        }
        let topic_after = adapter.read("patients-stats.md#1").unwrap().text.clone();
        assert_eq!(topic_after, topic_before);
        let mem_after = fs::read_to_string(dir.join("MEMORY.md")).unwrap();
        let (fm_after, _) = parse::split_frontmatter(&mem_after);
        assert_eq!(fm_after, fm_before);
        assert!(after.iter().any(|m| m.text == "NEW NOTE"));
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn annotate_pins_other_notes() {
        let _g = HOME_LOCK.lock().unwrap();
        let home = temp_dir("home");
        set_home(&home);
        let dir = temp_fixture();
        let adapter = MuseCodeAdapter::new(&dir);
        let first = adapter.list("").unwrap().into_iter().next().unwrap();
        let target_id = first.reference.id.clone();
        let before: Vec<(String, String)> = adapter
            .list("")
            .unwrap()
            .into_iter()
            .map(|m| (m.reference.id, m.text))
            .collect();
        let topic_before = adapter.read("patients-stats.md#1").unwrap().text.clone();
        let mem_before = fs::read_to_string(dir.join("MEMORY.md")).unwrap();
        let (fm_before, _) = parse::split_frontmatter(&mem_before);

        adapter.annotate(&target_id, "[STALE pin]").unwrap();

        let after = adapter.list("").unwrap();
        for (id, text) in &before {
            if id == &target_id {
                continue;
            }
            let found = after.iter().find(|m| &m.reference.id == id).unwrap();
            assert_eq!(&found.text, text);
        }
        let topic_after = adapter.read("patients-stats.md#1").unwrap().text.clone();
        assert_eq!(topic_after, topic_before);
        let mem_after = fs::read_to_string(dir.join("MEMORY.md")).unwrap();
        let (fm_after, _) = parse::split_frontmatter(&mem_after);
        assert_eq!(fm_after, fm_before);
        let read = adapter.read(&target_id).unwrap();
        assert!(read.text.contains("[STALE pin]"));
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn supersede_pins_other_notes() {
        let _g = HOME_LOCK.lock().unwrap();
        let home = temp_dir("home");
        set_home(&home);
        let dir = temp_fixture();
        let adapter = MuseCodeAdapter::new(&dir);
        let first = adapter.list("").unwrap().into_iter().next().unwrap();
        let target_id = first.reference.id.clone();
        let before: Vec<(String, String)> = adapter
            .list("")
            .unwrap()
            .into_iter()
            .map(|m| (m.reference.id, m.text))
            .collect();
        let topic_before = adapter.read("patients-stats.md#1").unwrap().text.clone();
        let mem_before = fs::read_to_string(dir.join("MEMORY.md")).unwrap();
        let (fm_before, _) = parse::split_frontmatter(&mem_before);

        let correction = adapter.supersede(&target_id, "replacement text").unwrap();

        let after = adapter.list("").unwrap();
        for (id, text) in &before {
            if id == &target_id {
                continue;
            }
            let found = after.iter().find(|m| &m.reference.id == id).unwrap();
            assert_eq!(&found.text, text);
        }
        let topic_after = adapter.read("patients-stats.md#1").unwrap().text.clone();
        assert_eq!(topic_after, topic_before);
        let mem_after = fs::read_to_string(dir.join("MEMORY.md")).unwrap();
        let (fm_after, _) = parse::split_frontmatter(&mem_after);
        assert_eq!(fm_after, fm_before);
        let stale = adapter.read(&target_id).unwrap();
        assert!(stale.text.contains(&correction.id));
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&home);
    }

    fn failing_rename(src: &Path, dst: &Path) -> std::io::Result<()> {
        if dst.file_name().and_then(|s| s.to_str()) == Some("patients-stats.md") {
            Err(std::io::Error::other("boom"))
        } else {
            fs::rename(src, dst)
        }
    }

    #[test]
    fn supersede_interrupted_same_file_leaves_original() {
        let _g = HOME_LOCK.lock().unwrap();
        let home = temp_dir("home");
        set_home(&home);
        let dir = temp_fixture();
        let adapter = MuseCodeAdapter::new(&dir);
        let first = adapter.list("").unwrap().into_iter().next().unwrap();
        let id = first.reference.id.clone();
        let path = dir.join("MEMORY.md");
        let before = fs::read_to_string(&path).unwrap();

        // failing rename on any file
        set_rename_hook(Some(|_src, _dst| Err(std::io::Error::other("boom"))));
        let res = adapter.supersede(&id, "replacement text");
        assert!(res.is_err());
        set_rename_hook(None);

        let after = fs::read_to_string(&path).unwrap();
        assert_eq!(before, after);
        let tmp = dir.join(".MEMORY.md.doyle-tmp");
        let _ = fs::remove_file(&tmp);
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn supersede_split_retry_converges() {
        let _g = HOME_LOCK.lock().unwrap();
        let home = temp_dir("home");
        set_home(&home);
        let dir = temp_fixture();
        let adapter = MuseCodeAdapter::new(&dir);
        let target_id = "patients-stats.md#1".to_string();

        set_rename_hook(Some(failing_rename));
        let res = adapter.supersede(&target_id, "replacement stats");
        assert!(res.is_err());
        set_rename_hook(None);

        let retry = adapter.supersede(&target_id, "replacement stats").unwrap();
        let mem = fs::read_to_string(dir.join("MEMORY.md")).unwrap();
        let count = mem
            .lines()
            .filter(|l| l.contains("replacement stats"))
            .count();
        assert_eq!(count, 1, "exactly one correction after retry");
        let stats = fs::read_to_string(dir.join("patients-stats.md")).unwrap();
        assert!(stats.contains(&retry.id));
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&home);
    }
}
