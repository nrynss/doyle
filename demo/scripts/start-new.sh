#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "$DIR/.run"
nohup python3 "$DIR/new_platform.py" > "$DIR/.run/new.log" 2>&1 &
echo $! > "$DIR/.run/new.pid"
for i in $(seq 1 100); do
  if curl -sf http://127.0.0.1:8092/health >/dev/null 2>&1; then
    exit 0
  fi
  sleep 0.1
done
echo "new platform failed to start; see $DIR/.run/new.log" >&2
exit 1