#!/usr/bin/env bash
set -euo pipefail
OLD="$(curl -sf http://127.0.0.1:8091/stats | python3 -c 'import sys,json; print(json.load(sys.stdin)["patient_count"])')"
NEW="$(curl -sf http://127.0.0.1:8092/stats | python3 -c 'import sys,json; print(json.load(sys.stdin)["patient_count"])')"
echo "old: $OLD new: $NEW"
if [ "$OLD" = "$NEW" ]; then
  echo "PASS"
  exit 0
else
  echo "FAIL"
  exit 1
fi