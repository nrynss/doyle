//! `doyle serve`: the review page.
//!
//! A local, server-rendered page with no build step. For each open flag it
//! shows what changed (the Vimanam change, old and new), the memory as each
//! configured memory system holds it, which agent sessions relied on it
//! (provenance, when recorded), and the proposal with its rationale. The
//! approve / edit / retire / keep actions call
//! [`apply_decision`](crate::core::decide::apply_decision); a settled flag
//! shows the recorded decision and, per memory system, what was written
//! where. Failed write-backs stay visible on the settled page and can be
//! retried from it ([`retry_failed`](crate::core::decide::retry_failed)) —
//! the per-adapter reports live in this process, as T5.3 left them to the
//! review page; after a restart the settled page still shows the recorded
//! decision and reads each memory back so its current state is visible.
//!
//! The server is `std::net` by hand: the page is one local, single-user
//! surface, requests are handled one at a time (the adapters keep
//! process-local state and are not `Sync`), and the standard library
//! covers listening, parsing one request and writing one response. No
//! server crate is needed for that, so none is added.
//!
//! Which copies hold a flagged memory is resolved here, as T5.3's caller
//! contract requires: the flag's own reference first (read back, or found
//! by its cited text when the store's ID is a logical one such as `M1`),
//! then every other configured adapter searched for the same text. A stored
//! note matches when it equals the cited text or starts with it (the cite
//! path keeps the short restated claim while the note itself is long); a
//! short note that merely prefixes the cited text is refused, so a prefix
//! decoy can never stand in for the true copy.
//!
//! Serving from another directory: `cite` and `check` anchor everything on
//! the state dir, and `serve` follows them. The provenance file resolves
//! relative to the state dir first (`<state_dir>/provenance.json`), then the
//! legacy demo location (`testdata/memories/provenance.json`, cwd-relative);
//! when neither exists the page names the state-dir path instead of claiming
//! nothing was recorded. Contract paths are used exactly as recorded (the
//! same way `check` resolves them), so cite/check with absolute contract
//! paths — or serve from the same cwd for relative ones. The muse-code
//! adapter resolves from serve's cwd, so serve from the cwd that holds the
//! memories.

pub mod logo;
mod render;

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::adapters::MemoryAdapter;
use crate::adapters::lambo::LamboAdapter;
use crate::adapters::muse_code::MuseCodeAdapter;
use crate::cli;
use crate::contract::ContractIndex;
use crate::core::decide::{
    AdapterOutcome, DecisionReport, PerAdapterResult, Target, apply_decision, retry_failed,
};
use crate::core::{Citation, Decision, DecisionVerdict, Flag, Proposal, ProposalAction, Store};

use render::{esc, page, severity_badge};

/// The port `doyle serve` listens on (reserved for Doyle in the demo's port
/// assignments), unless `DOYLE_SERVE_PORT` overrides it.
pub const DEFAULT_PORT: u16 = 8080;

// ---------------------------------------------------------------------------
// Provenance: which sessions relied on a memory, when that was recorded.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct ReliedOn {
    session: String,
    step: String,
    how: String,
}

#[derive(Debug, Clone)]
struct ProvenanceMemory {
    id: String,
    text: String,
    lambo_node: Option<String>,
    muse_file: Option<String>,
    written_by: Option<String>,
    relied_on_by: Vec<ReliedOn>,
}

#[derive(Debug, Clone, Default)]
pub struct Provenance {
    sessions: HashMap<String, (String, String)>,
    memories: Vec<ProvenanceMemory>,
}

