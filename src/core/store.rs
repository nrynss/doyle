//! Doyle's state store: one directory, JSON Lines, append-only.
//!
//! Doyle keeps its own state (the citation index plus the flag, proposal and
//! decision log) in a single directory (`.doyle/` by default, overridable).
//! Each record kind gets its own file of JSON Lines, appended to on every
//! write and rebuilt into memory on open. There is no database.
//!
//! Files written by the store:
//!
//! - `citations.jsonl`
//! - `flags.jsonl`
//! - `proposals.jsonl`
//! - `decisions.jsonl`
//!
//! A line that does not parse as JSON (a crash mid-write leaves a truncated
//! last line) is skipped with a warning on stderr; it never fails the open.

#![allow(dead_code)]

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

use super::model::{Citation, Decision, ElementId, Flag, MemoryRef, Proposal};

/// Default state directory name.
pub const DEFAULT_DIR: &str = ".doyle";

/// Errors from opening or writing the store.
#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "io error: {}", e),
            StoreError::Json(e) => write!(f, "json error: {}", e),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            StoreError::Io(e) => Some(e),
            StoreError::Json(e) => Some(e),
        }
    }
}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        StoreError::Io(e)
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(e: serde_json::Error) -> Self {
        StoreError::Json(e)
    }
}

/// Doyle's in-memory view of one state directory, rebuilt from its JSON Lines
/// files on open.
#[derive(Debug, Default)]
pub struct Store {
    dir: PathBuf,
    citations: Vec<Citation>,
    flags: Vec<Flag>,
    proposals: Vec<Proposal>,
    decisions: Vec<Decision>,
    next_flag: usize,
    next_proposal: usize,
    next_decision: usize,
}

