//! Parsing Muse Code memory files: frontmatter, inline notes, index lines.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::core::{Memory, MemoryRef};

use super::locate::fnv1a64;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Frontmatter {
    pub note_type: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Note {
    pub text: String,
    pub frontmatter: Frontmatter,
    pub source: String,
}

/// Split leading `---\n...\n---` frontmatter from the body.
pub fn split_frontmatter(content: &str) -> (Frontmatter, String) {
    let mut fm = Frontmatter::default();
    if !content.starts_with("---") {
        return (fm, content.to_string());
    }
    let after_first = match content.find('\n') {
        Some(i) => &content[i + 1..],
        None => return (fm, content.to_string()),
    };
    match after_first.find("\n---") {
        Some(idx) => {
            let block = &after_first[..idx];
            let body = &after_first[idx + 4..];
            // skip a leading newline after the closing fence
            let body = body.strip_prefix('\n').unwrap_or(body);
            for line in block.lines() {
                let line = line.trim();
                if let Some((k, v)) = line.split_once(':') {
                    let k = k.trim();
                    let v = v.trim().trim_matches('"').trim_matches('\'');
                    match k {
                        "type" => fm.note_type = Some(v.to_string()),
                        "description" => fm.description = Some(v.to_string()),
                        _ => {}
                    }
                }
            }
            (fm, body.to_string())
        }
        None => (fm, content.to_string()),
    }
}

/// If `content` (the text after "- ") is an index line pointing at a .md file,
/// return that target path.
pub fn index_target(content: &str) -> Option<String> {
    let content = content.trim();
    if !content.starts_with('[') {
        return None;
    }
    let close_bracket = content.find(']')?;
    let rest = &content[close_bracket..];
    let open_paren = rest.find('(')?;
    let close_paren = rest.find(')')?;
    let target = &rest[open_paren + 1..close_paren];
    if target.ends_with(".md") {
        Some(target.to_string())
    } else {
        None
    }
}

/// Collect notes from `entry_rel` (relative to `root`), following index lines.
/// `seen` prevents infinite loops.
pub fn collect_notes(root: &Path, entry_rel: &str, seen: &mut HashSet<String>) -> Vec<Note> {
    if !seen.insert(entry_rel.to_string()) {
        return Vec::new();
    }
    let full = root.join(entry_rel);
    let content = match fs::read_to_string(&full) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let (fm, body) = split_frontmatter(&content);
    let mut notes = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        let bullet = match trimmed.strip_prefix("- ") {
            Some(b) => b.trim(),
            None => continue,
        };
        if bullet.is_empty() {
            continue;
        }
        if let Some(target) = index_target(bullet) {
            // Resolve relative to the entry's directory.
            let base = full.parent().unwrap_or(root);
            let target_rel = if let Some(parent) = Path::new(entry_rel).parent() {
                if parent == Path::new("") {
                    target.clone()
                } else {
                    parent.join(&target).to_string_lossy().to_string()
                }
            } else {
                target.clone()
            };
            let _ = base;
            let sub = collect_notes(root, &target_rel, seen);
            notes.extend(sub);
        } else {
            notes.push(Note {
                text: bullet.to_string(),
                frontmatter: fm.clone(),
                source: entry_rel.to_string(),
            });
        }
    }
    notes
}

/// Stable id for a note: source file + short hash of its text.
pub fn note_id(source: &str, text: &str) -> String {
    format!("{}#{:016x}", source, fnv1a64(text.trim()))
}

/// Read all notes from a memory folder into Memories.
pub fn read_folder(folder: &Path, scope: &str, adapter_name: &str) -> Vec<Memory> {
    let entry = "MEMORY.md";
    let full = folder.join(entry);
    if !full.is_file() {
        return Vec::new();
    }
    let mut seen = HashSet::new();
    let notes = collect_notes(folder, entry, &mut seen);
    notes
        .into_iter()
        .map(|n| Memory {
            reference: MemoryRef {
                adapter: adapter_name.to_string(),
                id: note_id(&n.source, &n.text),
            },
            text: n.text,
            updated_at: None,
            scope: scope.to_string(),
        })
        .collect()
}

/// Expose PathBuf for callers.
pub fn folder_path(folder: &Path) -> PathBuf {
    folder.to_path_buf()
}
