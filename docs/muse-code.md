# Muse Code: what Doyle relies on

Everything here was measured on Muse Code 1.4.14 (`muse --version` prints `Muse Code 1.4.14 (1.4.14-R7309.1)`), Linux. Each claim names the command that showed it and the trimmed output. `$S` is a scratch directory; its absolute path is not kept. The recorded hook payloads are in [`testdata/muse/hooks/`](../testdata/muse/hooks/).

The probes ran as `muse exec --model muse-wonder-1.0 --max-model-steps 6 --trust-workspace "<prompt>"` inside `timeout 300`, eight sessions in all. Scratch repositories: `A` (a git repository with `A/sub/`), `C` (a linked worktree of `A`), `D.dir_x/Proj.v2` (a repository with dots and an underscore in its path), and `Q.d_Z` (a plain directory, reached through the symlink `L.k_Y`).

## 1. Memory

### 1.1 What Wonder remembers (already established, restated)

- `muse-wonder-1.0` recalls only what it wrote to memory. A fact it was told not to save came back UNKNOWN in a fresh session; only session logs held it.
- Told to remember, it calls a memory tool (`add_memory`; `edit_memory` also exists) with `scope: personal_project`. The default scope lands in `~/.local/share/muse/memory/projects/<folder>/MEMORY.md`. The shared scope is `<repo>/.agents/memory/` (not exercised in these probes).
- A note carries `type` and `description` frontmatter and no source.
- `[STALE per Doyle ...: the codename is now SAFFRON-7]` appended to a note changed Wonder's answer in a fresh session. Annotation in place is the override.
- Muse Code injects a memory snapshot at session start. Session 6 asked for it back:

```
Memory snapshot for this run. This snapshot is fixed for this run and may be stale after memory writes. Use read_memory for live memory or topic details.
## Memory scope: personal_project
MEMORY.md:
- The office plant is named Fernando.
```

So an annotation written before a session starts is in the snapshot, and one written during a session is not. The `add_memory` payload is in `testdata/muse/hooks/pre-tool-use-add-memory.json`: `{"content": ..., "path": "MEMORY.md", "scope": "personal_project"}`, with `description` and `type` added when the model chose to.

### 1.2 The memory folder name

The folder is `<slug>-<hash>`, and both parts come from the session's working directory, not from the git root and not from a remote.

**Hash.** FNV-1a, 64 bit (offset basis `0xcbf29ce484222325`, prime `0x100000001b3`), over the UTF-8 bytes of the physical working directory path, no trailing slash, no terminator; printed as 16 lower-case hex digits.

Found by trying candidate functions against the one folder known beforehand (`rt-a`, hash `172c46228899dd9e`): SHA-256, SHA-1, MD5, BLAKE2, SHA-3, CRC32, SipHash-1-3 and 2-4, xxHash64 and FNV-1/1a, over the path with and without trailing slash, newline, `.git` suffix, `file://` prefix and similar variants. Only FNV-1a over the bare path matched. Then predicted four more folders before reading them back:

```
$ python3 fnv.py $S/repos/A/sub $S/repos/A $S/repos/C "$S/repos/D.dir_x/Proj.v2" $S/Q.d_Z $S/L.k_Y
700ab9af450d5939 .../repos/A/sub
43479d6a77f02fbc .../repos/A
43479f6a77f03322 .../repos/C
afadbf78d22e6671 .../repos/D.dir_x/Proj.v2
7f8344ce4b2f4a11 .../Q.d_Z
63479c3caf33d996 .../L.k_Y

$ ls ~/.local/share/muse/memory/projects/     # after one session in each of A/sub, C, D.dir_x/Proj.v2, L.k_Y
...-scratchpad-t04-repos--700ab9af450d5939      <- session in A/sub
...-scratchpad-t04-repos--43479f6a77f03322      <- session in C
...-scratchpad-t04-repos--afadbf78d22e6671      <- session in D.dir_x/Proj.v2
...-scratchpad-t04-Q-d-Z-7f8344ce4b2f4a11       <- session in L.k_Y (the symlink)
```

Every folder matches its predicted hash.

What that shows:

