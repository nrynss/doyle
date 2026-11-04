//! `doyle serve`: the "For judges: start here" page.
//!
//! One table that links everything a judge needs: the recorded runs with and
//! without Doyle, the measures, the Muse Code session logs, and the review
//! page itself. It says plainly what is new (Doyle, built here during the
//! event) and what is an existing dependency (Vimanam, Lambo).
//!
//! Every evidence link is served by this process from the checkout's
//! `evidence/` directory, resolved relative to the working directory serve
//! runs from (the same cwd-relative convention as the provenance fallback in
//! `super`): `GET /judges` is the table, `/judges/measures` quotes
//! `evidence/measures.md` whole, `/judges/runs/<run>` describes one recorded
//! run and lists every file in its folder, and `/judges/file/<run>/<file>`
//! streams one committed file as plain text. The review page itself is linked
//! as `/`, with one link per flag in this serving state. Served from a fresh
//! clone, with the evidence committed, every link on every page answers 200.
//!
//! When the evidence directory is not where serve runs (any working directory
//! other than the checkout root), the pages say so and name the path looked
//! for, instead of linking nowhere. Nothing here calls a model, the network,
//! or a key.

use std::fs;
use std::path::{Path, PathBuf};

use super::render::{esc, page, severity_badge};
use super::{Response, ReviewServer, url_decode};

/// The checkout-relative directory the judges' pages read. Serve from the
/// checkout root (a fresh clone) and this resolves; anywhere else the pages
/// name this path instead of pretending the evidence is there.
fn evidence_root() -> PathBuf {
    PathBuf::from("evidence")
}

/// One committed file served as plain text is capped here. The largest
/// committed session export is under 8 MB; anything past this is not the
/// evidence and is refused rather than read into the server.
const MAX_FILE_BYTES: u64 = 32 << 20;

/// What the judges' page may claim, at most: quoted from
/// `evidence/measures.md` ("What the judges' page may claim, at most"),
/// unwrapped to one line. The measures test pins the two to each other, so
/// the page can never claim more than the measures state.
const AT_MOST: &str = "2 memories flagged; 2 stale memories left behind without Doyle; \
    0 records lost in any run (never \"losses averted\"); 6 guard denials in session one \
    (never \"damage prevented\"); re-check cost about even (114.2 s vs 111.7 s of work, \
    149 vs 136 approvals, confounds as above); escalations 1 / 2 / 0 / 0+3 as listed; \
    0 false alarms with the granularity confound stated; 2 of 2 citations right; \
    2 of 2 proposals accepted unedited by a person. Demo only; no second-upgrade numbers exist.";

/// One recorded run: its folder under `evidence/`, its one-paragraph measured
/// outcome (within `AT_MOST`), and the session logs the start-here table
/// links directly. Every file in the folder is linked from the run's own page.
struct RunRef {
    /// Folder under `evidence/`.
    dir: &'static str,
    /// Diary task that ran it.
    task: &'static str,
    /// Short label for the table.
    label: &'static str,
    /// One-paragraph measured outcome. Says what happened, never more.
    outcome: &'static str,
    /// Headline session logs, linked straight from the start-here table.
    logs: &'static [&'static str],
    /// Where to start reading, with why.
    start_with: &'static [(&'static str, &'static str)],
}

