# Differential-run recordings (T9.8)

Evidence by execution for the story batch (`../story/`): the v1 COBOL run
against Wonder's wrong rewrite and the corrected rewrite, compared record by
record with the paragraph trace as blame. The gate reads only these files —
nothing committed runs the compiler. All of them were recorded by hand with
the real GnuCOBOL 3.2.0 build (`docs/legacy.md`: user-prefix build at
`$HOME/.local/gnucobol-3.2`) and `rustc -O`, in throwaway directories under
`/tmp`; the commands below reproduce them byte-for-byte against the story
fixtures.

## The mini run: a real trace end to end

Ten records (the story data's first ten: two vaccination visits, 000003 and
000009), with the full `-ftrace` stderr and all three reports:

| File | What |
|---|---|
| `mini-inputs.dat` | the ten input records (`patients.dat` lines 1–10) |
| `mini-trace.txt` | the real paragraph trace: 78 `Paragraph:` events, 8 `CALC-LATE-FEE` against 2 `LOG-EXEMPT` |
| `mini-cobol-report.txt` | the batch's report: 10 detail lines, `TOTALS ... FEE $62.02` |
| `mini-wrong-report.txt` | Wonder's rewrite: same 10 lines, the two exempt visits charged, `TOTALS ... FEE $68.96` |
| `mini-fixed-report.txt` | the corrected rewrite: detail lines and TOTALS identical to the batch |

Recorded (filenames are `ASSIGN`ed literally, so each run happens in its own
directory holding a copy of the inputs as `patients.dat`):

```
export PATH=$HOME/.local/gnucobol-3.2/bin:$PATH
export LD_LIBRARY_PATH=$HOME/.local/gnucobol-3.2/lib
mkdir mini && cd mini
cp <story>/v1/BILLBATCH.cbl <story>/v1/patient.cpy .
head -10 <story>/v1/patients.dat > patients.dat
cobc -x -ftrace -o /tmp/mini-bill BILLBATCH.cbl
COB_SET_TRACE=1 /tmp/mini-bill 2> mini-trace.txt
rustc -O <story>/rewrite/wonder-bill.rs -o /tmp/mini-wrong
rustc -O <story>/rewrite/fixed-bill.rs -o /tmp/mini-fixed
mkdir wrong fixed && cp patients.dat wrong/ && cp patients.dat fixed/
(cd wrong && /tmp/mini-wrong) && (cd fixed && /tmp/mini-fixed)
```

`mini-cobol-report.txt` is `report.txt`; the rewrite reports are
`wrong/rewrite-report.txt` and `fixed/fixed-report.txt`. The gate test
(`src/evidence/diffrun.rs`) parses this trace with `parse_trace`, splits it
with `split_trace` (`PROCESS-ALL` .. `WRITE-DETAIL`), and checks the wrong
rewrite mismatches exactly the two exempt records — each blamed on
`LOG-EXEMPT` — while the fixed rewrite mismatches nothing and flags nothing.

Without `COB_SET_TRACE=1` the trace is empty and the run fails loudly (the
T9.1 footgun); with it, every record's path runs `PROCESS-ALL`,
`VALIDATE-RECORD`, `CHARGE-ACCOUNT`, one branch paragraph, `TALLY-BAND`,
`WRITE-DETAIL`, in that order.

## The full run: story scale

| File | What |
|---|---|
| `full-wrong-report.txt` | Wonder's rewrite on all 4,812 inputs: 4,812 detail lines, `TOTALS ... FEE $72,213.23` |
| `full-record-paths.txt` | one line per record: the account plus its ordered paragraph path from the real full trace |

