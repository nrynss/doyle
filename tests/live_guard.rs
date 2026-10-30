//! T6.1b (live): the guard denies a flagged action in a real Wonder session.
//!
//! Scratch repository only, never a real project. The test seeds one open
//! flag in `<scratch>/.doyle` (the demo's M1 citation, flagged through the
//! real `doyle cite` + `doyle check --no-recheck` path, so no model call is
//! spent on setup), registers the guard hooks in `<scratch>/.muse/hooks.json`
//! with the hook command `/path/to/doyle guard --state-dir
//! /path/to/repo/.doyle`, then runs `muse exec` on `muse-wonder-1.0` from the
//! scratch root (workspace trusted) attempting the flagged export and asserts
//! the session sees the denial with the reason.
//!
//! Live: needs the `muse` CLI with stored credentials and network. The test
//! spends one `muse exec` session per run (budget: three). It is `#[ignore]`d
//! and named `live_*`, so it runs only under `cargo test -- --ignored live_`,
//! never in CI, and never gates the build. Only project hooks inside the
//! scratch repository are registered; `~/.config/muse/settings.json` and
//! `trust.json` are checksummed before and after and must be byte-identical.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use doyle_tms::core::Store;

/// The flagged action the session attempts: the demo's one-call export
/// against `GET /patients`, denied while M1's flag is open.
const FLAGGED_COMMAND: &str = "curl -s http://127.0.0.1:9/patients";

/// What the denial must carry: the agent-visible prefix, the guard marker,
/// the flagged memory, what changed, and the review page.
const EXPECTED: &[&str] = &[
    "tool blocked by hook:",
    "DOYLE-GUARD:",
    "muse-code/M1",
    "GET /patients",
    "doyle serve",
];

#[test]
#[ignore]
fn live_guard_denies_flagged_export_in_wonder_session() {
    let muse = require_muse();
    let before = global_settings_snapshot();

    let scratch = scratch_dir();
    reset_dir(&scratch);
    seed_flag(&scratch);
    register_hooks(&scratch);

    let output = run_wonder(&muse, &scratch);

    for needle in EXPECTED {
        assert!(
            output.contains(needle),
            "live session must see the denial ({needle}); session output:\n{output}"
        );
    }

    let after = global_settings_snapshot();
    assert_eq!(
        before, after,
        "live session must leave global Muse settings byte-identical"
    );

    let _ = std::fs::remove_dir_all(&scratch);
}

/// The `muse` CLI, or a clear failure when it is not installed.
fn require_muse() -> PathBuf {
    let which = Command::new("which").arg("muse").output();
    match which {
        Ok(out) if out.status.success() => {
            PathBuf::from(String::from_utf8_lossy(&out.stdout).trim())
        }
        _ => panic!("live test needs the `muse` CLI on PATH"),
    }
}

/// Fresh scratch repository for this run.
fn scratch_dir() -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!("doyle-live-guard-{}", std::process::id()));
    dir
}

fn reset_dir(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir.join("contracts")).expect("create scratch contracts dir");
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    for name in ["patients-v1.yaml", "patients-v2.yaml"] {
        std::fs::copy(
            manifest.join("testdata/contracts").join(name),
            dir.join("contracts").join(name),
        )
        .expect("copy fixture contract into scratch repo");
    }
    let init = Command::new("git")
        .arg("init")
        .arg("-q")
        .arg(dir)
        .output()
        .expect("run git init");
    assert!(init.status.success(), "git init the scratch repo");
}

