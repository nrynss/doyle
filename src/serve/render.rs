//! Server-side rendering for the review page: one HTML shell, one inline
//! stylesheet, no build step and no framework.
//!
//! Readable at phone and desktop width (a single column that widens into a
//! two-column detail grid) and in light and dark (`color-scheme` plus
//! `prefers-color-scheme`; every colour is a custom property, and the logo
//! paints with `currentColor`). Everything a judge needs to understand one
//! correction is on the flag's own page.

use super::logo::DOYLE_LOGO;

/// Escape text for HTML content and attribute values.
pub fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// The page shell: header with the Doyle mark, then the caller's body.
pub fn page(title: &str, body: &str) -> String {
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <meta name=\"color-scheme\" content=\"light dark\">\n\
         <title>{title} · Doyle review</title>\n<style>{CSS}</style>\n</head>\n<body>\n\
         <header><a class=\"brand\" href=\"/\">{logo}<span>Doyle</span></a>\
         <span class=\"tagline\">truth maintenance for agent memory</span></header>\n\
         <main>\n{body}</main>\n\
         <footer>Doyle review page · decisions are recorded in Doyle's state and written back to each memory system shown.</footer>\n\
         </body>\n</html>\n",
        title = esc(title),
        logo = DOYLE_LOGO,
        body = body,
    )
}

/// The line a page without the reviewer cookie shows in place of a form.
pub fn reviewer_only_section() -> String {
    "<section class=\"card wide\"><h2>Decide</h2><p>Flags are settled from the reviewer link printed by <code>doyle serve</code>, by a person. This view is read-only.</p></section>".to_string()
}

/// The 403 page for a decision or retry without the reviewer cookie.
pub fn reviewer_only_page() -> String {
    page(
        "Reviewer only",
        "<section class=\"card\"><h1>A person settles flags</h1><p>Nothing was recorded. A person settles flags from the reviewer link printed by <code>doyle serve</code>.</p><p><a href=\"/\">Back to the review list</a>.</p></section>",
    )
}

/// A severity pill (`breaking` / `review`), matching `doyle check`'s words.
pub fn severity_badge(severity: crate::core::Severity) -> String {
    let word = match severity {
        crate::core::Severity::Breaking => "breaking",
        crate::core::Severity::Review => "review",
    };
    format!("<span class=\"badge sev-{word}\">{word}</span>")
}

const CSS: &str = r#"
:root {
  color-scheme: light dark;
  --bg: #f7f5f0; --panel: #fffdf8; --ink: #1d1a15; --muted: #6b6252;
  --line: #ddd5c4; --accent: #8a5a12; --accent-ink: #ffffff;
  --bad: #9c2b1b; --bad-bg: #fbe9e4; --warn-bg: #fdf3d7; --ok: #256d33; --ok-bg: #e6f2e4;
  --code-bg: #efe9db;
}
@media (prefers-color-scheme: dark) {
  :root {
    --bg: #171512; --panel: #211e19; --ink: #ece5d3; --muted: #a79c85;
    --line: #3d372c; --accent: #d9a441; --accent-ink: #211806;
    --bad: #ff9d8a; --bad-bg: #3d1f18; --warn-bg: #3a3013; --ok: #93d69e; --ok-bg: #1d2f1e;
    --code-bg: #2b271f;
  }
}
* { box-sizing: border-box; }
body { margin: 0; background: var(--bg); color: var(--ink);
  font: 16px/1.5 system-ui, -apple-system, "Segoe UI", sans-serif; }
header { display: flex; align-items: center; gap: .6rem; flex-wrap: wrap;
  padding: .7rem 1rem; border-bottom: 1px solid var(--line); background: var(--panel); }
.brand { display: inline-flex; align-items: center; gap: .45rem; color: var(--ink);
  text-decoration: none; font-weight: 700; font-size: 1.25rem; }
.logo { color: var(--accent); flex: none; align-self: center; }
.tagline { color: var(--muted); font-size: .9rem; }
main { max-width: 62rem; margin: 0 auto; padding: 1rem; }
footer { max-width: 62rem; margin: 0 auto; padding: 0 1rem 2rem; color: var(--muted); font-size: .85rem; }
h1 { font-size: 1.45rem; margin: .4rem 0 .6rem; }
h2 { font-size: 1.08rem; margin: 0 0 .5rem; }
p { margin: .35rem 0; }
a { color: var(--accent); }
.card { background: var(--panel); border: 1px solid var(--line); border-radius: .6rem;
  padding: .9rem 1rem; margin: 0 0 1rem; }
