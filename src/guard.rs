//! `doyle guard`: the Muse Code hook.
//!
//! Run by Muse Code as a `PreToolUse` hook (deny an action that touches an
//! element with an open flag) and a `SessionStart` hook (inject a short
//! notice of open flags). Reads one hook JSON object on stdin; see
//! `docs/guard.md` for registration and `docs/muse-code.md` for the wire
//! format the behaviour below was measured against.
//!
//! Deterministic and model-free: the guard loads Doyle's store, takes the
//! open flags, resolves each flagged memory's cited elements to native
//! locations (method and path, operation ID, type name) through the contract
//! named by that flag's `new_version`, and matches the tool input against
//! them. A broken guard never blocks the agent's work: an unreadable store
//! or contract allows the action with one warning on stderr. Unparsable
//! hook input is the one fail-closed case (exit 2): the hook setup itself
//! is broken and needs attention, and there is no event to route.

use std::io::Read;
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use crate::cli;
use crate::contract::ContractIndex;
use crate::core::{Severity, Store};

/// Tool inputs the guard inspects. Any other tool name passes through
/// unseen: allow unknown, deny known-bad.
const KNOWN_TOOLS: &[&str] = &[
    "bash",
    "read_file",
    "write_file",
    "edit_file",
    "add_memory",
    "apply_patch",
    "edit_memory",
];

/// Muse's internal tool, fired several times a session. Always passes.
const INTERNAL_TOOL: &str = "submit_reminder_decision";

/// Run the `guard` command.
pub fn run(cli: &cli::Cli) -> ExitCode {
    let started = Instant::now();
    let mut input = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut input) {
        eprintln!("doyle guard: cannot read stdin: {e}");
        return ExitCode::from(2);
    }
    let (code, out, err) = decide(&input, cli);
    if !out.is_empty() {
        print!("{out}");
    }
    if !err.is_empty() {
        eprintln!("{err}");
    }
    if cli.common.verbose {
        eprintln!(
            "doyle guard: decided in {} ms",
            started.elapsed().as_millis()
        );
    }
    code
}

/// Decide one hook call. Returns the exit code and the exact bytes for
/// stdout and stderr, so tests can pin the wire format without spawning.
pub fn decide(input: &str, cli: &cli::Cli) -> (ExitCode, String, String) {
    let call = match parse_hook(input) {
        Ok(call) => call,
        Err(problem) => {
            return (
                ExitCode::from(2),
                String::new(),
                format!("doyle guard: {problem}"),
            );
        }
    };
    match call.event.as_str() {
        "PreToolUse" => decide_pre_tool_use(&call, cli),
        "SessionStart" => decide_session_start(&call, cli),
        _ => (ExitCode::SUCCESS, String::new(), String::new()),
    }
}

/// One parsed hook call: the event, the workspace it fired in, the tool,
/// and every string in its input.
struct HookCall {
    event: String,
    cwd: Option<String>,
    tool_name: String,
    strings: Vec<String>,
}

