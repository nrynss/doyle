//! Locating a repository's Muse Code memory scopes.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// FNV-1a 64-bit over the UTF-8 bytes of `s`, no terminator, no trailing slash.
pub fn fnv1a64(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Resolve symlinks and drop a trailing slash.
pub fn canonical_dir(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

/// Slug: every non-alphanumeric char becomes '-', leading '-' dropped, cut to 96.
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else {
            out.push('-');
        }
    }
    let out = out.trim_start_matches('-').to_string();
    if out.len() > 96 {
        out[..96].to_string()
    } else {
        out
    }
}

/// Find the memory scopes for the directory `start_dir` is started in.
///
/// Returns `(folder, scope_label)` pairs:
/// - the private per-project folder under `~/.local/share/muse/memory/projects/`
///   whose name ends in `-<16 hex hash>` of the canonicalised start dir;
/// - the shared `<start_dir>/.agents/memory/` folder, if it exists.
pub fn locate(start_dir: &Path) -> Vec<(PathBuf, String)> {
    let mut result = Vec::new();
    let canon = canonical_dir(start_dir);
    let path_str = canon.to_string_lossy();
    let hash = fnv1a64(&path_str);

    // Private per-project folder: match on the hash suffix alone.
    if let Ok(home) = env::var("HOME") {
        let base = PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("muse")
            .join("memory")
            .join("projects");
        if let Ok(entries) = fs::read_dir(&base) {
            let suffix = format!("-{:016x}", hash);
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.ends_with(&suffix) && entry.path().is_dir() {
                    result.push((entry.path(), "personal_project".to_string()));
                    break;
                }
            }
        }
    }

    // Shared folder.
    let shared = start_dir.join(".agents").join("memory");
    if shared.is_dir() {
        result.push((shared, "shared".to_string()));
    }

    result
}
