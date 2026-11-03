//! End-to-end proof for T9.11: the built binary reproduces the story's
//! expected-flag table (`testdata/legacy/story/README.md`) and the guard
//! denies and allows the recorded steps on the resulting state.
//!
//! Everything here runs keyless: every binary call uses `--no-recheck`
//! except one default run per kind that pins the re-check skip note (the
//! re-check for the new kinds is a later task and makes no model calls).
//!
//! Citations are seeded through the library with real stable IDs (the same
//! keys the T9.6/T9.7/T9.8 unit tests derive); every flag, mismatch and
//! denial below comes out of the built binary.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use doyle_tms::core::{Citation, ElementId, MemoryRef, Store};
use doyle_tms::evidence::legacy::ids as legacy_ids;
use doyle_tms::evidence::screens::ids as screen_ids;

fn story(rel: &str) -> String {
    format!("{}/testdata/legacy/story/{rel}", env!("CARGO_MANIFEST_DIR"))
}

fn bin() -> String {
    format!("{}/target/debug/doyle", env!("CARGO_MANIFEST_DIR"))
}

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static CTR: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "doyle-story-e2e-{}-{}-{}",
        std::process::id(),
        tag,
        CTR.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(args: &[&str], stdin: Option<&str>) -> Output {
    use std::io::Write;
    let mut child = Command::new(bin())
        .args(args)
        .stdin(if stdin.is_some() {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::null()
        })
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn doyle");
    if let Some(input) = stdin {
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
    }
    child.wait_with_output().expect("doyle runs")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

/// The story's L-memories through the T9.6 citations: L1 cites the fee path
/// including the exemption, L2 the record layout, L3 the fee paragraph
/// alone, L4 the v2-only discount elements.
fn legacy_citations() -> Vec<Citation> {
    let el = legacy_ids::element_id;
    let para = |name: &str| el(&legacy_ids::paragraph_key("BILLBATCH", name));
    let field = |path: &str| el(&legacy_ids::field_key("BILLBATCH", path));
    let cite = |memory: &str, claim: &str, elements: Vec<ElementId>| Citation {
        memory: MemoryRef {
            adapter: "muse-code".to_string(),
            id: memory.to_string(),
        },
        elements,
        claim: claim.to_string(),
        recorded_by: "test-curator".to_string(),
        confidence: 0.9,
        source_text: claim.to_string(),
    };
    vec![
        cite(
            "L1",
            "1.5% late fee on the whole balance, no exceptions",
            vec![
                para("CALC-LATE-FEE"),
                field("PATIENT-RECORD.BALANCE"),
                para("LOG-EXEMPT"),
                field("PATIENT-RECORD.VISIT-CODE"),
            ],
        ),
        cite(
            "L2",
            "each record is 34 chars",
            vec![
                el(&legacy_ids::record_key("BILLBATCH", "PATIENT-RECORD")),
                field("PATIENT-RECORD.ACCT-NO"),
                field("PATIENT-RECORD.ACCT-NAME"),
                field("PATIENT-RECORD.BALANCE"),
                field("PATIENT-RECORD.VISIT-CODE"),
            ],
        ),
        cite(
            "L3",
            "CALC-LATE-FEE computes 1.5% of BALANCE",
            vec![para("CALC-LATE-FEE"), field("PATIENT-RECORD.BALANCE")],
        ),
        cite(
            "L4",
            "prompt-payment accounts keep 10% of the fee",
            vec![
                para("APPLY-PROMPT-DISCOUNT"),
                field("PATIENT-RECORD.DISCOUNT-FLAG"),
            ],
        ),
    ]
}

/// One training step as the story fixture records it.
struct StoryStep {
    description: String,
    start_s: f64,
    end_s: f64,
    screen: String,
}

fn story_steps() -> Vec<StoryStep> {
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(story("training-v1-steps.json")).unwrap())
            .unwrap();
    value["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let id = s["id"].as_str().unwrap();
            let screen = match id {
                "T2" | "T3" | "T4" => "lookup",
                "T5" | "T6" => "decommission",
                _ => "",
            };
            StoryStep {
                description: s["description"].as_str().unwrap().to_string(),
                start_s: s["start_s"].as_f64().unwrap(),
                end_s: s["end_s"].as_f64().unwrap(),
                screen: screen.to_string(),
            }
        })
        .collect()
}