The full `-ftrace` output is 2,897,645 bytes (33,692 paragraph events) and is
not committed. `full-record-paths.txt` is its per-record content, derived by
a mechanical pass over that trace: split the paragraph sequence on
`WRITE-DETAIL`, keep each record's `PROCESS-ALL` .. `WRITE-DETAIL` path with
its account, and verify every path is the shared six-paragraph skeleton with
exactly one branch paragraph (4,277 `CALC-LATE-FEE`, 535 `LOG-EXEMPT`, zero
`LOG-ERROR`). The recipe, run in the full-run directory after the COBOL run
above with all 4,812 inputs:

```
COB_SET_TRACE=1 /tmp/full-billbatch 2> full-trace.txt
python3 - <<'EOF'
import re
paras = [m.group(1) for line in open('full-trace.txt')
         for m in [re.search(r'Paragraph:\s+([A-Z0-9-]+)', line)] if m]
records, cur, started = [], [], False
for p in paras:
    if p == 'PROCESS-ALL':
        started = True; cur = [p]; continue
    if started:
        cur.append(p)
        if p == 'WRITE-DETAIL':
            records.append(cur); started = False
accts = [l[5:11] for l in open('report.txt') if l.startswith('ACCT ')]
assert len(records) == len(accts) == 4812
assert all(len(r) == 6 for r in records)
with open('full-record-paths.txt', 'w') as f:
    for a, r in zip(accts, records):
        f.write(a + ' ' + ' '.join(r) + '\n')
EOF
```

Cross-checks against the story fixtures (all in the gate test): the 535
`LOG-EXEMPT` paths name exactly the accounts in
`../story/rewrite/mismatches.txt`; every one reads `VISIT-CODE V` in
`patients.dat` with `$0.00` in `report.txt`; the wrong fee on each equals the
rounded 1.5% the wrong rewrite charges; the overcharge sums to $8,308.80
($72,213.23 − $63,904.43). The fixed rewrite's detail and TOTALS lines are
byte-identical to the batch run (the T9.4 `cmp`-clean claim, re-verified when
these files were recorded), so at full scale the correct rewrite mismatches
nothing: the gate covers the fixed side on the mini run's real reports plus
the empty-outcome rule.

## Reproduce (no commit runs these)

Full COBOL rerun, byte-identical to `../story/v1/report.txt` (verified when
these files were recorded; the only prose-number drift is the trace file's
total line count — 33,695 here against the story summary's 33,895 — while
every paragraph count matches exactly):

```
export PATH=$HOME/.local/gnucobol-3.2/bin:$PATH
export LD_LIBRARY_PATH=$HOME/.local/gnucobol-3.2/lib
rm -rf /tmp/repro && mkdir /tmp/repro && cd /tmp/repro
cp <repo>/testdata/legacy/story/v1/BILLBATCH.cbl \
   <repo>/testdata/legacy/story/v1/patient.cpy \
   <repo>/testdata/legacy/story/v1/patients.dat .
cobc -x -ftrace -o /tmp/repro-bill BILLBATCH.cbl
COB_SET_TRACE=1 /tmp/repro-bill 2> trace.txt
cmp report.txt <repo>/testdata/legacy/story/v1/report.txt
grep -c 'Paragraph: CALC-LATE-FEE' trace.txt   # 4277
grep -c 'Paragraph: LOG-EXEMPT' trace.txt      # 535
```

Rewrite reruns (TOTALS must read `$72,213.23` wrong, `$63,904.43` fixed):

```
rustc -O <repo>/testdata/legacy/story/rewrite/wonder-bill.rs -o /tmp/repro-wrong
rustc -O <repo>/testdata/legacy/story/rewrite/fixed-bill.rs -o /tmp/repro-fixed
mkdir wrong fixed && cp patients.dat wrong/ && cp patients.dat fixed/
(cd wrong && /tmp/repro-wrong) && (cd fixed && /tmp/repro-fixed)
grep '^TOTALS' wrong/rewrite-report.txt fixed/fixed-report.txt
grep '^ACCT' report.txt > cobol-detail.txt
grep '^ACCT' fixed/fixed-report.txt > fixed-detail.txt
cmp cobol-detail.txt fixed-detail.txt
```
