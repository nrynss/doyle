---
type: reference
description: Fernwhistle billing batch rules (legacy COBOL)
---

- BILLBATCH charges a 1.5% late fee on the whole balance, no exceptions; every account pays
- Each patient account record is 34 characters: a 6-digit account number, a 20-character name, a 7-digit balance with an implied decimal point, and a 1-character visit code
- CALC-LATE-FEE computes the 1.5% fee from BALANCE, rounded to the cent
- V2 prompt-payment accounts (DISCOUNT-FLAG Y) keep 10% of the fee, rounded; APPLY-PROMPT-DISCOUNT applies it after CALC-LATE-FEE
