#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "$0")/.." && pwd)"
for f in "$DIR/.run/old.pid" "$DIR/.run/new.pid"; do
  if [ -f "$f" ]; then
    pid="$(cat "$f" 2>/dev/null || true)"
    if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
    fi
    rm -f "$f"
  fi
done
pkill -f "old_service.py" 2>/dev/null || true
pkill -f "new_platform.py" 2>/dev/null || true
sleep 0.2