/// Write the two combined screens version files the `check` screens source
/// reads (the review page's version shape), built from the story fixtures
/// the way the T9.9 tests build them: v1 carries the training steps and
/// recording, v2 is stills only.
fn write_screen_versions(dir: &Path) -> (String, String) {
    let screen_doc = |file: &str| -> serde_json::Value {
        let value: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(story(&format!("screens/{file}.json"))).unwrap(),
        )
        .unwrap();
        serde_json::json!({
            "name": value["screen"],
            "elements": value["elements"],
        })
    };
    let steps: Vec<serde_json::Value> = story_steps()
        .iter()
        .map(|s| {
            serde_json::json!({
                "description": s.description,
                "start_s": s.start_s,
                "end_s": s.end_s,
                "screen": s.screen,
            })
        })
        .collect();
    let v1 = serde_json::json!({
        "console": "fernwhistle",
        "screens": [screen_doc("lookup-v1"), screen_doc("decomm-v1")],
        "steps": steps,
        "recording": {"name": "training-v1", "duration_s": 45.0},
    });
    let v2 = serde_json::json!({
        "console": "fernwhistle",
        "screens": [screen_doc("lookup-v2"), screen_doc("decomm-v2")],
        "steps": [],
    });
    let v1_path = dir.join("screens-v1.json");
    let v2_path = dir.join("screens-v2.json");
    std::fs::write(&v1_path, serde_json::to_string_pretty(&v1).unwrap()).unwrap();
    std::fs::write(&v2_path, serde_json::to_string_pretty(&v2).unwrap()).unwrap();
    (
        v1_path.to_string_lossy().to_string(),
        v2_path.to_string_lossy().to_string(),
    )
}

/// The story's S-memories: S1 cites the decommission screen plus the T5-T6
/// training span, S2 the lookup screen plus T2-T3. IDs derive from the
/// fixture steps, never hand-copied.
fn screen_citations() -> Vec<Citation> {
    let console = "fernwhistle";
    let decomm = screen_ids::screen_id(console, "decommission");
    let lookup = screen_ids::screen_id(console, "lookup");
    let step_id = |s: &StoryStep| {
        screen_ids::step_id(console, "training-v1", &s.description, s.start_s, s.end_s)
    };
    let steps = story_steps();
    let decomm_steps: Vec<ElementId> = steps
        .iter()
        .filter(|s| s.screen == "decommission")
        .map(step_id)
        .collect();
    let lookup_steps: Vec<ElementId> = steps
        .iter()
        .filter(|s| s.screen == "lookup")
        .take(2)
        .map(step_id)
        .collect();
    assert_eq!(decomm_steps.len(), 2, "T5 and T6 demonstrate decommission");
    assert_eq!(lookup_steps.len(), 2, "T2 and T3 demonstrate lookup");
    let cite = |memory: &str, claim: &str, elements: Vec<ElementId>| Citation {
        memory: MemoryRef {
            adapter: "muse-code".to_string(),
            id: memory.to_string(),
        },
        elements,
        claim: claim.to_string(),
        recorded_by: "test-curator".to_string(),
        confidence: 0.9,
        source_text: claim.to_string(),
    };
    vec![
        cite(
            "S1",
            "retire: open DECOMMISSION, type the clinic name, press PF12 CONFIRM",
            [vec![decomm], decomm_steps].concat(),
        ),
        cite(
            "S2",
            "look up: enter the ID on the lookup screen; PF5 edits",
            [vec![lookup], lookup_steps].concat(),
        ),
    ]
}