impl Store {
    /// Open (or create) a store in `dir`, loading every record kind into memory.
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, StoreError> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir)?;
        let citations: Vec<Citation> = load_jsonl(&dir.join("citations.jsonl"))?;
        let flags: Vec<Flag> = load_jsonl(&dir.join("flags.jsonl"))?;
        let proposals: Vec<Proposal> = load_jsonl(&dir.join("proposals.jsonl"))?;
        let decisions: Vec<Decision> = load_jsonl(&dir.join("decisions.jsonl"))?;

        let next_flag = next_id(&flags, "flag-");
        let next_proposal = next_id(&proposals, "proposal-");
        let next_decision = next_id(&decisions, "decision-");

        Ok(Store {
            dir,
            citations,
            flags,
            proposals,
            decisions,
            next_flag,
            next_proposal,
            next_decision,
        })
    }

    /// Open a store in the default `.doyle` directory of the current working dir.
    pub fn open_default() -> Result<Self, StoreError> {
        Self::open(DEFAULT_DIR)
    }

    /// The directory this store reads from and writes to.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Path to the default state directory.
    pub fn default_dir() -> PathBuf {
        PathBuf::from(DEFAULT_DIR)
    }

    // ---------- citations ----------

    pub fn citations(&self) -> &[Citation] {
        &self.citations
    }

    pub fn all_citations(&self) -> &[Citation] {
        &self.citations
    }

    pub fn record_citation(&mut self, citation: Citation) -> Result<(), StoreError> {
        self.append_line("citations.jsonl", &citation)?;
        self.citations.push(citation);
        Ok(())
    }

    pub fn add_citation(&mut self, citation: Citation) -> Result<(), StoreError> {
        self.record_citation(citation)
    }

    pub fn append_citation(&mut self, citation: Citation) -> Result<(), StoreError> {
        self.record_citation(citation)
    }

    pub fn push_citation(&mut self, citation: Citation) -> Result<(), StoreError> {
        self.record_citation(citation)
    }

    pub fn insert_citation(&mut self, citation: Citation) -> Result<(), StoreError> {
        self.record_citation(citation)
    }

    pub fn save_citation(&mut self, citation: Citation) -> Result<(), StoreError> {
        self.record_citation(citation)
    }

    pub fn citations_for_element(&self, element: &ElementId) -> Vec<Citation> {
        self.citations
            .iter()
            .filter(|c| c.elements.contains(element))
            .cloned()
            .collect()
    }

    pub fn citations_by_element(&self, element: &ElementId) -> Vec<Citation> {
        self.citations_for_element(element)
    }

    pub fn citations_with_element(&self, element: &ElementId) -> Vec<Citation> {
        self.citations_for_element(element)
    }

    pub fn find_citations_by_element(&self, element: &ElementId) -> Vec<Citation> {
        self.citations_for_element(element)
    }

    pub fn citations_for_memory(&self, memory: &MemoryRef) -> Vec<Citation> {
        self.citations
            .iter()
            .filter(|c| &c.memory == memory)
            .cloned()
            .collect()
    }

    pub fn citations_by_memory(&self, memory: &MemoryRef) -> Vec<Citation> {
        self.citations_for_memory(memory)
    }

    pub fn citations_for_memory_ref(&self, memory: &MemoryRef) -> Vec<Citation> {
        self.citations_for_memory(memory)
    }

    pub fn find_citations_by_memory(&self, memory: &MemoryRef) -> Vec<Citation> {
        self.citations_for_memory(memory)
    }

    // ---------- flags ----------

    pub fn flags(&self) -> &[Flag] {
        &self.flags
    }

    pub fn all_flags(&self) -> &[Flag] {
        &self.flags
    }

    pub fn flag(&self, id: &str) -> Option<Flag> {
        self.flags.iter().find(|f| f.id == id).cloned()
    }

    pub fn get_flag(&self, id: &str) -> Option<Flag> {
        self.flag(id)
    }

    pub fn find_flag(&self, id: &str) -> Option<Flag> {
        self.flag(id)
    }

    pub fn flag_ref(&self, id: &str) -> Option<&Flag> {
        self.flags.iter().find(|f| f.id == id)
    }

    pub fn get_flag_ref(&self, id: &str) -> Option<&Flag> {
        self.flag_ref(id)
    }

    pub fn open_flags(&self) -> Vec<Flag> {
        let decided: HashSet<String> = self
            .decisions
            .iter()
            .filter_map(|d| {
                self.proposals
                    .iter()
                    .find(|p| p.id == d.proposal_id)
                    .map(|p| p.flag_id.clone())
            })
            .collect();
        self.flags
            .iter()
            .filter(|f| !decided.contains(&f.id))
            .cloned()
            .collect()
    }

    pub fn open_flags_list(&self) -> Vec<Flag> {
        self.open_flags()
    }

    pub fn unsettled_flags(&self) -> Vec<Flag> {
        self.open_flags()
    }

    pub fn pending_flags(&self) -> Vec<Flag> {
        self.open_flags()
    }

    pub fn record_flag(&mut self, mut flag: Flag) -> Result<String, StoreError> {
        let id = format!("flag-{}", self.next_flag);
        self.next_flag += 1;
        flag.id = id.clone();
        self.append_line("flags.jsonl", &flag)?;
        self.flags.push(flag);
        Ok(id)
    }

    pub fn add_flag(&mut self, flag: Flag) -> Result<String, StoreError> {
        self.record_flag(flag)
    }

    pub fn append_flag(&mut self, flag: Flag) -> Result<String, StoreError> {
        self.record_flag(flag)
    }

    pub fn push_flag(&mut self, flag: Flag) -> Result<String, StoreError> {
        self.record_flag(flag)
    }

    pub fn insert_flag(&mut self, flag: Flag) -> Result<String, StoreError> {
        self.record_flag(flag)
    }

    pub fn create_flag(&mut self, flag: Flag) -> Result<String, StoreError> {
        self.record_flag(flag)
    }

    // ---------- proposals ----------

    pub fn proposals(&self) -> &[Proposal] {
        &self.proposals
    }

    pub fn all_proposals(&self) -> &[Proposal] {
        &self.proposals
    }

    pub fn proposal(&self, id: &str) -> Option<Proposal> {
        self.proposals.iter().find(|p| p.id == id).cloned()
    }

    pub fn get_proposal(&self, id: &str) -> Option<Proposal> {
        self.proposal(id)
    }

    pub fn find_proposal(&self, id: &str) -> Option<Proposal> {
        self.proposal(id)
    }

    pub fn proposal_ref(&self, id: &str) -> Option<&Proposal> {
        self.proposals.iter().find(|p| p.id == id)
    }

    pub fn proposals_for_flag(&self, flag_id: &str) -> Vec<Proposal> {
        self.proposals
            .iter()
            .filter(|p| p.flag_id == flag_id)
            .cloned()
            .collect()
    }

    pub fn proposals_by_flag(&self, flag_id: &str) -> Vec<Proposal> {
        self.proposals_for_flag(flag_id)
    }

    pub fn record_proposal(&mut self, mut proposal: Proposal) -> Result<String, StoreError> {
        let id = format!("proposal-{}", self.next_proposal);
        self.next_proposal += 1;
        proposal.id = id.clone();
        self.append_line("proposals.jsonl", &proposal)?;
        self.proposals.push(proposal);
        Ok(id)
    }

    pub fn add_proposal(&mut self, proposal: Proposal) -> Result<String, StoreError> {
        self.record_proposal(proposal)
    }

    pub fn append_proposal(&mut self, proposal: Proposal) -> Result<String, StoreError> {
        self.record_proposal(proposal)
    }

    pub fn push_proposal(&mut self, proposal: Proposal) -> Result<String, StoreError> {
        self.record_proposal(proposal)
    }

    pub fn insert_proposal(&mut self, proposal: Proposal) -> Result<String, StoreError> {
        self.record_proposal(proposal)
    }

    pub fn create_proposal(&mut self, proposal: Proposal) -> Result<String, StoreError> {
        self.record_proposal(proposal)
    }

    // ---------- decisions ----------

    pub fn decisions(&self) -> &[Decision] {
        &self.decisions
    }

    pub fn all_decisions(&self) -> &[Decision] {
        &self.decisions
    }

    pub fn decision(&self, id: &str) -> Option<Decision> {
        self.decisions.iter().find(|d| d.id == id).cloned()
    }

    pub fn get_decision(&self, id: &str) -> Option<Decision> {
        self.decision(id)
    }

    pub fn find_decision(&self, id: &str) -> Option<Decision> {
        self.decision(id)
    }

    pub fn decision_ref(&self, id: &str) -> Option<&Decision> {
        self.decisions.iter().find(|d| d.id == id)
    }

    pub fn decision_for_proposal(&self, proposal_id: &str) -> Option<Decision> {
        self.decisions
            .iter()
            .find(|d| d.proposal_id == proposal_id)
            .cloned()
    }

    pub fn decision_for(&self, proposal_id: &str) -> Option<Decision> {
        self.decision_for_proposal(proposal_id)
    }

    pub fn decision_by_proposal(&self, proposal_id: &str) -> Option<Decision> {
        self.decision_for_proposal(proposal_id)
    }

    pub fn get_decision_for_proposal(&self, proposal_id: &str) -> Option<Decision> {
        self.decision_for_proposal(proposal_id)
    }

    pub fn find_decision_for_proposal(&self, proposal_id: &str) -> Option<Decision> {
        self.decision_for_proposal(proposal_id)
    }

    pub fn decision_for_proposal_ref(&self, proposal_id: &str) -> Option<&Decision> {
        self.decisions.iter().find(|d| d.proposal_id == proposal_id)
    }

    pub fn record_decision(&mut self, mut decision: Decision) -> Result<String, StoreError> {
        let id = format!("decision-{}", self.next_decision);
        self.next_decision += 1;
        decision.id = id.clone();
        self.append_line("decisions.jsonl", &decision)?;
        self.decisions.push(decision);
        Ok(id)
    }

    pub fn add_decision(&mut self, decision: Decision) -> Result<String, StoreError> {
        self.record_decision(decision)
    }

    pub fn append_decision(&mut self, decision: Decision) -> Result<String, StoreError> {
        self.record_decision(decision)
    }

    pub fn push_decision(&mut self, decision: Decision) -> Result<String, StoreError> {
        self.record_decision(decision)
    }

    pub fn insert_decision(&mut self, decision: Decision) -> Result<String, StoreError> {
        self.record_decision(decision)
    }

    pub fn create_decision(&mut self, decision: Decision) -> Result<String, StoreError> {
        self.record_decision(decision)
    }

    // ---------- internals ----------

    fn append_line<T: Serialize>(&self, file: &str, value: &T) -> Result<(), StoreError> {
        let path = self.dir.join(file);
        let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
        let line = serde_json::to_string(value)?;
        writeln!(f, "{}", line)?;
        f.flush().ok();
        Ok(())
    }
}