fn parse_hook(input: &str) -> Result<HookCall, String> {
    let value: serde_json::Value =
        serde_json::from_str(input).map_err(|e| format!("cannot parse hook input: {e}"))?;
    let obj = value
        .as_object()
        .ok_or_else(|| String::from("hook input is not an object"))?;
    let event = obj
        .get("hook_event_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| String::from("hook input has no hook_event_name"))?
        .to_string();
    let cwd = obj.get("cwd").and_then(|v| v.as_str()).map(str::to_string);
    let tool_name = obj
        .get("tool_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let mut strings = Vec::new();
    if let Some(tool_input) = obj.get("tool_input") {
        collect_strings(tool_input, &mut strings);
    }
    Ok(HookCall {
        event,
        cwd,
        tool_name,
        strings,
    })
}

fn collect_strings(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(s) => out.push(s.clone()),
        serde_json::Value::Array(items) => {
            for item in items {
                collect_strings(item, out);
            }
        }
        serde_json::Value::Object(map) => {
            for (_, nested) in map {
                collect_strings(nested, out);
            }
        }
        _ => {}
    }
}

fn decide_pre_tool_use(call: &HookCall, cli: &cli::Cli) -> (ExitCode, String, String) {
    if call.tool_name == INTERNAL_TOOL {
        return (ExitCode::SUCCESS, String::new(), String::new());
    }
    if call.tool_name.is_empty() {
        return (
            ExitCode::from(2),
            String::new(),
            String::from("doyle guard: PreToolUse input has no tool_name"),
        );
    }
    if !KNOWN_TOOLS.contains(&call.tool_name.as_str()) {
        return (ExitCode::SUCCESS, String::new(), String::new());
    }
    let dir = resolve_against_cwd(&state_dir_raw(cli), call.cwd.as_deref());
    let store = match Store::open(&dir) {
        Ok(store) => store,
        Err(e) => {
            return (
                ExitCode::SUCCESS,
                String::new(),
                format!(
                    "doyle guard: warning: cannot read state at {dir}: {e}; allowing the action"
                ),
            );
        }
    };
    let open = store.open_flags();
    if open.is_empty() {
        return (ExitCode::SUCCESS, String::new(), String::new());
    }
    let index = build_match_index(&store, &open, call.cwd.as_deref());
    if index.natives.is_empty() {
        return (
            ExitCode::SUCCESS,
            String::new(),
            format!(
                "doyle guard: warning: cannot match open flags ({}); allowing the action",
                index.problem
            ),
        );
    }
    let candidates = extract_candidates(&call.strings);
    let joined = call.strings.join("\n");
    let mut hits: Vec<(usize, String)> = Vec::new();
    let mut seen_flag: Vec<usize> = Vec::new();
    for native in &index.natives {
        if seen_flag.contains(&native.flag) {
            continue;
        }
        let hit = match (&native.method, &native.path) {
            (Some(method), Some(path)) => {
                if candidates
                    .iter()
                    .any(|c| path_matches(method, path, &c.method, &c.path))
                {
                    Some(format!(
                        "{} {} ({})",
                        method,
                        path,
                        native.operation_id.as_deref().unwrap_or("no operation id")
                    ))
                } else {
                    None
                }
            }
            _ => None,
        }
        .or_else(|| {
            native.operation_id.as_deref().and_then(|op| {
                if contains_identifier(&joined, op) {
                    Some(format!("operation {op}"))
                } else {
                    None
                }
            })
        })
        .or_else(|| {
            native.type_name.as_deref().and_then(|name| {
                if contains_identifier(&joined, name) {
                    Some(format!("type {name}"))
                } else {
                    None
                }
            })
        });
        if let Some(detail) = hit {
            seen_flag.push(native.flag);
            hits.push((native.flag, detail));
        }
    }
    if hits.is_empty() {
        return (ExitCode::SUCCESS, String::new(), String::new());
    }
    hits.sort_by_key(|(flag, _)| *flag);
    let reason = deny_reason(&open, &store, &hits);
    let out = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": reason,
        }
    });
    (ExitCode::SUCCESS, format!("{out}\n"), String::new())
}

fn decide_session_start(call: &HookCall, cli: &cli::Cli) -> (ExitCode, String, String) {
    let dir = resolve_against_cwd(&state_dir_raw(cli), call.cwd.as_deref());
    let store = match Store::open(&dir) {
        Ok(store) => store,
        Err(e) => {
            return (
                ExitCode::SUCCESS,
                String::new(),
                format!(
                    "doyle guard: warning: cannot read state at {dir}: {e}; allowing the action"
                ),
            );
        }
    };
    let open = store.open_flags();
    if open.is_empty() {
        return (ExitCode::SUCCESS, String::new(), String::new());
    }
    let mut items = Vec::new();
    for flag in &open {
        items.push(format!(
            "{}/{} (\"{}\")",
            flag.memory.adapter,
            flag.memory.id,
            shorten(&memory_claim(&store, flag), 160)
        ));
    }
    let noun = if open.len() == 1 {
        "memory"
    } else {
        "memories"
    };
    let notice = format!(
        "DOYLE NOTICE: {} {noun} flagged stale: {}. Review with `doyle serve`.",
        open.len(),
        items.join("; ")
    );
    let out = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": notice,
        }
    });
    (ExitCode::SUCCESS, format!("{out}\n"), String::new())
}

fn state_dir_raw(cli: &cli::Cli) -> String {
    cli.common
        .state_dir
        .clone()
        .unwrap_or_else(|| String::from(".doyle"))
}

/// Resolve a possibly relative path against the hook's workspace: hooks
/// run with the workspace root as their working directory, so a relative
/// state dir or contract path means "under the workspace".
fn resolve_against_cwd(raw: &str, cwd: Option<&str>) -> String {
    if Path::new(raw).is_absolute() {
        return raw.to_string();
    }
    match cwd {
        Some(dir) if Path::new(dir).is_absolute() => {
            format!("{}/{}", dir.trim_end_matches('/'), raw)
        }
        _ => raw.to_string(),
    }
}

