//! T3.3b live probe of the Lambo adapter against the running server.
//!
//! Session under test: `doyle-probe` at
//! `http://127.0.0.1:7700/mcp/s/doyle-probe`. Never touches `lambo-dev` or
//! `doyle`. Budget: one write, one correct, one recall, one retry, one `*`
//! probe. Run with `cargo test -- --ignored live_`; never runs in CI.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::{SystemTime, UNIX_EPOCH};

use doyle_tms::adapters::MemoryAdapter;
use doyle_tms::adapters::lambo::LamboAdapter;
use doyle_tms::mcp::{HttpResponse, Transport, TransportError, UreqTransport};

const URL: &str = "http://127.0.0.1:7700/mcp/s/doyle-probe";
const AGENT_ID: &str = "doyle-probe";

/// Pass-through transport that counts `lambo_derive` calls so the retry
/// no-op can prove it issued no duplicate derive.
struct CountingTransport {
    inner: UreqTransport,
    derives: Arc<AtomicUsize>,
}

impl CountingTransport {
    fn new(derives: Arc<AtomicUsize>) -> Self {
        Self {
            inner: UreqTransport::new(),
            derives,
        }
    }
}

impl Transport for CountingTransport {
    fn post(
        &self,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> Result<HttpResponse, TransportError> {
        if body.contains("lambo_derive") {
            self.derives.fetch_add(1, Ordering::SeqCst);
        }
        self.inner.post(url, headers, body)
    }
}

fn unique_token() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_nanos();
    format!("t33b-{}-{}", std::process::id(), nanos)
}

#[test]
#[ignore]
fn live_write_correct_recall_retry_and_star_probe() {
    let token = unique_token();
    let stale_text = format!("{token}: the north gate opens at dawn");
    let correction_text = format!("{token}: the north gate opens at dusk, not dawn");
    let query = format!("{token} north gate");

    let derives = Arc::new(AtomicUsize::new(0));
    let transport = CountingTransport::new(Arc::clone(&derives));
    let adapter = LamboAdapter::new(URL, AGENT_ID, Box::new(transport));

    // (1a) Write a memory through the adapter.
    let stale_ref = adapter
        .write(&stale_text, "doyle-probe")
        .expect("live write failed");
    println!("WRITE stale id: {}", stale_ref.id);
    assert!(!stale_ref.id.is_empty());

    // (1b) Correct it through the adapter's supersede.
    let corr_ref = adapter
        .supersede(&stale_ref.id, &correction_text)
        .expect("live supersede failed");
    println!("CORRECT correction id: {}", corr_ref.id);
    println!("CORRECT against stale short id: {}", &stale_ref.id[..8]);
    assert_ne!(corr_ref.id, stale_ref.id, "correction must be a new node");

    // (1c) Recall it and show the warning.
    let listed = adapter.list(&query).expect("live recall failed");
    println!("RECALL hits: {}", listed.len());
    let warnings = adapter.recall_warnings();
    println!("RECALL warnings ({}):", warnings.len());
    for w in &warnings {
        println!("  WARNING: {w}");
    }
    assert!(
        warnings.iter().any(|w| w.contains("Contradict")),
        "expected a Contradicted/Contradicts warning, got: {warnings:?}"
    );
    let stale_read = adapter.read(&stale_ref.id).expect("live read failed");
    println!("READ stale text: {}", stale_read.text);
    assert!(
        stale_read.text.contains("Contradicted"),
        "stale read must carry the warning: {}",
        stale_read.text
    );
    assert!(
        stale_read.text.contains(&corr_ref.id[..8]),
        "stale read must name the correction: {}",
        stale_read.text
    );

    // (2) Supersede retry: correct the same stale memory again. The second
    // call must be a no-op returning the existing ref with no duplicate
    // derive.
    let derives_before = derives.load(Ordering::SeqCst);
    let retry_ref = adapter
        .supersede(&stale_ref.id, &correction_text)
        .expect("live supersede retry failed");
    let derives_after = derives.load(Ordering::SeqCst);
    println!("RETRY ref id: {}", retry_ref.id);
    println!("RETRY derive calls before={derives_before} after={derives_after}");
    assert_eq!(
        retry_ref.id, corr_ref.id,
        "retry must return the existing correction ref"
    );
    assert_eq!(
        derives_before, derives_after,
        "retry must issue no duplicate derive"
    );

    // (3) Broad-watch `*` probe: record what the server returns for `*`.
    match adapter.list("*") {
        Ok(hits) => {
            let ours = hits
                .iter()
                .filter(|m| m.reference.id == stale_ref.id || m.reference.id == corr_ref.id)
                .count();
            println!(
                "STAR probe: ok, {} hits total, {} of ours present",
                hits.len(),
                ours
            );
            for m in hits.iter().take(5) {
                println!("  STAR hit: {} :: {}", m.reference.id, m.text);
            }
            if ours == 2 {
                println!("STAR verdict: serves as match-all (both probe memories returned)");
            } else {
                println!("STAR verdict: NOT a reliable match-all (probe memories missing)");
            }
        }
        Err(e) => {
            println!("STAR probe: server refused `*`: {e}");
            println!("STAR verdict: `*` is not a usable match-all (call failed)");
        }
    }
}
