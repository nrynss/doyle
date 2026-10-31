       >>SOURCE FORMAT IS FREE
       IDENTIFICATION DIVISION.
       PROGRAM-ID. BILLBATCH.
      *> Fernwhistle billing batch, story v1 (T9.4 fixtures).
      *> Reads one fixed-width patient-account record per line through
      *> COPY "patient.cpy" (34 chars: number, name, balance, visit code),
      *> computes a 1.5% late fee with ROUNDED arithmetic, writes one
      *> detail line per record, then totals and a banded summary.
      *>
      *> Fee schedule (the rule Trap A turns on):
      *>   - Standard account: fee = 1.5% of BALANCE, rounded to the cent.
      *>   - Vaccination visit (VISIT-CODE = "V"): no fee. The visit, not
      *>     a flag on the account, decides: rabies and DHPP visits are
      *>     billed elsewhere, so the batch must not add a fee here.
      *>   - Prompt-payment discount does not exist in v1 (added in v2).
      *>
      *> Report bands (by balance, for the summary page only):
      *>   band 1 under $100, band 2 $100 to $500, band 3 over $500.
       ENVIRONMENT DIVISION.
       INPUT-OUTPUT SECTION.
       FILE-CONTROL.
           SELECT ACCT-FILE ASSIGN TO "patients.dat"
               ORGANIZATION IS LINE SEQUENTIAL.
           SELECT REPORT-FILE ASSIGN TO "report.txt"
               ORGANIZATION IS LINE SEQUENTIAL.
       DATA DIVISION.
       FILE SECTION.
       FD ACCT-FILE.
       COPY "patient.cpy".
       FD REPORT-FILE.
       01 REPORT-LINE PIC X(72).
       WORKING-STORAGE SECTION.
       01 WS-EOF-FLAG      PIC X VALUE "N".
       01 WS-ERR-FLAG      PIC X VALUE "N".
       01 WS-COUNT         PIC 9(4) VALUE 0.
       01 WS-EXEMPT-COUNT  PIC 9(4) VALUE 0.
       01 WS-ERROR-COUNT   PIC 9(4) VALUE 0.
       01 WS-LATE-FEE      PIC 9(5)V99 VALUE 0.
       01 WS-TOTAL-BALANCE PIC 9(7)V99 VALUE 0.
       01 WS-TOTAL-FEE     PIC 9(7)V99 VALUE 0.
       01 WS-FEE-RATE      PIC V999 VALUE 0.015.
       01 WS-BAND-1-COUNT  PIC 9(4) VALUE 0.
       01 WS-BAND-1-FEE    PIC 9(7)V99 VALUE 0.
       01 WS-BAND-2-COUNT  PIC 9(4) VALUE 0.
       01 WS-BAND-2-FEE    PIC 9(7)V99 VALUE 0.
       01 WS-BAND-3-COUNT  PIC 9(4) VALUE 0.
       01 WS-BAND-3-FEE    PIC 9(7)V99 VALUE 0.
       01 WS-CHECK-FEE     PIC 9(7)V99 VALUE 0.
       01 FOOTER-LINE.
           05 FILLER PIC X(23) VALUE "END OF RUN CROSS-FOOT ".
           05 F-CROSSFOOT PIC X(49) VALUE " ".
       01 HEADER-LINE-1.
           05 FILLER PIC X(72) VALUE
               "FERNWHISTLE VETERINARY GROUP - BILLING RUN (V1)".
       01 HEADER-LINE-2.
           05 FILLER PIC X(72) VALUE
               "RATE 1.5% OF BALANCE, ROUNDED. VACCINATION VISITS EXEMPT.".
       01 DETAIL-LINE.
           05 FILLER       PIC X(5) VALUE "ACCT ".
           05 D-ACCT-NO    PIC 9(6).
           05 FILLER       PIC X(7) VALUE " NAME ".
           05 D-ACCT-NAME  PIC X(20).
           05 FILLER       PIC X(9) VALUE " BALANCE ".
           05 D-BALANCE    PIC $$$,$$9.99.
           05 FILLER       PIC X(5) VALUE " FEE ".
           05 D-FEE        PIC $$$,$$9.99.
       01 TOTAL-LINE.
           05 FILLER       PIC X(16) VALUE "TOTALS RECORDS ".
           05 T-COUNT      PIC ZZZ9.
           05 FILLER       PIC X(9) VALUE " BALANCE ".
           05 T-BALANCE    PIC $$$$$,$$9.99.
           05 FILLER       PIC X(5) VALUE " FEE ".
           05 T-FEE        PIC $$$,$$9.99.
       01 SUMMARY-LINE-1.
           05 FILLER       PIC X(25) VALUE "SUMMARY EXEMPT VISITS ".
           05 S-EXEMPT     PIC ZZZ9.
           05 FILLER       PIC X(9) VALUE " ERRORS ".
           05 S-ERRORS     PIC ZZZ9.
           05 FILLER       PIC X(24) VALUE " BAND COUNTS UNDER/100+ ".
       01 SUMMARY-LINE-2.
           05 FILLER       PIC X(13) VALUE "UNDER $100 ".
           05 S-BAND-1     PIC ZZZ9.
           05 FILLER       PIC X(6) VALUE " FEE ".
           05 S-BAND-1-FEE PIC $$$,$$9.99.
           05 FILLER       PIC X(12) VALUE " $100-$500 ".
           05 S-BAND-2     PIC ZZZ9.
           05 FILLER       PIC X(6) VALUE " FEE ".
           05 S-BAND-2-FEE PIC $$$,$$9.99.
       01 SUMMARY-LINE-3.
           05 FILLER       PIC X(12) VALUE "OVER $500 ".
           05 S-BAND-3     PIC ZZZ9.
           05 FILLER       PIC X(6) VALUE " FEE ".
           05 S-BAND-3-FEE PIC $$$,$$9.99.
           05 FILLER       PIC X(40) VALUE " ".
       PROCEDURE DIVISION.
       MAIN-PARA.
           PERFORM OPEN-FILES
           PERFORM WRITE-HEADER
           PERFORM READ-NEXT
           PERFORM PROCESS-ALL UNTIL WS-EOF-FLAG = "Y"
           PERFORM WRITE-TOTALS
           PERFORM WRITE-SUMMARY
           PERFORM VERIFY-TOTALS
           PERFORM CLOSE-FILES
           STOP RUN.
       OPEN-FILES.
           OPEN INPUT ACCT-FILE
           OPEN OUTPUT REPORT-FILE.
       CLOSE-FILES.
           CLOSE ACCT-FILE REPORT-FILE.
       READ-NEXT.
           READ ACCT-FILE
               AT END MOVE "Y" TO WS-EOF-FLAG
           END-READ.
       PROCESS-ALL.
           ADD 1 TO WS-COUNT
           MOVE "N" TO WS-ERR-FLAG
           PERFORM VALIDATE-RECORD
           IF WS-ERR-FLAG = "Y"
               PERFORM LOG-ERROR
           ELSE
               PERFORM CHARGE-ACCOUNT
           END-IF
           PERFORM WRITE-DETAIL
           PERFORM READ-NEXT.
       VALIDATE-RECORD.
      *> A record is valid when its numeric fields read as numbers.
      *> The story data is clean, so this path counts zero errors;
      *> the paragraph exists so the trace shows the check ran.
           IF ACCT-NO IS NOT NUMERIC
               MOVE "Y" TO WS-ERR-FLAG
           END-IF
           IF BALANCE IS NOT NUMERIC
               MOVE "Y" TO WS-ERR-FLAG
           END-IF.
       CHARGE-ACCOUNT.
           ADD BALANCE TO WS-TOTAL-BALANCE
           IF VISIT-CODE = "V"
               PERFORM LOG-EXEMPT
           ELSE
               PERFORM CALC-LATE-FEE
           END-IF
           PERFORM TALLY-BAND.
       CALC-LATE-FEE.
      *> The 1.5% rule. ROUNDED settles half-cents up: 0.49995 of
      *> fee becomes 0.50, 14.99985 becomes 15.00.
           COMPUTE WS-LATE-FEE ROUNDED = BALANCE * WS-FEE-RATE
           ADD WS-LATE-FEE TO WS-TOTAL-FEE.
       LOG-EXEMPT.
      *> A vaccination visit carries no fee here. The count below is
      *> the evidence the differential run checks: every V record
      *> must show 0.00 in the report.
           MOVE 0 TO WS-LATE-FEE
           ADD 1 TO WS-EXEMPT-COUNT.
       LOG-ERROR.
           MOVE 0 TO WS-LATE-FEE
           ADD 1 TO WS-ERROR-COUNT.
       TALLY-BAND.
      *> Summary bands only; the fee was settled above.
           IF BALANCE < 100
               ADD 1 TO WS-BAND-1-COUNT
               ADD WS-LATE-FEE TO WS-BAND-1-FEE
           ELSE
               IF BALANCE < 500
                   ADD 1 TO WS-BAND-2-COUNT
                   ADD WS-LATE-FEE TO WS-BAND-2-FEE
               ELSE
                   ADD 1 TO WS-BAND-3-COUNT
                   ADD WS-LATE-FEE TO WS-BAND-3-FEE
               END-IF
           END-IF.
       VERIFY-TOTALS.
      *> Self-check: the three band fees must add back to the fee
      *> total. The run still writes the footer either way; a
      *> mismatch would mean the tally above is wrong, not the data.
           COMPUTE WS-CHECK-FEE =
               WS-BAND-1-FEE + WS-BAND-2-FEE + WS-BAND-3-FEE
           IF WS-CHECK-FEE = WS-TOTAL-FEE
               MOVE "CROSS-FOOT OK" TO F-CROSSFOOT
           ELSE
               MOVE "CROSS-FOOT MISMATCH" TO F-CROSSFOOT
           END-IF
           MOVE FOOTER-LINE TO REPORT-LINE
           WRITE REPORT-LINE.
       WRITE-HEADER.
           MOVE HEADER-LINE-1 TO REPORT-LINE
           WRITE REPORT-LINE
           MOVE HEADER-LINE-2 TO REPORT-LINE
           WRITE REPORT-LINE.
       WRITE-DETAIL.
           MOVE ACCT-NO TO D-ACCT-NO
           MOVE ACCT-NAME TO D-ACCT-NAME
           MOVE BALANCE TO D-BALANCE
           MOVE WS-LATE-FEE TO D-FEE
           MOVE DETAIL-LINE TO REPORT-LINE
           WRITE REPORT-LINE.
       WRITE-TOTALS.
           MOVE WS-COUNT TO T-COUNT
           MOVE WS-TOTAL-BALANCE TO T-BALANCE
           MOVE WS-TOTAL-FEE TO T-FEE
           MOVE TOTAL-LINE TO REPORT-LINE
           WRITE REPORT-LINE.
       WRITE-SUMMARY.
           MOVE WS-EXEMPT-COUNT TO S-EXEMPT
           MOVE WS-ERROR-COUNT TO S-ERRORS
           MOVE WS-BAND-1-COUNT TO S-BAND-1
           MOVE WS-BAND-1-FEE TO S-BAND-1-FEE
           MOVE WS-BAND-2-COUNT TO S-BAND-2
           MOVE WS-BAND-2-FEE TO S-BAND-2-FEE
           MOVE WS-BAND-3-COUNT TO S-BAND-3
           MOVE WS-BAND-3-FEE TO S-BAND-3-FEE
           MOVE SUMMARY-LINE-1 TO REPORT-LINE
           WRITE REPORT-LINE
           MOVE SUMMARY-LINE-2 TO REPORT-LINE
           WRITE REPORT-LINE
           MOVE SUMMARY-LINE-3 TO REPORT-LINE
           WRITE REPORT-LINE.
