---
title: The guard
description: How doyle guard holds tool calls that touch flagged elements or rewrite artifacts without making a model call.
---

`doyle guard` is the deterministic runtime gate for coding agents. It prevents agents from executing destructive actions against stale facts.

The guard runs as a hook process inside Muse Code. It makes zero model calls and zero network requests on its hot path.

Detailed specifications and test cases live in [`docs/guard.md`](https://github.com/nrynss/doyle/blob/main/docs/guard.md) and [`src/guard.rs`](https://github.com/nrynss/doyle/blob/main/src/guard.rs).

## Hook lifecycle

Muse Code invokes `doyle guard` on two primary lifecycle events:

1. **`SessionStart`:** The guard checks for open flags in the store. If any exist, it emits a concise `additionalContext` notification summarizing the unverified claims.
2. **`PreToolUse`:** Before executing any tool, Muse Code passes the tool name and input JSON to standard input. The guard evaluates the parameters and returns `permissionDecision: "deny"` if an open flag matches.

Decisions execute in roughly 1 to 5 milliseconds.

## Matching rules by flag kind

The guard inspects tool arguments against open flags according to specific evidence rules.

### API contract flags

For API contract flags, the guard parses commands and payloads for HTTP routes:

- It extracts HTTP methods and target paths, such as `GET /patients`.
- It matches operation identifiers and top-level schema names of 4 or more characters.
- It normalizes URLs by dropping query strings and trailing slashes.
- It strips hostnames and ports so aliases like `localhost` or `127.0.0.1` cannot bypass enforcement.

### Legacy COBOL flags

For legacy COBOL flags, the guard tracks programs and structural declarations:

- It scans tool inputs for cited program names and copybook records like `PATIENT-RECORD`.
- It inspects paragraph tokens like `CALC-LATE-FEE` and `LOG-EXEMPT`.
- It checks field identifiers with 4 or more characters.

### Console screen flags

For console screen flags, the guard observes terminal workflows:

- It detects references to protected screens like `decommission`.
- It identifies function key actions such as `PF12`.
- It catches console CLI subcommands that trigger state alterations.

### Differential run flags

Differential run flags protect rewrite projects as explained in [`src/evidence/diffrun.rs`](https://github.com/nrynss/doyle/blob/main/src/evidence/diffrun.rs):

- The flag stores target files under `rewrite_artifacts`, including source files and compiled binaries like `wonder-bill.rs` and `bin/billbatch`.
- The guard denies commands that compile, copy, move, install, or execute those binary artifacts while the flag remains unsettled.

## Allowed actions

The guard avoids trapping agents in unrecoverable deadlocks. It explicitly allows safe investigation and repair tasks:

- **Read-only shell commands:** Agents can inspect logs and source files using `cat`, `head`, `tail`, `less`, `grep`, `rg`, `sed -n`, `awk`, `wc`, `diff`, `cmp`, `git status`, `git diff`, `git log`, `git show`, `ls`, `stat`, and `file`.
- **Editor read tools:** Standard file inspection tools like `view`, `read`, `glob`, and `grep` operate without restriction.
- **Source edits:** Agents retain permission to edit modern source code files like `wonder-bill.rs` to fix discovered calculation flaws.

## Self-protection

Doyle strictly separates human review authority from autonomous agent action.

An agent must not approve its own proposals or tamper with evidence records. The guard enforces hard self-protection checks on every tool call:

- It blocks attempts to write, edit, or patch files inside `.doyle/`, including `flags.jsonl`, `decisions.jsonl`, and `citations.jsonl`.
- It blocks commands that access internal decision routes like `/flag/<id>/decide` or `/reviewer/`.
- It prevents agents from executing `doyle serve`, `doyle curate`, `doyle check`, or `doyle cite` directly in bash.

Even if an agent gains access to local web commands, it cannot complete decisions without a human reviewer session token.

## Fail-open design

The guard follows a fail-open philosophy for workspace resilience.

If the `.doyle/` state directory is missing or unreadable, `doyle guard` prints a warning to stderr and exits with status code 0.

If stdin JSON input is malformed or not a valid hook payload, `doyle guard` also prints a warning to stderr and exits with status code 0.

Valid workspace operations continue safely without stranding developers or CI pipelines.
