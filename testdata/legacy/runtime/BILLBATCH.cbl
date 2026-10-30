       >>SOURCE FORMAT IS FREE
       IDENTIFICATION DIVISION.
       PROGRAM-ID. BILLBATCH.
      *> Sample batch for Doyle T9.1: reads fixed-width patient accounts
      *> through COPY "patient.cpy", computes a 1.5% late fee with
      *> ROUNDED arithmetic, and writes a report. Exempt accounts
      *> (EXEMPT-FLAG = "Y") carry no fee.
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
       01 WS-COUNT         PIC 9(4) VALUE 0.
       01 WS-LATE-FEE      PIC 9(5)V99 VALUE 0.
       01 WS-TOTAL-BALANCE PIC 9(7)V99 VALUE 0.
       01 WS-TOTAL-FEE     PIC 9(7)V99 VALUE 0.
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
           05 T-COUNT      PIC ZZ9.
           05 FILLER       PIC X(9) VALUE " BALANCE ".
           05 T-BALANCE    PIC $$$,$$9.99.
           05 FILLER       PIC X(5) VALUE " FEE ".
           05 T-FEE        PIC $$$,$$9.99.
       PROCEDURE DIVISION.
       MAIN-PARA.
           OPEN INPUT ACCT-FILE
           OPEN OUTPUT REPORT-FILE
           PERFORM READ-NEXT
           PERFORM PROCESS-ALL UNTIL WS-EOF-FLAG = "Y"
           PERFORM WRITE-TOTALS
           CLOSE ACCT-FILE REPORT-FILE
           STOP RUN.
       READ-NEXT.
           READ ACCT-FILE
               AT END MOVE "Y" TO WS-EOF-FLAG
           END-READ.
       PROCESS-ALL.
           ADD 1 TO WS-COUNT
           ADD BALANCE TO WS-TOTAL-BALANCE
           IF EXEMPT-FLAG = "Y"
               PERFORM LOG-EXEMPT
           ELSE
               PERFORM CALC-LATE-FEE
           END-IF
           PERFORM WRITE-DETAIL
           PERFORM READ-NEXT.
       CALC-LATE-FEE.
           COMPUTE WS-LATE-FEE ROUNDED = BALANCE * 0.015
           ADD WS-LATE-FEE TO WS-TOTAL-FEE.
       LOG-EXEMPT.
           MOVE 0 TO WS-LATE-FEE.
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
