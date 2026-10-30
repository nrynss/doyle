//! T3.2b live test: annotate a scratch repository's REAL Muse Code memory
//! folder through the adapter, then show a fresh `muse exec` session on
//! `muse-wonder-1.0` follows the annotation.
//!
//! Ignored by default; run with `cargo test -- --ignored live_`. Never runs
//! in CI, never gates. Budget: two `muse exec` sessions per run.
//!
//! The fact is deliberately unique to this test (the office cactus) so a
//! concurrent session's memory cannot answer the probe question. The
//! correction name appears nowhere except the annotation Doyle writes, so a
//! probe answer containing it proves the fresh session followed the
//! annotation.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use doyle_tms::adapters::MemoryAdapter;
use doyle_tms::adapters::muse_code::MuseCodeAdapter;

/// Original name the seed session saves; correction the annotation carries.
/// Fixed strings: the model echoes memory content verbatim, and cleanup
/// removes the project folder, so re-runs start clean.
const ORIG_NAME: &str = "Zebulon";
const NEW_NAME: &str = "Saffron";
const ENTITY: &str = "office cactus";

/// Removes scratch state on drop, including on test failure.
struct Cleanup {
    folders: Vec<PathBuf>,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        for folder in &self.folders {
            let _ = fs::remove_dir_all(folder);
        }
    }
}

fn scratch_repo() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("doyle-t32b-live-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    // A plain directory works (memory is keyed on the starting directory),
    // but a git repository mirrors a real project; failure is non-fatal.
    let _ = Command::new("git").args(["init", "-q"]).arg(&dir).output();
    dir
}

/// One headless Wonder session in `dir`. Panics with stderr on failure so
/// the transcript shows what happened.
fn muse_exec(prompt: &str, dir: &Path) -> String {
    let out = Command::new("timeout")
        .args([
            "280",
            "muse",
            "exec",
            "--model",
            "muse-wonder-1.0",
            "--max-model-steps",
            "6",
            "--trust-workspace",
            prompt,
        ])
        .current_dir(dir)
        .output()
        .expect("spawn `timeout muse exec`; is `muse` on PATH?");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    println!("--- muse exec prompt ---\n{prompt}");
    println!("--- muse exec stdout ---\n{stdout}");
    println!("--- muse exec stderr (tail) ---\n{}", tail(&stderr, 10));
    assert!(
        out.status.success(),
        "muse exec failed with status {}",
        out.status
    );
    stdout
}

fn tail(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

/// The real per-project folder for `start_dir`, by the hash-suffix rule
/// (docs/muse-code.md section 1.2). `None` if no session created one.
fn project_folder_for(start_dir: &Path) -> Option<PathBuf> {
    let canon = start_dir
        .canonicalize()
        .unwrap_or_else(|_| start_dir.to_path_buf());
    let path_str = canon.to_string_lossy().to_string();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in path_str.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let suffix = format!("-{h:016x}");
    let home = std::env::var("HOME").ok()?;
    let base = PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("muse")
        .join("memory")
        .join("projects");
    fs::read_dir(&base).ok()?.flatten().find_map(|e| {
        let name = e.file_name().to_string_lossy().to_string();
        (name.ends_with(&suffix) && e.path().is_dir()).then(|| e.path())
    })
}

#[test]
#[ignore]
fn live_muse_code_annotate_guides_fresh_session() {
    let scratch = scratch_repo();
    let mut cleanup = Cleanup {
        folders: vec![scratch.clone()],
    };

    // Session 1 of 2: seed the fact. The scope instruction is load-bearing:
    // scope `personal` lands in the global `memory/personal/` folder, while
    // this test must exercise the per-project folder keyed on the starting
    // directory (docs/muse-code.md section 1.2).
    let seed_prompt = format!(
        "Remember this fact: the {ENTITY} is named {ORIG_NAME}. \
         Save it to memory with scope personal_project."
    );
    let seed_out = muse_exec(&seed_prompt, &scratch);
    assert!(
        seed_out.contains(ORIG_NAME),
        "seed session did not confirm the save; got: {seed_out}"
    );

    // Register the real project folder for cleanup right away.
    if let Some(folder) = project_folder_for(&scratch) {
        println!("project folder: {}", folder.display());
        cleanup.folders.push(folder);
    }

    // Through the adapter: find the seeded note in the real per-project
    // folder and annotate it in place.
    let adapter = MuseCodeAdapter::new(&scratch);
    let seeded = adapter
        .list("")
        .expect("adapter list over seeded memory")
        .into_iter()
        .find(|m| m.scope == "personal_project" && m.text.contains(ORIG_NAME))
        .expect("seeded note in the per-project folder (wrong scope or missing?)");
    println!(
        "seeded note: id={} scope={} text={:?}",
        seeded.reference.id, seeded.scope, seeded.text
    );
    let warning = format!("[STALE per Doyle: the cactus is now named {NEW_NAME}.]");
    adapter
        .annotate(&seeded.reference.id, &warning)
        .expect("adapter annotate");
    let read_back = adapter.read(&seeded.reference.id).expect("read back");
    println!("annotated note: {:?}", read_back.text);
    assert!(
        read_back.text.contains(NEW_NAME),
        "annotation missing after write"
    );
    drop(adapter);

    // Session 2 of 2: a fresh session answers the question the note answers.
    let probe_prompt = format!("What is the {ENTITY} named? Reply with just the name.");
    let probe_out = muse_exec(&probe_prompt, &scratch);
    println!(
        "probe mentions {ORIG_NAME}: {}",
        probe_out.contains(ORIG_NAME)
    );

    assert!(
        probe_out.to_lowercase().contains(&NEW_NAME.to_lowercase()),
        "fresh session did not follow the annotation; got: {probe_out}"
    );
}
