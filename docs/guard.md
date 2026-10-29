# `doyle guard`: the Muse Code hook

`doyle guard` is the enforcement end of Doyle. Muse Code runs it as a hook
on every tool call (`PreToolUse`) and at session start (`SessionStart`).
It is deterministic: it loads Doyle's store, takes the open flags, matches
the tool input against the flagged elements' native locations, and either
denies or stays silent. No model is called. One call takes about 5 ms
(PreToolUse) to 1 ms (SessionStart), including process startup.

## Registering it

One command serves both events: `doyle guard` reads `hook_event_name`
from stdin. Bake the absolute binary path and an absolute `--state-dir`
into the command. Hooks run with a cleared environment, so relative
paths and shell variables will not resolve; `cwd` in the hook payload
is the workspace root.

For one user, in `~/.config/muse/settings.json` (merge into the existing
object, keep `"schema_version"`):

```json
"hooks": {
  "SessionStart": [{"hooks": [{"type": "command", "command": "/path/to/doyle guard --state-dir /path/to/repo/.doyle"}]}],
  "PreToolUse":   [{"matcher": "*", "hooks": [{"type": "command", "command": "/path/to/doyle guard --state-dir /path/to/repo/.doyle"}]}]
}
```

For one project, the same `hooks` object in `<repo>/.muse/hooks.json`
as the only top-level key. It must sit in the directory the agent is
started in (the workspace root defaults to the starting directory, not
the git root), and the workspace must be trusted. Hooks need no plugin.

How the guard finds things: `--state-dir` (default `.doyle` under the
workspace) locates the store. Each open flag's `new_version` — the
contract path recorded when `doyle check` flagged it — locates the
contract the flag is matched through, resolved against the workspace
when relative. Run `doyle check` from the repository root so the
recorded paths resolve from wherever the hook fires.

## What it does

- **PreToolUse, action touches a flagged element:** deny. The agent sees
  `tool blocked by hook:` followed by the reason, which names the memory
  (`adapter/id`), quotes its stale claim in one line, names the matched
  native location, the versions on either side of the change and its
  severity, and points at the review page:

  `DOYLE-GUARD: memory muse-code/M1 is stale ("GET /patients returns every patient in one response"). Matched GET /patients (listPatients). Changed v1 -> v2 (breaking). Review with `doyle serve`, then retry against the current contract.`

  Technically: JSON `hookSpecificOutput` with `permissionDecision: deny`
  on stdout, exit 0.

- **PreToolUse, anything else:** allow, silently. Empty stdout, exit 0.

- **SessionStart, open flags exist:** inject one plain notice naming each
  flagged memory and the review page, as `additionalContext`:

  `DOYLE NOTICE: 1 memory flagged stale: muse-code/M1 ("GET /patients returns every patient in one response"). Review with `doyle serve`.`

- **SessionStart, no open flags:** nothing. Empty stdout, exit 0.

- **Doyle's state can't be read** (no store, no contract for a flag):
  allow, with one warning on stderr. A broken guard never blocks the
  agent's work.

- **Hook input can't be parsed** (no JSON, no `hook_event_name`, a
  `PreToolUse` without `tool_name`): exit 2 with the reason on stderr.
  The hook setup itself is broken and needs attention; there is no event
  to route.

## Matching rules

The guard matches the tool input's strings (shell commands, URLs, file
contents, memory text — every string in `tool_input`) against the native
locations of the flagged memories' cited elements. Every rule below is
pinned by a test in `src/guard.rs`; the tests use the recorded payloads
in `testdata/muse/hooks/` and a seeded fixture store with M1's flag open.

1. **Method and path match exactly, segment for segment.** A `{param}`
   segment matches any single non-empty segment. `GET /patients/{id}`
   matches `/patients/42` but never `/patients`; `GET /patients` never
   matches `/patients/42`.
2. **Queries are stripped, trailing slashes and trailing dots dropped,
   methods compared case-insensitively, paths case-sensitively.** `curl
   ".../patients?cursor=abc&limit=100"` still matches `GET /patients`,
   as do `.../patients/` and `.../patients.`; `curl -X POST .../patients`
   does not match a flagged `GET`.
3. **One method per URL, from the nearest preceding flag in its
   fragment.** The tool input is split into fragments at `;`, `&&`, `||`
   and newlines outside single and double quotes (a backslash escapes the
   next character inside quotes, so a quoted separator never splits; a
   lone `&` or `|` is not a boundary). Each URL or bare
   path in a fragment takes the method of the nearest `-X`/`--request`
   flag before it in that fragment; when no method flag precedes it, a
   fragment carrying data flags (`-d`, `-F`, `--data*`, `--form*`) means
   POST, else GET. So `curl -X DELETE .../patients/abc; curl
   .../patients` still denies the flagged `GET` fetch, in either order.
4. **Bare paths match too.** Each whitespace-separated token is split
   further on quotes, brackets, backticks, commas, semicolons, angle
   brackets and `=`; every `/`-led run that is neither a URL (no `://`)
   nor a `//host` leftover counts, without its query or fragment, and only
   when every segment after the leading slash is non-empty. So a
   `write_file` content of exactly `/patients` denies, as does
   `fetch("/patients")` in an edit.
5. **Operation IDs and type names match as whole identifiers,
   case-sensitively.** `getStats` never matches `getStatistics` or
   `xgetStats`; `Stats` never matches `status` or `MyStats`.
6. **Field names never match.** `patient_count` in a tool input does not
   flag, by decision: field-level signals are too noisy, and the
   operation and type anchors carry the match.
7. **Each flag is matched through both its contracts.** Every cited
   element is resolved through the flag's old version (what the memory
   was written against) and its new version (what made it stale); a side
   that fails to load is skipped. A rename therefore matches both names,
   and a removed endpoint still matches through the old contract. When
   no side yields a location, the action is allowed with a warning, never
   blocked.
8. **The internal `submit_reminder_decision` tool always passes**, before
   any state is read.
9. **Unknown tool names pass unseen.** Allow unknown, deny known-bad.
10. **Unknown event names pass silently**, so a future event the guard
   does not know stays out of the agent's way.

Only the host-independent part of a URL is matched: the path (and the
method around it). The host and port are ignored, so the check does not
depend on where the service under test happens to listen.
