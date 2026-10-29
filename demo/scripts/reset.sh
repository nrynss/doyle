#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "$0")/.." && pwd)"
"$DIR/scripts/stop.sh"
rm -rf "$DIR/.run"
mkdir -p "$DIR/.run"
rm -f "$DIR"/client/export*.json "$DIR"/client/import*.json
# clear new platform in-memory state is handled by restart; nothing on disk
echo "reset complete"