impl Provenance {
    /// Parse the provenance JSON shape committed under `testdata/memories/`.
    pub fn parse(text: &str) -> Option<Self> {
        let value: serde_json::Value = serde_json::from_str(text).ok()?;
        let mut sessions = HashMap::new();
        for s in value.get("sessions")?.as_array()? {
            let id = s.get("id")?.as_str()?.to_string();
            let agent = s.get("agent")?.as_str().unwrap_or("").to_string();
            let purpose = s.get("purpose")?.as_str().unwrap_or("").to_string();
            sessions.insert(id, (agent, purpose));
        }
        let mut memories = Vec::new();
        for m in value.get("memories")?.as_array()? {
            let relied_on_by = m
                .get("relied_on_by")
                .and_then(|v| v.as_array())
                .map(|rows| {
                    rows.iter()
                        .filter_map(|r| {
                            Some(ReliedOn {
                                session: r.get("session")?.as_str()?.to_string(),
                                step: r.get("step")?.as_str().unwrap_or("").to_string(),
                                how: r.get("how")?.as_str().unwrap_or("").to_string(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            memories.push(ProvenanceMemory {
                id: m.get("id")?.as_str()?.to_string(),
                text: m.get("text")?.as_str().unwrap_or("").to_string(),
                lambo_node: m
                    .get("lambo")
                    .and_then(|v| v.get("node_id"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                muse_file: m
                    .get("muse_code")
                    .and_then(|v| v.get("file"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                written_by: m
                    .get("written_by")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                relied_on_by,
            });
        }
        Some(Self { sessions, memories })
    }

    fn find(
        &self,
        flag: &Flag,
        texts: &[String],
        copy_ids: &[String],
    ) -> Option<&ProvenanceMemory> {
        self.memories.iter().find(|m| {
            m.id == flag.memory.id
                || texts.iter().any(|t| t == &m.text)
                || copy_ids.iter().any(|id| {
                    id.split('#').next() == m.muse_file.as_deref()
                        || m.lambo_node.as_deref().is_some_and(|n| id.starts_with(n))
                })
        })
    }
}

// ---------------------------------------------------------------------------
// The server state and its routes.
// ---------------------------------------------------------------------------

/// One HTTP response: a status, an HTML body, or a redirect location.
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub body: String,
    pub location: Option<String>,
}

impl Response {
    fn html(status: u16, body: String) -> Self {
        Self {
            status,
            body,
            location: None,
        }
    }
    fn redirect(location: String) -> Self {
        Self {
            status: 303,
            body: String::new(),
            location: Some(location),
        }
    }
}

/// What copy resolution found, and which adapters could not be searched.
struct Resolved {
    copies: Vec<Copy>,
    notes: Vec<String>,
    unsearchable: Vec<Unsearchable>,
}

/// An adapter whose list failed during resolution: no copy is known, but a
/// copy may still be held there. Decide time turns each of these into a
/// `Failed { retryable: true }` report entry (naming the adapter and the
/// list failure) instead of silently dropping the adapter; retry re-searches
/// it by the same cited-text search the decide path uses.
struct Unsearchable {
    adapter: String,
    message: String,
}

/// Placeholder memory id for a report entry where decide time could not
/// resolve a copy: the adapter was unsearchable, so no id is known. Retry
/// re-searches the adapter and replaces this entry with what the search
/// finds (exactly one correction per adapter once the transport heals).
const UNRESOLVED_ID: &str = "<unresolved>";

/// A memory copy the page resolved: which adapter holds it, under which ID.
#[derive(Debug, Clone)]
struct Copy {
    adapter: String,
    adapter_index: Option<usize>,
    memory_id: String,
}

/// The review page's state: Doyle's store, the configured adapters, any
/// recorded provenance, and the per-adapter decision reports this process
/// produced (T5.3 leaves retry state with this caller).
pub struct ReviewServer {
    store: Store,
    adapters: Vec<Box<dyn MemoryAdapter>>,
    provenance: Option<Provenance>,
    provenance_missing_at: Option<PathBuf>,
    reports: HashMap<String, DecisionReport>,
}

impl ReviewServer {
    pub fn new(store: Store, adapters: Vec<Box<dyn MemoryAdapter>>) -> Self {
        Self {
            store,
            adapters,
            provenance: None,
            provenance_missing_at: None,
            reports: HashMap::new(),
        }
    }

    pub fn with_provenance(mut self, provenance: Option<Provenance>) -> Self {
        self.provenance = provenance;
        self
    }

    /// Where the provenance file was looked for but not found. The flag page
    /// names this path instead of claiming nothing was recorded.
    pub fn with_provenance_missing_at(mut self, path: Option<PathBuf>) -> Self {
        self.provenance_missing_at = path;
        self
    }

    /// Route one request. `path` may carry a query string; `body` is the
    /// request body (form fields on the POST routes).
    pub fn handle(&mut self, method: &str, path: &str, body: &str) -> Response {
        let path = path.split(['?', '#']).next().unwrap_or("/");
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        match (method, segments.as_slice()) {
            ("GET", []) => Response::html(200, self.list_page()),
            ("GET", ["flag", id]) => {
                let id = url_decode(id);
                match self.store.flag(&id) {
                    Some(flag) => Response::html(200, self.flag_page(&flag)),
                    None => Response::html(
                        404,
                        page(
                            "Not found",
                            &format!(
                                "<section class=\"card\"><h1>No such flag</h1><p>There is no flag <code>{}</code> in Doyle's state. <a href=\"/\">Back to the review list</a>.</p></section>",
                                esc(&id)
                            ),
                        ),
                    ),
                }
            }
            ("POST", ["flag", id, "decide"]) => self.decide(&url_decode(id), body),
            ("POST", ["flag", id, "retry"]) => self.retry(&url_decode(id)),
            ("GET", ["flag", ..]) | ("POST", ["flag", ..]) => Response::html(
                404,
                page(
                    "Not found",
                    "<section class=\"card\"><h1>Not found</h1><p><a href=\"/\">Back to the review list</a>.</p></section>",
                ),
            ),
            _ if segments.first() == Some(&"flag") => Response::html(
                405,
                page(
                    "Method not allowed",
                    "<section class=\"card\"><h1>Method not allowed</h1><p><a href=\"/\">Back to the review list</a>.</p></section>",
                ),
            ),
            _ => Response::html(
                404,
                page(
                    "Not found",
                    "<section class=\"card\"><h1>Not found</h1><p><a href=\"/\">Back to the review list</a>.</p></section>",
                ),
            ),
        }
    }

    // ----- decision actions -----

    fn decide(&mut self, flag_id: &str, body: &str) -> Response {
        let form = parse_form(body);
        let Some(flag) = self.store.flag(flag_id) else {
            return Response::html(
                404,
                page(
                    "Not found",
                    "<section class=\"card\"><h1>No such flag</h1></section>",
                ),
            );
        };
        let Some(proposal) = self.actionable_proposal(&flag) else {
            return Response::html(
                409,
                page(
                    "Already settled",
                    &format!(
                        "<section class=\"card\"><h1>Already settled</h1><p>Flag <code>{}</code> already has a decision. <a href=\"/flag/{}\">See what was written</a>.</p></section>",
                        esc(&flag.id),
                        esc(&flag.id)
                    ),
                ),
            );
        };
        let verdict = match form.get("verdict").map(String::as_str) {
            Some("approve") => DecisionVerdict::Approve,
            Some("retire") => DecisionVerdict::Retire,
            Some("keep") => DecisionVerdict::Keep,
            Some("edit") => {
                let text = form
                    .get("new_text")
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                if text.is_empty() {
                    return Response::html(
                        400,
                        page(
                            "Missing text",
                            &format!(
                                "<section class=\"card\"><h1>The edited text is empty</h1><p>Write the corrected memory, or approve the proposal as it stands. <a href=\"/flag/{}\">Back to the flag</a>.</p></section>",
                                esc(&flag.id)
                            ),
                        ),
                    );
                }
                DecisionVerdict::EditThenApprove { new_text: text }
            }
            _ => {
                return Response::html(
                    400,
                    page(
                        "Unknown action",
                        "<section class=\"card\"><h1>Unknown action</h1><p>Use approve, edit, retire or keep.</p></section>",
                    ),
                );
            }
        };
        let decided_by = form
            .get("decided_by")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "reviewer".to_string());

        let copies = self.resolve_copies(&flag);
        let targets: Vec<Target> = copies
            .copies
            .iter()
            .filter_map(|c| {
                c.adapter_index.map(|i| Target {
                    adapter: self.adapters[i].as_ref(),
                    memory_id: c.memory_id.clone(),
                })
            })
            .collect();
        if targets.is_empty() && copies.unsearchable.is_empty() {
            return Response::html(
                409,
                page(
                    "No memory system configured",
                    &format!(
                        "<section class=\"card\"><h1>No configured memory system holds this memory</h1><p>Doyle knows the flag, but none of the configured adapters ({}) holds a copy it can write to, so no decision was recorded. <a href=\"/flag/{}\">Back to the flag</a>.</p></section>",
                        esc(&self.adapter_names()),
                        esc(&flag.id)
                    ),
                ),
            );
        }
        match apply_decision(
            &mut self.store,
            &proposal,
            &verdict,
            &decided_by,
            &utc_now_rfc3339(),
            &targets,
        ) {
            Ok(mut report) => {
                // An adapter that could not be searched contributes no
                // target, so the core never sees it: synthesize a
                // failed-retryable entry naming the adapter and the list
                // failure, or the correction for that system would need a
                // whole new flag cycle (the decision settles the flag, and
                // re-decide is refused 409).
                for unsearchable in &copies.unsearchable {
                    report.results.push(PerAdapterResult {
                        adapter: unsearchable.adapter.clone(),
                        memory_id: UNRESOLVED_ID.to_string(),
                        outcome: AdapterOutcome::Failed {
                            message: format!(
                                "{}: could not be searched at decide time ({}); no copy resolved, retry re-searches",
                                unsearchable.adapter, unsearchable.message
                            ),
                            retryable: true,
                        },
                    });
                }
                self.reports.insert(proposal.id.clone(), report);
                Response::redirect(format!("/flag/{}", flag.id))
            }
            Err(e) => Response::html(
                500,
                page(
                    "Could not record the decision",
                    &format!(
                        "<section class=\"card\"><h1>Could not record the decision</h1><p>{}</p><p>No memory was touched. <a href=\"/flag/{}\">Back to the flag</a>.</p></section>",
                        esc(&e.to_string()),
                        esc(&flag.id)
                    ),
                ),
            ),
        }
    }

    fn retry(&mut self, flag_id: &str) -> Response {
        let Some(flag) = self.store.flag(flag_id) else {
            return Response::html(
                404,
                page(
                    "Not found",
                    "<section class=\"card\"><h1>No such flag</h1></section>",
                ),
            );
        };
        let Some((proposal, decision)) = self.settled(&flag) else {
            return Response::html(
                409,
                page(
                    "Nothing to retry",
                    &format!(
                        "<section class=\"card\"><h1>Nothing to retry</h1><p>Flag <code>{}</code> has no decision yet. <a href=\"/flag/{}\">Back to the flag</a>.</p></section>",
                        esc(&flag.id),
                        esc(&flag.id)
                    ),
                ),
            );
        };
        let Some(report) = self.reports.get(&proposal.id).cloned() else {
            return Response::html(
                409,
                page(
                    "Nothing to retry here",
                    &format!(
                        "<section class=\"card\"><h1>The write-back results are not in this process</h1><p>The decision was recorded before this page last started, so there is no failed write-back here to retry. Read the memories on <a href=\"/flag/{}\">the flag's page</a> to see their current state.</p></section>",
                        esc(&flag.id)
                    ),
                ),
            );
        };
        let failed: Vec<(String, String)> = report
            .results
            .iter()
            .filter(|r| matches!(r.outcome, AdapterOutcome::Failed { .. }))
            .map(|r| (r.adapter.clone(), r.memory_id.clone()))
            .collect();
        if failed.is_empty() {
            return Response::redirect(format!("/flag/{}", flag.id));
        }
        // Adapters that were unsearchable at decide time carry the
        // UNRESOLVED_ID placeholder: re-resolve them by the same cited-text
        // search the decide path uses, now that the transport may have
        // healed. Resolved failures still match by (adapter, memory id).
        let unresolved: Vec<String> = failed
            .iter()
            .filter(|(_, m)| m == UNRESOLVED_ID)
            .map(|(a, _)| a.clone())
            .collect();
        let copies = self.resolve_copies(&flag);
        let targets: Vec<Target> = copies
            .copies
            .iter()
            .filter(|c| {
                failed
                    .iter()
                    .any(|(a, m)| a == &c.adapter && m == &c.memory_id)
                    || unresolved.iter().any(|a| a == &c.adapter)
            })
            .filter_map(|c| {
                c.adapter_index.map(|i| Target {
                    adapter: self.adapters[i].as_ref(),
                    memory_id: c.memory_id.clone(),
                })
            })
            .collect();
        let retried = retry_failed(&proposal, &decision.verdict, &targets);
        if let Some(stored) = self.reports.get_mut(&proposal.id) {
            for result in retried {
                if let Some(slot) = stored
                    .results
                    .iter_mut()
                    .find(|r| r.adapter == result.adapter && r.memory_id == result.memory_id)
                {
                    *slot = result;
                } else if let Some(slot) = stored
                    .results
                    .iter_mut()
                    .find(|r| r.adapter == result.adapter && r.memory_id == UNRESOLVED_ID)
                {
                    // The re-search found the copy decide time could not:
                    // replace the placeholder with the actual outcome.
                    *slot = result;
                } else {
                    // The re-search found more copies than placeholders
                    // (e.g. two copies in one healed adapter): disclose each
                    // additional outcome rather than dropping it.
                    stored.results.push(result);
                }
            }
        }
        Response::redirect(format!("/flag/{}", flag.id))
    }

    // ----- store lookups -----

    fn adapter_names(&self) -> String {
        if self.adapters.is_empty() {
            return "none".to_string();
        }
        self.adapters
            .iter()
            .map(|a| a.name().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn citation_for(&self, flag: &Flag) -> Option<Citation> {
        self.store
            .citations_for_memory(&flag.memory)
            .into_iter()
            .next()
    }

    fn proposals_for(&self, flag: &Flag) -> Vec<Proposal> {
        self.store.proposals_for_flag(&flag.id)
    }

    /// The proposal a decision would settle: the newest one with no decision.
    fn actionable_proposal(&self, flag: &Flag) -> Option<Proposal> {
        self.proposals_for(flag)
            .into_iter()
            .rev()
            .find(|p| self.store.decision_for_proposal(&p.id).is_none())
    }

    /// The settled (proposal, decision) pair behind a flag, if one exists.
    fn settled(&self, flag: &Flag) -> Option<(Proposal, Decision)> {
        for proposal in self.proposals_for(flag) {
            if let Some(decision) = self.store.decision_for_proposal(&proposal.id) {
                return Some((proposal, decision));
            }
        }
        None
    }

    // ----- resolving which copies hold the memory -----

    /// The texts that identify the flagged memory in any system: the cited
    /// source text, the claim, and the flag's evidence quote.
    fn match_texts(&self, flag: &Flag) -> Vec<String> {
        let mut texts = Vec::new();
        if let Some(citation) = self.citation_for(flag) {
            if !citation.source_text.is_empty() {
                texts.push(citation.source_text.clone());
            }
            if !citation.claim.is_empty() {
                texts.push(citation.claim.clone());
            }
        }
        if let Some(evidence) = &flag.evidence
            && !evidence.is_empty()
        {
            texts.push(evidence.clone());
        }
        texts
    }

    /// Whether a stored note is the flagged memory: exact equality, or the
    /// stored note starting with the cited text (length >= 8). The cite
    /// path records the short restated claim while the note itself is long,
    /// so this direction must match. The reverse — a short stored note that
    /// merely prefixes the cited text — is refused: it lets a short decoy
    /// sharing a prefix resolve as the copy. Residual: two distinct long
    /// notes sharing the cited prefix both resolve; the page lists every
    /// resolved copy so the operator sees exactly what approve will write.
    fn text_matches(memory_text: &str, texts: &[String]) -> bool {
        texts.iter().any(|t| {
            memory_text == t || (t.chars().count() >= 8 && memory_text.starts_with(t.as_str()))
        })
    }

    /// Every copy of the flagged memory the configured adapters hold. The
    /// flag's own system is tried first (by its recorded ID, then by text),
    /// then the others by text. Adapters that cannot be searched are noted
    /// for the page and recorded as unsearchable, never silently skipped:
    /// decide time marks them failed-retryable and retry re-searches them.
    fn resolve_copies(&self, flag: &Flag) -> Resolved {
        let texts = self.match_texts(flag);
        let mut copies: Vec<Copy> = Vec::new();
        let mut notes: Vec<String> = Vec::new();
        let mut unsearchable: Vec<Unsearchable> = Vec::new();
        let push = |copy: Copy, copies: &mut Vec<Copy>| {
            if !copies
                .iter()
                .any(|c: &Copy| c.adapter == copy.adapter && c.memory_id == copy.memory_id)
            {
                copies.push(copy);
            }
        };
        for (index, adapter) in self.adapters.iter().enumerate() {
            let name = adapter.name().to_string();
            if name == flag.memory.adapter && adapter.read(&flag.memory.id).is_ok() {
                push(
                    Copy {
                        adapter: name,
                        adapter_index: Some(index),
                        memory_id: flag.memory.id.clone(),
                    },
                    &mut copies,
                );
                continue;
            }
            // Search by the cited text: the list query, then an unfiltered
            // pass for enumerable adapters when the query found nothing.
            let query = texts.first().cloned().unwrap_or_default();
            let mut found: Vec<String> = Vec::new();
            let mut list_error: Option<String> = None;
            if !query.is_empty() {
                match adapter.list(&query) {
                    Ok(memories) => {
                        found.extend(
                            memories
                                .iter()
                                .filter(|m| Self::text_matches(&m.text, &texts))
                                .map(|m| m.reference.id.clone()),
                        );
                    }
                    Err(e) => {
                        list_error = Some(e.to_string());
                        notes.push(format!(
                            "{} could not be searched ({}), so a copy held there, if any, is not shown; it is marked failed-retryable and a retry re-searches it.",
                            name,
                            e
                        ));
                    }
                }
            }
            if found.is_empty()
                && let Ok(memories) = adapter.list("")
            {
                found.extend(
                    memories
                        .iter()
                        .filter(|m| Self::text_matches(&m.text, &texts))
                        .map(|m| m.reference.id.clone()),
                );
            }
            for memory_id in found {
                push(
                    Copy {
                        adapter: name.clone(),
                        adapter_index: Some(index),
                        memory_id,
                    },
                    &mut copies,
                );
            }
            if !copies
                .iter()
                .any(|c| c.adapter == name && c.adapter_index.is_some())
                && let Some(message) = list_error
            {
                unsearchable.push(Unsearchable {
                    adapter: name,
                    message,
                });
            }
        }
        // The flag's own reference, when no configured adapter holds it:
        // shown from Doyle's state, with the gap named on the page.
        if !copies.iter().any(|c| c.adapter == flag.memory.adapter) {
            copies.insert(
                0,
                Copy {
                    adapter: flag.memory.adapter.clone(),
                    adapter_index: None,
                    memory_id: flag.memory.id.clone(),
                },
            );
        }
        Resolved {
            copies,
            notes,
            unsearchable,
        }
    }

    // ----- pages -----

    fn list_page(&self) -> String {
        let open = self.store.open_flags();
        let settled: Vec<Flag> = self
            .store
            .flags()
            .iter()
            .filter(|f| !open.iter().any(|o| o.id == f.id))
            .cloned()
            .collect();
        let mut body = String::new();
        body.push_str(&format!(
            "<h1>Review</h1><p>{} open · {} settled. An open flag is a memory a contract change has made stale; settle it and the correction is written back to every memory system that holds it.</p>",
            open.len(),
            settled.len()
        ));
        if open.is_empty() {
            body.push_str("<section class=\"card\"><h2>Open flags</h2><p>Nothing is flagged stale. Every cited memory still matches the contracts Doyle has checked.</p></section>");
        } else {
            body.push_str("<h2>Open flags</h2>");
            for flag in &open {
                body.push_str(&self.flag_card(flag, true));
            }
        }
        if !settled.is_empty() {
            body.push_str("<h2>Settled</h2>");
            for flag in &settled {
                body.push_str(&self.flag_card(flag, false));
            }
        }
        page("Review", &body)
    }

    fn flag_card(&self, flag: &Flag, open: bool) -> String {
        let claim = self
            .citation_for(flag)
            .map(|c| c.claim)
            .or_else(|| flag.evidence.clone())
            .unwrap_or_else(|| flag.memory.id.clone());
        let proposal_note = match self.proposals_for(flag).last() {
            Some(p) => format!("Proposal: {}", action_word(&p.action)),
            None => "No proposal yet".to_string(),
        };
        let state = if open {
            "<span class=\"badge state-open\">open</span>"
        } else {
            "<span class=\"badge state-settled\">settled</span>"
        };
        format!(
            "<a class=\"flaglink\" href=\"/flag/{id}\"><section class=\"card\"><h2>{claim}</h2><p>{sev} {state} <span class=\"badge\">{proposal}</span></p><p class=\"small muted\">Flag <code>{id}</code> · memory <code>{adapter}/{mid}</code> · changed <span class=\"mono\">{old}</span> → <span class=\"mono\">{new}</span> · {changes} change(s)</p></section></a>",
            id = esc(&flag.id),
            claim = esc(&claim),
            sev = severity_badge(flag.severity),
            state = state,
            proposal = esc(&proposal_note),
            adapter = esc(&flag.memory.adapter),
            mid = esc(&flag.memory.id),
            old = esc(&short_version(&flag.old_version)),
            new = esc(&short_version(&flag.new_version)),
            changes = flag.changes.len(),
        )
    }

    fn flag_page(&self, flag: &Flag) -> String {
        let citation = self.citation_for(flag);
        let resolved = self.resolve_copies(flag);
        let copies = &resolved.copies;
        let settled = self.settled(flag);
        let mut body = String::new();

        let claim = citation
            .as_ref()
            .map(|c| c.claim.clone())
            .or_else(|| flag.evidence.clone())
            .unwrap_or_else(|| flag.memory.id.clone());
        let state = if settled.is_some() {
            "<span class=\"badge state-settled\">settled</span>"
        } else {
            "<span class=\"badge state-open\">open</span>"
        };
        body.push_str(&format!(
            "<p class=\"small\"><a href=\"/\">← All flags</a></p><h1>{}</h1><p>{} {} <span class=\"badge\">flag <span class=\"mono\">{}</span></span></p>",
            esc(&claim),
            severity_badge(flag.severity),
            state,
            esc(&flag.id),
        ));
        if let Some(evidence) = &flag.evidence {
            body.push_str(&format!(
                "<p class=\"small muted\">Evidence from the memory: “{}”</p>",
                esc(evidence)
            ));
        }

        body.push_str("<div class=\"grid\">");
        body.push_str(&self.change_section(flag, citation.as_ref()));
        body.push_str(&self.memory_section(flag, copies, settled.is_some(), &resolved.notes));
        body.push_str(&self.provenance_section(flag, copies));
        body.push_str(&self.proposal_section(flag));
        body.push_str("</div>");

        if let Some((proposal, decision)) = &settled {
            body.push_str(&self.settled_section(flag, proposal, decision, copies));
        } else if let Some(proposal) = self.actionable_proposal(flag) {
            body.push_str(&self.decide_section(flag, &proposal));
        } else {
            body.push_str("<section class=\"card\"><h2>Decide</h2><p>No proposal has been recorded for this flag yet, so there is nothing to approve. Run <code>doyle check</code> again to have the re-check propose one.</p></section>");
        }
        page(&format!("Flag {}", flag.id), &body)
    }

    /// What changed: each Vimanam change fact behind the flag, with the old
    /// and new contract text for the elements the memory cites.
    fn change_section(&self, flag: &Flag, citation: Option<&Citation>) -> String {
        let mut out = String::from("<section class=\"card wide\"><h2>What changed</h2>");
        out.push_str(&format!(
            "<p class=\"small\">Contract <span class=\"mono\">{}</span> → <span class=\"mono\">{}</span> · {} change(s) behind this flag.</p>",
            esc(&short_version(&flag.old_version)),
            esc(&short_version(&flag.new_version)),
            flag.changes.len()
        ));
        let old = ContractIndex::load(&flag.old_version);
        let new = ContractIndex::load(&flag.new_version);
        match (old, new) {
            (Ok(old), Ok(new)) => {
                let changes = old.diff(&new).unwrap_or_default();
                out.push_str("<table><tr><th>Change</th><th>Kind</th><th>Severity</th><th>Old → new</th></tr>");
                for cid in &flag.changes {
                    match changes.iter().find(|c| c.id == cid.0) {
                        Some(change) => {
                            let subject = subject_label(&old, &new, &change.subject);
                            out.push_str(&format!(
                                "<tr><td><span class=\"mono\">{}</span><br><span class=\"small muted\">{}</span></td><td>{}</td><td>{}</td><td>{}</td></tr>",
                                esc(&short_id(&cid.0)),
                                esc(&subject),
                                esc(&json_word(&change.kind)),
                                esc(&json_word(&change.severity)),
                                esc(&change_summary(change)),
                            ));
                        }
                        None => {
                            out.push_str(&format!(
                                "<tr><td><span class=\"mono\">{}</span></td><td colspan=\"3\" class=\"muted\">Not in the diff of the two contract files as recorded.</td></tr>",
                                esc(&short_id(&cid.0))
                            ));
                        }
                    }
                }
                out.push_str("</table>");
                if let Some(citation) = citation
                    && !citation.elements.is_empty()
                {
                    out.push_str("<h3 class=\"small\">The cited elements, old and new</h3>");
                    for element in &citation.elements {
                        let name = subject_label(&old, &new, &element.0);
                        let old_text = old
                            .entity(element)
                            .map(|e| {
                                pretty_json(
                                    &serde_json::to_value(&e.attributes).unwrap_or_default(),
                                )
                            })
                            .unwrap_or_else(|| "(absent in the old contract)".to_string());
                        let new_text = new
                            .entity(element)
                            .map(|e| {
                                pretty_json(
                                    &serde_json::to_value(&e.attributes).unwrap_or_default(),
                                )
                            })
                            .unwrap_or_else(|| "(removed in the new contract)".to_string());
                        out.push_str(&format!(
                            "<p class=\"small\"><strong>{}</strong> <span class=\"mono muted\">{}</span></p><div class=\"diff\"><div class=\"old\"><h3>Old</h3><pre>{}</pre></div><div class=\"new\"><h3>New</h3><pre>{}</pre></div></div>",
                            esc(&name),
                            esc(&short_id(&element.0)),
                            esc(&old_text),
                            esc(&new_text),
                        ));
                    }
                }
            }
            _ => {
                out.push_str("<p>The contract files named by this flag cannot be read from here, so the change facts are listed by ID:</p><ul>");
                for cid in &flag.changes {
                    out.push_str(&format!(
                        "<li><span class=\"mono\">{}</span></li>",
                        esc(&cid.0)
                    ));
                }
                out.push_str("</ul>");
            }
        }
        out.push_str("</section>");
        out
    }

    /// The memory as each system holds it.
    fn memory_section(
        &self,
        flag: &Flag,
        copies: &[Copy],
        settled: bool,
        notes: &[String],
    ) -> String {
        let mut out = String::from(
            "<section class=\"card wide\"><h2>The memory, as each system holds it</h2>",
        );
        if copies.is_empty() {
            out.push_str("<p>No copy could be resolved.</p>");
        }
        for note in notes {
            out.push_str(&format!("<p class=\"failed\">{}</p>", esc(note)));
        }
        for copy in copies {
            out.push_str(&format!(
                "<h3 class=\"small\">{} · <span class=\"mono\">{}</span></h3>",
                esc(&copy.adapter),
                esc(&copy.memory_id)
            ));
            match copy.adapter_index {
                Some(index) => match self.adapters[index].read(&copy.memory_id) {
                    Ok(memory) => {
                        out.push_str(&format!(
                            "<p class=\"memory-text\">{}</p><p class=\"small muted\">Scope: {}{}</p>",
                            esc(&memory.text),
                            esc(&memory.scope),
                            match &memory.updated_at {
                                Some(t) => format!(" · last updated {}", esc(t)),
                                None => String::new(),
                            }
                        ));
                    }
                    Err(e) => {
                        out.push_str(&format!(
                            "<p class=\"failed\">Could not be read: {}</p>",
                            esc(&e.to_string())
                        ));
                    }
                },
                None => {
                    let held = self
                        .citation_for(flag)
                        .map(|c| {
                            if c.source_text.is_empty() {
                                c.claim.clone()
                            } else {
                                c.source_text.clone()
                            }
                        })
                        .or_else(|| flag.evidence.clone())
                        .unwrap_or_default();
                    out.push_str(&format!(
                        "<p class=\"memory-text\">{}</p><p class=\"small muted\">Held by <strong>{}</strong>, which is not configured in this serving process, so Doyle shows the text it cited and cannot write the correction there.{}</p>",
                        esc(&held),
                        esc(&copy.adapter),
                        if settled { "" } else { " Configure it and reopen this page to decide." },
                    ));
                }
            }
        }
        out.push_str("</section>");
        out
    }

    /// Which agent sessions relied on the memory, when provenance is recorded.
    fn provenance_section(&self, flag: &Flag, copies: &[Copy]) -> String {
        let mut out = String::from("<section class=\"card\"><h2>Who relied on it</h2>");
        let texts = self.match_texts(flag);
        let mut read_texts = texts.clone();
        for copy in copies {
            if let Some(index) = copy.adapter_index
                && let Ok(memory) = self.adapters[index].read(&copy.memory_id)
            {
                read_texts.push(memory.text);
            }
        }
        let copy_ids: Vec<String> = copies.iter().map(|c| c.memory_id.clone()).collect();
        match self
            .provenance
            .as_ref()
            .and_then(|p| p.find(flag, &read_texts, &copy_ids))
        {
            Some(entry) => {
                if let Some(writer) = &entry.written_by {
                    let label = self
                        .provenance
                        .as_ref()
                        .and_then(|p| p.sessions.get(writer))
                        .map(|(agent, purpose)| format!("{writer} ({agent} — {purpose})"))
                        .unwrap_or_else(|| writer.clone());
                    out.push_str(&format!(
                        "<p class=\"small\">Written by {}</p>",
                        esc(&label)
                    ));
                }
                if entry.relied_on_by.is_empty() {
                    out.push_str("<p>No later session is recorded as relying on this memory.</p>");
                } else {
                    out.push_str("<table><tr><th>Session</th><th>Step</th><th>How it relied on the memory</th></tr>");
                    for row in &entry.relied_on_by {
                        let session = self
                            .provenance
                            .as_ref()
                            .and_then(|p| p.sessions.get(&row.session))
                            .map(|(agent, _)| format!("{} ({})", row.session, agent))
                            .unwrap_or_else(|| row.session.clone());
                        out.push_str(&format!(
                            "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
                            esc(&session),
                            esc(&row.step),
                            esc(&row.how)
                        ));
                    }
                    out.push_str("</table>");
                }
            }
            None => {
                match &self.provenance_missing_at {
                    Some(path) => out.push_str(&format!(
                        "<p>Doyle cannot name the sessions that relied on this memory: provenance file not found at {}.</p>",
                        esc(&path.display().to_string())
                    )),
                    None => out.push_str("<p>No provenance is recorded for this memory, so Doyle cannot name the sessions that relied on it.</p>"),
                }
                if let Some(citation) = self.citation_for(flag) {
                    out.push_str(&format!(
                        "<p class=\"small muted\">The citation was recorded by {} (confidence {:.2}).</p>",
                        esc(&citation.recorded_by),
                        citation.confidence
                    ));
                }
            }
        }
        out.push_str("</section>");
        out
    }

    /// The proposal and its rationale.
    fn proposal_section(&self, flag: &Flag) -> String {
        let mut out = String::from("<section class=\"card\"><h2>Proposal</h2>");
        let proposals = self.proposals_for(flag);
        match proposals.last() {
            Some(proposal) => {
                out.push_str(&format!(
                    "<p><span class=\"badge\">{}</span> <span class=\"small muted\">{} · proposed by {}</span></p><p>{}</p>",
                    esc(action_word(&proposal.action)),
                    esc(&proposal.id),
                    esc(&proposal.proposed_by),
                    esc(&proposal.rationale)
                ));
                if let ProposalAction::Correct { new_text } = &proposal.action {
                    out.push_str(&format!(
                        "<p class=\"small\"><strong>Corrected memory:</strong></p><pre>{}</pre>",
                        esc(new_text)
                    ));
                }
                if proposals.len() > 1 {
                    out.push_str(&format!(
                        "<p class=\"small muted\">{} earlier proposal(s) for this flag are in Doyle's log.</p>",
                        proposals.len() - 1
                    ));
                }
            }
            None => {
                out.push_str("<p>No proposal has been recorded for this flag yet.</p>");
            }
        }
        out.push_str("</section>");
        out
    }

    /// The approve / edit / retire / keep form for an open flag.
    fn decide_section(&self, flag: &Flag, proposal: &Proposal) -> String {
        let proposed_text = match &proposal.action {
            ProposalAction::Correct { new_text } => new_text.clone(),
            _ => String::new(),
        };
        format!(
            "<section class=\"card wide\"><h2>Decide</h2>\
             <p class=\"small\">The decision is recorded first, then written back to every system above. A system that fails is marked for retry on the settled view — never half-silent.</p>\
             <form method=\"post\" action=\"/flag/{id}/decide\">\
             <label for=\"decided_by\">Your name</label>\
             <input type=\"text\" id=\"decided_by\" name=\"decided_by\" value=\"reviewer\">\
             <div class=\"editbox\"><label for=\"new_text\">Edited correction (used only by “Edit then approve”)</label>\
             <textarea id=\"new_text\" name=\"new_text\" rows=\"3\">{text}</textarea></div>\
             <p class=\"actions\">\
             <button type=\"submit\" name=\"verdict\" value=\"approve\">Approve proposal</button>\
             <button type=\"submit\" name=\"verdict\" value=\"edit\" class=\"secondary\">Edit then approve</button>\
             <button type=\"submit\" name=\"verdict\" value=\"retire\" class=\"danger\">Retire memory</button>\
             <button type=\"submit\" name=\"verdict\" value=\"keep\" class=\"secondary\">Keep as is</button>\
             </p></form></section>",
            id = esc(&flag.id),
            text = esc(&proposed_text),
        )
    }

    /// A settled flag: the recorded decision and what was written where.
    fn settled_section(
        &self,
        flag: &Flag,
        proposal: &Proposal,
        decision: &Decision,
        copies: &[Copy],
    ) -> String {
        let mut out =
            String::from("<section class=\"card wide\"><h2>Settled — what was written where</h2>");
        out.push_str(&format!(
            "<p>Decision <code>{}</code>: <strong>{}</strong> on proposal <code>{}</code> ({}) by {} at {}.</p>",
            esc(&decision.id),
            esc(&verdict_word(&decision.verdict)),
            esc(&proposal.id),
            esc(action_word(&proposal.action)),
            esc(&decision.decided_by),
            esc(&decision.decided_at),
        ));
        match self.reports.get(&proposal.id) {
            Some(report) => {
                out.push_str(
                    "<table><tr><th>System</th><th>Memory</th><th>What was done</th></tr>",
                );
                for result in &report.results {
                    out.push_str(&format!(
                        "<tr><td>{}</td><td><span class=\"mono\">{}</span></td><td>{}</td></tr>",
                        esc(&result.adapter),
                        esc(&result.memory_id),
                        self.outcome_html(result),
                    ));
                }
                out.push_str("</table>");
                if report.needs_retry()
                    || report
                        .results
                        .iter()
                        .any(|r| matches!(r.outcome, AdapterOutcome::Failed { .. }))
                {
                    out.push_str(&format!(
                        "<form method=\"post\" action=\"/flag/{}/retry\"><p><button type=\"submit\" class=\"secondary\">Retry the failed write-back</button> <span class=\"small muted\">Retries re-issue the identical text, so a write that already landed is not duplicated.</span></p></form>",
                        esc(&flag.id)
                    ));
                }
            }
            None => {
                out.push_str("<p>This decision was recorded before this page last started, so the per-system write results are not in this process. Each memory above is read back as its system now holds it: a superseded note names its correction, and a contradicted memory warns on recall.</p>");
                for copy in copies {
                    if let Some(index) = copy.adapter_index
                        && let Ok(memory) = self.adapters[index].read(&copy.memory_id)
                    {
                        out.push_str(&format!(
                            "<p class=\"small\"><strong>{}</strong> <span class=\"mono\">{}</span> now reads: {}</p>",
                            esc(&copy.adapter),
                            esc(&copy.memory_id),
                            esc(&memory.text)
                        ));
                    }
                }
            }
        }
        out.push_str("</section>");
        out
    }

    fn outcome_html(&self, result: &crate::core::decide::PerAdapterResult) -> String {
        match &result.outcome {
            AdapterOutcome::Superseded { correction } => {
                let mut text = format!(
                    "<span class=\"ok\">Correction written</span> as <span class=\"mono\">{}</span>; the stale memory now names it.",
                    esc(&correction.id)
                );
                if let Some(adapter) = self.adapters.iter().find(|a| a.name() == result.adapter)
                    && let Ok(memory) = adapter.read(&correction.id)
                {
                    text.push_str(&format!("<pre>{}</pre>", esc(&memory.text)));
                }
                text
            }
            AdapterOutcome::Annotated => {
                "<span class=\"ok\">Annotated in place</span>; the warning shows on the memory itself.".to_string()
            }
            AdapterOutcome::WrittenNew { new_memory, stale_remains } => {
                format!(
                    "<span class=\"ok\">New memory written</span> as <span class=\"mono\">{}</span>{}.",
                    esc(&new_memory.id),
                    if *stale_remains { "; the old memory stays, unmarked" } else { "" }
                )
            }
            AdapterOutcome::Kept => {
                "<span class=\"ok\">Kept</span>; nothing was written, by the decision.".to_string()
            }
            AdapterOutcome::Failed { message, retryable } => {
                format!(
                    "<span class=\"failed\">Failed{}</span>: {}",
                    if *retryable { " — marked for retry" } else { "" },
                    esc(message)
                )
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Small rendering helpers.
// ---------------------------------------------------------------------------

fn action_word(action: &ProposalAction) -> &'static str {
    match action {
        ProposalAction::Correct { .. } => "correct",
        ProposalAction::Retire => "retire",
        ProposalAction::Keep => "keep",
    }
}

fn verdict_word(verdict: &DecisionVerdict) -> String {
    match verdict {
        DecisionVerdict::Approve => "approve".to_string(),
        DecisionVerdict::Retire => "retire".to_string(),
        DecisionVerdict::Keep => "keep".to_string(),
        DecisionVerdict::EditThenApprove { new_text } => {
            format!(
                "edit then approve (“{}…”)",
                new_text.chars().take(40).collect::<String>()
            )
        }
    }
}

fn short_id(id: &str) -> String {
    let short: String = id.chars().take(12).collect();
    if id.chars().count() > 12 {
        format!("{short}…")
    } else {
        short
    }
}

fn short_version(version: &str) -> String {
    Path::new(version)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| version.to_string())
}

fn json_word(value: &impl serde::Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

fn pretty_json(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

/// A human label for a contract entity: its name and native location.
fn subject_label(old: &ContractIndex, new: &ContractIndex, entity_id: &str) -> String {
    let id = crate::core::ElementId(entity_id.to_string());
    for index in [old, new] {
        if let Some(entity) = index.entity(&id) {
            let mut label = entity.name.clone();
            if let Some(loc) = index.native_location(&id) {
                let mut parts = Vec::new();
                if let (Some(m), Some(p)) = (&loc.method, &loc.path) {
                    parts.push(format!("{m} {p}"));
                }
                if let Some(op) = &loc.operation_id {
                    parts.push(op.clone());
                }
                if let Some(t) = &loc.type_name {
                    parts.push(match &loc.field_name {
                        Some(f) => format!("{t}.{f}"),
                        None => t.clone(),
                    });
                }
                if !parts.is_empty() {
                    label = format!("{} ({})", label, parts.join(" · "));
                }
            }
            return label;
        }
    }
    entity_id.to_string()
}

/// One line on what a change fact did, old → new where Vimanam recorded it.
fn change_summary(change: &vimanam::contract::Change) -> String {
    let details = &change.details;
    if let Some(schema) = details.get("schema_change") {
        let pointer = schema.get("pointer").and_then(|v| v.as_str()).unwrap_or("");
        let side = |key: &str| -> String {
            let s = schema.get(key).cloned().unwrap_or_default();
            if s.get("present").and_then(|v| v.as_bool()) == Some(false) {
                return "(absent)".to_string();
            }
            // A scalar names itself ("array" -> "object"); a schema is
            // named by its kind, never dumped whole into the table.
            s.get("value")
                .map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Object(map) => map
                        .get("type")
                        .and_then(|t| t.as_str())
                        .map(|t| format!("{t} schema"))
                        .unwrap_or_else(|| "schema".to_string()),
                    other => other.to_string(),
                })
                .unwrap_or_else(|| "(absent)".to_string())
        };
        return format!("{pointer}: {} → {}", side("before"), side("after"));
    }
    if change.native_kind == "pagination_inferred"
        || json_word(&change.kind) == "pagination_changed"
    {
        return "now pages: follow the continuation (next_cursor) to the end".to_string();
    }
    let before = details
        .get("before")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let after = details
        .get("after")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    if !before.is_null() || !after.is_null() {
        return format!("{} → {}", compact(&before), compact(&after));
    }
    change.native_kind.clone()
}

fn compact(value: &serde_json::Value) -> String {
    let text = value.to_string();
    if text.chars().count() > 120 {
        format!("{}…", text.chars().take(120).collect::<String>())
    } else {
        text
    }
}

// ---------------------------------------------------------------------------
// HTTP plumbing: std::net, one request at a time.
// ---------------------------------------------------------------------------

/// Run the `serve` command.
pub fn run(cli: &cli::Cli) -> ExitCode {
    let state_dir = cli
        .common
        .state_dir
        .clone()
        .unwrap_or_else(|| ".doyle".to_string());
    let store = match Store::open(&state_dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("doyle serve: cannot open state at {state_dir}: {e}");
            return ExitCode::from(2);
        }
    };
    let adapters = match build_adapters(cli) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("doyle serve: {e}");
            return ExitCode::from(2);
        }
    };
    let provenance = load_provenance(&state_dir);
    let mut server = ReviewServer::new(store, adapters)
        .with_provenance(provenance.0)
        .with_provenance_missing_at(provenance.1);

    let port = std::env::var("DOYLE_SERVE_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);
    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("doyle serve: cannot listen on 127.0.0.1:{port}: {e}");
            return ExitCode::from(2);
        }
    };
    println!(
        "doyle serve: http://127.0.0.1:{port} (state: {state_dir}, adapters: {})",
        server.adapter_names()
    );
    match serve(&listener, &mut server) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("doyle serve: {e}");
            ExitCode::from(2)
        }
    }
}

fn build_adapters(cli: &cli::Cli) -> Result<Vec<Box<dyn MemoryAdapter>>, String> {
    // The muse-code adapter resolves from serve's cwd: run serve from the
    // cwd that holds the memories (same cwd as cite/check for relative
    // contract paths).
    let names: Vec<String> = if cli.common.adapters.is_empty() {
        vec!["muse-code".to_string(), "lambo".to_string()]
    } else {
        cli.common.adapters.clone()
    };
    let mut adapters: Vec<Box<dyn MemoryAdapter>> = Vec::new();
    for name in &names {
        match name.as_str() {
            "muse-code" | "muse_code" | "musecode" => {
                adapters.push(Box::new(MuseCodeAdapter::new(Path::new("."))));
            }
            "lambo" => {
                let url = std::env::var("LAMBO_URL")
                    .unwrap_or_else(|_| "http://127.0.0.1:7700/mcp".to_string());
                adapters.push(Box::new(LamboAdapter::http(url, "doyle")));
            }
            other => return Err(format!("unknown adapter: {other}")),
        }
    }
    Ok(adapters)
}

/// Where `doyle serve` looks for the provenance file, in order:
/// 1. `<state_dir>/provenance.json` — the primary location, anchored on the
///    same state dir `cite` and `check` use, so serving from any cwd finds it;
/// 2. `<cwd>/testdata/memories/provenance.json` — the legacy demo location,
///    kept so a checkout served from its own root still renders.
fn provenance_candidates(state_dir: &str, cwd: &Path) -> Vec<PathBuf> {
    vec![
        PathBuf::from(state_dir).join("provenance.json"),
        cwd.join("testdata/memories/provenance.json"),
    ]
}

/// Load the provenance file, or report where it was looked for but not
/// found. Returns the parsed provenance (when some candidate reads and
/// parses) and, when no candidate reads, the primary path the flag page
/// names instead of claiming nothing was recorded. A candidate that reads
/// but does not parse counts as absent, as before.
fn load_provenance(state_dir: &str) -> (Option<Provenance>, Option<PathBuf>) {
    load_provenance_from(state_dir, Path::new("."))
}

fn load_provenance_from(state_dir: &str, cwd: &Path) -> (Option<Provenance>, Option<PathBuf>) {
    let candidates = provenance_candidates(state_dir, cwd);
    for candidate in &candidates {
        if let Ok(text) = std::fs::read_to_string(candidate) {
            return (Provenance::parse(&text), None);
        }
    }
    (None, candidates.into_iter().next())
}

/// Accept and answer requests until the listener fails. One at a time: the
/// adapters hold process-local state behind `&self` methods, and a local
/// review page has one reader.
pub fn serve(listener: &TcpListener, server: &mut ReviewServer) -> std::io::Result<()> {
    for stream in listener.incoming() {
        let stream = stream?;
        if let Err(e) = handle_connection(stream, server) {
            eprintln!("doyle serve: request failed: {e}");
        }
    }
    Ok(())
}

fn handle_connection(stream: TcpStream, server: &mut ReviewServer) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);
    let mut first_line = String::new();
    if reader.read_line(&mut first_line)? == 0 {
        return Ok(());
    }
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_string();
    let path = parts.next().unwrap_or("/").to_string();
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            content_length = value.trim().parse().unwrap_or(0);
        }
    }
    if content_length > 1_048_576 {
        let response = Response::html(
            400,
            page(
                "Request too large",
                "<section class=\"card\"><h1>Request too large</h1></section>",
            ),
        );
        return write_response(&mut writer, &response);
    }
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body)?;
    let response = server.handle(&method, &path, &String::from_utf8_lossy(&body));
    write_response(&mut writer, &response)
}

fn write_response(writer: &mut TcpStream, response: &Response) -> std::io::Result<()> {
    let reason = match response.status {
        200 => "OK",
        303 => "See Other",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        _ => "Internal Server Error",
    };
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n",
        response.status,
        reason,
        response.body.len()
    );
    if let Some(location) = &response.location {
        head.push_str(&format!("Location: {location}\r\n"));
    }
    head.push_str("\r\n");
    writer.write_all(head.as_bytes())?;
    writer.write_all(response.body.as_bytes())?;
    writer.flush()
}

// ---------------------------------------------------------------------------
// Form and URL decoding, and the clock (no time crate: RFC 3339 from epoch).
// ---------------------------------------------------------------------------

fn parse_form(body: &str) -> HashMap<String, String> {
    let mut form = HashMap::new();
    for pair in body.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        form.insert(url_decode(key), url_decode(value));
    }
    form
}

