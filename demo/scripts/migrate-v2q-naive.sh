#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "$0")/.." && pwd)"
"$DIR/scripts/reset.sh"
"$DIR/scripts/start-old.sh" 2q
"$DIR/scripts/start-new.sh"

# Export: naive single GET /patients (no cursor) -> only the first page.
curl -sf http://127.0.0.1:8091/patients -o "$DIR/client/export-v2q-naive.json"

# Verify (the plan's own check): repeat the call and compare with the export.
curl -sf http://127.0.0.1:8091/patients -o "$DIR/client/export-v2q-naive-repeat.json"
python3 - "$DIR/client/export-v2q-naive.json" "$DIR/client/export-v2q-naive-repeat.json" <<'PY'
import json, sys
with open(sys.argv[1]) as f:
    first = json.load(f)
with open(sys.argv[2]) as f:
    second = json.load(f)
assert isinstance(first, list) and isinstance(second, list), "2q returns a bare array"
assert [p["id"] for p in first] == [p["id"] for p in second], "repeat must match the export"
print("verify: export = repeat = {} patients".format(len(first)))
PY

# Import whatever came back (100 patients).
python3 - "$DIR/client/export-v2q-naive.json" "$DIR/client/import-v2q-naive.json" <<'PY'
import json, sys
src, dst = sys.argv[1], sys.argv[2]
with open(src) as f:
    patients = json.load(f)
with open(dst, "w") as f:
    json.dump({"patients": patients}, f)
PY
curl -sf -X POST http://127.0.0.1:8092/import -H 'Content-Type: application/json' -d @"$DIR/client/import-v2q-naive.json"

# The plan's own import check: the new platform's count against the export's.
python3 - "$DIR/client/export-v2q-naive.json" <<'PY'
import json, sys, urllib.request
with open(sys.argv[1]) as f:
    exported = len(json.load(f))
with urllib.request.urlopen("http://127.0.0.1:8092/stats") as r:
    new_count = json.load(r)["patient_count"]
assert new_count == exported, (new_count, exported)
print("import check: new platform = export = {} patients (plan's own checks pass)".format(exported))
PY

echo "--- check.sh: old service (honest /stats) vs new platform (expected to FAIL: 100 vs 4812) ---"
set +e
"$DIR/scripts/check.sh"
rc=$?
set -e
echo "migrate-v2q-naive check rc=$rc (expected non-zero)"
exit 0
