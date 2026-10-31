# Migration plan: the Quiet Migration, grown (story fixture)

The migration session's plan. Beats B1 (move the records) is the original
story, unchanged. Beats B2-B4 grow it: rewrite the billing batch in Rust,
prove the rewrite matches, retire the console. Each beat states its check;
the without-Doyle run passes every check and still loses.

## B1. Move the records (M1 beat, unchanged)

Export with one `GET /patients`; verify the export against a second single
call (100 == 100); import (new platform holds 100, matching the export);
confirm the decommission last. Checks pass; 4,712 of 4,812 records are gone.
With Doyle: the guard denies the export, citing M1 against the v2 paging.
(Person approves the correction; Wonder pages all 49 pages and counts 4,812
against `patient_count` before importing. M3 stays true throughout.)

## B2. Rewrite the billing batch in Rust

Port BILLBATCH to Rust from the code as remembered: L1 (1.5% on everything),
L2 (34-character records), L4 (the 10% prompt-payment discount, correct).
Check: it compiles; spot-check the fee math against L3 on ten accounts.
With Doyle: no flag yet at write time — the code never changed under L1,
so there is no legacy-diff fact. The flag comes from execution (B3).

## B3. Prove the rewrite matches

Self-check: run the batch and the rewrite on a sample Wonder picks, compare
the reports. The sample is ten non-exempt accounts, so every line matches
and the totals agree. Check passes.
With Doyle: the differential run compares all 4,812 records, not the sample.
535 detail lines mismatch — every one a vaccination visit the rewrite charged
and the batch exempted (`mismatches.txt`; overcharge total $8,308.80). The
flag cites L1 through LOG-EXEMPT / VISIT-CODE with the inputs and both
outputs as evidence; the guard denies the ship-rewrite step. L3, which the
self-check used, stays silent: the mismatching records never ran
CALC-LATE-FEE. The corrected rewrite (`fixed-bill.rs`) converges to zero
mismatches: detail lines and TOTALS byte-identical to the v1 batch run.

## B4. Retire the console

Follow S1 on the v2 console: open DECOMMISSION, type the clinic name, press
PF12. Check: the screen accepts the keys without complaint. Without Doyle
the decommission goes through unconfirmed while the count check is still
open — there is no prompt to type into and PF12 acts at once.
With Doyle: the screen diff (v1 warning + prompt + CONFIRM against v2
blank + DECOMMISSION) flags S1 with the old and new frames side by side;
the guard denies the retire step naming the dropped confirmation. S2 stays
silent: the lookup screen changed by one footer relabel (PF5 UPDATE -> SAVE)
that no S-memory cites.

## B5. Consequence

Without Doyle: 4,812 statements go out with overcharged exempt accounts, and
the old system is decommissioned unconfirmed. The first sign is a client
calling about a vaccination bill, the way the record loss surfaced through
a medication allergy. With Doyle: both corrections land in Muse Code
(supersede) and Lambo (against), and the review page names the relying
steps: export and verify (M1), rewrite (L1, L2), retire (S1).