const RUNS: &[RunRef] = &[
    RunRef {
        dir: "without-doyle",
        task: "T7.2",
        label: "Without Doyle, loud v2",
        outcome: "Wonder probed past the stale memory, found the next_cursor pagination, paged \
            all 49 pages, imported 4,812 and decommissioned only after the counts were proven. \
            0 records lost. The stale memory was left behind: the snapshot still says one response \
            returns everyone, with no marker. 1 safety escalation on the first decommission, \
            approved after the counts were proven.",
        logs: &[
            "seed-transcript.txt",
            "seed-export.json",
            "migration-transcript.txt",
            "migration-export.json",
        ],
        start_with: &[
            ("README.md", "the run as it happened, in one page"),
            (
                "migration-export.json",
                "the migration session log: the probe, the 49 pages, the decommission",
            ),
        ],
    },
    RunRef {
        dir: "without-doyle-quiet",
        task: "T7.2b",
        label: "Without Doyle, quiet v2",
        outcome: "Wonder caught the change at step 1 through the Link header, paged all 49 pages \
            and imported 4,812. The 30-minute limit ended the run during re-checks; the decommission \
            was attempted but aborted at the limit, so the old system still holds 4,812. 0 records \
            lost. 1 stale memory left behind. 2 safety escalations, both unanswered to the limit.",
        logs: &[
            "seed-transcript.txt",
            "seed-export.json",
            "migration-transcript.txt",
            "migration-export.json",
        ],
        start_with: &[
            ("README.md", "the run as it happened, in one page"),
            (
                "migration-export.json",
                "the migration session log: the step-1 catch, the cursor pages, the aborted decommission",
            ),
        ],
    },
    RunRef {
        dir: "with-doyle-self-approved",
        task: "T7.3a",
        label: "With Doyle, first attempt (self-approved)",
        outcome: "The guard denied the first GET /patients; Wonder then read the review page and \
            posted both decisions itself (decided_by: muse-wonder-migration) — no person approved. \
            It paged, imported and decommissioned 4,812 with no safety escalation. 0 records lost. \
            Published as the self-approval finding; its 0 of 2 proposals are excluded from every claim.",
        logs: &[
            "seed-transcript.txt",
            "seed-export.json",
            "migration-1-transcript.txt",
            "migration-1-export.json",
        ],
        start_with: &[
            (
                "README.md",
                "what happened and why it is kept as a finding, not the run",
            ),
            (
                "migration-1-export.json",
                "the session log: the denial, then the two self-posted decisions",
            ),
        ],
    },
    RunRef {
        dir: "with-doyle",
        task: "T7.3",
        label: "With Doyle",
        outcome: "Session one: the SessionStart notice named both stale memories, then 6 guard \
            denials (5 on stale-memory actions, 1 self-protection); Wonder paused and asked for a \
            person, touching neither the review page nor a decision route. A person approved both \
            flags (decided_by: reviewer); the corrections landed in Muse Code memory and in Lambo. \
            Session two started from the corrected memory, still probed first, paged all 49 pages and \
            imported 4,812, requesting the decommission at t+330.7 s (T7.2b: t+350.4 s) at about the \
            same cost (26 tool calls and 136 approval requests before it, against 20 and 149). \
            0 records lost; nothing decommissioned — the safety escalations (0 in session one, \
            3 in session two) went unanswered to the limit.",
        logs: &[
            "seed-transcript.txt",
            "seed-export.json",
            "migration-1-transcript.txt",
            "migration-1-export.json",
            "migration-2-transcript.txt",
            "migration-2-export.json",
        ],
        start_with: &[
            (
                "README.md",
                "the run as it happened: the denial, the approval, session two",
            ),
            (
                "migration-1-export.json",
                "session one's log: the notice, the 6 denials, the pause",
            ),
            (
                "state/decisions.jsonl",
                "both approvals, decided_by: reviewer",
            ),
            (
                "migration-2-export.json",
                "session two's log: the probe, the 49 pages, the unanswered decommission",
            ),
            (
                "session-two-measures.txt",
                "the side-by-side cost count against T7.2b",
            ),
        ],
    },
];

fn run_by_name(name: &str) -> Option<&'static RunRef> {
    RUNS.iter().find(|r| r.dir == name)
}

impl ReviewServer {
    /// Route one judges' request. `rest` is the path after `/judges`: empty
    /// for the start-here table, `measures`, `runs/<run>`, or
    /// `file/<run>/<file…>` below it. Unknown shapes 404 with the known pages
    /// named, so no link the table prints can land nowhere.
    pub(super) fn judges_route(&self, rest: &[&str]) -> Response {
        let root = evidence_root();
        match rest {
            [] => Response::html(200, self.judges_index(&root)),
            ["measures"] => match measures_page_html(&root) {
                Some(body) => Response::html(200, body),
                None => Response::html(404, missing_evidence_page(&root, "evidence/measures.md")),
            },
            ["runs", name] => match run_page_html(&root, name) {
                Some(body) => Response::html(200, body),
                None => Response::html(404, unknown_run_page(&root, name)),
            },
            ["file", parts @ ..] => judges_file_response(&root, parts),
            _ => Response::html(404, unknown_run_page(&root, &rest.join("/"))),
        }
    }

