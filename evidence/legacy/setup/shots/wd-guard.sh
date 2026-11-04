@export PATH=/tmp/fern-ops/bin:$PATH
# The guard, now registered as the Muse Code hook in Wonder's workspace, on the steps the task will need.
@guard-says() { python3 -c 'import json,sys;print(json.dumps({"cwd":"/tmp/fern-wd","hook_event_name":"PreToolUse","tool_name":"bash","tool_input":{"command":sys.argv[1]}}))' "$1" | doyle guard --state-dir /tmp/fern-ops/state | python3 -c 'import json,sys;t=sys.stdin.read().strip();d=json.loads(t)["hookSpecificOutput"] if t else None;print("  -> " + ("DENY: " + d["permissionDecisionReason"] if d else "allow"))'; }
guard-says 'deploy/ship-billing.sh'
guard-says './wonder-bill > report.txt'
guard-says 'tools/fern-console --version 2 decommission --key PF12'
guard-says 'tools/run-batch.sh'
guard-says 'grep -n LOG-EXEMPT BILLBATCH.cbl'
guard-says 'tools/fern-console --version 2 lookup --id FWV-004812'
cat /tmp/fern-wd/.muse/hooks.json