.grid { display: grid; gap: 1rem; grid-template-columns: 1fr; }
@media (min-width: 46rem) { .grid { grid-template-columns: 1fr 1fr; } .grid .wide { grid-column: 1 / -1; } }
.badge { display: inline-block; padding: .05rem .5rem; border-radius: 1rem; font-size: .8rem;
  font-weight: 600; border: 1px solid var(--line); }
.sev-breaking { background: var(--bad-bg); color: var(--bad); }
.sev-review { background: var(--warn-bg); }
.state-open { background: var(--warn-bg); }
.state-settled { background: var(--ok-bg); color: var(--ok); }
code, .mono { font-family: ui-monospace, "SF Mono", Menlo, monospace; font-size: .86em;
  background: var(--code-bg); padding: .05rem .3rem; border-radius: .3rem; word-break: break-all; }
pre { background: var(--code-bg); padding: .6rem .7rem; border-radius: .45rem; overflow-x: auto;
  font: .82rem/1.45 ui-monospace, "SF Mono", Menlo, monospace; white-space: pre-wrap; word-break: break-word; margin: .4rem 0; }
.memory-text { font-size: 1.02rem; }
.muted { color: var(--muted); }
.small { font-size: .88rem; }
table { border-collapse: collapse; width: 100%; font-size: .92rem; }
th, td { text-align: left; vertical-align: top; padding: .35rem .5rem .35rem 0; border-bottom: 1px solid var(--line); }
.diff { display: grid; gap: .6rem; grid-template-columns: 1fr; }
@media (min-width: 46rem) { .diff { grid-template-columns: 1fr 1fr; } }
.diff h3 { margin: 0 0 .3rem; font-size: .9rem; }
.old h3 { color: var(--bad); } .new h3 { color: var(--ok); }
.actions { display: flex; flex-wrap: wrap; gap: .8rem; align-items: flex-end; }
.actions form { display: inline; }
button { font: inherit; font-weight: 600; cursor: pointer; border-radius: .5rem;
  border: 1px solid var(--accent); background: var(--accent); color: var(--accent-ink);
  padding: .5rem 1rem; }
button.secondary { background: transparent; color: var(--accent); }
button.danger { background: transparent; border-color: var(--bad); color: var(--bad); }
label { display: block; font-size: .88rem; font-weight: 600; margin: .5rem 0 .2rem; }
input[type="text"], textarea { font: inherit; width: 100%; padding: .45rem .6rem;
  border: 1px solid var(--line); border-radius: .45rem; background: var(--panel); color: var(--ink); }
.editbox { flex: 1 1 16rem; }
@media (max-width: 46rem) {
  table { display: block; overflow-x: auto; font-size: .8rem; }
  th, td { padding-right: .8rem; overflow-wrap: anywhere; }
}
ul { margin: .3rem 0 .3rem 1.1rem; padding: 0; }
.notice { border: 1px solid var(--line); border-left: .3rem solid var(--accent);
  background: var(--panel); border-radius: .4rem; padding: .6rem .8rem; margin: 0 0 1rem; }
.failed { color: var(--bad); font-weight: 600; }
.ok { color: var(--ok); font-weight: 600; }
.flaglink { text-decoration: none; color: inherit; display: block; }
.flaglink:hover .card { border-color: var(--accent); }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn esc_covers_markup_characters() {
        assert_eq!(
            esc("<a href=\"x\">&'</a>"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;&lt;/a&gt;"
        );
        assert_eq!(esc("plain /patients"), "plain /patients");
    }

    #[test]
    fn page_shell_has_logo_viewport_and_color_scheme() {
        let html = page("flag-1", "<p>body</p>");
        assert!(html.contains(DOYLE_LOGO), "header carries the logo");
        assert!(html.contains("name=\"viewport\""));
        assert!(html.contains("prefers-color-scheme: dark"));
        assert!(html.contains("color-scheme: light dark"));
        assert!(html.contains("<p>body</p>"));
        assert!(html.contains("flag-1 · Doyle review"));
    }
}
