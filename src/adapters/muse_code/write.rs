//! Muse Code write side: new notes, in-place annotation, supersede.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::adapters::muse_code::MuseCodeAdapter;
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

fn note_text_at(folder: &Path, source: &str, ordinal: usize) -> Option<String> {
    let full = folder.join(source);
    let content = fs::read_to_string(&full).ok()?;
    // split frontmatter
    let body = if content.starts_with("---") {
        // find closing fence
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
            let _ = rest;
            count += 1;
            if count == ordinal {
                // return the bullet content (after "- ")
                let trimmed = line.trim();
                let bullet = trimmed.strip_prefix("- ").unwrap_or("").trim();
                return Some(bullet.to_string());
            }
        }
    }
    None
}

fn count_bullets(content: &str) -> usize {
    // count bullets in body
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
    body.lines().filter(|l| l.trim().starts_with("- ")).count()
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
        let fm = "---\ntype: reference\n---\n\n";
        atomic_write(&path, fm, adapter.name())?;
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
    // record scope
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
    let mut lines: Vec<String> = content
        .split('\n')
        .collect::<Vec<_>>()
        .into_iter()
        .map(String::from)
        .collect();
    let mut count = 0usize;
    let mut found = false;
    for line in lines.iter_mut() {
        let t = line.trim();
        if t.starts_with("- ") {
            count += 1;
            if count == ordinal {
                let trimmed = line.trim_end().to_string();
                *line = format!("{} {}", trimmed, warning);
                found = true;
                break;
            }
        }
    }
    if !found {
        return Err(AdapterError::not_found(adapter.name(), id));
    }
    let new_content = lines.join("\n");
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
    // find existing correction id in stale text
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

    // Create correction note in MEMORY.md of first scope.
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
    let path = first_folder.join("MEMORY.md");
    if !path.is_file() {
        let fm = "---\ntype: reference\n---\n\n";
        atomic_write(&path, fm, adapter.name())?;
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
    new_content.push_str(&format!("- {}\n", correction_text));
    atomic_write(&path, &new_content, adapter.name())?;
    let correction_id = format!("MEMORY.md#{}", n);

    // Record scope for correction = stale's scope (if recorded) else folder label.
    let stale_scope = adapter
        .state
        .borrow()
        .scopes
        .get(stale_id)
        .cloned()
        .unwrap_or_else(|| {
            // folder label for stale's folder
            adapter
                .scopes
                .iter()
                .find(|(f, _)| f == &folder)
                .map(|(_, l)| l.clone())
                .unwrap_or_default()
        });
    adapter
        .state
        .borrow_mut()
        .scopes
        .insert(correction_id.clone(), stale_scope);

    // Link stale note to correction.
    let stale_path = folder.join(&source);
    let stale_content = fs::read_to_string(&stale_path).map_err(|e| AdapterError::Backend {
        adapter: adapter.name().to_string(),
        memory: Some(stale_id.to_string()),
        message: format!("read memory file: {e}"),
    })?;
    let mut lines: Vec<String> = stale_content.split('\n').map(String::from).collect();
    let mut count = 0usize;
    let mut found = false;
    for line in lines.iter_mut() {
        let t = line.trim();
        if t.starts_with("- ") {
            count += 1;
            if count == ordinal {
                let trimmed = line.trim_end().to_string();
                *line = format!("{} [Superseded by {}]", trimmed, correction_id);
                found = true;
                break;
            }
        }
    }
    if !found {
        return Err(AdapterError::not_found(adapter.name(), stale_id));
    }
    let new_stale = lines.join("\n");
    atomic_write(&stale_path, &new_stale, adapter.name())?;

    Ok(MemoryRef {
        adapter: adapter.name().to_string(),
        id: correction_id,
    })
}
