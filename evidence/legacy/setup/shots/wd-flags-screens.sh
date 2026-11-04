@export PATH=/tmp/fern-ops/bin:$PATH LAMBO_URL=http://127.0.0.1:7700/mcp/s/doyle-t910
# With Doyle, the console. Doyle cites the console memories to what Wonder saw on the screens and in the video.
doyle curate --live --project /tmp/fern-wd --adapters muse-code,lambo --contract screens-v1.json --state-dir state
# Then it diffs the old console against the redesigned one.
doyle check screens-v1.json screens-v2.json --live --state-dir state; echo "exit $?"
