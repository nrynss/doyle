# Without Doyle, after Wonder's session. The old system is retired, the records are safe.
curl -s http://127.0.0.1:8191/stats; echo
curl -s http://127.0.0.1:8092/stats; echo
cat out/console-last-result
# The billing run shipped, after Wonder found the exemption and fixed its own port.
cat prod/SHIPPED
git diff --stat
grep 'ACCT 000003' report.txt rewrite-report.txt
# But the memory it started from is still wrong, and nothing corrected it.
grep -- '- BILLBATCH' .agents/memory/billing-rules.md
grep -- 'retire' .agents/memory/console-procedures.md
git status --short .agents
