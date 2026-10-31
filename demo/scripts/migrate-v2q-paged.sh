#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "$0")/.." && pwd)"
"$DIR/scripts/reset.sh"
"$DIR/scripts/start-old.sh" 2q
"$DIR/scripts/start-new.sh"

# Export: follow the Link rel="next" header until no next page remains.
python3 - "$DIR/client/export-v2q-paged.json" <<'PY'
import json, sys, urllib.request
out = sys.argv[1]
items = []
url = "http://127.0.0.1:8091/patients"
pages = 0
while url:
    with urllib.request.urlopen(url) as r:
        data = json.load(r)
        link = r.headers.get("Link")
    assert isinstance(data, list), "2q returns a bare array"
    items.extend(data)
    pages += 1
    url = None
    if link:
        for part in link.split(","):
            halves = part.split(";")
            if len(halves) == 2 and 'rel="next"' in halves[1]:
                target = halves[0].strip()
                assert target.startswith("<") and target.endswith(">"), link
                url = target[1:-1]
print("exported {} patients in {} pages".format(len(items), pages))
with open(out, "w") as f:
    json.dump(items, f)
PY

python3 - "$DIR/client/export-v2q-paged.json" "$DIR/client/import-v2q-paged.json" <<'PY'
import json, sys
src, dst = sys.argv[1], sys.argv[2]
with open(src) as f:
    patients = json.load(f)
with open(dst, "w") as f:
    json.dump({"patients": patients}, f)
PY
curl -sf -X POST http://127.0.0.1:8092/import -H 'Content-Type: application/json' -d @"$DIR/client/import-v2q-paged.json"

echo "--- check ---"
"$DIR/scripts/check.sh"
