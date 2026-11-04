//! Live test for `doyle screens read` (T9.16): reads the T9.4 console's v1
//! and v2 through the built binary against `muse-wonder-1.0`, with
//! `DOYLE_RECORD_DIR` pointing at `testdata/replies/vision/` so the two
//! replies become the committed recordings `tests/screens_read.rs` replays.
//!
//! `#[ignore]`d: it calls the real API, needs a key and never runs in CI or
//! the gate. Run it explicitly:
//!
//! ```sh
//! cargo test --test live_screens_read -- --ignored live_ --nocapture
//! ```
//!
//! The key comes from `MODEL_API_KEY` or the keyring (the binary's own
//! loader); it is never printed, and the recordings are checked for it
//! before the test passes. The transcript (stated cost, usage, warnings,
//! file shapes, no key) goes to stderr.
//!
//! Budget: $1.00 for the whole test, on the T9.2 basis (~$15 per million
//! tokens, input plus output; the API returns no cost field). v2, the
//! cheaper call, goes first; its usage is checked before v1 is spent.

use std::process::{Command, Output};

const BUDGET_USD: f64 = 1.00;
/// Upper bound for the v1 call (T9.7b measured ~$0.32), checked before it
/// is spent.
const V1_ESTIMATE_USD: f64 = 0.35;

fn story(rel: &str) -> String {
    format!("{}/testdata/legacy/story/{rel}", env!("CARGO_MANIFEST_DIR"))
}

fn usd(stderr: &str) -> f64 {
    // "usage N input / M output tokens (request K)"
    let line = stderr
        .lines()
        .find(|l| l.contains("usage "))
        .expect("stderr states the usage");
    let numbers: Vec<f64> = line
        .split_whitespace()
        .filter_map(|w| w.parse::<f64>().ok())
        .collect();
    assert!(numbers.len() >= 2, "usage line carries both counts: {line}");
    (numbers[0] + numbers[1]) / 1_000_000.0 * 15.0
}

fn transcript(label: &str, out: &Output) -> String {
    let err = String::from_utf8_lossy(&out.stderr).to_string();
    eprintln!("--- {label} (exit {:?})\n{err}", out.status.code());
    err
}

#[test]
#[ignore = "calls the real Meta Model API; run with --ignored live_"]
fn live_screens_read_v2_then_v1_records_the_replay_fixtures() {
    let record_dir = format!("{}/testdata/replies/vision", env!("CARGO_MANIFEST_DIR"));
    let out_dir = std::env::temp_dir().join(format!("doyle-live-screens-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out_dir);
    std::fs::create_dir_all(&out_dir).unwrap();
    let v1_file = out_dir.join("v1.json").to_string_lossy().to_string();
    let v2_file = out_dir.join("v2.json").to_string_lossy().to_string();

    let read = |args: &[String]| -> Output {
        Command::new(env!("CARGO_BIN_EXE_doyle"))
            .args(["screens", "read", "--console", "fernwhistle", "--live"])
            .args(args)
            .env("DOYLE_RECORD_DIR", &record_dir)
            .output()
            .expect("run doyle")
    };

    let out = read(&[
        "--version".into(),
        "v2".into(),
        "--screen".into(),
        format!("lookup={}", story("screens/lookup-v2.png")),
        "--screen".into(),
        format!("decommission={}", story("screens/decomm-v2.png")),
        "-o".into(),
        v2_file.clone(),
    ]);
    let err = transcript("v2", &out);
    assert!(out.status.success(), "v2 read succeeds");
    let spent = usd(&err);
    eprintln!("v2 spend ~${spent:.3}");
    assert!(
        spent + V1_ESTIMATE_USD <= BUDGET_USD,
        "v1 would take the total over ${BUDGET_USD}: stop here"
    );

    let steps: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(story("training-v1-steps.json")).unwrap())
            .unwrap();
    let out = read(&[
        "--version".into(),
        "v1".into(),
        "--screen".into(),
        format!("lookup={}", story("screens/lookup-v1.png")),
        "--screen".into(),
        format!("decommission={}", story("screens/decomm-v1.png")),
        "--recording".into(),
        format!("training-v1={}", story("training-v1.mp4")),
        "--duration-s".into(),
        steps["duration_s"].as_f64().unwrap().to_string(),
        "-o".into(),
        v1_file.clone(),
    ]);
    let err = transcript("v1", &out);
    assert!(out.status.success(), "v1 read succeeds");
    let total = spent + usd(&err);
    eprintln!("total spend ~${total:.3} of ${BUDGET_USD}");
    assert!(total <= BUDGET_USD, "total stays inside the budget");

    for (label, path) in [("v1", &v1_file), ("v2", &v2_file)] {
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(value["screens"].as_array().unwrap().len(), 2);
        eprintln!(
            "{label}: {} steps, recording {}",
            value["steps"].as_array().unwrap().len(),
            value["recording"]
        );
    }
    let v1: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&v1_file).unwrap()).unwrap();
    assert_eq!(v1["recording"]["name"], "training-v1");

    // No key in the recordings.
    for entry in std::fs::read_dir(&record_dir).unwrap() {
        let text = std::fs::read_to_string(entry.unwrap().path()).unwrap();
        let lower = text.to_lowercase();
        assert!(
            !lower.contains("authorization") && !lower.contains("bearer "),
            "a recording carries a credential header"
        );
        if let Ok(key) = std::env::var("MODEL_API_KEY")
            && !key.is_empty()
        {
            assert!(!text.contains(&key), "a recording carries the key");
        }
    }
    let _ = std::fs::remove_dir_all(&out_dir);
}
