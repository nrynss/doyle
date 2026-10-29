//! `doyle cite`: record a citation by hand.
//!
//! A citation is written to the store. The element may be given by its
//! `ElementId` or by its native location (`GET /patients`, or
//! `GET /patients 200` to pick a particular response). An element the
//! contract does not have is refused, with the closest matches named.

use std::collections::BTreeSet;
use std::io::{self, Read};
use std::process::ExitCode;

use serde_json::Value;

use crate::cli;
use crate::contract::ContractIndex;
use crate::core::{Citation, ElementId, MemoryRef};

/// A citation as supplied to `doyle cite`.
#[derive(Debug, Clone)]
pub struct CiteSpec {
    pub adapter: String,
    pub memory_id: String,
    pub contract: String,
    pub elements: Vec<String>,
    pub claim: String,
    pub recorded_by: String,
    pub confidence: f32,
}

/// Errors from resolving an element against a contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CiteError {
    UnknownElement {
        spec: String,
        suggestions: Vec<String>,
    },
    Contract(String),
    Store(String),
    Input(String),
}

impl std::fmt::Display for CiteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CiteError::UnknownElement { spec, .. } => {
                write!(f, "unknown element: {spec}")
            }
            CiteError::Contract(m) => write!(f, "contract: {m}"),
            CiteError::Store(m) => write!(f, "store: {m}"),
            CiteError::Input(m) => write!(f, "input: {m}"),
        }
    }
}

impl std::error::Error for CiteError {}

/// Run the `cite` command.
pub fn run(cli: &cli::Cli) -> ExitCode {
    let state_dir = cli
        .common
        .state_dir
        .clone()
        .unwrap_or_else(|| ".doyle".to_string());

    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        eprintln!("doyle cite: cannot read stdin: {e}");
        return ExitCode::from(2);
    }

    let trimmed = input.trim();
    let specs = match parse_specs(trimmed, cli) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("doyle cite: {e}");
            return ExitCode::from(2);
        }
    };

    if specs.is_empty() {
        eprintln!("doyle cite: no citation given");
        return ExitCode::from(2);
    }

    if let Err(e) = std::fs::create_dir_all(&state_dir) {
        eprintln!("doyle cite: io error: {e}");
        return ExitCode::from(2);
    }

    for spec in &specs {
        let contract = match ContractIndex::load(&spec.contract) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("doyle cite: {e}");
                return ExitCode::from(2);
            }
        };

        let entities = match load_entities(&spec.contract) {
            Ok(e) => e,
            Err(m) => {
                eprintln!("doyle cite: {m}");
                return ExitCode::from(2);
            }
        };

        let mut elements = Vec::new();
        for raw in &spec.elements {
            match resolve_element(&entities, raw) {
                Ok(id) => elements.push(id),
                Err(suggestions) => {
                    eprintln!("doyle cite: unknown element: {raw}");
                    if !suggestions.is_empty() {
                        eprintln!("suggestions:");
                        for s in suggestions {
                            eprintln!("  {s}");
                        }
                    }
                    return ExitCode::from(2);
                }
            }
        }

        if elements.is_empty() {
            eprintln!("doyle cite: at least one element is required");
            return ExitCode::from(2);
        }

        let citation = Citation {
            memory: MemoryRef {
                adapter: spec.adapter.clone(),
                id: spec.memory_id.clone(),
            },
            elements,
            claim: spec.claim.clone(),
            recorded_by: spec.recorded_by.clone(),
            confidence: spec.confidence,
        };

        let line = match serde_json::to_string(&citation) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("doyle cite: json error: {e}");
                return ExitCode::from(2);
            }
        };
        let path = std::path::Path::new(&state_dir).join("citations.jsonl");
        use std::io::Write;
        let mut f = match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            Ok(f) => f,
            Err(e) => {
                eprintln!("doyle cite: io error: {e}");
                return ExitCode::from(2);
            }
        };
        if let Err(e) = writeln!(f, "{}", line) {
            eprintln!("doyle cite: io error: {e}");
            return ExitCode::from(2);
        }
        let _ = f.flush();

        // Keep the loaded contract index referenced so a future reader can
        // see the connection; silence unused-variable warnings.
        let _ = &contract;
    }

    let n = specs.len();
    if n == 1 {
        println!("recorded citation");
    } else {
        println!("recorded {n} citations");
    }
    ExitCode::SUCCESS
}