| Case | Result |
|---|---|
| Session started in a subdirectory of a repository (`A/sub`, root `A`) | Keyed on `A/sub`. The repository root `A` (`43479d6a77f02fbc`) was not used |
| Linked worktree `C` of `A` | Its own folder, keyed on `C`. A worktree does not share the main checkout's memory |
| Session started through a symlink | Keyed on the resolved path (`Q.d_Z`, not `L.k_Y`). `muse` printed `workspace root: .../Q.d_Z (cwd default)` |
| Git remote | Irrelevant; the hash is a function of the path string alone |

The memory is per directory where `muse` starts, so the same repository started from two directories has two memories.

**Slug.** The same path with every character outside `[A-Za-z0-9]` replaced by `-`, the leading `-` dropped, case kept, cut to 96 characters. The folder is `<slug>-<hash>`.

```
/tmp/work/-home-me-proj/<uuid>/scratchpad/rt-a   ->  tmp-work--home-me-proj-<uuid>-scratchpad-rt-a-172c46228899dd9e   (path shortened for this page; the real folder was 90 characters of slug plus the hash)
.../scratchpad/t04/Q.d_Z                                        ->  ...-scratchpad-t04-Q-d-Z-7f8344ce4b2f4a11           ('.' and '_' became '-', 'Q' and 'Z' kept)
.../scratchpad/t04/repos/A/sub                                  ->  ...-scratchpad-t04-repos--700ab9af450d5939         (cut at 96; the slug ends in "repos-")
```

Measured with `for d in *; do s=${d%-????????????????}; echo "${#s} $s"; done`: lengths 90 and 96. The cut is why a long path's slug alone cannot tell two directories apart (the `A`, `C` and `D.dir_x/Proj.v2` folders above share the slug `...-t04-repos-`); the hash does. Non-ASCII path characters were not tried.

**Rule for the adapter.** Compute the hash from the canonicalised (symlinks resolved, no trailing slash) directory the agent is started in, and open the folder whose name ends in `-<hash>`. Match on the hash suffix alone and ignore the slug; the slug is cosmetic. The fallback of matching the slug and taking the newest folder is not needed, and would be wrong for any path longer than 96 characters, where unrelated directories share a slug.

```rust
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes { h = (h ^ *b as u64).wrapping_mul(0x0000_0100_0000_01b3); }
    h
}
// folder suffix: format!("-{:016x}", fnv1a64(canonical_cwd.as_os_str().as_bytes()))
```

Open: the cost of the key being the starting directory is that Doyle must be told, or must infer, where the agent is started (for the demo, the repository root). Doyle should take it from its config, and may list every folder whose `MEMORY.md` it can reach if asked to curate more than one.

## 2. Hook registration

Plugins are not needed. Both the user and the project route load and fire on 1.4.14 with the Claude Code shape (an event name mapping to a list of `{matcher, hooks: [{type, command}]}` groups).

### 2.1 User hooks: a `hooks` block in `~/.config/muse/settings.json`

The file must keep `"schema_version": 1`. Added (original saved first, then restored):

```json
"hooks": {
  "SessionStart": [{"hooks": [{"type": "command", "command": "$S/log-hook.sh user-SessionStart"}]}],
  "PreToolUse":   [{"matcher": "*", "hooks": [{"type": "command", "command": "$S/log-hook.sh user-PreToolUse"}]}]
}
```

`log-hook.sh TAG` only copies stdin to `$S/log/<time>-TAG-<pid>.json` and exits 0. Session 1, in `A/sub` (with `--trust-workspace`):

```
$ muse exec --model muse-wonder-1.0 --max-model-steps 6 --trust-workspace "Remember ... Fernando. Save it to memory."
Saved — the office plant is named Fernando.
$ ls $S/log
...-user-SessionStart-1132583.json
...-user-PreToolUse-1132623.json   (submit_reminder_decision)
...-user-PreToolUse-1132630.json   (add_memory)
... three more submit_reminder_decision
```

User hooks fire with no separate trust step, and in any workspace the user starts `muse` in.

### 2.2 Project hooks: `<workspace root>/.muse/hooks.json`

Same shape, under a top-level `"hooks"` key:

```json
{
  "hooks": {
    "SessionStart": [{"hooks": [{"type": "command", "command": "$S/log-hook.sh project-SessionStart"}]}],
    "PreToolUse":   [{"matcher": "*", "hooks": [{"type": "command", "command": "$S/log-hook.sh project-PreToolUse"}]}]
  }
}
```

