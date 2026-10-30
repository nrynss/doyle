# P6: Surfaces

Where people and agents meet Doyle: the guard inside Muse Code, and the review page where a person settles each correction.

Read first: [`../docs/architecture.md`](../docs/architecture.md) (Commands), `docs/muse-code.md` and `testdata/muse/hooks/` from T0.4, [`../product.md`](../product.md) (Cautions).

---

### T6.1: Guard ★
```yaml
requires:   [T0.4, T1.2, T2.1, T2.2]
fixture-ok: yes
size:       M · frontier
owns:       src/guard.rs, docs/guard.md
status:     done
```
`doyle guard`, run by Muse Code as a hook (registered the way T0.4 found works).

- **`PreToolUse`:** read the hook JSON on stdin. If the tool input touches an element with an open flag (method and path in a `curl` or a URL, an operation ID, a type name, matched against the contract index's native locations), deny with a reason naming the memory, what changed and where the review page is. Otherwise allow, silently.
- **`SessionStart`:** inject a short plain notice of open flags in this repository, or nothing when there are none.

No model call. Fast enough not to be felt (measure it). If Doyle's state can't be read, allow and print one warning to stderr: a broken guard never blocks the agent's work. `docs/guard.md` gives the hook configuration to paste.

**Done when:** on the recorded payloads, the demo's one-call export against `GET /patients` is denied with the reason, and unrelated calls pass; the SessionStart output is checked; the measured time per call is in the handoff text.

**T6.1b (live):** register the guard in a scratch repository, open a flag, and watch `muse exec` on `muse-wonder-1.0` get denied and say why. Owns `tests/live_guard.rs`. Budget: three sessions.

**Notes (T6.1, in review)**
- Built: `src/guard.rs` holds the full guard (`run` keeps its shape): parse one hook JSON on stdin, route on `hook_event_name`. PreToolUse denies (JSON `permissionDecision: deny`, exit 0) when the tool input touches a flagged element, else allows silently; SessionStart injects `additionalContext` when open flags exist, else nothing. No model call; std only.
- Decided: the guard needs no new CLI surface (frozen files untouched). `--state-dir` locates the store; each open flag's `new_version` locates the contract it is matched through (resolved against the hook `cwd`), so `doyle check` should run from the repository root. A flag whose contract cannot be loaded cannot deny: the call is allowed with one stderr warning. Unparsable hook input is the one fail-closed case (exit 2): the hook setup itself is broken. Unknown tools, unknown events, and `submit_reminder_decision` always pass.
- Decided: matching rules (all pinned by tests, documented in `docs/guard.md`): exact method+path with `{param}` segments matching one segment only; query stripped, trailing slash tolerated, method case-insensitive, path case-sensitive; per-fragment method attribution (`-X`, else data-flags imply POST, else GET); operation IDs and type names as case-sensitive whole identifiers; field names never match; host/port ignored.
- Measured: 21 guard tests green (recorded payloads: demo export against `GET /patients` denied naming M1, the change and the review page; `getPatient` detail, `getStats`, decommission, `submit_reminder_decision`, unknown tools and the recorded edit/write/memory payloads pass silently; SessionStart notice present with flags, empty without; generous 5 s upper bound asserted). Real binary, fixture store via `doyle cite` + `doyle check`: deny/session/allow median 5.0 / 1.2 / 5.2 ms over 10 runs.
- Surprised me: `check` records its OLD/NEW argument strings as the flag's versions, which is exactly the pointer the guard needs — no new state was required. Also, `NativeLocation` carries no field-level anchor need: `patient_count` matching was deliberately left out as noise.
- For T6.1b: the hook command to register is `/path/to/doyle guard --state-dir /path/to/repo/.doyle`, run from the repository root, workspace trusted.
- **T6.1b notes (live):**
  - Built: `tests/live_guard.rs` (`live_guard_denies_flagged_export_in_wonder_session`, `#[ignore]`d, only under `cargo test -- --ignored live_`). Seeds the demo's M1 flag in a scratch repo through the real `doyle cite` + `doyle check --no-recheck` path (no model spent on setup; relative contract paths so they resolve against the hook cwd), registers the guard hooks pointing at the worktree's own build (`CARGO_BIN_EXE_doyle`), then runs one `muse exec` on `muse-wonder-1.0` attempting `curl .../patients` and asserts the session sees `tool blocked by hook: DOYLE-GUARD:` naming `muse-code/M1`, `GET /patients` and `doyle serve`. Global `settings.json`/`trust.json` are checksummed before and after (asserted byte-identical); the scratch repo is removed at the end.
  - Measured live: the session's bash call was rejected before running; the tool result and the final answer both read `tool blocked by hook: DOYLE-GUARD: memory muse-code/M1 is stale ("GET /patients returns every patient in one response"). Matched GET /patients (listPatients). Changed contracts/patients-v1.yaml -> contracts/patients-v2.yaml (breaking). Review with `doyle serve`, then retry against the current contract.` SessionStart also fired, injecting `DOYLE NOTICE: 1 memory flagged stale: muse-code/M1 (...)`. Spend: 2 of the 3 budgeted sessions (one manual probe, one test run); no memory was written, no settings changed.
  - Surprised me: the committed test's run finished in one short session — the deny lands on the first tool call, so the model quotes it and stops.

---

### T6.2: Review page ★
```yaml
requires:   [T5.2, T5.3]
fixture-ok: yes
size:       L · frontier
owns:       src/serve/ (except judges.rs, T7.5), Cargo.toml (the server line, if any), Cargo.lock
status:     done
```
`doyle serve`: a local page, server-rendered, no build step. For each open flag: what changed (the Vimanam change, old and new), the memory as each system holds it, which agents relied on it, the proposal and its rationale, and approve / edit / retire / keep, which call T5.3. A settled flag shows what was written where. Decide here whether `serve` uses a small server crate or `std::net`, and say why in the commit.

Readable at phone and desktop width, in light and dark. Everything a judge needs to understand one correction fits on one screen.

**Done when:** on the demo fixtures, a person can open the page, read the stale memory's flag, approve the proposal, and see the correction recorded for both systems; tests cover each route; the page has been looked at in a browser at both widths and in both themes.

**Notes (T6.2, in review)**
- Built: `src/serve/mod.rs` holds `ReviewServer` (store + adapters + provenance + in-process decision reports) with routes `GET /`, `GET /flag/<id>`, `POST /flag/<id>/decide`, `POST /flag/<id>/retry`; `src/serve/render.rs` holds the HTML shell and the one inline stylesheet; `src/serve/logo.rs` holds the inline-SVG Doyle mark (magnifying glass over tangled threads, `currentColor` only, separate commit). The detail page fits one correction on one screen: what changed (each Vimanam fact with old → new, plus the cited elements' old/new contract text), the memory as each system holds it, who relied on it, the proposal and rationale, then approve / edit / retire / keep calling T5.3's `apply_decision`; a settled flag shows the recorded decision and, per system, what was written where.
- Decided: `std::net` by hand, no server crate (`Cargo.toml` untouched; `ureq` stays client-only). The page is one local single-user surface, requests are handled one at a time because the adapters keep process-local state behind `&self` and are not `Sync`, and the standard library covers listen / parse / respond. Listens on `127.0.0.1:8080` (the demo's reserved Doyle port; `DOYLE_SERVE_PORT` overrides), adapters from `--adapters` or `muse-code,lambo` by default.
- Decided: copy resolution (T5.3's caller job) — the flag's own reference read back first, then every other configured adapter searched by the cited text (source text, claim, evidence), so a logical store ID like `M1` maps to `MEMORY.md#1` in Muse Code and to the concept in Lambo. An adapter that cannot be searched is named on the page with its error instead of being silently skipped; a system not configured at all is shown from Doyle's cited text with the gap stated, and deciding with no writable copy is refused (409) without recording.
- Decided: retry state lives in the serving process, as T5.3 left it to this page. A failed write-back stays on the settled view marked for retry with a retry button (`retry_failed`, identical text, converges); after a restart the settled view still shows the recorded decision and reads each memory back as its system now holds it, saying plainly that the per-system results are not in this process.
- Decided: provenance comes from `testdata/memories/provenance.json` when it sits under the working directory (matched by memory text, then by system ID); without it the page says no provenance is recorded and names the citation's recorder instead. Contract old/new display loads the two files the flag names and falls back to change IDs when they cannot be read from here.
- Verified: 15 serve tests (seeded end-to-end: M1–M4 through the real `cite` path, flag + proposal through the real `check` path with replayed replies, Muse Code on a temp fixture copy, Lambo over a scripted FakeTransport — approve lands the correction in both, edit writes the edited text, retire/keep hold, failed write-back shows and its retry lands exactly one correction, restart read-back, 400/404/409 refusals, one socket-level test). Browser evidence: headless Firefox screenshots of the flag page at desktop (1280 px) and phone (390 px) widths, in light and dark, plus the settled view after a real approve POST to the running binary (303, `[Superseded by MEMORY.md#5]`); the screenshots are large PNGs and were not committed.
- Surprised me: `check`'s verbatim re-check reuse is gated on the store ID `M1`, so the seeded flag's memory ID is logical, not either system's own — text-based copy resolution turned out to be load-bearing, not a fallback. Also, Vimanam's `constraint_changed` fact carries the whole old schema in its details; the page names a schema by its `type` instead of dumping it.

---

## Exit Criteria

- [x] The guard denies an action resting on a stale memory and allows the rest (T6.1 offline; T6.1b live: Wonder session denied with the reason, settings untouched).
- [x] A person settles a correction from the review page, and it lands in both systems (T6.2).
