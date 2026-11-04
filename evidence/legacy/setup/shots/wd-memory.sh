@export PATH=/tmp/fern-ops/bin:$PATH
# After the review. Wonder's memory: each stale note is marked superseded, and the corrections are in the index it loads.
grep -n 'Superseded' .agents/memory/billing-rules.md .agents/memory/console-procedures.md
cat .agents/memory/MEMORY.md
# Lambo: the correction is recorded against the stale belief, so recall warns.
lambo-recall BILLBATCH late fee exempt | head -12
