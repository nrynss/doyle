#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "$DIR/.run"
VERSION="${1:-${OLD_SERVICE_VERSION:-1}}"
nohup python3 "$DIR/old_service.py" --api-version "$VERSION" > "$DIR/.run/old.log" 2>&1 &
echo $! > "$DIR/.run/old.pid"
for i in $(seq 1 100); do
  if curl -sf http://127.0.0.1:8091/health >/dev/null 2>&1; then
    exit 0
  fi
  sleep 0.1
done
echo "old service failed to start; see $DIR/.run/old.log" >&2
exit 1