fn load_jsonl<T: DeserializeOwned>(path: &Path) -> Result<Vec<T>, StoreError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path)?;
    let mut out = Vec::new();
    for (i, raw) in content.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<T>(line) {
            Ok(v) => out.push(v),
            Err(e) => {
                eprintln!(
                    "doyle: warning: skipping corrupt line {} in {}: {}",
                    i + 1,
                    path.display(),
                    e
                );
            }
        }
    }
    Ok(out)
}

fn next_id<T>(records: &[T], prefix: &str) -> usize
where
    T: HasId,
{
    let mut max = 0usize;
    for r in records {
        let id = r.id();
        if let Some(rest) = id.strip_prefix(prefix)
            && let Ok(n) = rest.parse::<usize>()
            && n > max
        {
            max = n;
        }
    }
    if max == 0 && !records.is_empty() {
        // fall back to count if ids don't follow the prefix-n pattern
        records.len() + 1
    } else if records.is_empty() {
        1
    } else {
        max + 1
    }
}

trait HasId {
    fn id(&self) -> &str;
}

impl HasId for Flag {
    fn id(&self) -> &str {
        &self.id
    }
}
impl HasId for Proposal {
    fn id(&self) -> &str {
        &self.id
    }
}
impl HasId for Decision {
    fn id(&self) -> &str {
        &self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_dir(label: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("doyle-store-test-{}-{}", label, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn sample_citation(elem: &str, adapter: &str, id: &str) -> Citation {
        Citation {
            memory: MemoryRef {
                adapter: adapter.to_string(),
                id: id.to_string(),
            },
            elements: vec![ElementId(elem.to_string())],
            claim: "claim".to_string(),
            recorded_by: "tester".to_string(),
            confidence: 0.9,
        }
    }

    #[test]
    fn append_and_reload_citations() {
        let dir = temp_dir("append");
        let store_dir = dir.join(".doyle");
        {
            let mut store = Store::open(&store_dir).unwrap();
            let c = sample_citation("e1", "a", "m1");
            store.record_citation(c.clone()).unwrap();
            assert_eq!(store.citations().len(), 1);
        }
        let store = Store::open(&store_dir).unwrap();
        assert_eq!(store.citations().len(), 1);
        assert_eq!(store.citations()[0].claim, "claim");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn citations_by_element() {
        let dir = temp_dir("byelem");
        let store_dir = dir.join(".doyle");
        let mut store = Store::open(&store_dir).unwrap();
        let c1 = sample_citation("e1", "a", "m1");
        let c2 = sample_citation("e2", "a", "m2");
        store.record_citation(c1).unwrap();
        store.record_citation(c2).unwrap();
        let res = store.citations_for_element(&ElementId("e1".to_string()));
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].elements[0], ElementId("e1".to_string()));
        let alias = store.citations_by_element(&ElementId("e2".to_string()));
        assert_eq!(alias.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn citations_by_memory() {
        let dir = temp_dir("bymem");
        let store_dir = dir.join(".doyle");
        let mut store = Store::open(&store_dir).unwrap();
        let c1 = sample_citation("e1", "a", "m1");
        let c2 = sample_citation("e1", "a", "m2");
        store.record_citation(c1).unwrap();
        store.record_citation(c2).unwrap();
        let mem = MemoryRef {
            adapter: "a".to_string(),
            id: "m1".to_string(),
        };
        let res = store.citations_for_memory(&mem);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].memory.id, "m1");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_flags_and_decision_for_proposal() {
        let dir = temp_dir("flags");
        let store_dir = dir.join(".doyle");
        let mut store = Store::open(&store_dir).unwrap();
        let f1 = Flag {
            id: String::new(),
            memory: MemoryRef {
                adapter: "a".to_string(),
                id: "m1".to_string(),
            },
            changes: vec![],
            severity: Severity::Breaking,
            old_version: "v1".to_string(),
            new_version: "v2".to_string(),
            evidence: None,
        };
        let fid1 = store.record_flag(f1).unwrap();
        let f2 = Flag {
            id: String::new(),
            memory: MemoryRef {
                adapter: "a".to_string(),
                id: "m2".to_string(),
            },
            changes: vec![],
            severity: Severity::Review,
            old_version: "v1".to_string(),
            new_version: "v2".to_string(),
            evidence: None,
        };
        let fid2 = store.record_flag(f2).unwrap();

        assert_eq!(store.open_flags().len(), 2);

        let p = Proposal {
            id: String::new(),
            flag_id: fid1.clone(),
            action: ProposalAction::Keep,
            rationale: "r".to_string(),
            proposed_by: "m".to_string(),
        };
        let pid = store.record_proposal(p).unwrap();

        let d = Decision {
            id: String::new(),
            proposal_id: pid.clone(),
            verdict: DecisionVerdict::Approve,
            decided_by: "u".to_string(),
            decided_at: "2026-01-01T00:00:00Z".to_string(),
        };
        store.record_decision(d).unwrap();

        let open = store.open_flags();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].id, fid2);

        let got = store.decision_for_proposal(&pid).unwrap();
        assert_eq!(got.proposal_id, pid);
        assert_eq!(store.decision_for_proposal("nope"), None);

        let store2 = Store::open(&store_dir).unwrap();
        assert_eq!(store2.flags().len(), 2);
        assert_eq!(store2.open_flags().len(), 1);
        assert!(store2.decision_for_proposal(&pid).is_some());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn truncated_last_line_skipped() {
        let dir = temp_dir("trunc");
        let store_dir = dir.join(".doyle");
        let mut store = Store::open(&store_dir).unwrap();
        let c = sample_citation("e1", "a", "m1");
        store.record_citation(c).unwrap();
        use std::io::Write;
        let path = store_dir.join("citations.jsonl");
        let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
        write!(f, "{{\"memory\":").unwrap();
        drop(f);

        let store2 = Store::open(&store_dir).unwrap();
        assert_eq!(store2.citations().len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn two_stores_isolated() {
        let dir1 = temp_dir("iso1");
        let dir2 = temp_dir("iso2");
        let s1dir = dir1.join(".doyle");
        let s2dir = dir2.join(".doyle");
        {
            let mut s1 = Store::open(&s1dir).unwrap();
            s1.record_citation(sample_citation("e1", "a", "m1"))
                .unwrap();
        }
        {
            let mut s2 = Store::open(&s2dir).unwrap();
            s2.record_citation(sample_citation("e2", "b", "m2"))
                .unwrap();
        }
        let a = Store::open(&s1dir).unwrap();
        let b = Store::open(&s2dir).unwrap();
        assert_eq!(a.citations().len(), 1);
        assert_eq!(a.citations()[0].elements[0], ElementId("e1".to_string()));
        assert_eq!(b.citations().len(), 1);
        assert_eq!(b.citations()[0].elements[0], ElementId("e2".to_string()));
        let _ = fs::remove_dir_all(&dir1);
        let _ = fs::remove_dir_all(&dir2);
    }

    #[test]
    fn reload_flags_and_proposals() {
        let dir = temp_dir("reload2");
        let store_dir = dir.join(".doyle");
        let mut store = Store::open(&store_dir).unwrap();
        let fid = store
            .record_flag(Flag {
                id: String::new(),
                memory: MemoryRef {
                    adapter: "a".to_string(),
                    id: "m".to_string(),
                },
                changes: vec![],
                severity: Severity::Breaking,
                old_version: "v1".to_string(),
                new_version: "v2".to_string(),
                evidence: None,
            })
            .unwrap();
        let pid = store
            .record_proposal(Proposal {
                id: String::new(),
                flag_id: fid.clone(),
                action: ProposalAction::Retire,
                rationale: "r".to_string(),
                proposed_by: "m".to_string(),
            })
            .unwrap();
        let _ = store
            .record_decision(Decision {
                id: String::new(),
                proposal_id: pid.clone(),
                verdict: DecisionVerdict::Retire,
                decided_by: "u".to_string(),
                decided_at: "t".to_string(),
            })
            .unwrap();
        let s = Store::open(&store_dir).unwrap();
        assert_eq!(s.flags().len(), 1);
        assert_eq!(s.proposals().len(), 1);
        assert_eq!(s.decisions().len(), 1);
        assert_eq!(s.proposals_for_flag(&fid).len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ids_continue_across_reopen() {
        let dir = temp_dir("idcont");
        let store_dir = dir.join(".doyle");
        // round 1
        {
            let mut store = Store::open(&store_dir).unwrap();
            let fid = store
                .record_flag(Flag {
                    id: String::new(),
                    memory: MemoryRef {
                        adapter: "a".to_string(),
                        id: "m".to_string(),
                    },
                    changes: vec![],
                    severity: Severity::Breaking,
                    old_version: "v1".to_string(),
                    new_version: "v2".to_string(),
                    evidence: None,
                })
                .unwrap();
            assert_eq!(fid, "flag-1");
            let pid = store
                .record_proposal(Proposal {
                    id: String::new(),
                    flag_id: fid.clone(),
                    action: ProposalAction::Keep,
                    rationale: "r".to_string(),
                    proposed_by: "m".to_string(),
                })
                .unwrap();
            assert_eq!(pid, "proposal-1");
            let did = store
                .record_decision(Decision {
                    id: String::new(),
                    proposal_id: pid.clone(),
                    verdict: DecisionVerdict::Approve,
                    decided_by: "u".to_string(),
                    decided_at: "t".to_string(),
                })
                .unwrap();
            assert_eq!(did, "decision-1");
        }
        // round 2: reopen, ids must be fresh
        {
            let mut store = Store::open(&store_dir).unwrap();
            let fid = store
                .record_flag(Flag {
                    id: String::new(),
                    memory: MemoryRef {
                        adapter: "a".to_string(),
                        id: "m".to_string(),
                    },
                    changes: vec![],
                    severity: Severity::Breaking,
                    old_version: "v1".to_string(),
                    new_version: "v2".to_string(),
                    evidence: None,
                })
                .unwrap();
            assert_eq!(fid, "flag-2");
            let pid = store
                .record_proposal(Proposal {
                    id: String::new(),
                    flag_id: fid.clone(),
                    action: ProposalAction::Keep,
                    rationale: "r".to_string(),
                    proposed_by: "m".to_string(),
                })
                .unwrap();
            assert_eq!(pid, "proposal-2");
            let did = store
                .record_decision(Decision {
                    id: String::new(),
                    proposal_id: pid.clone(),
                    verdict: DecisionVerdict::Approve,
                    decided_by: "u".to_string(),
                    decided_at: "t".to_string(),
                })
                .unwrap();
            assert_eq!(did, "decision-2");
        }
        // round 3: reload, all persist
        {
            let store = Store::open(&store_dir).unwrap();
            assert_eq!(store.flags().len(), 2);
            assert_eq!(store.proposals().len(), 2);
            assert_eq!(store.decisions().len(), 2);
            let flag_ids: Vec<&str> = store.flags().iter().map(|f| f.id.as_str()).collect();
            assert!(flag_ids.contains(&"flag-1"));
            assert!(flag_ids.contains(&"flag-2"));
            assert_ne!(flag_ids[0], flag_ids[1]);
            let proposal_ids: Vec<&str> = store.proposals().iter().map(|p| p.id.as_str()).collect();
            assert!(proposal_ids.contains(&"proposal-1"));
            assert!(proposal_ids.contains(&"proposal-2"));
            let decision_ids: Vec<&str> = store.decisions().iter().map(|d| d.id.as_str()).collect();
            assert!(decision_ids.contains(&"decision-1"));
            assert!(decision_ids.contains(&"decision-2"));
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_flag_with_proposal_but_no_decision() {
        let dir = temp_dir("openprop");
        let store_dir = dir.join(".doyle");
        let mut store = Store::open(&store_dir).unwrap();
        let fid = store
            .record_flag(Flag {
                id: String::new(),
                memory: MemoryRef {
                    adapter: "a".to_string(),
                    id: "m".to_string(),
                },
                changes: vec![],
                severity: Severity::Review,
                old_version: "v1".to_string(),
                new_version: "v2".to_string(),
                evidence: None,
            })
            .unwrap();
        let _pid = store
            .record_proposal(Proposal {
                id: String::new(),
                flag_id: fid.clone(),
                action: ProposalAction::Keep,
                rationale: "r".to_string(),
                proposed_by: "m".to_string(),
            })
            .unwrap();
        let open = store.open_flags();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].id, fid);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_flag_with_decided_and_pending_proposals() {
        let dir = temp_dir("openmulti");
        let store_dir = dir.join(".doyle");
        let mut store = Store::open(&store_dir).unwrap();
        let fid = store
            .record_flag(Flag {
                id: String::new(),
                memory: MemoryRef {
                    adapter: "a".to_string(),
                    id: "m".to_string(),
                },
                changes: vec![],
                severity: Severity::Breaking,
                old_version: "v1".to_string(),
                new_version: "v2".to_string(),
                evidence: None,
            })
            .unwrap();
        let pid1 = store
            .record_proposal(Proposal {
                id: String::new(),
                flag_id: fid.clone(),
                action: ProposalAction::Keep,
                rationale: "r1".to_string(),
                proposed_by: "m".to_string(),
            })
            .unwrap();
        store
            .record_decision(Decision {
                id: String::new(),
                proposal_id: pid1.clone(),
                verdict: DecisionVerdict::Approve,
                decided_by: "u".to_string(),
                decided_at: "t".to_string(),
            })
            .unwrap();
        let _pid2 = store
            .record_proposal(Proposal {
                id: String::new(),
                flag_id: fid.clone(),
                action: ProposalAction::Keep,
                rationale: "r2".to_string(),
                proposed_by: "m".to_string(),
            })
            .unwrap();
        let open = store.open_flags();
        // documented rule: closed because a decision exists behind one of its proposals
        assert!(open.iter().all(|f| f.id != fid));
        assert_eq!(open.len(), 0);
        let _ = fs::remove_dir_all(&dir);
    }
}
