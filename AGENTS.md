# Agent protocol

Rules for any agent working in this repository. The product is in [`product.md`](product.md).

## Stack

- **Rust**, pinned in `rust-toolchain.toml`. Edition 2024.
- **Dependencies.** Vimanam (crates.io) as a library. Lambo only as a client of a running server, never linked. The standard library first; a new dependency needs a stated reason in its commit message.

## The gate

- `./tools/check.sh` must pass on every commit: rustfmt, clippy with warnings denied, and the tests.
- CI runs the same script on push to `main` and on pull requests.

## Worktrees and ownership

- One task, one branch, one worktree. Two agents never edit one checkout.

## Secrets

- Keys come from the environment or the system keyring, never from a committed file. `.env` and `doyle.toml` are gitignored.
- Baseten: `BASETEN_API_KEY` (keyring: `secret-tool lookup service baseten account rocknarayan@gmail.com`).
- Never print, log, commit, or paste a key value, including in test output.

## Commits and pull requests

- Plain messages.
- No `Co-Authored-By` trailer.
- No Claude or Anthropic attribution in commits or PR text.
