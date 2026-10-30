      *> Copybook for the T9.1 sample batch program.
      *> One fixed-width record per line: 6 + 20 + 7 + 1 = 34 chars.
      *> BALANCE is 9(5)V99 stored as 7 digits with an implied decimal.
       01 PATIENT-RECORD.
           05 ACCT-NO       PIC 9(6).
           05 ACCT-NAME     PIC X(20).
           05 BALANCE       PIC 9(5)V99.
           05 EXEMPT-FLAG   PIC X.