With the user hooks removed (settings restored), session 5, started in `A` (where `A/.muse/hooks.json` lives), asked for `curl -s http://127.0.0.1:9/records` and a file edit:

```
muse: workspace root: $S/repos/A (cwd default)
muse: workspace trust: trusted source=run-flag
done
$ ls $S/log
...-project-SessionStart-1135965.json
...-project-PreToolUse-1136003.json ... (eight PreToolUse in all: bash, write_file, and submit_reminder_decision)
```

Three things the probes pinned down:

- **The root is the workspace root, which defaults to the starting directory, not the git root.** Session 6 started in `A/sub` with the same `A/.muse/hooks.json` in place: no hook ran, no log file was written, and `muse` printed no warning (`ls $S/log` was empty). Session 7, started in `A`, ran them. So `.muse/hooks.json` must sit in the directory the agent is started in.
- **The workspace must be trusted.** These runs used `--trust-workspace` (`workspace trust: trusted source=run-flag`). The docs say project hooks load only after the folder is trusted. That an untrusted workspace skips them was not run, so it stays documentation. `~/.config/muse/trust.json` was left unchanged.
- **Hooks run with a cleared environment.** The hook saw only `HOME LANG LOGNAME MUSE_SESSION_ID MUSE_THREAD_ID PATH PWD SHELL USER TRACEPARENT`, with `PWD` the workspace root. Use absolute paths in the command, and do not rely on `BASETEN_API_KEY` or any other variable being set.

The hook also fires for Muse's own internal tool `submit_reminder_decision` (five or six times a session), whose `tool_input` has `decision`, `reason`, and so on. The guard must pass those through.

### 2.3 What to paste to register `doyle guard`

For a user, in `~/.config/muse/settings.json` (merge into the existing object; keep `schema_version`):

```json
"hooks": {
  "SessionStart": [{"hooks": [{"type": "command", "command": "/path/to/doyle guard"}]}],
  "PreToolUse":   [{"matcher": "*", "hooks": [{"type": "command", "command": "/path/to/doyle guard"}]}]
}
```

For a project, the same `hooks` object in `<repo>/.muse/hooks.json` as the only top-level key, in the directory the agent starts in. One command serves both events: `doyle guard` reads `hook_event_name` from stdin. Hooks run outside Muse's sandbox, so the user should read the command before adding it.

The deny and injection outputs below were seen to work with exactly this registration shape (the commands were test scripts, not `doyle guard`).

## 3. The wire format

### 3.1 Input on stdin

One JSON object, snake_case. Recorded payloads, home-relative paths rewritten to `/tmp/scratch/repo-a`:

`testdata/muse/hooks/session-start.json`

```json
{"cwd":"/tmp/scratch/repo-a/sub","hook_event_name":"SessionStart","model":"muse-wonder-1.0","model_provider":"meta","permission_mode":"default","session_id":"01a18827-6b38-7ec3-a1c6-5558ed25e74e","source":"startup","transcript_path":null}
```

`testdata/muse/hooks/pre-tool-use-bash.json` (the shell tool is `bash`; the field is `command`)

```json
{"cwd":"/tmp/scratch/repo-a","hook_event_name":"PreToolUse","model":"muse-wonder-1.0","model_provider":"meta","permission_mode":"bypassPermissions","session_id":"01a1882a-271e-7a30-bc20-3e463b67e1cc","tool_input":{"command":"curl -s http://127.0.0.1:9/records; echo \"curl exit: $?\"","description":"Curl local records endpoint","workdir":"/tmp/scratch/repo-a"},"tool_name":"bash","tool_use_id":"call_01a1882a347f7559ac4f232786ce29cf","transcript_path":null,"turn_id":"01a1882a-2953-7862-a649-f41f75c876f1"}
```

`testdata/muse/hooks/pre-tool-use-edit-file.json` (a find-and-replace edit)

```json
{"cwd":"/tmp/scratch/repo-a","hook_event_name":"PreToolUse","model":"muse-wonder-1.0","model_provider":"meta","permission_mode":"bypassPermissions","session_id":"01a1882b-cb18-7782-9fca-324700dca6b2","tool_input":{"find":"hello world","path":"sub/notes.txt","replace":"hello there"},"tool_name":"edit_file","tool_use_id":"call_01a1882be87972d58f83623bf3d2d073","transcript_path":null,"turn_id":"01a1882b-cd68-7942-9f43-4be2970c49e4"}
```

