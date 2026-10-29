#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "$0")/.." && pwd)"
"$DIR/scripts/reset.sh"
"$DIR/scripts/start-old.sh" 2
"$DIR/scripts/start-new.sh"

# Export: naive single GET /patients (no cursor) -> only the first page.
curl -sf http://127.0.0.1:8091/patients -o "$DIR/client/export-v2-naive.json"

# Import whatever came back (100 patients).
python3 - "$DIR/client/export-v2-naive.json" "$DIR/client/import-v2-naive.json" <<'PY'
import json, sys
src, dst = sys.argv[1], sys.argv[2]
with open(src) as f:
    data = json.load(f)
patients = data.get("items", data) if isinstance(data, dict) else data
with open(dst, "w") as f:
    json.dump({"patients": patients}, f)
PY
curl -sf -X POST http://127.0.0.1:8092/import -H 'Content-Type: application/json' -d @"$DIR/client/import-v2-naive.json"

echo "--- check (expected to FAIL: 100 vs 4812) ---"
set +e
"$DIR/scripts/check.sh"
rc=$?
set -e
echo "migrate-v2-naive check rc=$rc (expected non-zero)"
exit 0