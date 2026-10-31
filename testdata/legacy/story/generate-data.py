#!/usr/bin/env python3
"""Deterministic patient-account data for the T9.4 story fixtures.

One fixed-width record per patient, 4,812 records: record i is API patient
p-%05d from demo/old_service.py (same seed family, name "Patient %05d").
Balances come from random.Random(20240614); the first five records mirror
the T9.1 sample values so ROUNDED is exercised both ways.

  v1 record (34 chars): ACCT-NO 9(6) + ACCT-NAME X(20) + BALANCE 9(5)V99
      (7 digits) + VISIT-CODE X ("V" = vaccination visit, else " ").
  v2 record (35 chars): v1 + DISCOUNT-FLAG X ("Y" = prompt-payment
      discount, only on non-vaccination accounts).

Vaccination rule (Trap A2): i % 9 == 0, plus record 3 (the T9.1 sample's
exempt record).  Discount rule (Trap B1): i % 40 == 0 on a
non-vaccination account.  Counts are printed for the README.
Run from this directory:  python3 generate-data.py
"""
import os
import random

HERE = os.path.dirname(os.path.abspath(__file__))
TOTAL = 4812

FIRST_FIVE_CENTS = [10000, 3333, 25000, 1001, 99999]


def is_vaccination(i):
    return i % 9 == 0 or i == 3


def is_discount(i):
    return i % 40 == 0 and not is_vaccination(i)


def main():
    rng = random.Random(20240614)
    balances = [0] + [rng.randint(1, 200000) for _ in range(TOTAL)]
    for idx, cents in enumerate(FIRST_FIVE_CENTS, start=1):
        balances[idx] = cents
    v1_lines, v2_lines = [], []
    for i in range(1, TOTAL + 1):
        acct = "%06d" % i
        name = ("Patient %05d" % i).ljust(20)[:20]
        bal = "%07d" % balances[i]
        visit = "V" if is_vaccination(i) else " "
        v1_lines.append(acct + name + bal + visit)
        disc = "Y" if is_discount(i) else " "
        v2_lines.append(acct + name + bal + visit + disc)
    assert all(len(line) == 34 for line in v1_lines)
    assert all(len(line) == 35 for line in v2_lines)
    with open(os.path.join(HERE, "v1", "patients.dat"), "w") as f:
        f.write("\n".join(v1_lines) + "\n")
    with open(os.path.join(HERE, "v2", "patients.dat"), "w") as f:
        f.write("\n".join(v2_lines) + "\n")
    nv = sum(1 for i in range(1, TOTAL + 1) if is_vaccination(i))
    nd = sum(1 for i in range(1, TOTAL + 1) if is_discount(i))
    print("v1 records: %d, vaccination (V): %d" % (TOTAL, nv))
    print("v2 records: %d, discount (Y): %d" % (TOTAL, nd))
    print("record 1: %r" % v1_lines[0])
    print("record 3 (exempt): %r" % v1_lines[2])


if __name__ == "__main__":
    main()