/// Seed one open flag through the real binaries: `cite` records M1 against
/// the old contract, `check --no-recheck` flags it against the new one with
/// no model call. Relative contract paths are recorded, so they resolve
/// against the hook's workspace root exactly as `docs/guard.md` requires.
fn seed_flag(scratch: &Path) {
    let doyle = PathBuf::from(env!("CARGO_BIN_EXE_doyle"));
    let state_dir = scratch.join(".doyle").to_string_lossy().to_string();

    let spec = serde_json::json!({
        "memory": {"adapter": "muse-code", "id": "M1"},
        "contract": "contracts/patients-v1.yaml",
        "elements": ["GET /patients"],
        "claim": "GET /patients returns every patient in one response",
    });
    let mut cite = Command::new(&doyle)
        .arg("cite")
        .arg("--state-dir")
        .arg(&state_dir)
        .current_dir(scratch)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn doyle cite");
    cite.stdin
        .as_mut()
        .expect("cite stdin")
        .write_all(spec.to_string().as_bytes())
        .expect("write cite spec");
    let cite = cite.wait_with_output().expect("wait for doyle cite");
    assert!(
        cite.status.success(),
        "doyle cite records M1: {}",
        String::from_utf8_lossy(&cite.stderr)
    );

    let check = Command::new(&doyle)
        .arg("check")
        .arg("contracts/patients-v1.yaml")
        .arg("contracts/patients-v2.yaml")
        .arg("--no-recheck")
        .arg("--state-dir")
        .arg(&state_dir)
        .current_dir(scratch)
        .output()
        .expect("run doyle check");
    let report = String::from_utf8_lossy(&check.stdout).to_string();
    assert!(
        report.contains("muse-code") && report.contains("M1"),
        "doyle check flags M1: {report}"
    );

    let store = Store::open(scratch.join(".doyle")).expect("open seeded store");
    let open = store.open_flags();
    assert_eq!(open.len(), 1, "exactly one open flag is seeded");
    assert_eq!(open[0].memory.id, "M1");
}

/// Register the guard hooks for the scratch project: one command serves both
/// events, with the absolute binary path and an absolute `--state-dir`, run
/// from the repository root with a trusted workspace.
fn register_hooks(scratch: &Path) {
    let doyle = PathBuf::from(env!("CARGO_BIN_EXE_doyle"));
    let command = format!(
        "{} guard --state-dir {}",
        doyle.display(),
        scratch.join(".doyle").display()
    );
    let hooks = serde_json::json!({
        "hooks": {
            "SessionStart": [{"hooks": [{"type": "command", "command": command}]}],
            "PreToolUse": [{"matcher": "*", "hooks": [{"type": "command", "command": command}]}],
        }
    });
    std::fs::create_dir_all(scratch.join(".muse")).expect("create scratch .muse dir");
    std::fs::write(
        scratch.join(".muse/hooks.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&hooks).expect("render hooks")
        ),
    )
    .expect("write scratch hooks.json");
}

/// One Wonder session attempting the flagged export, run from the scratch
/// root with a trusted workspace. Killed after five minutes so a stuck run
/// fails the test instead of hanging the runner.
fn run_wonder(muse: &Path, scratch: &Path) -> String {
    let prompt = format!(
        "Run the shell command: {FLAGGED_COMMAND} -- then quote verbatim exactly what the tool told you back."
    );
    let mut child = Command::new(muse)
        .arg("exec")
        .arg("--model")
        .arg("muse-wonder-1.0")
        .arg("--max-model-steps")
        .arg("6")
        .arg("--trust-workspace")
        .arg(prompt)
        .current_dir(scratch)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn muse exec");

    let deadline = Instant::now() + Duration::from_secs(280);
    loop {
        match child.try_wait().expect("poll muse exec") {
            Some(_) => break,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                panic!("muse exec did not finish within five minutes");
            }
            None => std::thread::sleep(Duration::from_secs(2)),
        }
    }
    let out = child.wait_with_output().expect("wait for muse exec");
    let mut combined = String::from_utf8_lossy(&out.stdout).to_string();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    combined
}

/// Byte snapshots of the global Muse settings files the test must not touch.
/// Only project hooks inside the scratch repository are registered.
fn global_settings_snapshot() -> Vec<(String, Vec<u8>)> {
    let home = std::env::var("HOME").unwrap_or_default();
    ["settings.json", "trust.json"]
        .iter()
        .map(|name| {
            let path = format!("{home}/.config/muse/{name}");
            let bytes = std::fs::read(&path).unwrap_or_default();
            (path, bytes)
        })
        .collect()
}
