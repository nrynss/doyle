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

---

### T6.2: Review page ★
```yaml
requires:   [T5.2, T5.3]
fixture-ok: yes
size:       L · frontier
owns:       src/serve/ (except judges.rs, T7.5), Cargo.toml (the server line, if any), Cargo.lock
status:     claimed:meta-wonder-t6.2
```
`doyle serve`: a local page, server-rendered, no build step. For each open flag: what changed (the Vimanam change, old and new), the memory as each system holds it, which agents relied on it, the proposal and its rationale, and approve / edit / retire / keep, which call T5.3. A settled flag shows what was written where. Decide here whether `serve` uses a small server crate or `std::net`, and say why in the commit.

Readable at phone and desktop width, in light and dark. Everything a judge needs to understand one correction fits on one screen.

**Done when:** on the demo fixtures, a person can open the page, read the stale memory's flag, approve the proposal, and see the correction recorded for both systems; tests cover each route; the page has been looked at in a browser at both widths and in both themes.

---

## Exit Criteria

- [x] The guard denies an action resting on a stale memory and allows the rest, offline (T6.1). Live Wonder session follows in T6.1b.
- [ ] A person settles a correction from the review page, and it lands in both systems (T6.2).