/// Every matchable native location of every open flag: the method and path,
/// operation ID and type name of each cited element, through the contract
/// that flag's `new_version` names.
struct Native {
    method: Option<String>,
    path: Option<String>,
    operation_id: Option<String>,
    type_name: Option<String>,
    flag: usize,
}

struct MatchIndex {
    natives: Vec<Native>,
    problem: String,
}

fn build_match_index(store: &Store, open: &[crate::core::Flag], cwd: Option<&str>) -> MatchIndex {
    let mut natives = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    let mut loaded: Vec<(String, Option<ContractIndex>)> = Vec::new();
    for (position, flag) in open.iter().enumerate() {
        let slot = match loaded.iter().position(|(key, _)| key == &flag.new_version) {
            Some(slot) => slot,
            None => {
                let path = resolve_against_cwd(&flag.new_version, cwd);
                let index = match ContractIndex::load(&path) {
                    Ok(index) => Some(index),
                    Err(_) => {
                        failures.push(flag.new_version.clone());
                        None
                    }
                };
                loaded.push((flag.new_version.clone(), index));
                loaded.len() - 1
            }
        };
        let Some(index) = loaded[slot].1.as_ref() else {
            continue;
        };
        for citation in store.citations_for_memory(&flag.memory) {
            for element in &citation.elements {
                let Some(location) = index.native_location(element) else {
                    continue;
                };
                natives.push(Native {
                    method: location.method,
                    path: location.path,
                    operation_id: location.operation_id,
                    type_name: location.type_name,
                    flag: position,
                });
            }
        }
    }
    let problem = if failures.is_empty() {
        String::from("flagged elements have no matchable native locations")
    } else {
        format!("cannot load contract: {}", failures.join(", "))
    };
    MatchIndex { natives, problem }
}

/// One extracted action: an HTTP method and a path from a URL or a bare
/// path in a tool input string.
struct Candidate {
    method: String,
    path: String,
}

fn extract_candidates(strings: &[String]) -> Vec<Candidate> {
    let mut out = Vec::new();
    for text in strings {
        let methods = methods_for(text);
        for url in find_urls(text) {
            if let Some(path) = path_of_url(&url) {
                for method in &methods {
                    out.push(Candidate {
                        method: method.clone(),
                        path: path.clone(),
                    });
                }
            }
        }
        for path in find_bare_paths(text) {
            for method in &methods {
                out.push(Candidate {
                    method: method.clone(),
                    path: path.clone(),
                });
            }
        }
    }
    out
}

/// The methods a shell fragment means: every explicit `-X`/`--request`
/// when any is present, else POST when data flags are present, else GET.
fn methods_for(text: &str) -> Vec<String> {
    let mut explicit = Vec::new();
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        if token == "-X" || token == "--request" {
            if let Some(next) = tokens.get(index + 1)
                && next.chars().all(|c| c.is_ascii_alphabetic())
                && !next.is_empty()
            {
                let method = next.to_ascii_uppercase();
                if !explicit.contains(&method) {
                    explicit.push(method);
                }
            }
            index += 1;
        } else if let Some(rest) = token.strip_prefix("-X")
            && !rest.is_empty()
            && rest.chars().all(|c| c.is_ascii_alphabetic())
        {
            let method = rest.to_ascii_uppercase();
            if !explicit.contains(&method) {
                explicit.push(method);
            }
        } else if let Some(rest) = token.strip_prefix("--request=")
            && !rest.is_empty()
            && rest.chars().all(|c| c.is_ascii_alphabetic())
        {
            let method = rest.to_ascii_uppercase();
            if !explicit.contains(&method) {
                explicit.push(method);
            }
        }
        index += 1;
    }
    if !explicit.is_empty() {
        return explicit;
    }
    let data = tokens.iter().any(|t| {
        *t == "-d"
            || *t == "-F"
            || *t == "--form"
            || *t == "--data"
            || t.starts_with("--data-")
            || t.starts_with("--form-")
    });
    if data {
        vec![String::from("POST")]
    } else {
        vec![String::from("GET")]
    }
}

/// Every `http(s)` URL in the text, cut at whitespace, quotes, angle
/// brackets, backticks, semicolons, closing parens and commas.
fn find_urls(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    loop {
        let next = rest.find("http://").or_else(|| rest.find("https://"));
        let Some(start) = next else {
            break;
        };
        let tail = &rest[start..];
        let end = tail
            .find(|c: char| {
                c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | '`' | ';' | ')' | ',')
            })
            .unwrap_or(tail.len());
        out.push(tail[..end].to_string());
        rest = &tail[end..];
    }
    out
}