    /// The start-here table.
    fn judges_index(&self, root: &Path) -> String {
        let mut body = String::new();
        body.push_str(
            "<h1>For judges: start here</h1><p>Doyle keeps what the migration agent remembers \
            true. Below: the recorded runs with and without Doyle, the measured numbers, the \
            session logs, and the review page itself. Every evidence link is served from this checkout, \
            with the evidence committed.</p><p>The demo video is on YouTube: \
            <code>youtube.com/watch?v=cBD3CQrDqUc</code> (given as text, so every link on \
            this page stays on this server).</p>",
        );
        body.push_str(new_or_existing());
        body.push_str(&self.runs_table(root));
        body.push_str(&self.numbers_card());
        body.push_str(&self.review_card());
        page("For judges: start here", &body)
    }

    /// The recorded runs, with the session logs linked straight from the table.
    fn runs_table(&self, root: &Path) -> String {
        // The table keeps a minimum width so a phone-width screen scrolls it
        // sideways (the shared stylesheet) instead of shredding the outcome
        // column; the session-log names never wrap mid-name (above).
        let mut out = String::from(
            "<section class=\"card wide\"><h2>The recorded runs</h2><table style=\"min-width:44rem\">\
            <tr><th>Run</th><th>Measured outcome</th><th>Session logs</th></tr>",
        );
        let missing = !root.is_dir();
        for run in RUNS {
            out.push_str(&format!(
                "<tr><td><a style=\"white-space:nowrap\" href=\"/judges/runs/{dir}\"><strong>{label}</strong></a> \
                <span class=\"small muted\">({task})</span><br>\
                <a class=\"small\" href=\"/judges/file/{dir}/README.md\">README</a></td>\
                <td>{outcome}</td><td class=\"logs\">",
                dir = run.dir,
                label = esc(run.label),
                task = esc(run.task),
                outcome = esc(run.outcome),
            ));
            for log in run.logs {
                // One link per line, never broken mid-name: on a phone-width
                // screen the table scrolls sideways (the shared stylesheet)
                // and the filenames stay intact instead of stacking fragments.
                out.push_str(&format!(
                    "<a class=\"mono small\" style=\"white-space:nowrap\" \
                    href=\"/judges/file/{dir}/{log}\">{log}</a><br>",
                    dir = run.dir,
                    log = esc(log),
                ));
            }
            out.push_str("</td></tr>");
        }
        out.push_str("</table>");
        if missing {
            out.push_str(&format!(
                "<p class=\"failed\">The evidence directory was not found here: looked for \
                <span class=\"mono\">{}</span> relative to the directory serve runs from. \
                Serve from the checkout root (a fresh clone) so every link above resolves.</p>",
                esc(&root.display().to_string()),
            ));
        }
        out.push_str("</section>");
        out
    }

    /// The numbers, at most: the measures file's own claim list, quoted whole.
    fn numbers_card(&self) -> String {
        format!(
            "<section class=\"card wide\"><h2>The numbers, at most</h2>\
            <p>{at_most}</p>\
            <p class=\"small\">Quoted from \
            <a href=\"/judges/measures\">the measures, with method, files and confounds</a> \
            (<a class=\"mono\" href=\"/judges/file/measures.md\">evidence/measures.md</a>). \
            The judges' page claims no more than that file does.</p></section>",
            at_most = esc(AT_MOST),
        )
    }

