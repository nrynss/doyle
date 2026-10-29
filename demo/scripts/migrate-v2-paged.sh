#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "$0")/.." && pwd)"
"$DIR/scripts/reset.sh"
"$DIR/scripts/start-old.sh" 2
"$DIR/scripts/start-new.sh"

# Export: follow next_cursor until null.
python3 - "$DIR/client/export-v2-paged.json" <<'PY'
import json, urllib.request, sys
out = sys.argv[1]
items = []
cursor = None
while True:
    url = "http://127.0.0.1:8091/patients"
    if cursor:
        url += "?cursor=" + cursor
    with urllib.request.urlopen(url) as r:
        data = json.load(r)
    items.extend(data.get("items", []))
    cursor = data.get("next_cursor")
    if not cursor:
        break
with open(out, "w") as f:
    json.dump(items, f)
print("exported", len(items))
PY

python3 - "$DIR/client/export-v2-paged.json" "$DIR/client/import-v2-paged.json" <<'PY'
import json, sys
src, dst = sys.argv[1], sys.argv[2]
with open(src) as f:
    patients = json.load(f)
with open(dst, "w") as f:
    json.dump({"patients": patients}, f)
PY
curl -sf -X POST http://127.0.0.1:8092/import -H 'Content-Type: application/json' -d @"$DIR/client/import-v2-paged.json"

echo "--- check ---"
"$DIR/scripts/check.sh"