#!/usr/bin/env bash
# Start the Fern console's prerequisite: the old records service.
# The console itself is one-shot (demo/console/fern-console); there is
# no console process to start. If the old service is not answering with
# a records-service health reply, start it with the requested version
# (default 1).
set -euo pipefail
DIR="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${1:-${OLD_SERVICE_VERSION:-1}}"
if curl -sf http://127.0.0.1:8091/health 2>/dev/null | python3 -c 'import sys,json; json.load(sys.stdin)["patient_count"]' 2>/dev/null; then
  echo "old service already running"
else
  "$DIR/scripts/start-old.sh" "$VERSION"
fi
echo "console ready: demo/console/fern-console --version $VERSION lookup"
echo "               demo/console/fern-console --version $VERSION decommission --key PF12"