    /// The review page itself: the live list, one link per flag in this
    /// serving state, and the recorded state as committed files. A fresh clone
    /// serves with an empty state, so the recorded flags are linked as files
    /// too — and how to serve them live.
    fn review_card(&self) -> String {
        let open = self.store.open_flags();
        let open_ids: Vec<String> = open.iter().map(|f| f.id.clone()).collect();
        let settled: Vec<_> = self
            .store
            .flags()
            .iter()
            .filter(|f| !open_ids.contains(&f.id))
            .cloned()
            .collect();
        let mut out = String::from("<section class=\"card wide\"><h2>The review page itself</h2>");
        out.push_str(&format!(
            "<p><a href=\"/\"><strong>Review list</strong></a> — {} open · {} settled in this \
            serving state.</p>",
            open.len(),
            settled.len()
        ));
        if open.is_empty() && settled.is_empty() {
            out.push_str(
                "<p class=\"small\">No flags in this serving state: a fresh clone serves with an \
                empty state directory. The recorded flags live below, as committed files.</p>",
            );
        } else {
            out.push_str("<ul>");
            for flag in open.iter().chain(settled.iter()) {
                let claim = self
                    .citation_for(flag)
                    .map(|c| c.claim)
                    .or_else(|| flag.evidence.clone())
                    .unwrap_or_else(|| flag.memory.id.clone());
                out.push_str(&format!(
                    "<li>{sev} <a href=\"/flag/{id}\">{claim}</a> \
                    <span class=\"small muted\">flag <span class=\"mono\">{id}</span></span></li>",
                    sev = severity_badge(flag.severity),
                    id = esc(&flag.id),
                    claim = esc(&claim),
                ));
            }
            out.push_str("</ul>");
        }
        out.push_str(
            "<p class=\"small\">The recorded state, as committed: \
            <a class=\"mono\" href=\"/judges/file/with-doyle/state/flags.jsonl\">flags.jsonl</a> · \
            <a class=\"mono\" href=\"/judges/file/with-doyle/state/proposals.jsonl\">proposals.jsonl</a> · \
            <a class=\"mono\" href=\"/judges/file/with-doyle/state/decisions.jsonl\">decisions.jsonl</a> · \
            <a class=\"mono\" href=\"/judges/file/with-doyle/state/citations.jsonl\">citations.jsonl</a>. \
            To review the recorded flags live, copy that folder out of the checkout and serve it: \
            <span class=\"mono\">cp -r evidence/with-doyle/state /tmp/doyle-state \\&\\& \
            doyle serve --state-dir /tmp/doyle-state</span> (a copy, so deciding cannot touch \
            the committed files).</p>",
        );
        out.push_str("</section>");
        out
    }
}

/// What is new (Doyle) and what is an existing dependency (Vimanam, Lambo).
fn new_or_existing() -> &'static str {
    "<section class=\"card wide\"><h2>What is new, what is existing</h2>\
    <p><strong>New — Doyle, built during the event in this repository:</strong> the citation index \
    (what each memory rests on), the contract watch (<span class=\"mono\">doyle check</span> diffs \
    two contract versions), flagging the stale, the re-check's proposal, the review page that takes \
    a person's sign-off, and the guard that holds the stale step whatever the agent chooses.</p>\
    <p><strong>Existing dependencies, not Doyle's work and never vendored:</strong> \
    <strong>Vimanam</strong> (<span class=\"mono\">vimanam</span> 2.x on crates.io, linked as a \
    library: the contract model, the stable element IDs, the diff) and <strong>Lambo</strong> \
    (a running server Doyle reaches as a client: corrections are recorded \
    <span class=\"mono\">against</span> the stale memory so recall warns). The migration agent is \
    Muse Wonder in Muse Code; the curator (Muse Spark Open on Baseten) and the re-check \
    (Muse Spark) run on Meta models. Nothing here is copied from Lambo, Vimanam, or any earlier \
    project: Doyle depends on published crates or talks to a running server.</p></section>"
}