/// The path component of a URL: from the first `/` after the host, without
/// query or fragment.
fn path_of_url(url: &str) -> Option<String> {
    let after_scheme = url.split("://").nth(1)?;
    let path = match after_scheme.find('/') {
        Some(index) => &after_scheme[index..],
        None => return Some(String::from("/")),
    };
    let end = path.find(['?', '#']).unwrap_or(path.len());
    Some(path[..end].to_string())
}

/// Bare path tokens (`/patients`, not inside a URL) in a shell fragment,
/// without query or fragment.
fn find_bare_paths(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in text.split_whitespace() {
        let token =
            token.trim_matches(|c: char| matches!(c, '"' | '\'' | '(' | ')' | '`' | ',' | ';'));
        if token.len() < 2 || !token.starts_with('/') || token.contains("://") {
            continue;
        }
        let path = match token.find(['?', '#']) {
            Some(index) => &token[..index],
            None => token,
        };
        if path.len() > 1 && path.split('/').all(|segment| !segment.is_empty()) {
            out.push(path.to_string());
        }
    }
    out
}

/// Whether a candidate action matches a flagged native location: the
/// method (case-insensitive) and the path template, segment for segment.
/// A `{param}` segment matches any single non-empty segment, so
/// `GET /patients/{id}` matches `/patients/42` but never `/patients`,
/// and `GET /patients` never matches `/patients/42`.
fn path_matches(native_method: &str, pattern: &str, cand_method: &str, cand_path: &str) -> bool {
    if !native_method.eq_ignore_ascii_case(cand_method) {
        return false;
    }
    let pattern = pattern.trim_end_matches('/');
    let candidate = cand_path.trim_end_matches('/');
    let pattern = if pattern.is_empty() { "/" } else { pattern };
    let candidate = if candidate.is_empty() { "/" } else { candidate };
    let expected: Vec<&str> = pattern.split('/').collect();
    let actual: Vec<&str> = candidate.split('/').collect();
    if expected.len() != actual.len() {
        return false;
    }
    expected
        .iter()
        .zip(actual.iter())
        .all(|(want, got)| (want.starts_with('{') && want.ends_with('}')) || want == got)
}

/// Whether `name` appears as a standalone identifier: case-sensitive, with
/// no ASCII letter, digit or underscore on either side. `getStats` never
/// matches `getStatistics`.
fn contains_identifier(haystack: &str, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    haystack.match_indices(name).any(|(start, _)| {
        let before = haystack[..start].bytes().next_back();
        let after = haystack[start + name.len()..].bytes().next();
        !is_identifier_char(before) && !is_identifier_char(after)
    })
}