/// Parse citation specs from stdin text.
fn parse_specs(input: &str, cli: &cli::Cli) -> Result<Vec<CiteSpec>, CiteError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(CiteError::Input(String::from("no input on stdin")));
    }

    // JSON path.
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        let value: Value = serde_json::from_str(trimmed)
            .map_err(|e| CiteError::Input(format!("invalid JSON: {e}")))?;
        let mut specs = Vec::new();
        match value {
            Value::Array(items) => {
                for item in items {
                    specs.push(spec_from_json(&item, cli)?);
                }
            }
            Value::Object(_) => {
                specs.push(spec_from_json(&value, cli)?);
            }
            _ => return Err(CiteError::Input(String::from("expected object or array"))),
        }
        // JSONL: if the first parse failed but there are multiple lines of JSON,
        // the above would already have failed; handle JSONL separately below is
        // not reached. Try JSONL as a fallback only when the whole input did not
        // parse as one JSON value.
        return Ok(specs);
    }

    // Try JSONL: every non-empty line a JSON object.
    let mut jsonl_specs = Vec::new();
    let mut all_json = true;
    for line in trimmed.lines() {
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        if !l.starts_with('{') {
            all_json = false;
            break;
        }
        let v: Value = serde_json::from_str(l)
            .map_err(|e| CiteError::Input(format!("invalid JSON line: {e}")))?;
        jsonl_specs.push(spec_from_json(&v, cli)?);
    }
    if all_json && !jsonl_specs.is_empty() {
        return Ok(jsonl_specs);
    }

    // key=value lines
    let mut spec = empty_spec(cli);
    for line in trimmed.lines() {
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        if let Some((key, value)) = l.split_once('=') {
            let key = key.trim().to_lowercase();
            let value = value.trim();
            apply_field(&mut spec, &key, value);
        } else {
            // treat a bare line as an element spec for the single citation
            if spec.elements.is_empty() && !value_looks_like_field(l) {
                spec.elements.push(l.to_string());
            }
        }
    }
    Ok(vec![spec])
}

fn value_looks_like_field(line: &str) -> bool {
    line.contains('=')
}

fn empty_spec(cli: &cli::Cli) -> CiteSpec {
    let adapter = cli
        .common
        .adapters
        .first()
        .cloned()
        .unwrap_or_else(|| "manual".to_string());
    CiteSpec {
        adapter,
        memory_id: String::new(),
        contract: String::new(),
        elements: Vec::new(),
        claim: String::new(),
        recorded_by: "manual".to_string(),
        confidence: 1.0,
    }
}

fn spec_from_json(value: &Value, cli: &cli::Cli) -> Result<CiteSpec, CiteError> {
    let mut spec = empty_spec(cli);
    let obj = value
        .as_object()
        .ok_or_else(|| CiteError::Input(String::from("expected object")))?;

    // memory reference: nested or flat
    if let Some(mem) = obj.get("memory").and_then(|v| v.as_object()) {
        if let Some(a) = mem.get("adapter").and_then(|v| v.as_str()) {
            spec.adapter = a.to_string();
        }
        if let Some(i) = mem.get("id").and_then(|v| v.as_str()) {
            spec.memory_id = i.to_string();
        }
    }
    for key in ["adapter", "memory_adapter", "system"] {
        if let Some(v) = obj.get(key).and_then(|v| v.as_str()) {
            spec.adapter = v.to_string();
        }
    }
    for key in ["memory_id", "memory", "id"] {
        if let Some(v) = obj.get(key).and_then(|v| v.as_str()) {
            // avoid overwriting adapter's id with memory object's id already handled;
            // only set if not nested
            if obj.get("memory").is_none() || key != "id" || obj.get("id").is_some() {
                spec.memory_id = v.to_string();
            }
        }
    }
    // If both flat adapter and id present, fine.

    for key in ["contract", "contract_path", "contract_file", "path"] {
        if let Some(v) = obj.get(key).and_then(|v| v.as_str()) {
            spec.contract = v.to_string();
        }
    }

    // elements
    if let Some(v) = obj.get("elements") {
        collect_elements(v, &mut spec.elements);
    }
    if let Some(v) = obj.get("element") {
        collect_elements(v, &mut spec.elements);
    }

    for key in ["claim", "text", "note"] {
        if let Some(v) = obj.get(key).and_then(|v| v.as_str()) {
            spec.claim = v.to_string();
        }
    }
    for key in ["recorded_by", "recordedBy", "by", "author"] {
        if let Some(v) = obj.get(key).and_then(|v| v.as_str()) {
            spec.recorded_by = v.to_string();
        }
    }
    if let Some(v) = obj.get("confidence").and_then(|v| v.as_f64()) {
        spec.confidence = v as f32;
    }

    // env fallbacks
    apply_env(&mut spec);

    if spec.memory_id.is_empty() {
        return Err(CiteError::Input(String::from("memory id is required")));
    }
    if spec.contract.is_empty() {
        return Err(CiteError::Input(String::from("contract path is required")));
    }

    Ok(spec)
}