/// The measures page: `evidence/measures.md` quoted whole inside the page
/// shell, so the page cannot reword a number. Escaped, not rendered: the file
/// is the claim, and reformatting it would risk claiming something it does not.
fn measures_page_html(root: &Path) -> Option<String> {
    let text = fs::read_to_string(root.join("measures.md")).ok()?;
    Some(page(
        "Measures",
        &format!(
            "<p class=\"small\"><a href=\"/judges\">← For judges: start here</a></p>\
            <h1>Measures</h1>\
            <p class=\"small\"><span class=\"mono\">evidence/measures.md</span> as committed, quoted \
            whole. Every number below states its method, its files and its confounds; the \
            start-here table claims no more than this file's at-most list.</p>\
            <section class=\"card wide\"><pre>{}</pre></section>",
            esc(&text),
        ),
    ))
}

/// One recorded run: its measured outcome, where to start, and every file in
/// its folder, each linked. The listing is read from the checkout, so a file
/// committed later is linked without touching this page.
fn run_page_html(root: &Path, name: &str) -> Option<String> {
    let run = run_by_name(name)?;
    let files = list_run_files(root, run.dir)?;
    let mut body = format!(
        "<p class=\"small\"><a href=\"/judges\">← For judges: start here</a></p>\
        <h1>{label} <span class=\"small muted\">({task})</span></h1>\
        <p>{outcome}</p>",
        label = esc(run.label),
        task = esc(run.task),
        outcome = esc(run.outcome),
    );
    body.push_str("<section class=\"card wide\"><h2>Start with</h2><ul>");
    for (file, why) in run.start_with {
        body.push_str(&format!(
            "<li><a class=\"mono\" href=\"/judges/file/{dir}/{file}\">{file}</a> — {why}</li>",
            dir = run.dir,
            file = esc(file),
            why = esc(why),
        ));
    }
    body.push_str("</ul></section>");
    body.push_str(&format!(
        "<section class=\"card wide\"><h2>Every file in evidence/{dir}/</h2>\
        <table><tr><th>File</th><th>Size</th></tr>",
        dir = esc(run.dir),
    ));
    for (rel, size) in &files {
        body.push_str(&format!(
            "<tr><td><a class=\"mono\" \
            href=\"/judges/file/{dir}/{rel}\">{rel}</a></td>\
            <td>{size} bytes</td></tr>",
            dir = run.dir,
            rel = esc(rel),
            size = size,
        ));
    }
    body.push_str("</table></section>");
    Some(page(run.label, &body))
}

/// Every file under one run's evidence folder, sorted by relative path, with
/// sizes. `None` when the folder is not where serve runs.
fn list_run_files(root: &Path, dir: &str) -> Option<Vec<(String, u64)>> {
    let base = root.join(dir);
    if !base.is_dir() {
        return None;
    }
    let mut out = Vec::new();
    let mut stack = vec![base.clone()];
    while let Some(dir_path) = stack.pop() {
        let mut entries: Vec<_> = fs::read_dir(&dir_path)
            .ok()?
            .collect::<Result<_, _>>()
            .ok()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let rel = path
                .strip_prefix(&base)
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, entry.metadata().ok()?.len()));
        }
    }
    out.sort();
    Some(out)
}

/// Whether a file of this length is served as text.
fn file_fits(len: u64) -> bool {
    len <= MAX_FILE_BYTES
}

/// One committed evidence file as plain text. Anything that is not a plain
/// relative file path under the evidence root 404s: empty shapes, `.`, `..`
/// (before and after decoding), and separators smuggled through decoding.
fn judges_file_response(root: &Path, parts: &[&str]) -> Response {
    if parts.is_empty()
        || parts
            .iter()
            .any(|s| s.is_empty() || *s == "." || *s == "..")
    {
        return Response::html(404, unknown_file_page(&parts.join("/")));
    }
    let decoded: Vec<String> = parts.iter().map(|s| url_decode(s)).collect();
    if decoded
        .iter()
        .any(|s| s.is_empty() || s == "." || s == ".." || s.contains('/') || s.contains('\\'))
    {
        return Response::html(404, unknown_file_page(&parts.join("/")));
    }
    let mut path = root.to_path_buf();
    for part in &decoded {
        path.push(part);
    }
    match fs::read(&path) {
        Ok(bytes) if file_fits(bytes.len() as u64) => {
            Response::media("text/plain; charset=utf-8".to_string(), bytes)
        }
        Ok(_) => Response::html(
            413,
            page(
                "File too large",
                "<section class=\"card\"><h1>File too large</h1><p>The committed evidence files \
                are small; this one is not served as text. <a href=\"/judges\">Back to \
                start here</a>.</p></section>",
            ),
        ),
        Err(_) => Response::html(404, unknown_file_page(&parts.join("/"))),
    }
}

