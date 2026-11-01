//! Parsing Muse Code memory files: frontmatter, inline notes, index lines.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::core::{Memory, MemoryRef};

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
    pub ordinal: usize,
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

/// True when `content` opens with a complete `---` ... `---` frontmatter block.
pub fn has_frontmatter(content: &str) -> bool {
    if !content.starts_with("---") {
        return false;
    }
    match content.find('\n') {
        Some(i) => content[i + 1..].contains("\n---"),
        None => false,
    }
}

/// Ordinal of a topic file's whole-body note. Bullets count from 1, so this
/// never collides with a bullet id.
pub const BODY_ORDINAL: usize = 0;

const INDEX: &str = "MEMORY.md";

/// Top-level `*.md` topic files that carry frontmatter, sorted by name.
/// Skips dotfiles (Muse's lock, Doyle's temp files), `MEMORY.md` itself, and
/// anything that is not a regular `.md` file.
pub fn topic_files(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = match fs::read_dir(folder) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.ends_with(".md") && !n.starts_with('.') && n != INDEX)
            .filter(|n| {
                fs::read_to_string(folder.join(n))
                    .map(|c| has_frontmatter(&c))
                    .unwrap_or(false)
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    names
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
    let mut ordinal = 0usize;
    for line in body.lines() {
        let trimmed = line.trim();
        let bullet = match trimmed.strip_prefix("- ") {
            Some(b) => b.trim(),
            None => continue,
        };
        if bullet.is_empty() {
            continue;
        }
        ordinal += 1;
        if let Some(target) = index_target(bullet) {
            // Resolve relative to the entry's directory.
            let target_rel = if let Some(parent) = Path::new(entry_rel).parent() {
                if parent == Path::new("") {
                    target.clone()
                } else {
                    parent.join(&target).to_string_lossy().to_string()
                }
            } else {
                target.clone()
            };
            let sub = collect_notes(root, &target_rel, seen);
            notes.extend(sub);
        } else {
            notes.push(Note {
                text: bullet.to_string(),
                frontmatter: fm.clone(),
                source: entry_rel.to_string(),
                ordinal,
            });
        }
    }
    // A topic file (not the index) with frontmatter and no bullets is one note:
    // its whole body, with an id derived from the file name alone.
    if ordinal == 0 && entry_rel != INDEX && has_frontmatter(&content) {
        let text = body.trim();
        if !text.is_empty() {
            notes.push(Note {
                text: text.to_string(),
                frontmatter: fm,
                source: entry_rel.to_string(),
                ordinal: BODY_ORDINAL,
            });
        }
    }
    notes
}

/// Stable id for a note: source file + ordinal position within that file.
/// The ordinal is positional, not content-derived, so the id survives both
/// unrelated appends and in-place annotation of the note itself, and two
/// notes with identical text never collide.
pub fn note_id(source: &str, ordinal: usize) -> String {
    format!("{}#{}", source, ordinal)
}

/// Read all notes from a memory folder into Memories.
pub fn read_folder(folder: &Path, scope: &str, adapter_name: &str) -> Vec<Memory> {
    let mut seen = HashSet::new();
    let mut notes = Vec::new();
    if folder.join(INDEX).is_file() {
        notes = collect_notes(folder, INDEX, &mut seen);
    }
    for name in topic_files(folder) {
        if !seen.contains(&name) {
            notes.extend(collect_notes(folder, &name, &mut seen));
        }
    }
    notes
        .into_iter()
        .map(|n| Memory {
            reference: MemoryRef {
                adapter: adapter_name.to_string(),
                id: note_id(&n.source, n.ordinal),
            },
            text: n.text,
            updated_at: None,
            scope: scope.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn split_frontmatter_memory_md() {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let content =
            fs::read_to_string(base.join("testdata/memories/muse-code/MEMORY.md")).unwrap();
        let (fm, body) = split_frontmatter(&content);
        assert_eq!(fm.note_type.as_deref(), Some("reference"));
        assert_eq!(
            fm.description.as_deref(),
            Some("Fernwhistle patient records API")
        );
        assert!(body.contains("GET /patients"));
        assert!(!body.trim_start().starts_with("---"));
    }

    #[test]
    fn split_frontmatter_patients_stats() {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let content =
            fs::read_to_string(base.join("testdata/memories/muse-code/patients-stats.md")).unwrap();
        let (fm, body) = split_frontmatter(&content);
        assert_eq!(fm.note_type.as_deref(), Some("reference"));
        assert_eq!(
            fm.description.as_deref(),
            Some("Fernwhistle patient records stats endpoint")
        );
        assert!(body.contains("GET /stats"));
    }
}
