//! `doyle screens read` through the built binary, keyless, replayed from the
//! committed live recordings in `testdata/replies/vision/` (T9.16).
//!
//! v1 reads the lookup and decommission stills plus the training recording,
//! v2 the two v2 stills. `doyle check` then diffs the two written files:
//! the decommission change flags S1.
//!
//! Known limit, pinned rather than hidden: the recordings are single live
//! reads, and a single read of the lookup screen wobbles (column drift, a
//! split or merged line, the dashes rule, `FWV-` read as `FW-`), which the
//! differ turns into a lookup fact. Across the live runs made for T9.16 no
//! v1/v2 pair read the lookup screen identically, so S2 flags here too. The
//! test asserts S1 flags and that nothing but the two seeded memories can;
//! it does not claim lookup silence.

use std::path::PathBuf;
use std::process::{Command, Output};

use doyle_tms::core::{Citation, MemoryRef, Store};
use doyle_tms::evidence::screens::ids as screen_ids;

fn story(rel: &str) -> String {
    format!("{}/testdata/legacy/story/{rel}", env!("CARGO_MANIFEST_DIR"))
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("doyle-screens-read-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn doyle(args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_doyle"));
    command
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env_remove("DOYLE_REPLAY_DIR")
        .env_remove("DOYLE_RECORD_DIR")
        .env_remove("MODEL_API_KEY");
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("run doyle")
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

fn duration_s() -> String {
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(story("training-v1-steps.json")).unwrap())
            .unwrap();
    value["duration_s"].as_f64().unwrap().to_string()
}

fn read_v1(out_file: &str, envs: &[(&str, &str)]) -> Output {
    let lookup = format!("lookup={}", story("screens/lookup-v1.png"));
    let decommission = format!("decommission={}", story("screens/decomm-v1.png"));
    let recording = format!("training-v1={}", story("training-v1.mp4"));
    doyle(
        &[
            "screens",
            "read",
            "--console",
            "fernwhistle",
            "--version",
            "v1",
            "--screen",
            &lookup,
            "--screen",
            &decommission,
            "--recording",
            &recording,
            "--duration-s",
            &duration_s(),
            "-o",
            out_file,
        ],
        envs,
    )
}

fn read_v2(out_file: &str) -> Output {
    let lookup = format!("lookup={}", story("screens/lookup-v2.png"));
    let decommission = format!("decommission={}", story("screens/decomm-v2.png"));
    doyle(
        &[
            "screens",
            "read",
            "--console",
            "fernwhistle",
            "--version",
            "v2",
            "--screen",
            &lookup,
            "--screen",
            &decommission,
            "-o",
            out_file,
        ],
        &[],
    )
}

fn json(path: &str) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn replayed_reads_diff_to_the_decommission_change_alone() {
    let root = temp_dir("diff");
    let v1 = root.join("v1.json").to_string_lossy().to_string();
    let v2 = root.join("v2.json").to_string_lossy().to_string();

    // Default replay dir: testdata/replies/vision, no key, no network.
    let out = read_v1(&v1, &[]);
    assert!(out.status.success(), "v1 reads: {}", stderr(&out));
    let err = stderr(&out);
    assert!(err.contains("stated cost"), "cost stated first: {err}");
    assert!(err.contains("usage "), "usage reported: {err}");
    assert!(err.contains("wrote"), "summary printed: {err}");
    let out = read_v2(&v2);
    assert!(out.status.success(), "v2 reads: {}", stderr(&out));

    let v1_doc = json(&v1);
    assert_eq!(v1_doc["console"], "fernwhistle");
    assert_eq!(v1_doc["screens"].as_array().unwrap().len(), 2);
    assert_eq!(v1_doc["recording"]["name"], "training-v1");
    assert_eq!(v1_doc["recording"]["duration_s"], 45.0);
    assert!(
        !v1_doc["steps"].as_array().unwrap().is_empty(),
        "the recording gave steps"
    );
    let v2_doc = json(&v2);
    assert_eq!(v2_doc["screens"].as_array().unwrap().len(), 2);
    assert!(v2_doc["recording"].is_null(), "v2 has no recording");

    // Seed S1 (decommission) and S2 (lookup) and let check diff the files.
    let state = root.join("state");
    {
        let mut store = Store::open(&state).unwrap();
        for (memory, screen) in [("S1", "decommission"), ("S2", "lookup")] {
            store
                .record_citation(Citation {
                    memory: MemoryRef {
                        adapter: "muse-code".to_string(),
                        id: memory.to_string(),
                    },
                    elements: vec![screen_ids::screen_id("fernwhistle", screen)],
                    claim: format!("memory {memory} rests on the {screen} screen"),
                    recorded_by: "test-curator".to_string(),
                    confidence: 0.9,
                    source_text: format!("memory {memory}"),
                })
                .unwrap();
        }
    }
    let out = doyle(
        &[
            "check",
            &v1,
            &v2,
            "--state-dir",
            &state.to_string_lossy(),
            "--format",
            "json",
            "--no-recheck",
        ],
        &[],
    );
    assert_eq!(out.status.code(), Some(1), "check flags: {}", stderr(&out));
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let flagged: Vec<&str> = report["flags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["memory"]["id"].as_str().unwrap())
        .collect();
    assert!(flagged.contains(&"S1"), "the decommission change flags S1");
    assert!(
        flagged.iter().all(|m| ["S1", "S2"].contains(m)),
        "only the seeded memories flag: {flagged:?}"
    );
    // The S1 flag is the breaking decommission fact, not lookup wobble.
    let s1 = report["flags"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["memory"]["id"] == "S1")
        .unwrap();
    assert_eq!(s1["severity"], "breaking", "S1 is breaking: {s1}");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_replay_miss_is_loud_and_writes_nothing() {
    let root = temp_dir("miss");
    let empty = root.join("empty-replies");
    std::fs::create_dir_all(&empty).unwrap();
    let out_file = root.join("v1.json").to_string_lossy().to_string();
    let out = read_v1(&out_file, &[("DOYLE_REPLAY_DIR", &empty.to_string_lossy())]);
    assert_eq!(out.status.code(), Some(2), "a miss fails: {}", stderr(&out));
    let err = stderr(&out);
    assert!(
        err.contains("9c36c6a78111e423"),
        "names the request key: {err}"
    );
    assert!(
        !PathBuf::from(&out_file).exists(),
        "no silent empty file on a miss"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn bad_arguments_exit_2() {
    let out = doyle(&["screens", "read", "--console", "c"], &[]);
    assert_eq!(out.status.code(), Some(2));
    let out = doyle(
        &[
            "screens",
            "read",
            "--console",
            "c",
            "--version",
            "v",
            "--screen",
            "x=/nope.png",
            "-o",
            "/tmp/never.json",
        ],
        &[],
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("/nope.png"), "{}", stderr(&out));
}

#[test]
fn help_documents_the_command() {
    let out = doyle(&["screens", "--help"], &[]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    for needle in [
        "--screen",
        "--recording",
        "--duration-s",
        "--live",
        "DOYLE_REPLAY_DIR",
    ] {
        assert!(text.contains(needle), "help names {needle}");
    }
    let top = doyle(&["--help"], &[]);
    assert!(String::from_utf8_lossy(&top.stdout).contains("screens"));
}