/// Write a diffrun manifest beside `dir` naming absolute fixture paths.
fn write_manifest(dir: &Path, name: &str, rewrite_report: &str) -> String {
    let manifest = serde_json::json!({
        "inputs": story("v1/patients.dat"),
        "cobol_report": story("v1/report.txt"),
        "rewrite_report": rewrite_report,
        "record_paths": format!("{}/testdata/legacy/diffrun/full-record-paths.txt", env!("CARGO_MANIFEST_DIR")),
    });
    let path = dir.join(name);
    std::fs::write(&path, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();
    path.to_string_lossy().to_string()
}

fn seed(dir: &Path, citations: Vec<Citation>) {
    let mut store = Store::open(dir).unwrap();
    for citation in citations {
        store.record_citation(citation).unwrap();
    }
}

fn flag_memory_ids(json_report: &str) -> Vec<String> {
    let value: serde_json::Value = serde_json::from_str(json_report).unwrap();
    value["flags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["memory"]["id"].as_str().unwrap().to_string())
        .collect()
}

/// The full proof in one state dir: legacy diff flags L2 alone, screen diff
/// S1 alone, differential run L1 alone with the 535 vaccination-visit
/// mismatches, and the guard denies the ship and retire steps while
/// allowing the traps and an ordinary command.
#[test]
fn story_expected_flags_and_guard_from_the_binary() {
    let root = temp_dir("story");
    let state = root.join("state");
    let mut citations = legacy_citations();
    citations.extend(screen_citations());
    seed(&state, citations);

    let v1_cbl = std::path::absolute(story("v1/BILLBATCH.cbl"))
        .unwrap()
        .to_string_lossy()
        .to_string();
    let v2_cbl = std::path::absolute(story("v2/BILLBATCH.cbl"))
        .unwrap()
        .to_string_lossy()
        .to_string();
    let state_arg = state.to_string_lossy().to_string();

    // Legacy diff: L2 alone.
    let out = run(
        &[
            "check",
            &v1_cbl,
            &v2_cbl,
            "--state-dir",
            &state_arg,
            "--format",
            "json",
            "--no-recheck",
        ],
        None,
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "legacy diff flags: {}",
        stderr(&out)
    );
    assert_eq!(
        flag_memory_ids(&stdout(&out)),
        ["L2"],
        "legacy diff flags L2 alone"
    );
    assert!(
        stderr(&out).is_empty(),
        "no-recheck stays silent: {}",
        stderr(&out)
    );

    // Screen diff: S1 alone.
    let (screens_v1, screens_v2) = write_screen_versions(&root);
    let out = run(
        &[
            "check",
            &screens_v1,
            &screens_v2,
            "--state-dir",
            &state_arg,
            "--format",
            "json",
            "--no-recheck",
        ],
        None,
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "screen diff flags: {}",
        stderr(&out)
    );
    assert_eq!(
        flag_memory_ids(&stdout(&out)),
        ["S1"],
        "screen diff flags S1 alone"
    );

    // Differential run: L1 alone with the 535 vaccination-visit mismatches.
    let wrong_report = format!(
        "{}/testdata/legacy/diffrun/full-wrong-report.txt",
        env!("CARGO_MANIFEST_DIR")
    );
    let manifest = write_manifest(&root, "run-wrong.json", &wrong_report);
    let out = run(
        &[
            "diffrun",
            &v1_cbl,
            &manifest,
            "--state-dir",
            &state_arg,
            "--format",
            "json",
            "--no-recheck",
        ],
        None,
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "diffrun flags: {}",
        stderr(&out)
    );
    let report: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    let flags = report["flags"].as_array().unwrap();
    assert_eq!(flags.len(), 1, "diffrun flags exactly one memory");
    assert_eq!(flags[0]["memory"]["id"], "L1", "the wrong rule's memory");
    assert!(
        flags[0]["id"]
            .as_str()
            .is_some_and(|id| id.starts_with("flag-")),
        "the store assigns the rendered flag ID: {}",
        flags[0]["id"]
    );
    assert_eq!(report["record_count"], 4812);
    assert_eq!(report["mismatch_count"], 535);
    assert_eq!(flags[0]["mismatch_count"], 535);
    assert_eq!(report["overcharge_cents"], 830880, "$8,308.80 overcharge");
    assert_eq!(
        flags[0]["paragraphs"],
        serde_json::json!(["LOG-EXEMPT"]),
        "blame is the exemption path"
    );
    let recorded: Vec<String> = std::fs::read_to_string(story("rewrite/mismatches.txt"))
        .unwrap()
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| line.split_whitespace().next().unwrap().to_string())
        .collect();
    assert_eq!(recorded.len(), 535);
    let mismatched: Vec<String> = flags[0]["mismatches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["acct"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(mismatched, recorded, "exactly the story's 535 accounts");
    for m in flags[0]["mismatches"].as_array().unwrap() {
        assert_eq!(m["cobol_fee_cents"], 0, "the batch exempted every mismatch");
        assert!(m["rewrite_fee_cents"].as_i64().unwrap() > 0);
    }

    // Reruns flag nothing new: exit 0 across all three.
    for args in [
        vec!["check", v1_cbl.as_str(), v2_cbl.as_str()],
        vec!["check", screens_v1.as_str(), screens_v2.as_str()],
    ] {
        let out = run(
            &[
                args[0],
                args[1],
                args[2],
                "--state-dir",
                &state_arg,
                "--no-recheck",
            ],
            None,
        );
        assert_eq!(
            out.status.code(),
            Some(0),
            "rerun exits 0: {}",
            stderr(&out)
        );
    }
    let out = run(
        &[
            "diffrun",
            &v1_cbl,
            &manifest,
            "--state-dir",
            &state_arg,
            "--no-recheck",
        ],
        None,
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "diffrun rerun exits 0: {}",
        stderr(&out)
    );
    let store = Store::open(&state).unwrap();
    assert_eq!(store.flags().len(), 3, "one flag per kind: L2, S1, L1");

    // The guard on the resulting state dir.
    let cwd = env!("CARGO_MANIFEST_DIR");
    let hook = |tool: &str, input: serde_json::Value| {
        serde_json::json!({
            "cwd": cwd,
            "hook_event_name": "PreToolUse",
            "tool_name": tool,
            "tool_input": input,
        })
        .to_string()
    };
    let bash = |command: &str| hook("bash", serde_json::json!({"command": command}));

    // Deny: the ship-rewrite step naming the blamed paragraph.
    let out = run(
        &["guard", "--state-dir", &state_arg],
        Some(&bash(
            "deploy wonder-bill to production --fee-path LOG-EXEMPT",
        )),
    );
    assert_eq!(out.status.code(), Some(0), "deny exits 0");
    let denial: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(denial["hookSpecificOutput"]["permissionDecision"], "deny");
    let reason = denial["hookSpecificOutput"]["permissionDecisionReason"]
        .as_str()
        .unwrap();
    assert!(reason.contains("L1"), "names the memory: {reason}");
    assert!(
        reason.contains("LOG-EXEMPT"),
        "names the blamed paragraph: {reason}"
    );
    assert!(reason.contains("DOYLE-GUARD"), "Doyle's reason: {reason}");
    assert!(
        reason.ends_with(
            "A person reviews this flag on Doyle's review page; this session cannot settle it."
        ),
        "person-reviews sentence: {reason}"
    );

    // Deny: the console-decommission step naming the screen (T9.12's
    // commands name the screen; here in capitals, matched case-insensitively).
    let out = run(
        &["guard", "--state-dir", &state_arg],
        Some(&bash("console DECOMMISSION --clinic MAPLE-ST")),
    );
    assert_eq!(out.status.code(), Some(0), "deny exits 0");
    let denial: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(denial["hookSpecificOutput"]["permissionDecision"], "deny");
    let reason = denial["hookSpecificOutput"]["permissionDecisionReason"]
        .as_str()
        .unwrap();
    assert!(reason.contains("S1"), "names the memory: {reason}");
    assert!(
        reason.contains("decommission"),
        "names the screen: {reason}"
    );

    // Allow: the traps' steps and an ordinary command.
    for (label, command) in [
        (
            "L4 discount step",
            "verify APPLY-PROMPT-DISCOUNT discount wiring on v2",
        ),
        ("S2 lookup step", "open lookup and list the matches"),
        (
            "L3 clean path",
            "cross-foot WRITE-TOTALS against the summary",
        ),
        ("ordinary command", "ls -la"),
    ] {
        let out = run(&["guard", "--state-dir", &state_arg], Some(&bash(command)));
        assert_eq!(out.status.code(), Some(0), "{label} exits 0");
        assert!(stdout(&out).is_empty(), "{label} silent on stdout");
        assert!(
            stderr(&out).is_empty(),
            "{label} silent on stderr: {}",
            stderr(&out)
        );
    }

    let _ = std::fs::remove_dir_all(&root);
}

/// The correct rewrite flags nothing: the recorded byte-identical control
/// (`rewrite/evidence.txt`) as the rewrite report.
#[test]
fn fixed_rewrite_flags_nothing() {
    let root = temp_dir("fixed");
    let state = root.join("state");
    seed(&state, legacy_citations());
    // The story records fixed-bill.rs as byte-identical to the batch; the
    // binary compares reports, so the batch report stands in for the fixed
    // rewrite's report.
    let manifest = write_manifest(&root, "run-fixed.json", &story("v1/report.txt"));
    let v1_cbl = std::path::absolute(story("v1/BILLBATCH.cbl"))
        .unwrap()
        .to_string_lossy()
        .to_string();
    let state_arg = state.to_string_lossy().to_string();
    let out = run(
        &[
            "diffrun",
            &v1_cbl,
            &manifest,
            "--state-dir",
            &state_arg,
            "--format",
            "markdown",
            "--no-recheck",
        ],
        None,
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "fixed rewrite flags nothing: {}",
        stderr(&out)
    );
    assert!(
        stdout(&out).contains("No flags."),
        "says so: {}",
        stdout(&out)
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// `--no-recheck` output for each kind: flags render, no proposal, no model.
#[test]
fn no_recheck_output_per_kind() {
    let root = temp_dir("norecheck");
    let state_arg = root.join("state").to_string_lossy().to_string();
    seed(&root.join("state"), {
        let mut all = legacy_citations();
        all.extend(screen_citations());
        all
    });
    let v1_cbl = std::path::absolute(story("v1/BILLBATCH.cbl"))
        .unwrap()
        .to_string_lossy()
        .to_string();
    let v2_cbl = std::path::absolute(story("v2/BILLBATCH.cbl"))
        .unwrap()
        .to_string_lossy()
        .to_string();

    let out = run(
        &[
            "check",
            &v1_cbl,
            &v2_cbl,
            "--state-dir",
            &state_arg,
            "--no-recheck",
        ],
        None,
    );
    assert_eq!(out.status.code(), Some(1));
    let text = stdout(&out);
    assert!(text.contains("L2"), "legacy flag renders: {text}");
    assert!(text.contains("breaking"), "severity renders: {text}");
    assert!(
        !text.contains("Proposal"),
        "no-recheck proposes nothing: {text}"
    );
    assert!(stderr(&out).is_empty(), "silent: {}", stderr(&out));

    let (screens_v1, screens_v2) = write_screen_versions(&root);
    let out = run(
        &[
            "check",
            &screens_v1,
            &screens_v2,
            "--state-dir",
            &state_arg,
            "--no-recheck",
        ],
        None,
    );
    assert_eq!(out.status.code(), Some(1));
    let text = stdout(&out);
    assert!(text.contains("S1"), "screen flag renders: {text}");
    assert!(
        !text.contains("Proposal"),
        "no-recheck proposes nothing: {text}"
    );

    let wrong_report = format!(
        "{}/testdata/legacy/diffrun/full-wrong-report.txt",
        env!("CARGO_MANIFEST_DIR")
    );
    let manifest = write_manifest(&root, "run-wrong.json", &wrong_report);
    let out = run(
        &[
            "diffrun",
            &v1_cbl,
            &manifest,
            "--state-dir",
            &state_arg,
            "--no-recheck",
        ],
        None,
    );
    assert_eq!(out.status.code(), Some(1));
    let text = stdout(&out);
    assert!(text.contains("L1"), "diffrun flag renders: {text}");
    assert!(text.contains("535"), "mismatch count renders: {text}");
    assert!(text.contains("LOG-EXEMPT"), "blame renders: {text}");
    assert!(
        text.contains("ACCT 000003"),
        "record evidence renders: {text}"
    );
    assert!(
        !text.contains("Proposal"),
        "no-recheck proposes nothing: {text}"
    );
    assert!(stderr(&out).is_empty(), "silent: {}", stderr(&out));
    let _ = std::fs::remove_dir_all(&root);
}

/// A default run (no `--no-recheck`) flags the same memories and notes the
/// missing re-check once per flag instead of calling a model: keyless, and
/// the re-check itself is a later task.
#[test]
fn default_run_notes_the_missing_recheck() {
    let root = temp_dir("default");
    let state_arg = root.join("state").to_string_lossy().to_string();
    seed(&root.join("state"), legacy_citations());
    let v1_cbl = std::path::absolute(story("v1/BILLBATCH.cbl"))
        .unwrap()
        .to_string_lossy()
        .to_string();
    let v2_cbl = std::path::absolute(story("v2/BILLBATCH.cbl"))
        .unwrap()
        .to_string_lossy()
        .to_string();
    let out = run(
        &["check", &v1_cbl, &v2_cbl, "--state-dir", &state_arg],
        None,
    );
    assert_eq!(out.status.code(), Some(1), "still flags: {}", stderr(&out));
    assert!(
        stdout(&out).contains("L2"),
        "flag renders: {}",
        stdout(&out)
    );
    let err = stderr(&out);
    assert!(
        err.contains("L2") && err.contains("not built yet"),
        "one skip note naming the flag: {err}"
    );

    let wrong_report = format!(
        "{}/testdata/legacy/diffrun/full-wrong-report.txt",
        env!("CARGO_MANIFEST_DIR")
    );
    let manifest = write_manifest(&root, "run-wrong.json", &wrong_report);
    let out = run(
        &["diffrun", &v1_cbl, &manifest, "--state-dir", &state_arg],
        None,
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "diffrun still flags: {}",
        stderr(&out)
    );
    assert!(
        stderr(&out).contains("L1") && stderr(&out).contains("not built yet"),
        "one skip note: {}",
        stderr(&out)
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Loud failures, never silent passes: an unreadable program, an unreadable
/// version file and a broken manifest each exit 2 naming the file.
#[test]
fn unreadable_inputs_exit_2() {
    let root = temp_dir("bad");
    let state_arg = root.join("state").to_string_lossy().to_string();
    let v2_cbl = std::path::absolute(story("v2/BILLBATCH.cbl"))
        .unwrap()
        .to_string_lossy()
        .to_string();
    let out = run(
        &[
            "check",
            "/nonexistent/BILLBATCH.cbl",
            &v2_cbl,
            "--state-dir",
            &state_arg,
        ],
        None,
    );
    assert_eq!(out.status.code(), Some(2), "missing program exits 2");
    assert!(!stderr(&out).is_empty(), "names the problem");

    let (screens_v1, _) = write_screen_versions(&root);
    let out = run(
        &[
            "check",
            &screens_v1,
            "/nonexistent/v2.json",
            "--state-dir",
            &state_arg,
        ],
        None,
    );
    assert_eq!(
        out.status.code(),
        Some(2),
        "missing screens version exits 2"
    );

    let manifest = root.join("run-bad.json");
    std::fs::write(&manifest, r#"{"inputs": "x"}"#).unwrap();
    let v1_cbl = std::path::absolute(story("v1/BILLBATCH.cbl"))
        .unwrap()
        .to_string_lossy()
        .to_string();
    let out = run(
        &[
            "diffrun",
            &v1_cbl,
            &manifest.to_string_lossy(),
            "--state-dir",
            &state_arg,
        ],
        None,
    );
    assert_eq!(out.status.code(), Some(2), "broken manifest exits 2");
    assert!(!stderr(&out).is_empty(), "names the problem");
    let _ = std::fs::remove_dir_all(&root);
}

/// The new command shapes parse and document themselves.
#[test]
fn command_shapes_parse_and_document() {
    let argv = |words: &[&str]| words.iter().map(|w| w.to_string()).collect::<Vec<_>>();
    match doyle_tms::cli::parse_from(&argv(&["diffrun", "prog.cbl", "run.json"])) {
        doyle_tms::cli::ParseResult::Run(cli) => match cli.command {
            doyle_tms::cli::Command::Diffrun {
                program,
                run,
                format,
                no_recheck,
            } => {
                assert_eq!((program.as_str(), run.as_str()), ("prog.cbl", "run.json"));
                assert_eq!(format, "markdown");
                assert!(!no_recheck);
            }
            other => panic!("expected Diffrun, got {other:?}"),
        },
        other => panic!("expected Run, got {other:?}"),
    }
    match doyle_tms::cli::parse_from(&argv(&[
        "diffrun",
        "prog.cbl",
        "run.json",
        "--format",
        "json",
        "--no-recheck",
    ])) {
        doyle_tms::cli::ParseResult::Run(cli) => match cli.command {
            doyle_tms::cli::Command::Diffrun {
                format, no_recheck, ..
            } => {
                assert_eq!(format, "json");
                assert!(no_recheck);
            }
            other => panic!("expected Diffrun, got {other:?}"),
        },
        other => panic!("expected Run, got {other:?}"),
    }
    match doyle_tms::cli::parse_from(&argv(&["diffrun", "only-one"])) {
        doyle_tms::cli::ParseResult::Error(_) => {}
        other => panic!("expected Error, got {other:?}"),
    }
    for command in ["check", "diffrun"] {
        let help = doyle_tms::cli::help_text(Some(command));
        assert!(help.contains(command), "{command} help names itself");
    }
    assert!(
        doyle_tms::cli::CHECK_HELP.contains(".cbl"),
        "check help names the legacy source"
    );
    assert!(
        doyle_tms::cli::CHECK_HELP.contains(".json"),
        "check help names the screens source"
    );
    assert!(
        doyle_tms::cli::DIFRUN_HELP.contains("record_paths"),
        "diffrun help names the manifest"
    );
}