fn is_identifier_char(byte: Option<u8>) -> bool {
    byte.is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn deny_reason(open: &[crate::core::Flag], store: &Store, hits: &[(usize, String)]) -> String {
    let mut parts = Vec::new();
    for (position, detail) in hits {
        let flag = &open[*position];
        let severity = match flag.severity {
            Severity::Breaking => "breaking",
            Severity::Review => "review",
        };
        parts.push(format!(
            "memory {}/{} is stale (\"{}\"). Matched {}. Changed {} -> {} ({}).",
            flag.memory.adapter,
            flag.memory.id,
            shorten(&memory_claim(store, flag), 200),
            detail,
            flag.old_version,
            flag.new_version,
            severity
        ));
    }
    format!(
        "DOYLE-GUARD: {} Review with `doyle serve`, then retry against the current contract.",
        parts.join(" ")
    )
}

fn memory_claim(store: &Store, flag: &crate::core::Flag) -> String {
    store
        .citations_for_memory(&flag.memory)
        .into_iter()
        .next()
        .map(|citation| citation.claim)
        .or_else(|| flag.evidence.clone())
        .unwrap_or_else(|| String::from("stale memory"))
}

fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max).collect();
    format!("{kept}…")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{ChangeId, Citation, ElementId, Flag, MemoryRef, flag_citations};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    // Entity IDs from testdata/vimanam/README.md, stable across v1 and v2.
    const LIST_PATIENTS: &str =
        "ve2_c81e37b7c42c484a1859c09e95bf8cec89a360db6743a691afab818f177c7bc6";
    const LIST_PATIENTS_200: &str =
        "ve2_86d3cdb7ce3a7f0b1a2fefa696f0acbbcf2d5406c717bb1f1f1cb5f4f62667ba";
    const GET_PATIENT: &str =
        "ve2_c69ae2bf6960e6f28e01bf913a93aa0a325f6d3ee49d445a68b1f580f27af2b5";
    const GET_PATIENT_404: &str =
        "ve2_6add08d53def059f4e973257dc9e174a5398208bce2fa4d719e0418ca523349e";
    const GET_STATS: &str = "ve2_88f1296b143cdad99952319ae783095b3386438f0c8932538d9bfbe1b7fcfb0c";
    const STATS: &str = "ve2_c09c07ec8983f09afc5dafed67ee0e3e0d271de3b8f8f1bd61daa1111f063809";
    const PATIENT_COUNT: &str =
        "ve2_99aaf77708e08db6ee0c7bd151e3044ef33e089e1bbe915fe889e7330b79058f";
    const DECOMMISSION: &str =
        "ve2_9e9663295485ddf808651f117c8ca42ab58e33fbe49a27309e0547f77c866846";

    static CTR: AtomicU64 = AtomicU64::new(0);

    fn fixture(name: &str) -> String {
        format!("{}/testdata/{name}", env!("CARGO_MANIFEST_DIR"))
    }

    fn temp_state_dir(tag: &str) -> PathBuf {
        let n = CTR.fetch_add(1, Ordering::Relaxed);
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "doyle-guard-test-{}-{}-{}",
            std::process::id(),
            tag,
            n
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn citation(memory: &str, claim: &str, elements: &[&str]) -> Citation {
        Citation {
            memory: MemoryRef {
                adapter: "muse-code".to_string(),
                id: memory.to_string(),
            },
            elements: elements
                .iter()
                .map(|e| ElementId((*e).to_string()))
                .collect(),
            claim: claim.to_string(),
            recorded_by: "curator".to_string(),
            confidence: 0.9,
        }
    }

    /// The demo's store: M1-M4 cited as in testdata/README.md, with M1's
    /// real flag open. The flag's `new_version` is the absolute v2 fixture
    /// path, the way `doyle check` records the path it was given.
    fn seed_demo_store(dir: &Path) -> Store {
        let mut store = Store::open(dir).unwrap();
        let seeds: [(&str, &str, &[&str]); 4] = [
            (
                "M1",
                "GET /patients returns every patient in one response",
                &[LIST_PATIENTS, LIST_PATIENTS_200],
            ),
            (
                "M2",
                "GET /patients/{id} returns 404 for an unknown patient id",
                &[GET_PATIENT, GET_PATIENT_404],
            ),
            (
                "M3",
                "GET /stats patient_count is the total number of patients",
                &[GET_STATS, PATIENT_COUNT],
            ),
            (
                "M4",
                "POST /admin/decommission is permanent once confirmed",
                &[DECOMMISSION],
            ),
        ];
        for (memory, claim, elements) in seeds {
            store
                .record_citation(citation(memory, claim, elements))
                .unwrap();
        }
        let old = ContractIndex::load(fixture("contracts/patients-v1.yaml")).unwrap();
        let new = ContractIndex::load(fixture("contracts/patients-v2.yaml")).unwrap();
        let changes = old.diff(&new).unwrap();
        let v2 = std::path::absolute(fixture("contracts/patients-v2.yaml"))
            .unwrap()
            .to_string_lossy()
            .to_string();
        let flags = flag_citations(
            store.citations(),
            &changes,
            &old,
            &store.open_flags(),
            "v1",
            &v2,
        );
        assert_eq!(flags.len(), 1, "demo flags exactly M1: {flags:?}");
        store
            .record_flag(flags.into_iter().next().unwrap())
            .unwrap();
        store
    }

    /// A store with one open flag on one memory citing `elements`.
    fn seed_single_flag(dir: &Path, memory: &str, claim: &str, elements: &[&str]) -> Store {
        let mut store = Store::open(dir).unwrap();
        store
            .record_citation(citation(memory, claim, elements))
            .unwrap();
        let v2 = std::path::absolute(fixture("contracts/patients-v2.yaml"))
            .unwrap()
            .to_string_lossy()
            .to_string();
        store
            .record_flag(Flag {
                id: String::new(),
                memory: MemoryRef {
                    adapter: "muse-code".to_string(),
                    id: memory.to_string(),
                },
                changes: vec![ChangeId(
                    "vc2_test00000000000000000000000000000001".to_string(),
                )],
                severity: Severity::Review,
                old_version: "v1".to_string(),
                new_version: v2,
                evidence: Some(claim.to_string()),
            })
            .unwrap();
        store
    }

    fn cli_for(dir: &Path) -> cli::Cli {
        cli::Cli {
            command: cli::Command::Guard,
            common: cli::CommonOpts {
                state_dir: Some(dir.to_string_lossy().to_string()),
                ..Default::default()
            },
        }
    }

    fn hook_json(cwd: &str, tool_name: &str, tool_input: &str) -> String {
        format!(
            "{{\"cwd\":\"{cwd}\",\"hook_event_name\":\"PreToolUse\",\"tool_input\":{tool_input},\"tool_name\":\"{tool_name}\"}}"
        )
    }

    fn bash_hook(command: &str) -> String {
        let escaped = command.replace('\\', "\\\\").replace('"', "\\\"");
        hook_json(
            "/tmp/scratch/repo-a",
            "bash",
            &format!("{{\"command\":\"{escaped}\"}}"),
        )
    }

    fn is_deny(code: ExitCode, out: &str) -> serde_json::Value {
        assert_eq!(code, ExitCode::SUCCESS, "deny exits 0: {out}");
        let value: serde_json::Value = serde_json::from_str(out).expect("deny is JSON");
        assert_eq!(
            value["hookSpecificOutput"]["permissionDecision"], "deny",
            "permissionDecision is deny: {out}"
        );
        value
    }

    fn is_allow(code: ExitCode, out: &str, err: &str) {
        assert_eq!(code, ExitCode::SUCCESS, "allow exits 0");
        assert!(out.is_empty(), "allow is silent on stdout: {out}");
        assert!(err.is_empty(), "allow is silent on stderr: {err}");
    }

    #[test]
    fn demo_export_against_get_patients_is_denied() {
        let dir = temp_state_dir("export");
        seed_demo_store(&dir);
        let cli = cli_for(&dir);
        let input = bash_hook("curl -s http://127.0.0.1:8091/patients");
        let (code, out, err) = decide(&input, &cli);
        assert!(err.is_empty(), "deny warns nothing: {err}");
        let value = is_deny(code, &out);
        let reason = value["hookSpecificOutput"]["permissionDecisionReason"]
            .as_str()
            .expect("reason is a string");
        assert!(reason.contains("M1"), "reason names the memory: {reason}");
        assert!(
            reason.contains("GET /patients"),
            "reason names what changed: {reason}"
        );
        assert!(
            reason.contains("breaking"),
            "reason carries the severity: {reason}"
        );
        assert!(
            reason.contains("doyle serve"),
            "reason names the review page: {reason}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recorded_bash_payload_passes() {
        let dir = temp_state_dir("recorded-bash");
        seed_demo_store(&dir);
        let cli = cli_for(&dir);
        let input = std::fs::read_to_string(fixture("muse/hooks/pre-tool-use-bash.json")).unwrap();
        let (code, out, err) = decide(&input, &cli);
        is_allow(code, &out, &err);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recorded_edit_write_and_memory_payloads_pass() {
        let dir = temp_state_dir("recorded-others");
        seed_demo_store(&dir);
        let cli = cli_for(&dir);
        for name in [
            "pre-tool-use-edit-file.json",
            "pre-tool-use-write-file.json",
            "pre-tool-use-add-memory.json",
        ] {
            let input = std::fs::read_to_string(fixture(&format!("muse/hooks/{name}"))).unwrap();
            let (code, out, err) = decide(&input, &cli);
            is_allow(code, &out, &err);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn get_patient_detail_does_not_match_list() {
        let dir = temp_state_dir("detail");
        seed_demo_store(&dir);
        let cli = cli_for(&dir);
        // Two segments: matches neither GET /patients (one) nor the
        // unflagged GET /patients/{id}.
        let input = bash_hook("curl -s http://127.0.0.1:8091/patients/abc123");
        let (code, out, err) = decide(&input, &cli);
        is_allow(code, &out, &err);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn query_string_is_stripped_before_matching() {
        let dir = temp_state_dir("query");
        seed_demo_store(&dir);
        let cli = cli_for(&dir);
        let input = bash_hook("curl -s \"http://127.0.0.1:8091/patients?cursor=abc&limit=100\"");
        let (code, out, _) = decide(&input, &cli);
        is_deny(code, &out);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn method_mismatch_allows() {
        let dir = temp_state_dir("method");
        seed_demo_store(&dir);
        let cli = cli_for(&dir);
        // M1 flags GET /patients; an explicit POST is another action.
        let input = bash_hook("curl -X POST http://127.0.0.1:8091/patients");
        let (code, out, err) = decide(&input, &cli);
        is_allow(code, &out, &err);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn getstats_does_not_match_getstatistics() {
        let dir = temp_state_dir("opid");
        seed_single_flag(
            &dir,
            "M9",
            "GET /stats patient_count is the total",
            &[GET_STATS],
        );
        let cli = cli_for(&dir);
        let allow = |content: &str| {
            let escaped = content.replace('\\', "\\\\").replace('"', "\\\"");
            decide(
                &hook_json(
                    "/tmp/scratch/repo-a",
                    "write_file",
                    &format!("{{\"content\":\"{escaped}\",\"path\":\"notes.txt\"}}"),
                ),
                &cli,
            )
        };
        // A longer identifier containing the operation ID stays quiet.
        let (code, out, err) = allow("function getStatistics() { return 1; }");
        is_allow(code, &out, &err);
        // A prefixed identifier stays quiet too.
        let (code, out, err) = allow("xgetStats");
        is_allow(code, &out, &err);
        // The bare operation ID denies.
        let (code, out, _) = allow("call getStats() to fetch totals");
        is_deny(code, &out);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn type_name_match_is_case_sensitive_with_boundaries() {
        let dir = temp_state_dir("type");
        seed_single_flag(&dir, "M9", "Stats carries patient_count", &[STATS]);
        let cli = cli_for(&dir);
        let run = |content: &str| {
            let escaped = content.replace('\\', "\\\\").replace('"', "\\\"");
            decide(
                &hook_json(
                    "/tmp/scratch/repo-a",
                    "write_file",
                    &format!("{{\"content\":\"{escaped}\",\"path\":\"notes.txt\"}}"),
                ),
                &cli,
            )
        };
        let (code, out, _) = run("the Stats type changed");
        is_deny(code, &out);
        let (code, out, err) = run("the status is fine");
        is_allow(code, &out, &err);
        let (code, out, err) = run("MyStats");
        is_allow(code, &out, &err);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn field_names_never_match() {
        let dir = temp_state_dir("field");
        seed_single_flag(&dir, "M9", "patient_count is the total", &[PATIENT_COUNT]);
        let cli = cli_for(&dir);
        let input = hook_json(
            "/tmp/scratch/repo-a",
            "write_file",
            "{\"content\":\"patient_count is checked here\",\"path\":\"notes.txt\"}",
        );
        let (code, out, err) = decide(&input, &cli);
        is_allow(code, &out, &err);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn templated_path_matches_only_its_shape() {
        let dir = temp_state_dir("template");
        seed_single_flag(&dir, "M2", "404 for an unknown id", &[GET_PATIENT]);
        let cli = cli_for(&dir);
        // The bare collection path is not the flagged item path.
        let (code, out, err) = decide(&bash_hook("curl -s http://x/patients"), &cli);
        is_allow(code, &out, &err);
        // One concrete id fills the template.
        let (code, out, _) = decide(&bash_hook("curl -s http://x/patients/abc"), &cli);
        is_deny(code, &out);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn internal_tool_always_passes() {
        let dir = temp_state_dir("internal");
        seed_demo_store(&dir);
        let cli = cli_for(&dir);
        let input = hook_json(
            "/tmp/scratch/repo-a",
            "submit_reminder_decision",
            "{\"decision\":\"proceed\",\"reason\":\"ok\"}",
        );
        let (code, out, err) = decide(&input, &cli);
        is_allow(code, &out, &err);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_tool_passes_even_when_naming_a_flagged_element() {
        let dir = temp_state_dir("unknown");
        seed_demo_store(&dir);
        let cli = cli_for(&dir);
        let input = hook_json(
            "/tmp/scratch/repo-a",
            "frobnicate",
            "{\"command\":\"curl http://127.0.0.1:8091/patients\"}",
        );
        let (code, out, err) = decide(&input, &cli);
        is_allow(code, &out, &err);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_event_passes() {
        let dir = temp_state_dir("event");
        seed_demo_store(&dir);
        let cli = cli_for(&dir);
        let input = "{\"cwd\":\"/tmp/scratch/repo-a\",\"hook_event_name\":\"PostToolUse\"}";
        let (code, out, err) = decide(input, &cli);
        is_allow(code, &out, &err);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unparsable_input_exits_2() {
        let dir = temp_state_dir("garbage");
        let cli = cli_for(&dir);
        let (code, out, err) = decide("this is not json", &cli);
        assert_eq!(code, ExitCode::from(2));
        assert!(out.is_empty(), "no stdout on parse failure: {out}");
        assert!(!err.is_empty(), "parse failure explains itself: {err}");
        let (code, _, _) = decide("{\"cwd\":\"x\"}", &cli);
        assert_eq!(code, ExitCode::from(2));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn broken_state_allows_with_one_warning() {
        let dir = temp_state_dir("broken");
        let blocker = dir.join("blocker");
        std::fs::write(&blocker, "not a directory").unwrap();
        let cli = cli::Cli {
            command: cli::Command::Guard,
            common: cli::CommonOpts {
                state_dir: Some(blocker.join(".doyle").to_string_lossy().to_string()),
                ..Default::default()
            },
        };
        let input = bash_hook("curl -s http://127.0.0.1:8091/patients");
        let (code, out, err) = decide(&input, &cli);
        assert_eq!(code, ExitCode::SUCCESS, "a broken guard allows");
        assert!(out.is_empty(), "allow is silent on stdout: {out}");
        assert!(
            err.contains("warning") && err.contains("allowing"),
            "one warning on stderr: {err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn session_start_notice_lists_open_flags() {
        let dir = temp_state_dir("notice");
        seed_demo_store(&dir);
        let cli = cli_for(&dir);
        let input = std::fs::read_to_string(fixture("muse/hooks/session-start.json")).unwrap();
        let (code, out, err) = decide(&input, &cli);
        assert_eq!(code, ExitCode::SUCCESS);
        assert!(err.is_empty(), "notice warns nothing: {err}");
        let value: serde_json::Value = serde_json::from_str(&out).expect("notice is JSON");
        assert_eq!(
            value["hookSpecificOutput"]["hookEventName"], "SessionStart",
            "notice uses the SessionStart envelope: {out}"
        );
        let context = value["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .expect("notice carries additionalContext");
        assert!(context.contains("M1"), "notice names the memory: {context}");
        assert!(
            context.contains("doyle serve"),
            "notice names the review page: {context}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn session_start_is_empty_without_open_flags() {
        let dir = temp_state_dir("quiet");
        Store::open(&dir).unwrap();
        let cli = cli_for(&dir);
        let input = std::fs::read_to_string(fixture("muse/hooks/session-start.json")).unwrap();
        let (code, out, err) = decide(&input, &cli);
        is_allow(code, &out, &err);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pre_tool_use_is_silent_without_open_flags() {
        let dir = temp_state_dir("quiet-pre");
        Store::open(&dir).unwrap();
        let cli = cli_for(&dir);
        let input = bash_hook("curl -s http://127.0.0.1:8091/patients");
        let (code, out, err) = decide(&input, &cli);
        is_allow(code, &out, &err);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn decision_is_fast() {
        let dir = temp_state_dir("timing");
        seed_demo_store(&dir);
        let cli = cli_for(&dir);
        let input = bash_hook("curl -s http://127.0.0.1:8091/patients");
        // Warm the filesystem, then measure one call the way the hook pays it.
        let _ = decide(&input, &cli);
        let started = Instant::now();
        let (code, out, _) = decide(&input, &cli);
        let elapsed = started.elapsed();
        is_deny(code, &out);
        eprintln!("doyle guard: measured per-call time: {:?}", elapsed);
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "one call must stay well under hook timeouts, took {elapsed:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn bin_path() -> String {
        format!("{}/target/debug/doyle", env!("CARGO_MANIFEST_DIR"))
    }

    #[test]
    fn binary_denies_export_and_reports_notice() {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let dir = temp_state_dir("binary");
        seed_demo_store(&dir);
        let state = dir.join("state");
        // Re-seed under the state dir the binary will open: the helper
        // seeded `dir` itself, so point the binary at `dir`.
        let state_arg = dir.to_string_lossy().to_string();
        let _ = &state;

        let run = |stdin_text: &str| {
            let mut child = Command::new(bin_path())
                .args(["guard", "--state-dir", &state_arg])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("spawn doyle");
            child
                .stdin
                .as_mut()
                .unwrap()
                .write_all(stdin_text.as_bytes())
                .unwrap();
            child.wait_with_output().unwrap()
        };

        let export = bash_hook("curl -s http://127.0.0.1:8091/patients");
        let out = run(&export);
        assert!(out.status.success(), "deny exits 0");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("\"deny\""), "binary denies: {stdout}");
        assert!(stdout.contains("M1"), "binary names the memory: {stdout}");

        let session = std::fs::read_to_string(fixture("muse/hooks/session-start.json")).unwrap();
        let out = run(&session);
        assert!(out.status.success(), "notice exits 0");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("additionalContext"),
            "binary injects: {stdout}"
        );
        assert!(stdout.contains("M1"), "binary names the memory: {stdout}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
