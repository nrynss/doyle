# Agent protocol

Rules for any agent working in this repository. The product is in [`product.md`](product.md), the architecture in [`docs/architecture.md`](docs/architecture.md), and the work in [`dev-diary/`](dev-diary/README.md).

## Task rules

- Work only on a task from a phase file in `dev-diary/`, claimed for you by the orchestrator. The orchestrator's process is in [`dev-diary/ORCHESTRATOR.md`](dev-diary/ORCHESTRATOR.md).
- One agent per task. The orchestrator validates your work itself and lands it, or sends it back to you with what is missing. There are no review rounds.
- Meet every **Done when** clause, with evidence in your report. A clause you could not meet is reported as unmet, never glossed.
- A decision the task names as the user's goes back to the orchestrator. Don't make it yourself.
- Never schedule. No dates, day windows, target days or timelines in any document, commit or report. Work is ordered by `requires` only.

## Stack

- **Rust**, pinned in `rust-toolchain.toml`. Edition 2024.
- **Dependencies.** Vimanam (crates.io) as a library. Lambo only as a client of a running server, never linked. The standard library first; a new dependency needs a stated reason in its commit message. The planned ones are in `docs/architecture.md`.

## Worktrees

- One task, one branch, one worktree. Two agents never edit one checkout.
- Commit on your branch. Never push, merge, or touch the main checkout; the orchestrator lands.
- A worktree has no gitignored files (`.env`, `doyle.toml`, `.doyle/`). Don't assume they exist.

## File ownership

- Edit only the paths your task's `owns` lists. A change outside them is a new task or a note in your report.
- `src/main.rs`, `src/lib.rs`, `src/cli.rs` and the module declaration lists change only with the orchestrator's approval, after T1.5.
- `Cargo.toml` and `Cargo.lock` are shared: add only the dependency your task names. The orchestrator resolves conflicts when it lands.
- Only the orchestrator edits `dev-diary/README.md`.

## The gate

- `./tools/check.sh` must pass on every commit: rustfmt, clippy with warnings denied, and the tests.
- CI runs the same script on push to `main` and on pull requests.
- The gate needs no key, no network and no running server. Tests run on `testdata/` and recorded replies.

## Live tests

- A live test calls a real service: the Meta Model API, Baseten, a running Lambo, or Muse Code. It lives in `tests/live_*.rs`, is `#[ignore]`d, and runs only with `cargo test -- --ignored live_`.
- Live tests never run in CI and never gate a build.
- Each has a budget in its task. Run one case first, check the cost, and stop and report if it would go over.
- A live run is evidence: commit the test, and put its transcript (no key, no personal data) in your report.
- Live tests that touch Muse Code memory or settings use scratch repositories and restore anything they change.

## Secrets

- Keys come from the environment or the system keyring, never from a committed file. `.env` and `doyle.toml` are gitignored; `.env.example` holds names only.
- Baseten: `BASETEN_API_KEY` (keyring: `secret-tool lookup service baseten account rocknarayan@gmail.com`).
- Never print, log, commit, or paste a key value, including in test output, fixtures and recordings.

## Commits and pull requests

- Plain messages.
- No `Co-Authored-By` trailer.
- No Claude or Anthropic attribution in commits or PR text.

## Dev-diary

- Set your task to `in-review` in its phase file when you report, and add your notes under the task: what you built, what you decided and why, what surprised you.
- Put your handoff-log entry in your report; the orchestrator adds it to `dev-diary/README.md`.
- The repository goes public. Nothing private in the diary: no key, no account detail, no private repository's name.

## New work only

- This is a hackathon entry under a new-work-only rule. Write Doyle's code here, during the event. Never copy code from Lambo, Vimanam, Ribhu or any earlier project; depend on published crates or talk to a running server instead.