Also recorded: `pre-tool-use-write-file.json` (`write_file`: `content`, `path`) and `pre-tool-use-add-memory.json` (`add_memory`: `content`, `path`, `scope`).

Notes on the shape:

- `tool_name` is lower-case snake_case and Muse's own: `bash`, `read_file`, `write_file`, `edit_file`, `add_memory`; `apply_patch` and `edit_memory` also appear as strings in the binary but were not seen in a payload. This is not Claude Code's `Bash`/`Edit`. A matcher of `*` sees them all; the matcher `bash` matched the shell tool (session 7).
- The file paths in `tool_input` are relative to `cwd` as the model wrote them (`sub/notes.txt`), so the guard resolves them against `cwd`. `bash`'s `workdir` is absolute.
- `permission_mode` is `default`, or `bypassPermissions` when the run used `--disable-approval`.
- `transcript_path` is `null` in `exec`. `tool_use_id` and `turn_id` are present on PreToolUse. `SessionStart` has `source: "startup"`.
- Muse runs the hook once per tool call, as its own process.

### 3.2 Deny a PreToolUse with a reason the agent sees

Hook script (variant A) printed this on stdout and exited 0 when stdin contained `127.0.0.1:9`; the same text is in `testdata/muse/hooks/out-pre-tool-use-deny.json`:

```json
{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"DOYLE-GUARD: /records is flagged STALE (memory says GET /records, contract now has GET /v2/records). Use the new path."}}
```

Session 7 in `A`, prompt: "Run the shell command: curl -s http://127.0.0.1:9/records and then quote verbatim exactly what the tool told you back". The agent's reply:

```
2) "tool blocked by hook: DOYLE-GUARD: /records is flagged STALE (memory says GET /records, contract now has GET /v2/records). Use the new path."
```

The same command without the hook (session 6, hook not loaded) returned `upstream connection failed: Connection refused (os error 111)`, so the curl really did not run in session 7.

Fallback form (variant B): print the reason on stderr and exit with code 2, no stdout. Session 8 reply:

```
2) The tool returned verbatim:
`tool blocked by hook: DOYLE-GUARD-B: /records is flagged STALE (use GET /v2/records).`
```

Both forms reach the agent as `tool blocked by hook: <reason>`. In the same session 8, the `edit_file` call was not blocked by the exit-2 script (it only denied on the curl address), so a deny is per call.

The guard should print the JSON form, with a reason that tells the agent what to do next, and keep exit 2 as the path for a guard that fails to parse its input.

### 3.3 Inject context at SessionStart

Hook script (variant A) printed this on stdout, exit 0 (`testdata/muse/hooks/out-session-start-context.json`):

```json
{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"DOYLE NOTICE: 1 memory is flagged stale. The codename is PINEAPPLE-9, not the one in MEMORY.md."}}
```

Session 7, prompt: "Quote verbatim any notice or extra context about stale memories or a codename that was given to you at the start of this session, or say NONE." Reply:

```
1) "DOYLE NOTICE: 1 memory is flagged stale. The codename is PINEAPPLE-9, not the one in MEMORY.md."
```

With the hook not loaded (session 6) the same question returned only the memory snapshot, with no notice. Plain text on stdout (variant B, `echo "DOYLE NOTICE B: plain stdout. The staging region is EU-WEST-9."`, exit 0) also reached the agent (session 8):

```
1) `DOYLE NOTICE B: plain stdout. The staging region is EU-WEST-9.`
```

The guard should use the `additionalContext` form: it is explicit, and it is the same envelope as the deny.

### 3.4 Not tested

- `PermissionRequest`, `PostToolUse`, `UserPromptSubmit` and the `systemMessage` advisory field: not run. The `compatibilityName` strings in the binary were not exercised.
- `managed_hooks_path`: not run.
- An untrusted workspace skipping project hooks: not run (documented only).
- The shared memory scope `<repo>/.agents/memory/`: not run.
- Whether a hook that exits 1 or times out blocks the call, and what the agent then sees: not run. The guard should therefore never exit non-zero except by choosing exit 2 to deny.

## Cleanup done by the probes

`~/.config/muse/settings.json` and `trust.json` were copied before the probes and compared with `cmp` afterwards: identical. The four memory folders the probes created were removed; the pre-existing `rt-a` folder was left alone.