/// The evidence root is not where serve runs: name the path looked for.
fn missing_evidence_page(root: &Path, want: &str) -> String {
    page(
        "Evidence not found",
        &format!(
            "<section class=\"card\"><h1>Evidence not found</h1><p><span class=\"mono\">{}</span> \
            is not under <span class=\"mono\">{}</span>, relative to the directory serve runs from. \
            Serve from the checkout root (a fresh clone), with the evidence committed. \
            <a href=\"/judges\">Back to start here</a>.</p></section>",
            esc(want),
            esc(&root.display().to_string()),
        ),
    )
}

/// No such run: name the four recorded ones.
fn unknown_run_page(root: &Path, name: &str) -> String {
    let mut body = format!(
        "<section class=\"card\"><h1>No such run</h1><p>There is no recorded run \
        <code>{}</code>. The recorded runs:</p><ul>",
        esc(name),
    );
    for run in RUNS {
        body.push_str(&format!(
            "<li><a href=\"/judges/runs/{dir}\">{label}</a> \
            <span class=\"small muted\">({task})</span></li>",
            dir = run.dir,
            label = esc(run.label),
            task = esc(run.task),
        ));
    }
    body.push_str("</ul>");
    if !root.is_dir() {
        body.push_str(&format!(
            "<p class=\"failed\">The evidence directory was not found here either: looked for \
            <span class=\"mono\">{}</span> relative to the directory serve runs from.</p>",
            esc(&root.display().to_string()),
        ));
    }
    body.push_str("<p><a href=\"/judges\">Back to start here</a>.</p></section>");
    page("No such run", &body)
}

