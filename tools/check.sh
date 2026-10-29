#!/usr/bin/env bash
# The gate: every commit must pass this. CI runs the same script.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "-- cargo fmt"
cargo fmt --all --check
echo "-- cargo clippy"
cargo clippy --all-targets --locked -- -D warnings
echo "-- cargo build"
cargo build --locked
echo "-- cargo test"
cargo test --all-targets --locked
echo "gate passed"
