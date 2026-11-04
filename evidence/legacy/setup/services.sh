#!/usr/bin/env bash
# T9.10 services, post-B1: the old records service (2q) on 8191 (8091 is
# taken on this machine by an unrelated app) and the new platform on 8092,
# then the paged records move (the T7.x B1 step, done here by the operator),
# then the counts. Restarting resets both (state is in memory).
set -euo pipefail
DEMO="${1:?demo dir}"
for p in 8191 8092; do
  pid=$(ss -ltnpH "sport = :$p" 2>/dev/null | sed -n 's/.*pid=\([0-9]*\).*/\1/p' | head -1)
  if [ -n "$pid" ] && tr '\0' ' ' < /proc/$pid/cmdline | grep -q -E 'old_service.py|new_platform.py'; then kill "$pid"; sleep 1; fi
done
(setsid timeout 28800 python3 "$DEMO/old_service.py" --version 2q --port 8191 >/dev/null 2>&1 &)
(setsid timeout 28800 python3 "$DEMO/new_platform.py" --port 8092 >/dev/null 2>&1 &)
for i in $(seq 1 20); do curl -sf http://127.0.0.1:8191/health >/dev/null && curl -sf http://127.0.0.1:8092/health >/dev/null && break; sleep 0.5; done
python3 - <<'PY'
import json, re, urllib.request
url, rows = "http://127.0.0.1:8191/patients", []
while url:
    with urllib.request.urlopen(url) as r:
        rows += json.load(r)
        m = re.search(r'<([^>]+)>;\s*rel="next"', r.headers.get("Link") or "")
        url = m.group(1) if m else None
        if url and url.startswith("/"): url = "http://127.0.0.1:8191" + url
req = urllib.request.Request("http://127.0.0.1:8092/import", data=json.dumps({"patients": rows}).encode(),
                             headers={"Content-Type": "application/json"}, method="POST")
urllib.request.urlopen(req).read()
PY
old=$(curl -sf http://127.0.0.1:8191/stats | python3 -c 'import sys,json;print(json.load(sys.stdin)["patient_count"])')
new=$(curl -sf http://127.0.0.1:8092/stats | python3 -c 'import sys,json;print(json.load(sys.stdin)["patient_count"])')
echo "old records service: $old patients"
echo "new platform:        $new patients"
