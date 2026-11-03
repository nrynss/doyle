      *> Copybook for the Fernwhistle billing batch.
      *> One fixed-width record per line: 6 + 20 + 7 + 1 = 34 chars.
      *> BALANCE is 9(5)V99 stored as 7 digits with an implied decimal.
      *> VISIT-CODE is "V" for a vaccination visit, blank otherwise.
      *>
      *>
       01 PATIENT-RECORD.
           05 ACCT-NO       PIC 9(6).
           05 ACCT-NAME     PIC X(20).
           05 BALANCE       PIC 9(5)V99.
           05 VISIT-CODE    PIC X.
