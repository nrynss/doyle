      *> Copybook for the T9.4 story batch, v2.
      *> One fixed-width record per line: 6 + 20 + 7 + 1 + 1 = 35 chars.
      *> Adds DISCOUNT-FLAG (Trap B1): "Y" marks a prompt-payment account,
      *> whose late fee is cut by 10%. The record grows by one char, so
      *> every field after BALANCE moves: a layout change on PATIENT-RECORD.
       01 PATIENT-RECORD.
           05 ACCT-NO       PIC 9(6).
           05 ACCT-NAME     PIC X(20).
           05 BALANCE       PIC 9(5)V99.
           05 VISIT-CODE    PIC X.
           05 DISCOUNT-FLAG PIC X.