fn collect_elements(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(s) => {
            for part in s.split(',') {
                let t = part.trim();
                if !t.is_empty() {
                    out.push(t.to_string());
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                if let Some(s) = item.as_str() {
                    for part in s.split(',') {
                        let t = part.trim();
                        if !t.is_empty() {
                            out.push(t.to_string());
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

fn apply_field(spec: &mut CiteSpec, key: &str, value: &str) {
    match key {
        "adapter" | "memory_adapter" | "system" => spec.adapter = value.to_string(),
        "memory_id" | "memory" | "id" => spec.memory_id = value.to_string(),
        "contract" | "contract_path" | "contract_file" | "path" => {
            spec.contract = value.to_string()
        }
        "element" | "elements" => {
            for part in value.split(',') {
                let t = part.trim();
                if !t.is_empty() {
                    spec.elements.push(t.to_string());
                }
            }
        }
        "claim" | "text" | "note" => spec.claim = value.to_string(),
        "recorded_by" | "recordedby" | "by" | "author" => spec.recorded_by = value.to_string(),
        "confidence" => {
            if let Ok(v) = value.parse::<f32>() {
                spec.confidence = v;
            }
        }
        _ => {}
    }
}

fn apply_env(spec: &mut CiteSpec) {
    if let Ok(v) = std::env::var("DOYLE_CITE_ADAPTER")
        && !v.is_empty()
    {
        spec.adapter = v;
    }
    if let Ok(v) = std::env::var("DOYLE_CITE_MEMORY")
        && !v.is_empty()
    {
        spec.memory_id = v;
    }
    if let Ok(v) = std::env::var("DOYLE_CITE_CONTRACT")
        && !v.is_empty()
    {
        spec.contract = v;
    }
    if let Ok(v) = std::env::var("DOYLE_CITE_ELEMENT")
        && !v.is_empty()
    {
        for part in v.split(',') {
            let t = part.trim();
            if !t.is_empty() {
                spec.elements.push(t.to_string());
            }
        }
    }
    if let Ok(v) = std::env::var("DOYLE_CITE_CLAIM")
        && !v.is_empty()
    {
        spec.claim = v;
    }
    if let Ok(v) = std::env::var("DOYLE_CITE_RECORDED_BY")
        && !v.is_empty()
    {
        spec.recorded_by = v;
    }
    if let Ok(v) = std::env::var("DOYLE_CITE_CONFIDENCE")
        && let Ok(f) = v.parse::<f32>()
    {
        spec.confidence = f;
    }
}

/// Resolve an element spec against the entities of a contract (loaded via
/// Vimanam). Returns the `ElementId` or a list of closest matches.
pub fn resolve_element(
    entities: &[vimanam::contract::Entity],
    spec: &str,
) -> Result<ElementId, Vec<String>> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err(Vec::new());
    }

    // Direct ElementId.
    if entities.iter().any(|e| e.id == spec) {
        return Ok(ElementId(spec.to_string()));
    }

    // Native location: "METHOD /path" with an optional qualifier (status code,
    // entity name, or kind) to disambiguate responses sharing a path.
    let parts: Vec<&str> = spec.split_whitespace().collect();
    if parts.len() >= 2 && parts[1].starts_with('/') {
        let method = parts[0].to_uppercase();
        let path = parts[1];
        let qualifier = parts[2..].join(" ").to_lowercase();

        let mut candidates: Vec<&vimanam::contract::Entity> = entities
            .iter()
            .filter(|e| match &e.native {
                vimanam::contract::NativeLocation::Openapi {
                    method: m, path: p, ..
                } => m.as_deref() == Some(method.as_str()) && p.as_deref() == Some(path),
                _ => false,
            })
            .collect();

        if !qualifier.is_empty() {
            let filtered: Vec<&vimanam::contract::Entity> = candidates
                .iter()
                .copied()
                .filter(|e| {
                    let name = e.name.to_lowercase();
                    let kind = format!("{:?}", e.kind).to_lowercase();
                    let status = match &e.native {
                        vimanam::contract::NativeLocation::Openapi { status, .. } => {
                            status.clone().unwrap_or_default().to_lowercase()
                        }
                        _ => String::new(),
                    };
                    qualifier == name
                        || qualifier == status
                        || qualifier == kind
                        || (qualifier == "operation" && kind.contains("operation"))
                        || (qualifier == "response" && kind.contains("result"))
                        || (qualifier == "result" && kind.contains("result"))
                })
                .collect();
            if !filtered.is_empty() {
                candidates = filtered;
            }
        }

        if !candidates.is_empty() {
            // Prefer an operation for an unqualified location.
            if qualifier.is_empty()
                && let Some(op) = candidates
                    .iter()
                    .copied()
                    .find(|e| format!("{:?}", e.kind).to_lowercase().contains("operation"))
            {
                return Ok(ElementId(op.id.clone()));
            }
            // Single candidate, or first by id for determinism.
            let mut sorted = candidates;
            sorted.sort_by(|a, b| a.id.cmp(&b.id));
            return Ok(ElementId(sorted[0].id.clone()));
        }
    }

    // Unknown: suggest closest matches.
    Err(suggestions_for(entities, spec))
}

/// Load contract entities via Vimanam, mirroring `ContractIndex`.
fn load_entities(path: &str) -> Result<Vec<vimanam::contract::Entity>, String> {
    let api = vimanam::parse_openapi(path).map_err(|e| format!("{e:#}"))?;
    let contract = vimanam::contract::openapi::adapt(&api);
    Ok(contract.entities)
}

/// Build a list of closest candidate strings (native locations and IDs).
fn suggestions_for(entities: &[vimanam::contract::Entity], spec: &str) -> Vec<String> {
    let mut candidates: BTreeSet<String> = BTreeSet::new();
    for e in entities {
        candidates.insert(e.id.clone());
        if let vimanam::contract::NativeLocation::Openapi {
            method: Some(m),
            path: Some(p),
            ..
        } = &e.native
        {
            candidates.insert(format!("{m} {p}"));
        }
        candidates.insert(e.name.clone());
    }

    let mut scored: Vec<(usize, String)> = candidates
        .into_iter()
        .map(|c| (levenshtein(&spec.to_lowercase(), &c.to_lowercase()), c))
        .collect();
    scored.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    let mut out = Vec::new();
    for (_, c) in scored.into_iter().take(5) {
        if !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

/// Levenshtein distance (std only).
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

// Public aliases / helpers for tests that want to resolve without going
// through stdin.
pub fn resolve_element_path(path: &str, spec: &str) -> Result<ElementId, Vec<String>> {
    let entities = load_entities(path).map_err(|_| Vec::new())?;
    resolve_element(&entities, spec)
}

pub fn suggest_elements(path: &str, spec: &str) -> Vec<String> {
    match load_entities(path) {
        Ok(entities) => suggestions_for(&entities, spec),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        format!("{}/testdata/{name}", env!("CARGO_MANIFEST_DIR"))
    }

    #[test]
    fn resolves_operation_by_native_location() {
        let path = fixture("contracts/patients-v1.yaml");
        let entities = load_entities(&path).unwrap();
        let id = resolve_element(&entities, "GET /patients").unwrap();
        assert_eq!(
            id.0,
            "ve2_c81e37b7c42c484a1859c09e95bf8cec89a360db6743a691afab818f177c7bc6"
        );
    }

    #[test]
    fn resolves_response_by_status_qualifier() {
        let path = fixture("contracts/patients-v1.yaml");
        let entities = load_entities(&path).unwrap();
        let id = resolve_element(&entities, "GET /patients 200").unwrap();
        assert_eq!(
            id.0,
            "ve2_86d3cdb7ce3a7f0b1a2fefa696f0acbbcf2d5406c717bb1f1f1cb5f4f62667ba"
        );
    }

    #[test]
    fn resolves_direct_element_id() {
        let path = fixture("contracts/patients-v1.yaml");
        let entities = load_entities(&path).unwrap();
        let id = "ve2_c81e37b7c42c484a1859c09e95bf8cec89a360db6743a691afab818f177c7bc6";
        assert_eq!(resolve_element(&entities, id).unwrap().0, id);
    }

    #[test]
    fn unknown_element_returns_suggestions() {
        let path = fixture("contracts/patients-v1.yaml");
        let entities = load_entities(&path).unwrap();
        let err = resolve_element(&entities, "GET /patiens").unwrap_err();
        assert!(err.iter().any(|s| s == "GET /patients"));
    }

    #[test]
    fn levenshtein_basic() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("", "abc"), 3);
    }
}