fn url_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

/// The current UTC time as RFC 3339, from the system clock.
pub fn utc_now_rfc3339() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    rfc3339_from_epoch(seconds)
}

fn rfc3339_from_epoch(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let secs = seconds % 86_400;
    // Civil date from a day count (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::FakeAdapter;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn temp_dir(label: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!(
            "doyle-serve-test-{label}-{}-{n}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn fixture(rel: &str) -> String {
        format!("{}/{rel}", env!("CARGO_MANIFEST_DIR"))
    }

    const STALE: &str =
        "GET /patients returns every patient in one response; one call exports the whole clinic";
    const M1_CLAIM: &str = "GET /patients returns every patient in one response";
    const LIST_PATIENTS: &str =
        "ve2_c81e37b7c42c484a1859c09e95bf8cec89a360db6743a691afab818f177c7bc6";
    const LIST_PATIENTS_200: &str =
        "ve2_86d3cdb7ce3a7f0b1a2fefa696f0acbbcf2d5406c717bb1f1f1cb5f4f62667ba";
    const GET_PATIENT: &str =
        "ve2_c69ae2bf6960e6f28e01bf913a93aa0a325f6d3ee49d445a68b1f580f27af2b5";
    const GET_PATIENT_404: &str =
        "ve2_6add08d53def059f4e973257dc9e174a5398208bce2fa4d719e0418ca523349e";
    const GET_STATS: &str = "ve2_88f1296b143cdad99952319ae783095b3386438f0c8932538d9bfbe1b7fcfb0c";
    const PATIENT_COUNT: &str =
        "ve2_99aaf77708e08db6ee0c7bd151e3044ef33e089e1bbe915fe889e7330b79058f";
    const DECOMMISSION: &str =
        "ve2_9e9663295485ddf808651f117c8ca42ab58e33fbe49a27309e0547f77c866846";
    const LAMBO_M1: &str = "5e1d0a73-0000-0000-0000-000000000000";

    fn copy_dir(src: &Path, dst: &Path) {
        fs::create_dir_all(dst).unwrap();
        for entry in fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let to = dst.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_dir(&entry.path(), &to);
            } else {
                fs::copy(entry.path(), to).unwrap();
            }
        }
    }

    // ----- a scripted Lambo server, shaped by the recorded transcripts -----

    #[derive(Default)]
    struct LamboState {
        concepts: Vec<(String, String)>,
        links: Vec<(String, String)>,
        warnings: Vec<String>,
        derives: usize,
        receipts: HashMap<String, Vec<String>>,
        next: u32,
        down: bool,
    }

    fn sse(message: serde_json::Value) -> crate::mcp::HttpResponse {
        crate::mcp::HttpResponse {
            status: 200,
            headers: vec![("Mcp-Session-Id".to_string(), "<mcp-session-id>".to_string())],
            body: format!("id: 0\nretry: 3000\ndata: {message}\n\n"),
        }
    }

    fn scripted_lambo(
        seed: Vec<(String, String)>,
    ) -> (
        crate::mcp::FakeTransport,
        std::sync::Arc<std::sync::Mutex<LamboState>>,
    ) {
        let state = std::sync::Arc::new(std::sync::Mutex::new(LamboState {
            concepts: seed,
            ..LamboState::default()
        }));
        let st = state.clone();
        let transport = crate::mcp::FakeTransport::scripted(move |url: &str, body: &str| {
            let v: serde_json::Value =
                serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
            let method = v.get("method").and_then(|m| m.as_str()).unwrap_or("");
            if method == "initialize" {
                return Ok(sse(serde_json::json!({
                    "jsonrpc": "2.0", "id": v.get("id"),
                    "result": {"protocolVersion": "2025-06-18", "capabilities": {"tools": {}},
                               "serverInfo": {"name": "lambo", "version": "0.8.0"}, "instructions": "x"}
                })));
            }
            if method == "notifications/initialized" {
                return Ok(crate::mcp::HttpResponse {
                    status: 202,
                    headers: vec![],
                    body: String::new(),
                });
            }
            if method != "tools/call" {
                return Err(crate::mcp::TransportError {
                    url: url.to_string(),
                    message: "unknown method".to_string(),
                });
            }
            let params = v.get("params").cloned().unwrap_or_default();
            let tool = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or_default();
            let rid = v.get("id").cloned().unwrap_or_default();
            let mut st = st.lock().unwrap_or_else(|e| e.into_inner());
            if st.down && tool == "lambo_derive" {
                return Err(crate::mcp::TransportError {
                    url: url.to_string(),
                    message: "connection refused (test)".to_string(),
                });
            }
            match tool {
                "lambo_derive" => {
                    let concepts = args
                        .get("concepts")
                        .and_then(|c| c.as_array())
                        .cloned()
                        .unwrap_or_default();
                    let mut created = Vec::new();
                    for c in &concepts {
                        st.next += 1;
                        let cid = format!("{:08x}-0000-4000-8000-000000000000", st.next);
                        let content = c
                            .get("content")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string();
                        let against = c
                            .get("against")
                            .and_then(|a| a.as_array())
                            .cloned()
                            .unwrap_or_default();
                        for a in &against {
                            let stale_short = a.as_str().unwrap_or("").to_string();
                            let corr_short: String = cid.chars().take(8).collect();
                            st.links.push((stale_short.clone(), cid.clone()));
                            st.warnings.push(format!(
                                "\u{26a0} Contradicted at 2026-10-29T15:07:26Z by doyle: \"{content}\" (id {corr_short})"
                            ));
                            let stale_content = st
                                .concepts
                                .iter()
                                .find(|(id, _)| id.starts_with(&stale_short))
                                .map(|(_, t)| t.clone())
                                .unwrap_or_default();
                            st.warnings.push(format!(
                                "\u{26a0} Contradicts \"{stale_content}\" (id {stale_short})"
                            ));
                        }
                        st.concepts.push((cid.clone(), content));
                        created.push(cid);
                    }
                    st.derives += 1;
                    let receipt = format!("receipt-{}", st.derives);
                    st.receipts.insert(receipt.clone(), created);
                    Ok(sse(serde_json::json!({
                        "jsonrpc": "2.0", "id": rid,
                        "result": {"content": [{"type": "text", "text": "accepted"}],
                                   "structuredContent": {"agent_id": args.get("agent_id"), "receipt": receipt, "receipt_state": "pending", "warnings": []},
                                   "isError": false}
                    })))
                }
                "lambo_stats" => {
                    let receipt = args.get("receipt").and_then(|x| x.as_str()).unwrap_or("");
                    let created = st.receipts.get(receipt).cloned().unwrap_or_default();
                    Ok(sse(serde_json::json!({
                        "jsonrpc": "2.0", "id": rid,
                        "result": {"content": [{"type": "text", "text": "stats"}],
                                   "structuredContent": {"receipt": {"created": created, "state": "applied"}},
                                   "isError": false}
                    })))
                }
                "lambo_recall" => {
                    let hits: Vec<serde_json::Value> = st.concepts.iter().map(|(cid, content)| {
                        serde_json::json!({"node_id": cid, "content": content, "concept_type": "Logic",
                                           "score": 1.0, "is_canonical": false, "blast_radius": null})
                    }).collect();
                    let warnings = st.warnings.clone();
                    Ok(sse(serde_json::json!({
                        "jsonrpc": "2.0", "id": rid,
                        "result": {"content": [{"type": "text", "text": "ctx"}],
                                   "structuredContent": {"context": "ctx", "hits": hits, "warnings": warnings},
                                   "isError": false}
                    })))
                }
                "lambo_inspect" => {
                    let focus = args.get("focus").and_then(|x| x.as_str()).unwrap_or("");
                    let found = st
                        .concepts
                        .iter()
                        .find(|(cid, _)| cid == focus || cid.starts_with(focus));
                    let (label, node_id) = match found {
                        Some((cid, content)) => (format!("{content} [Logic]"), cid.clone()),
                        None => (String::new(), String::new()),
                    };
                    let focus_short: String = focus.chars().take(8).collect();
                    let warnings: Vec<String> = st.links.iter()
                        .filter(|(stale_short, _)| *stale_short == focus_short)
                        .map(|(_, corr_full)| {
                            let corr_short: String = corr_full.chars().take(8).collect();
                            format!("\u{26a0} Contradicted at 2026-10-29T15:07:26Z by doyle: \"...\" (id {corr_short})")
                        }).collect();
                    Ok(sse(serde_json::json!({
                        "jsonrpc": "2.0", "id": rid,
                        "result": {"content": [{"type": "text", "text": format!("focus: {label}")}],
                                   "structuredContent": {"focus": {"label": label, "node_id": node_id}, "warnings": warnings},
                                   "isError": false}
                    })))
                }
                _ => Err(crate::mcp::TransportError {
                    url: url.to_string(),
                    message: format!("unknown tool {tool}"),
                }),
            }
        });
        (transport, state)
    }

    fn lambo_adapter(transport: crate::mcp::FakeTransport) -> crate::adapters::lambo::LamboAdapter {
        crate::adapters::lambo::LamboAdapter::new(
            "http://127.0.0.1:7700/mcp".to_string(),
            "doyle".to_string(),
            Box::new(transport),
        )
    }

    // ----- the demo, seeded through the real cite and check paths -----

    struct Demo {
        root: PathBuf,
        muse_dir: PathBuf,
        server: ReviewServer,
        flag_id: String,
        proposal_text: String,
        lambo_state: std::sync::Arc<std::sync::Mutex<LamboState>>,
    }

    fn demo_server() -> Demo {
        let root = temp_dir("demo");
        let muse_dir = root.join("muse-memory");
        copy_dir(
            &PathBuf::from(fixture("testdata/memories/muse-code")),
            &muse_dir,
        );
        let state_dir = root.join(".doyle");

        // Citations through the real cite path (M1–M4, as the demo records).
        let v1 = fixture("testdata/contracts/patients-v1.yaml");
        let cite_input = serde_json::json!([
            {"adapter": "muse-code", "memory_id": "M1", "contract": v1,
             "elements": [LIST_PATIENTS, LIST_PATIENTS_200], "claim": M1_CLAIM,
             "recorded_by": "test-curator", "confidence": 0.95},
            {"adapter": "muse-code", "memory_id": "M2", "contract": v1,
             "elements": [GET_PATIENT, GET_PATIENT_404],
             "claim": "GET /patients/{id} returns 404 for an unknown patient id",
             "recorded_by": "test-curator", "confidence": 0.95},
            {"adapter": "muse-code", "memory_id": "M3", "contract": v1,
             "elements": [GET_STATS, PATIENT_COUNT],
             "claim": "GET /stats patient_count is the total number of patients",
             "recorded_by": "test-curator", "confidence": 0.95},
            {"adapter": "muse-code", "memory_id": "M4", "contract": v1,
             "elements": [DECOMMISSION],
             "claim": "POST /admin/decommission is permanent once confirmed; there is no undo",
             "recorded_by": "test-curator", "confidence": 0.95},
        ]);
        let cite_cli = crate::cli::Cli {
            command: crate::cli::Command::Cite,
            common: crate::cli::CommonOpts {
                state_dir: Some(state_dir.to_string_lossy().to_string()),
                ..Default::default()
            },
        };
        let code = crate::cite::run_with_input(
            &cite_cli,
            &cite_input.to_string(),
            &state_dir.to_string_lossy(),
        );
        assert_eq!(code, ExitCode::SUCCESS, "cite path must record M1–M4");

        // Flag + proposal through the real check path with replayed replies.
        let check_cli = crate::cli::Cli {
            command: crate::cli::Command::Check {
                old: fixture("testdata/contracts/patients-v1.yaml"),
                new: fixture("testdata/contracts/patients-v2.yaml"),
                format: "json".to_string(),
                no_recheck: false,
            },
            common: crate::cli::CommonOpts {
                state_dir: Some(state_dir.to_string_lossy().to_string()),
                ..Default::default()
            },
        };
        let (code, _out) = crate::check::execute(&check_cli, "json").expect("check runs");
        assert_eq!(code, ExitCode::from(1), "check must flag M1");

        let store = Store::open(&state_dir).unwrap();
        assert_eq!(store.flags().len(), 1, "exactly M1 is flagged");
        let flag_id = store.flags()[0].id.clone();
        let proposal_text = match &store.proposals()[0].action {
            ProposalAction::Correct { new_text } => new_text.clone(),
            other => panic!("demo proposal must be a correction, got {other:?}"),
        };

        let muse =
            MuseCodeAdapter::from_scopes(vec![(muse_dir.clone(), "personal_project".to_string())]);
        let (transport, lambo_state) = scripted_lambo(vec![
            (LAMBO_M1.to_string(), STALE.to_string()),
            (
                "a94c2f18-0000-0000-0000-000000000000".to_string(),
                "GET /patients/{id} returns 404 for an unknown patient id".to_string(),
            ),
            (
                "2d7b86e0-0000-0000-0000-000000000000".to_string(),
                "GET /stats patient_count is the total number of patients".to_string(),
            ),
            (
                "f03a9c51-0000-0000-0000-000000000000".to_string(),
                "POST /admin/decommission is permanent once confirmed; there is no undo"
                    .to_string(),
            ),
        ]);
        let provenance = Provenance::parse(
            &fs::read_to_string(fixture("testdata/memories/provenance.json")).unwrap(),
        );
        let server = ReviewServer::new(
            store,
            vec![Box::new(muse), Box::new(lambo_adapter(transport))],
        )
        .with_provenance(provenance);
        Demo {
            root,
            muse_dir,
            server,
            flag_id,
            proposal_text,
            lambo_state,
        }
    }

    // ----- route tests -----

    #[test]
    fn clock_renders_known_epochs() {
        assert_eq!(rfc3339_from_epoch(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_from_epoch(1_762_000_000), "2025-11-01T12:26:40Z");
    }

    #[test]
    fn list_route_shows_open_flag_and_logo() {
        let mut demo = demo_server();
        let response = demo.server.handle("GET", "/", "");
        assert_eq!(response.status, 200);
        assert!(response.body.contains("<svg"), "list carries the logo");
        assert!(
            response.body.contains("1 open"),
            "counts the open flag: {}",
            response.body
        );
        assert!(response.body.contains(M1_CLAIM), "names the stale memory");
        assert!(response.body.contains("breaking"));
        assert!(
            response.body.contains(&format!("/flag/{}", demo.flag_id)),
            "links to the flag"
        );
        let _ = fs::remove_dir_all(&demo.root);
    }

    #[test]
    fn detail_route_shows_change_memory_provenance_and_proposal() {
        let mut demo = demo_server();
        let response = demo
            .server
            .handle("GET", &format!("/flag/{}", demo.flag_id), "");
        assert_eq!(response.status, 200);
        let body = &response.body;
        // What changed, old and new.
        assert!(body.contains("What changed"), "change section");
        assert!(
            body.contains("field_type_changed"),
            "the Vimanam kind: {body}"
        );
        assert!(body.contains("array"), "the old shape");
        assert!(body.contains("object"), "the new shape");
        assert!(body.contains("pagination"), "the pagination fact");
        // The memory as each system holds it.
        assert!(body.contains(STALE), "Muse Code's copy");
        assert!(body.contains("lambo"), "Lambo is shown");
        assert!(body.contains(LAMBO_M1), "Lambo's node id");
        // Who relied on it.
        assert!(body.contains("report-2"), "provenance session");
        assert!(body.contains("migration-1"), "the migration session");
        assert!(body.contains("export"), "the relying step");
        // The proposal, its rationale, and the four actions.
        assert!(body.contains("Proposal"), "proposal section");
        assert!(body.contains(&demo.proposal_text), "the corrected text");
        for verdict in ["approve", "edit", "retire", "keep"] {
            assert!(
                body.contains(&format!("value=\"{verdict}\"")),
                "action {verdict}"
            );
        }
        let _ = fs::remove_dir_all(&demo.root);
    }

    #[test]
    fn approve_records_correction_in_both_systems() {
        let mut demo = demo_server();
        let flag_path = format!("/flag/{}", demo.flag_id);
        let response = demo.server.handle(
            "POST",
            &format!("{flag_path}/decide"),
            "verdict=approve&decided_by=judge",
        );
        assert_eq!(response.status, 303, "decide redirects: {}", response.body);
        assert_eq!(response.location.as_deref(), Some(flag_path.as_str()));

        // Doyle's store: the decision is recorded and the flag is settled.
        let store = &demo.server.store;
        assert!(store.open_flags().is_empty(), "flag settled in the store");
        let decision = store
            .decision_for_proposal(&store.proposals()[0].id)
            .unwrap();
        assert_eq!(decision.decided_by, "judge");
        assert_eq!(decision.verdict, DecisionVerdict::Approve);

        // Muse Code: the stale note names its correction, which reads back.
        let report = demo.server.reports.values().next().unwrap().clone();
        let muse_result = &report.results[0];
        assert_eq!(muse_result.adapter, "muse-code");
        let correction = match &muse_result.outcome {
            AdapterOutcome::Superseded { correction } => correction.clone(),
            other => panic!("Muse Code must supersede, got {other:?}"),
        };
        let muse_adapter = MuseCodeAdapter::from_scopes(vec![(
            demo.muse_dir.clone(),
            "personal_project".to_string(),
        )]);
        let stale_now = muse_adapter.read(&muse_result.memory_id).unwrap();
        assert!(
            stale_now
                .text
                .contains(&format!("[Superseded by {}]", correction.id)),
            "stale note names its correction: {}",
            stale_now.text
        );
        assert!(
            muse_adapter
                .read(&correction.id)
                .unwrap()
                .text
                .contains(&demo.proposal_text)
        );

        // Lambo: the correction was derived against M1 and reads back.
        let lambo_result = &report.results[1];
        assert_eq!(lambo_result.adapter, "lambo");
        let lambo_correction = match &lambo_result.outcome {
            AdapterOutcome::Superseded { correction } => correction.clone(),
            other => panic!("Lambo must supersede, got {other:?}"),
        };
        {
            let state = demo.lambo_state.lock().unwrap();
            assert!(
                state
                    .concepts
                    .iter()
                    .any(|(id, text)| id == &lambo_correction.id && text == &demo.proposal_text),
                "Lambo holds the correction: {:?}",
                state.concepts
            );
        }
        assert!(
            demo.lambo_state
                .lock()
                .unwrap()
                .links
                .iter()
                .any(|(stale, _)| stale == "5e1d0a73"),
            "the derive was against M1"
        );

        // The settled view shows what was written where, for both systems.
        let settled = demo.server.handle("GET", &flag_path, "");
        assert_eq!(settled.status, 200);
        assert!(settled.body.contains("Settled — what was written where"));
        assert!(settled.body.contains("Correction written"), "Muse outcome");
        assert!(
            settled.body.contains(&demo.proposal_text),
            "the correction text"
        );
        assert!(
            settled.body.contains(&lambo_correction.id),
            "Lambo's correction id"
        );
        // And the list now counts it settled.
        let list = demo.server.handle("GET", "/", "");
        assert!(list.body.contains("0 open"), "list settles: {}", list.body);
        assert!(list.body.contains("1 settled"));
        let _ = fs::remove_dir_all(&demo.root);
    }

    #[test]
    fn edit_then_approve_writes_the_edited_text() {
        let mut demo = demo_server();
        let flag_path = format!("/flag/{}", demo.flag_id);
        let edited =
            "GET /patients returns 100 at a time; keep following next_cursor until it is empty";
        let body = format!(
            "verdict=edit&decided_by=judge&new_text={}",
            edited.replace(' ', "+").replace(';', "%3B")
        );
        let response = demo
            .server
            .handle("POST", &format!("{flag_path}/decide"), &body);
        assert_eq!(response.status, 303);
        let settled = demo.server.handle("GET", &flag_path, "");
        assert!(
            settled.body.contains(edited),
            "edited text landed: {}",
            settled.body
        );
        let muse_file = fs::read_to_string(demo.muse_dir.join("MEMORY.md")).unwrap();
        assert!(muse_file.contains(edited), "Muse Code holds the edit");
        assert!(
            demo.lambo_state
                .lock()
                .unwrap()
                .concepts
                .iter()
                .any(|(_, t)| t == edited),
            "Lambo holds the edit"
        );
        let _ = fs::remove_dir_all(&demo.root);
    }

    #[test]
    fn retire_and_keep_verdicts_settle_without_the_proposed_correction() {
        // Retire: both systems get the retirement, not the correction.
        let mut demo = demo_server();
        let flag_path = format!("/flag/{}", demo.flag_id);
        let r = demo.server.handle(
            "POST",
            &format!("{flag_path}/decide"),
            "verdict=retire&decided_by=judge",
        );
        assert_eq!(r.status, 303);
        let settled = demo.server.handle("GET", &flag_path, "");
        assert!(settled.body.contains("retire"), "verdict shown");
        assert!(
            demo.lambo_state
                .lock()
                .unwrap()
                .concepts
                .iter()
                .any(|(_, t)| t == crate::core::decide::RETIREMENT_TEXT),
            "Lambo holds the retirement"
        );
        assert!(
            !demo
                .lambo_state
                .lock()
                .unwrap()
                .concepts
                .iter()
                .any(|(_, t)| t == &demo.proposal_text),
            "the proposed correction was not written"
        );
        let _ = fs::remove_dir_all(&demo.root);

        // Keep: nothing is written anywhere, the decision still recorded.
        let mut demo = demo_server();
        let flag_path = format!("/flag/{}", demo.flag_id);
        let before = fs::read_to_string(demo.muse_dir.join("MEMORY.md")).unwrap();
        let r = demo.server.handle(
            "POST",
            &format!("{flag_path}/decide"),
            "verdict=keep&decided_by=judge",
        );
        assert_eq!(r.status, 303);
        let settled = demo.server.handle("GET", &flag_path, "");
        assert!(
            settled.body.contains("Kept"),
            "kept outcome shown: {}",
            settled.body
        );
        assert_eq!(
            fs::read_to_string(demo.muse_dir.join("MEMORY.md")).unwrap(),
            before
        );
        assert_eq!(
            demo.lambo_state.lock().unwrap().derives,
            0,
            "Lambo untouched"
        );
        assert!(
            demo.server.store.open_flags().is_empty(),
            "keep still settles"
        );
        let _ = fs::remove_dir_all(&demo.root);
    }

    #[test]
    fn failed_writeback_is_visible_and_retry_lands_it_once() {
        let mut demo = demo_server();
        // Lambo goes down before the decision.
        demo.lambo_state.lock().unwrap().down = true;
        let flag_path = format!("/flag/{}", demo.flag_id);
        let r = demo.server.handle(
            "POST",
            &format!("{flag_path}/decide"),
            "verdict=approve&decided_by=judge",
        );
        assert_eq!(r.status, 303);
        let settled = demo.server.handle("GET", &flag_path, "");
        assert!(
            settled.body.contains("Failed"),
            "failure visible: {}",
            settled.body
        );
        assert!(settled.body.contains("marked for retry"));
        assert!(settled.body.contains("Retry the failed write-back"));

        // Lambo heals; the retry lands exactly one correction.
        demo.lambo_state.lock().unwrap().down = false;
        let r = demo
            .server
            .handle("POST", &format!("{flag_path}/retry"), "");
        assert_eq!(r.status, 303);
        let settled = demo.server.handle("GET", &flag_path, "");
        assert!(
            !settled.body.contains("marked for retry"),
            "retry settled it: {}",
            settled.body
        );
        let corrections = demo
            .lambo_state
            .lock()
            .unwrap()
            .concepts
            .iter()
            .filter(|(_, t)| t == &demo.proposal_text)
            .count();
        assert_eq!(corrections, 1, "exactly one Lambo correction");
        // A second retry is a no-op redirect (nothing failed any more).
        let r = demo
            .server
            .handle("POST", &format!("{flag_path}/retry"), "");
        assert_eq!(r.status, 303);
        let _ = fs::remove_dir_all(&demo.root);
    }

    #[test]
    fn unknown_routes_and_bad_actions_are_refused() {
        let mut demo = demo_server();
        assert_eq!(demo.server.handle("GET", "/flag/nope", "").status, 404);
        assert_eq!(demo.server.handle("GET", "/judges", "").status, 404);
        assert_eq!(demo.server.handle("DELETE", "/", "").status, 404);
        let flag_path = format!("/flag/{}", demo.flag_id);
        let bad = demo
            .server
            .handle("POST", &format!("{flag_path}/decide"), "verdict=frobnicate");
        assert_eq!(bad.status, 400);
        let empty_edit = demo.server.handle(
            "POST",
            &format!("{flag_path}/decide"),
            "verdict=edit&new_text=",
        );
        assert_eq!(empty_edit.status, 400);
        // Nothing was recorded by the refused actions.
        assert_eq!(demo.server.store.open_flags().len(), 1);
        assert!(demo.server.store.decisions().is_empty());
        // A second decision on a settled flag is refused.
        let ok = demo
            .server
            .handle("POST", &format!("{flag_path}/decide"), "verdict=keep");
        assert_eq!(ok.status, 303);
        let again = demo
            .server
            .handle("POST", &format!("{flag_path}/decide"), "verdict=keep");
        assert_eq!(again.status, 409);
        let _ = fs::remove_dir_all(&demo.root);
    }

    #[test]
    fn settled_view_after_restart_reads_memories_back() {
        let mut demo = demo_server();
        let flag_path = format!("/flag/{}", demo.flag_id);
        let r = demo
            .server
            .handle("POST", &format!("{flag_path}/decide"), "verdict=approve");
        assert_eq!(r.status, 303);
        // A fresh serving process over the same store and adapters: no
        // in-process report, so the page reads each memory back.
        let store = Store::open(demo.root.join(".doyle")).unwrap();
        let muse = MuseCodeAdapter::from_scopes(vec![(
            demo.muse_dir.clone(),
            "personal_project".to_string(),
        )]);
        let (transport, _state) = scripted_lambo(vec![(LAMBO_M1.to_string(), STALE.to_string())]);
        let mut restarted = ReviewServer::new(
            store,
            vec![Box::new(muse), Box::new(lambo_adapter(transport))],
        );
        let settled = restarted.handle("GET", &flag_path, "");
        assert_eq!(settled.status, 200);
        assert!(settled.body.contains("Settled — what was written where"));
        assert!(
            settled.body.contains("before this page last started"),
            "restart disclosed"
        );
        assert!(
            settled.body.contains("Superseded by"),
            "Muse note read back with its marker"
        );
        let _ = fs::remove_dir_all(&demo.root);
    }

    #[test]
    fn keep_with_fake_adapter_covers_each_route_without_demo_files() {
        // A minimal store over the fake adapter: list, detail, decide, settled.
        let root = temp_dir("fake");
        let state_dir = root.join(".doyle");
        let mut store = Store::open(&state_dir).unwrap();
        let fake = FakeAdapter::new("fake");
        let stale = fake.write(STALE, "scope").unwrap();
        store
            .record_citation(Citation {
                memory: stale.clone(),
                elements: vec![crate::core::ElementId(LIST_PATIENTS.to_string())],
                claim: M1_CLAIM.to_string(),
                recorded_by: "test".to_string(),
                confidence: 1.0,
                source_text: STALE.to_string(),
            })
            .unwrap();
        let flag_id = store
            .record_flag(Flag {
                id: String::new(),
                memory: stale.clone(),
                changes: vec![crate::core::ChangeId("vc2_test".to_string())],
                severity: crate::core::Severity::Review,
                old_version: "v1".to_string(),
                new_version: "v2".to_string(),
                evidence: Some(M1_CLAIM.to_string()),
            })
            .unwrap();
        store
            .record_proposal(Proposal {
                id: String::new(),
                flag_id: flag_id.clone(),
                action: ProposalAction::Correct {
                    new_text: "paged".to_string(),
                },
                rationale: "it pages now".to_string(),
                proposed_by: "test-model".to_string(),
            })
            .unwrap();
        drop(store);
        let store = Store::open(&state_dir).unwrap();
        let mut server = ReviewServer::new(store, vec![Box::new(fake)]);
        // Contracts named v1/v2 cannot be read: the page falls back to IDs.
        let detail = server.handle("GET", &format!("/flag/{flag_id}"), "");
        assert_eq!(detail.status, 200);
        assert!(detail.body.contains("vc2_test"), "change id fallback");
        assert!(detail.body.contains("No provenance is recorded"));
        let r = server.handle(
            "POST",
            &format!("/flag/{flag_id}/decide"),
            "verdict=keep&decided_by=a",
        );
        assert_eq!(r.status, 303);
        let settled = server.handle("GET", &format!("/flag/{flag_id}"), "");
        assert!(settled.body.contains("Kept"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn socket_serves_the_page() {
        use std::io::{Read as _, Write as _};
        let mut demo = demo_server();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let flag_id = demo.flag_id.clone();
        for path in ["/".to_string(), format!("/flag/{flag_id}")] {
            let mut stream = TcpStream::connect(addr).unwrap();
            stream
                .write_all(format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").as_bytes())
                .unwrap();
            let (accepted, _) = listener.accept().unwrap();
            handle_connection(accepted, &mut demo.server).unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            assert!(
                response.starts_with("HTTP/1.1 200 OK"),
                "got: {response:.80}"
            );
            assert!(response.contains("Content-Type: text/html"));
            assert!(response.contains("Doyle"), "page over the socket");
        }
        let _ = fs::remove_dir_all(&demo.root);
    }

    #[test]
    fn form_and_url_decoding() {
        let form = parse_form("verdict=edit&new_text=a+b%3Bc%26d&decided_by=");
        assert_eq!(form["new_text"], "a b;c&d");
        assert_eq!(url_decode("/flag/flag%2D1"), "/flag/flag-1");
        assert_eq!(url_decode("100%"), "100%");
    }

    // LB1 repro: one adapter whose list fails must not be silently dropped.
    #[test]
    fn lb1_list_failure_marks_failed_retryable_and_retry_heals() {
        use crate::adapters::{AdapterError, Capabilities, MemoryAdapter};
        use crate::core::Memory;
        use std::sync::atomic::{AtomicBool, Ordering};

        struct ListFailing {
            inner: FakeAdapter,
            fail: std::sync::Arc<AtomicBool>,
        }
        impl MemoryAdapter for ListFailing {
            fn name(&self) -> &str {
                self.inner.name()
            }
            fn capabilities(&self) -> Capabilities {
                self.inner.capabilities()
            }
            fn list(&self, query: &str) -> Result<Vec<Memory>, AdapterError> {
                if self.fail.load(Ordering::SeqCst) {
                    Err(AdapterError::Backend {
                        adapter: self.name().to_string(),
                        memory: None,
                        message: "connection refused (test)".to_string(),
                    })
                } else {
                    self.inner.list(query)
                }
            }
            fn read(&self, id: &str) -> Result<Memory, AdapterError> {
                self.inner.read(id)
            }
            fn write(
                &self,
                text: &str,
                scope: &str,
            ) -> Result<crate::core::MemoryRef, AdapterError> {
                self.inner.write(text, scope)
            }
            fn annotate(&self, id: &str, warning: &str) -> Result<(), AdapterError> {
                self.inner.annotate(id, warning)
            }
            fn supersede(
                &self,
                stale_id: &str,
                correction_text: &str,
            ) -> Result<crate::core::MemoryRef, AdapterError> {
                self.inner.supersede(stale_id, correction_text)
            }
            fn watch(&self) -> Result<Vec<Memory>, AdapterError> {
                self.inner.watch()
            }
        }

        const LONG: &str = "GET /patients returns every patient in one response; one call exports the whole clinic";
        let root = temp_dir("lb1");
        let state_dir = root.join(".doyle");
        let mut store = Store::open(&state_dir).unwrap();
        let good = FakeAdapter::new("good");
        let flaky_inner = FakeAdapter::new("flaky");
        let stale_good = good.write(LONG, "s").unwrap();
        let _stale_flaky = flaky_inner.write(LONG, "s").unwrap();
        store
            .record_citation(Citation {
                memory: stale_good.clone(),
                elements: vec![crate::core::ElementId(LIST_PATIENTS.to_string())],
                claim: M1_CLAIM.to_string(),
                recorded_by: "test".to_string(),
                confidence: 1.0,
                source_text: LONG.to_string(),
            })
            .unwrap();
        let flag_id = store
            .record_flag(Flag {
                id: String::new(),
                memory: stale_good.clone(),
                changes: vec![crate::core::ChangeId("vc2_test".to_string())],
                severity: crate::core::Severity::Breaking,
                old_version: "v1".to_string(),
                new_version: "v2".to_string(),
                evidence: Some(M1_CLAIM.to_string()),
            })
            .unwrap();
        store
            .record_proposal(Proposal {
                id: String::new(),
                flag_id: flag_id.clone(),
                action: ProposalAction::Correct {
                    new_text: "paged".to_string(),
                },
                rationale: "it pages now".to_string(),
                proposed_by: "test-model".to_string(),
            })
            .unwrap();
        drop(store);
        let store = Store::open(&state_dir).unwrap();
        let fail = std::sync::Arc::new(AtomicBool::new(true));
        let flaky = ListFailing {
            inner: flaky_inner,
            fail: fail.clone(),
        };
        let mut server = ReviewServer::new(store, vec![Box::new(good), Box::new(flaky)]);
        let flag_path = format!("/flag/{flag_id}");
        let r = server.handle(
            "POST",
            &format!("{flag_path}/decide"),
            "verdict=approve&decided_by=judge",
        );
        assert_eq!(r.status, 303, "decide records: {}", r.body);
        let settled = server.handle("GET", &flag_path, "");
        assert!(
            settled.body.contains("Failed") && settled.body.contains("marked for retry"),
            "settled marks the unsearchable adapter failed-retryable, not silently complete: {}",
            settled.body
        );
        assert!(
            settled.body.contains("flaky"),
            "settled names the adapter: {}",
            settled.body
        );
        // The transport heals; the retry re-searches the adapter by the
        // cited text and lands exactly one correction there.
        fail.store(false, Ordering::SeqCst);
        let r = server.handle("POST", &format!("{flag_path}/retry"), "");
        assert_eq!(r.status, 303, "retry redirects: {}", r.body);
        let settled = server.handle("GET", &flag_path, "");
        assert!(
            !settled.body.contains("marked for retry"),
            "retry healed the failed write-back: {}",
            settled.body
        );
        let report = server.reports.values().next().unwrap().clone();
        assert_eq!(
            report.results.len(),
            2,
            "one entry per adapter: {:?}",
            report.results
        );
        for result in &report.results {
            match &result.outcome {
                AdapterOutcome::Superseded { .. } => {}
                other => panic!("{} must have succeeded, got {other:?}", result.adapter),
            }
        }
        let _ = fs::remove_dir_all(&root);
    }

    // LB2: the match is asymmetric. A long note starting with the short
    // cited claim still resolves (the cite path the demo relies on); a
    // short note that merely prefixes the cited text is refused, so a
    // prefix decoy alongside the true copy never resolves as the copy.
    #[test]
    fn lb2_short_prefix_decoy_is_refused_but_long_note_kept() {
        const LONG: &str = "GET /patients returns every patient in one response; one call exports the whole clinic";
        const CLAIM: &str = "GET /patients returns every patient in one response";
        const DECOY: &str = "GET /patients returns every";
        assert!(CLAIM.chars().count() >= 8 && DECOY.chars().count() >= 8);
        // Kept direction: the cite path's short claim resolves the long note.
        assert!(
            ReviewServer::text_matches(LONG, &[CLAIM.to_string()]),
            "long note starting with the short claim must resolve"
        );
        // Refused direction: the short decoy prefixes the cited text.
        assert!(
            !ReviewServer::text_matches(DECOY, &[CLAIM.to_string()]),
            "short note prefixing the claim must not resolve"
        );
        assert!(
            !ReviewServer::text_matches(DECOY, &[LONG.to_string()]),
            "short note prefixing the cited text must not resolve"
        );

        // Resolution level: decoy alongside the true copy in one adapter
        // resolves to exactly the true target. The nonsense source text
        // matches no note, so the query pass finds nothing and the
        // unfiltered pass filters both notes by the predicate: the true
        // copy matches via the kept direction, the decoy via neither.
        let root = temp_dir("lb2");
        let state_dir = root.join(".doyle");
        let mut store = Store::open(&state_dir).unwrap();
        let fake = FakeAdapter::new("fake");
        let true_ref = fake.write(LONG, "s").unwrap();
        let decoy_ref = fake.write(DECOY, "s").unwrap();
        assert_ne!(true_ref.id, decoy_ref.id);
        let flagged = crate::core::MemoryRef {
            adapter: "muse-code".to_string(),
            id: "M1".to_string(),
        };
        store
            .record_citation(Citation {
                memory: flagged.clone(),
                elements: vec![crate::core::ElementId(LIST_PATIENTS.to_string())],
                claim: CLAIM.to_string(),
                recorded_by: "test".to_string(),
                confidence: 1.0,
                source_text: "zzz-no-such-phrase-lb2".to_string(),
            })
            .unwrap();
        let flag_id = store
            .record_flag(Flag {
                id: String::new(),
                memory: flagged,
                changes: vec![crate::core::ChangeId("vc2_test".to_string())],
                severity: crate::core::Severity::Breaking,
                old_version: "v1".to_string(),
                new_version: "v2".to_string(),
                evidence: None,
            })
            .unwrap();
        drop(store);
        let store = Store::open(&state_dir).unwrap();
        let server = ReviewServer::new(store, vec![Box::new(fake)]);
        let flag = server.store.flag(&flag_id).unwrap();
        let resolved = server.resolve_copies(&flag);
        let fake_copies: Vec<&str> = resolved
            .copies
            .iter()
            .filter(|c| c.adapter == "fake")
            .map(|c| c.memory_id.as_str())
            .collect();
        assert_eq!(
            fake_copies,
            vec![true_ref.id.as_str()],
            "exactly the true copy resolves; the prefix decoy is refused"
        );
        let _ = fs::remove_dir_all(&root);
    }

    // LB3: provenance resolves from the state dir, so serving from another
    // cwd still renders it. The temp-cwd test drives the real binary (its own
    // process, so no process-wide chdir can disturb parallel tests); the
    // loader order and the honest-absence message are pinned in-process.
    fn lb3_free_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    fn lb3_get_flag_page(port: u16, flag_id: &str) -> String {
        use std::io::{Read, Write};
        let addr = format!("127.0.0.1:{port}");
        for _ in 0..200 {
            if let Ok(mut stream) = std::net::TcpStream::connect(&addr) {
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .ok();
                let request = format!(
                    "GET /flag/{flag_id} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
                );
                stream.write_all(request.as_bytes()).ok();
                let mut out = String::new();
                stream.read_to_string(&mut out).ok();
                if out.contains("<html") {
                    return out;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        panic!("doyle serve did not answer on {addr}");
    }

    /// Seed M1 through the real cite and check paths with absolute contract
    /// paths, and copy the provenance fixture into the state dir (marked so
    /// the test can tell it apart from the legacy checkout copy). Returns
    /// the temp root and the flag id.
    fn lb3_seed() -> (PathBuf, String) {
        let root = temp_dir("lb3");
        let state_dir = root.join(".doyle");
        let v1 = fixture("testdata/contracts/patients-v1.yaml");
        let v2 = fixture("testdata/contracts/patients-v2.yaml");
        let cite_input = serde_json::json!([
            {"adapter": "muse-code", "memory_id": "M1", "contract": v1,
             "elements": [LIST_PATIENTS, LIST_PATIENTS_200], "claim": M1_CLAIM,
             "recorded_by": "test-curator", "confidence": 0.95},
        ]);
        let cite_cli = crate::cli::Cli {
            command: crate::cli::Command::Cite,
            common: crate::cli::CommonOpts {
                state_dir: Some(state_dir.to_string_lossy().to_string()),
                ..Default::default()
            },
        };
        let code = crate::cite::run_with_input(
            &cite_cli,
            &cite_input.to_string(),
            &state_dir.to_string_lossy(),
        );
        assert_eq!(code, ExitCode::SUCCESS, "cite path records M1");
        let check_cli = crate::cli::Cli {
            command: crate::cli::Command::Check {
                old: v1.clone(),
                new: v2.clone(),
                format: "json".to_string(),
                no_recheck: false,
            },
            common: crate::cli::CommonOpts {
                state_dir: Some(state_dir.to_string_lossy().to_string()),
                ..Default::default()
            },
        };
        let (code, _out) = crate::check::execute(&check_cli, "json").expect("check runs");
        assert_eq!(code, ExitCode::from(1), "check must flag M1");
        let store = Store::open(&state_dir).unwrap();
        assert_eq!(store.flags().len(), 1, "exactly M1 is flagged");
        let flag_id = store.flags()[0].id.clone();
        let text = fs::read_to_string(fixture("testdata/memories/provenance.json")).unwrap();
        fs::write(
            state_dir.join("provenance.json"),
            text.replace(
                "Build the monthly patient-count report",
                "state-dir provenance marker",
            ),
        )
        .unwrap();
        (root, flag_id)
    }

    #[test]
    fn lb3_serve_from_temp_cwd_renders_provenance_and_contracts() {
        let (root, flag_id) = lb3_seed();
        let state_dir = root.join(".doyle");
        // The state-dir copy wins even though the legacy checkout copy also
        // exists under this process's cwd.
        let (provenance, missing) = load_provenance(&state_dir.to_string_lossy());
        assert!(provenance.is_some(), "state-dir provenance loads");
        assert!(missing.is_none(), "nothing missing to report");
        // The real binary, serving from an empty cwd against the
        // absolute-path state dir.
        let port = lb3_free_port();
        let empty = temp_dir("lb3cwd");
        let bin = format!("{}/target/debug/doyle", env!("CARGO_MANIFEST_DIR"));
        let mut child = std::process::Command::new(&bin)
            .args(["serve", "--state-dir", &state_dir.to_string_lossy()])
            .env("DOYLE_SERVE_PORT", port.to_string())
            .current_dir(&empty)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn doyle serve");
        let body = lb3_get_flag_page(port, &flag_id);
        child.kill().ok();
        let _ = child.wait();
        assert!(
            body.contains("state-dir provenance marker"),
            "provenance came from the state dir, not the legacy path"
        );
        assert!(body.contains("migration-1"), "relying session named");
        assert!(
            body.contains("field_type_changed"),
            "contract change facts render from absolute paths"
        );
        let _ = fs::remove_dir_all(&empty);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn lb3_state_dir_wins_and_absence_names_the_path() {
        assert_eq!(
            provenance_candidates("/s", Path::new("/c")),
            vec![
                PathBuf::from("/s/provenance.json"),
                PathBuf::from("/c/testdata/memories/provenance.json"),
            ],
            "state dir first, legacy demo path second"
        );
        // Absence, independent of this process's cwd: a state dir without
        // the file and a cwd without the legacy copy.
        let root = temp_dir("lb3absent");
        let state_dir = root.join(".doyle");
        std::fs::create_dir_all(&state_dir).unwrap();
        let nowhere = temp_dir("lb3nowhere");
        let (provenance, missing) = load_provenance_from(&state_dir.to_string_lossy(), &nowhere);
        assert!(provenance.is_none(), "no provenance anywhere");
        let missing = missing.expect("names the state-dir path");
        assert_eq!(missing, state_dir.join("provenance.json"));
        let _ = fs::remove_dir_all(&nowhere);
        // The flag page names that path instead of claiming nothing exists.
        let mut store = Store::open(&state_dir).unwrap();
        let fake = FakeAdapter::new("fake");
        let stale = fake.write(STALE, "scope").unwrap();
        store
            .record_citation(Citation {
                memory: stale.clone(),
                elements: vec![crate::core::ElementId(LIST_PATIENTS.to_string())],
                claim: M1_CLAIM.to_string(),
                recorded_by: "test".to_string(),
                confidence: 1.0,
                source_text: STALE.to_string(),
            })
            .unwrap();
        let flag_id = store
            .record_flag(Flag {
                id: String::new(),
                memory: stale.clone(),
                changes: vec![crate::core::ChangeId("vc2_test".to_string())],
                severity: crate::core::Severity::Review,
                old_version: "v1".to_string(),
                new_version: "v2".to_string(),
                evidence: Some(M1_CLAIM.to_string()),
            })
            .unwrap();
        drop(store);
        let store = Store::open(&state_dir).unwrap();
        let mut server = ReviewServer::new(store, vec![Box::new(fake)])
            .with_provenance(provenance)
            .with_provenance_missing_at(Some(missing.clone()));
        let response = server.handle("GET", &format!("/flag/{flag_id}"), "");
        assert_eq!(response.status, 200);
        assert!(
            response.body.contains(&format!(
                "provenance file not found at {}",
                missing.display()
            )),
            "honest absence names the path: {}",
            response.body
        );
        let _ = fs::remove_dir_all(&root);
    }
}
