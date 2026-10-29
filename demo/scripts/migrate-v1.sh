#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "$0")/.." && pwd)"
"$DIR/scripts/reset.sh"
"$DIR/scripts/start-old.sh" 1
"$DIR/scripts/start-new.sh"

# Export: v1 returns every patient in one response (M1's belief).
curl -sf http://127.0.0.1:8091/patients -o "$DIR/client/export-v1.json"

# Import into the new platform.
python3 - "$DIR/client/export-v1.json" "$DIR/client/import-v1.json" <<'PY'
import json, sys
src, dst = sys.argv[1], sys.argv[2]
with open(src) as f:
    patients = json.load(f)
with open(dst, "w") as f:
    json.dump({"patients": patients}, f)
PY
curl -sf -X POST http://127.0.0.1:8092/import -H 'Content-Type: application/json' -d @"$DIR/client/import-v1.json"

echo "--- check ---"
set +e
"$DIR/scripts/check.sh"
rc=$?
set -e
echo "migrate-v1 exit (check rc=$rc)"
exit 0