fn unknown_file_page(name: &str) -> String {
    page(
        "Not found",
        &format!(
            "<section class=\"card\"><h1>Not found</h1><p>There is no evidence file \
            <code>{}</code>. <a href=\"/judges\">Back to start here</a>.</p></section>",
            esc(name),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn temp_dir(label: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "doyle-judges-test-{label}-{}-{n}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn empty_server(state_dir: &Path) -> ReviewServer {
        let store = crate::core::Store::open(state_dir).unwrap();
        ReviewServer::new(store, Vec::new())
    }

    /// Every `href="/…"` on a judges' page, in order.
    fn hrefs(body: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = body;
        while let Some(i) = rest.find("href=\"") {
            rest = &rest[i + 6..];
            match rest.find('"') {
                Some(end) => {
                    out.push(rest[..end].to_string());
                    rest = &rest[end + 1..];
                }
                None => break,
            }
        }
        out
    }

    /// The real checkout root: unit tests run with the package root as the
    /// working directory, so this is the fresh-clone layout with the evidence
    /// committed.
    fn checkout_evidence() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("evidence")
    }

    #[test]
    fn at_most_box_quotes_measures_md() {
        let measures = fs::read_to_string(checkout_evidence().join("measures.md"))
            .expect("measures.md is committed");
        let flat = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            flat(&measures).contains(&flat(AT_MOST)),
            "the index claims exactly what measures.md allows"
        );
    }

    #[test]
    fn index_through_stack_links_everything_the_brief_names() {
        let root = temp_dir("index");
        let mut server = empty_server(&root.join("state"));
        let response = server.handle("GET", "/judges", "");
        assert_eq!(response.status, 200);
        let body = &response.body;
        assert!(body.contains("For judges: start here"), "the title");
        // What is new (Doyle) and what is an existing dependency.
        assert!(body.contains("Doyle"), "names what is new");
        assert!(body.contains("Vimanam"), "names the contract dependency");
        assert!(body.contains("Lambo"), "names the memory dependency");
        // The recorded runs, the measures, the session logs, the review page.
        for run in RUNS {
            assert!(
                body.contains(&format!("/judges/runs/{}", run.dir)),
                "links the {} run",
                run.dir
            );
            for log in run.logs {
                assert!(
                    body.contains(&format!("/judges/file/{}/{}", run.dir, log)),
                    "links the {}/{} session log",
                    run.dir,
                    log
                );
            }
        }
        assert!(body.contains("/judges/measures"), "links the measures");
        assert!(body.contains("href=\"/\""), "links the review page itself");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn every_index_link_resolves_from_the_committed_evidence() {
        let root = temp_dir("closure");
        let mut server = empty_server(&root.join("state"));
        // A fresh clone serves with an empty state: no flag links, and the
        // page says so while the recorded state stays linked as files.
        let index = server.handle("GET", "/judges", "");
        assert_eq!(index.status, 200);
        assert!(index.body.contains("No flags in this serving state"));
        let mut responses = vec![index];
        // The measures page and every run page, then every file they link.
        for path in ["/judges/measures".to_string()]
            .into_iter()
            .chain(RUNS.iter().map(|r| format!("/judges/runs/{}", r.dir)))
        {
            responses.push(server.handle("GET", &path, ""));
        }
        let pages: Vec<String> = responses
            .iter()
            .map(|r| {
                assert_eq!(r.status, 200, "every linked page resolves");
                r.body.clone()
            })
            .collect();
        let mut checked = 0;
        for page_body in &pages {
            for href in hrefs(page_body) {
                assert!(
                    href.starts_with('/'),
                    "every judges' link stays on this server: {href}"
                );
                let response = server.handle("GET", &href, "");
                assert_eq!(response.status, 200, "link resolves: {href}");
                checked += 1;
            }
        }
        assert!(
            checked > 40,
            "the closure is the whole table: {checked} links"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn recorded_flags_link_to_live_flag_pages() {
        // The recorded state loads as a store: the judges' page links each
        // recorded flag, and each flag page serves (contract files are
        // absolute paths from the run, so the change facts list by ID).
        let root = temp_dir("recorded");
        let state = root.join("state");
        fs::create_dir_all(&state).unwrap();
        for file in [
            "citations.jsonl",
            "flags.jsonl",
            "proposals.jsonl",
            "decisions.jsonl",
        ] {
            fs::copy(
                checkout_evidence().join("with-doyle/state").join(file),
                state.join(file),
            )
            .unwrap();
        }
        let mut server = empty_server(&state);
        let index = server.handle("GET", "/judges", "");
        assert_eq!(index.status, 200);
        for flag in ["flag-1", "flag-2"] {
            assert!(
                index.body.contains(&format!("/flag/{flag}")),
                "links the recorded {flag}"
            );
            let flag_page = server.handle("GET", &format!("/flag/{flag}"), "");
            assert_eq!(flag_page.status, 200, "{flag} serves");
            assert!(
                flag_page.body.contains("vc2_85021e"),
                "names the change fact"
            );
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn run_page_lists_every_committed_file_and_file_route_streams_it() {
        let root = temp_dir("files");
        let mut server = empty_server(&root.join("state"));
        let page_response = server.handle("GET", "/judges/runs/with-doyle", "");
        assert_eq!(page_response.status, 200);
        assert!(
            page_response.body.contains("6 guard"),
            "the measured outcome"
        );
        assert!(
            page_response.body.contains("state/decisions.jsonl"),
            "state files are listed"
        );
        // The run page links only files the checkout holds, and each streams.
        for href in hrefs(&page_response.body) {
            assert!(href.starts_with('/'), "stays on this server: {href}");
            let response = server.handle("GET", &href, "");
            assert_eq!(response.status, 200, "resolves: {href}");
            if let Some(rel) = href.strip_prefix("/judges/file/") {
                let on_disk =
                    fs::read(checkout_evidence().join(rel)).expect("linked file is committed");
                let (content_type, bytes) = response.media.expect("file route streams bytes");
                assert_eq!(content_type, "text/plain; charset=utf-8");
                assert_eq!(bytes, on_disk, "streams {rel} byte-equal");
            }
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn run_pages_against_a_temp_copy_of_the_evidence_layout() {
        // The miniature layout: one run folder with a nested state file, plus
        // the measures. Proves the pages read files rather than assuming them.
        let layout = temp_dir("layout");
        let root = layout.join("evidence");
        fs::create_dir_all(root.join("with-doyle/state")).unwrap();
        fs::write(
            root.join("measures.md"),
            "# Measures\n\n2 memories flagged.\n",
        )
        .unwrap();
        fs::write(root.join("with-doyle/README.md"), "# Run\n").unwrap();
        fs::write(root.join("with-doyle/seed-export.json"), "{}\n").unwrap();
        for stub in [
            "with-doyle/migration-1-export.json",
            "with-doyle/migration-2-export.json",
            "with-doyle/session-two-measures.txt",
        ] {
            fs::write(root.join(stub), "stub\n").unwrap();
        }
        fs::write(
            root.join("with-doyle/state/decisions.jsonl"),
            "{\"id\":\"decision-1\"}\n",
        )
        .unwrap();

        let measures = measures_page_html(&root).expect("measures render");
        assert!(measures.contains("2 memories flagged"));

        let run = run_page_html(&root, "with-doyle").expect("run renders");
        for href in hrefs(&run) {
            // The shell's own links (brand, back link) are not files; the
            // through-the-stack tests prove they resolve.
            let Some(rel) = href.strip_prefix("/judges/file/") else {
                continue;
            };
            assert!(
                !rel.contains("..") && root.join(rel).is_file(),
                "every run link names a file in the layout: {href}"
            );
        }
        assert!(
            run.contains("with-doyle/state/decisions.jsonl"),
            "nested state files are listed"
        );
        assert!(
            run_page_html(&root, "no-such-run").is_none(),
            "unknown run is none"
        );

        // The file route against the same layout.
        assert!(
            judges_file_response(&root, &["with-doyle", "README.md"])
                .media
                .is_some()
        );
        assert_eq!(
            judges_file_response(&root, &["with-doyle", "missing.json"]).status,
            404
        );
        let _ = fs::remove_dir_all(&layout);
    }

    #[test]
    fn missing_evidence_names_its_path_and_escapes_are_refused() {
        let base = temp_dir("missing");
        let root = base.join("no-evidence-here");
        let state_base = temp_dir("missing-state");
        assert!(measures_page_html(&root).is_none());
        assert!(run_page_html(&root, "with-doyle").is_none());
        assert!(list_run_files(&root, "with-doyle").is_none());
        let mut server = empty_server(&state_base.join("state"));
        for path in [
            "/judges/runs/with-doyle",
            "/judges/runs/no-such-run",
            "/judges/measures",
            "/judges/file/with-doyle/README.md",
            "/judges/file/../Cargo.toml",
            "/judges/file/with-doyle/../../Cargo.toml",
            "/judges/no-such-page",
        ] {
            let response = server.handle("GET", path, "");
            if response.media.is_some() {
                continue; // a streamed file: resolves by definition
            }
            // Nothing here lands nowhere: a defined page names what is missing.
            assert!(
                response.body.contains("For judges")
                    || response.body.contains("start here")
                    || response.body.contains("Evidence not found")
                    || response.body.contains("No such run")
                    || response.body.contains("Not found"),
                "{path} explains itself: {}",
                response.body
            );
        }
        // Decoded separators and parent escapes are refused, not served.
        assert_eq!(judges_file_response(&root, &["..", "x"]).status, 404);
        assert_eq!(judges_file_response(&root, &["%2e%2e", "x"]).status, 404);
        assert_eq!(judges_file_response(&root, &[]).status, 404);
        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&state_base);
    }

    #[test]
    fn file_size_cap_refuses_absurd_files() {
        assert!(file_fits(0));
        assert!(file_fits(MAX_FILE_BYTES));
        assert!(!file_fits(MAX_FILE_BYTES + 1));
    }
}
