@export PATH=/tmp/fern-ops/bin:$HOME/.local/gnucobol-3.2/bin:$PATH LD_LIBRARY_PATH=$HOME/.local/gnucobol-3.2/lib LAMBO_URL=http://127.0.0.1:7700/mcp/s/doyle-t910
# With Doyle. First, Doyle reads what Wonder remembers and cites each billing memory to the COBOL it rests on.
doyle curate --live --project /tmp/fern-wd --adapters muse-code,lambo --contract v1/BILLBATCH.cbl --state-dir state
# The batch has a newer version. Doyle diffs the COBOL itself.
doyle check v1/BILLBATCH.cbl v2/BILLBATCH.cbl --live --state-dir state; echo "exit $?"
# And Doyle runs both programs on all 4,812 accounts: the COBOL batch and the Rust port.
@cp v1/BILLBATCH.cbl v1/patient.cpy run/; cp /tmp/fern-wd/wonder-bill.rs run/
cd run
cobc -x -o batch BILLBATCH.cbl && ./batch && mv report.txt cobol-report.txt
rustc -O wonder-bill.rs -o rewrite 2>/dev/null && ./rewrite && wc -l cobol-report.txt rewrite-report.txt
doyle diffrun ../v1/BILLBATCH.cbl wonder-bill.json --live --state-dir ../state; echo "exit $?"
@cd ..
# Doyle's eyes are Wonder's: muse-wonder-1.0 read the old console's screens and training video, and the new screens.
python3 -c "import json;[print(f, [ (s['name'], len(s['elements'])) for s in json.load(open(f))['screens']], len(json.load(open(f))['steps']), 'steps') for f in ('screens-v1.json','screens-v2.json')]"
# Doyle cites the console memories to what Wonder saw, and diffs the two versions.
doyle curate --live --project /tmp/fern-wd --adapters muse-code,lambo --contract screens-v1.json --state-dir state
doyle check screens-v1.json screens-v2.json --live --state-dir state; echo "exit $?"
# Every open flag, one per memory in each memory system.
python3 -c "import json;[print(f['id'], f['memory']['adapter'], f['memory']['id'][:28]) for f in map(json.loads, open('state/flags.jsonl'))]"
