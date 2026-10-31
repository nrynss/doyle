#!/usr/bin/env python3
"""Legacy element lists for the T9.4 story (docs/legacy-ids.md scheme).

IDs are le2_ + SHA-256 hex of the canonical key, computed here, never
hand-copied. Line numbers ride along as native-location hints only.
Also writes legacy-diff.json: the expected v1 -> v2 change facts
(subject le2_ + kind + severity; T9.6 fixes the lc2_ derivation, so no
change IDs are invented here). Run from this directory:

    python3 generate-elements.py
"""
import hashlib
import json
import os
import re

HERE = os.path.dirname(os.path.abspath(__file__))
PROGRAM = "BILLBATCH"


def le2(key):
    return "le2_" + hashlib.sha256(key.encode("utf-8")).hexdigest()


PARAS_V1 = [
    ("MAIN-PARA", 96), ("OPEN-FILES", 106), ("CLOSE-FILES", 109),
    ("READ-NEXT", 111), ("PROCESS-ALL", 115), ("VALIDATE-RECORD", 126),
    ("CHARGE-ACCOUNT", 136), ("CALC-LATE-FEE", 144), ("LOG-EXEMPT", 149),
    ("LOG-ERROR", 155), ("TALLY-BAND", 158), ("VERIFY-TOTALS", 172),
    ("WRITE-HEADER", 185), ("WRITE-DETAIL", 190), ("WRITE-TOTALS", 197),
    ("WRITE-SUMMARY", 203),
]
PARAS_V2 = [
    ("MAIN-PARA", 106), ("OPEN-FILES", 116), ("CLOSE-FILES", 119),
    ("READ-NEXT", 121), ("PROCESS-ALL", 125), ("VALIDATE-RECORD", 136),
    ("CHARGE-ACCOUNT", 146), ("APPLY-PROMPT-DISCOUNT", 157),
    ("CALC-LATE-FEE", 165), ("LOG-EXEMPT", 170),
    ("LOG-ERROR", 176), ("TALLY-BAND", 179), ("VERIFY-TOTALS", 193),
    ("WRITE-HEADER", 207), ("WRITE-DETAIL", 212), ("WRITE-TOTALS", 219),
    ("WRITE-SUMMARY", 225),
]

FIELDS_V1 = [
    ("PATIENT-RECORD.ACCT-NO", "9(6)", 6),
    ("PATIENT-RECORD.ACCT-NAME", "X(20)", 20),
    ("PATIENT-RECORD.BALANCE", "9(5)V99", 7),
    ("PATIENT-RECORD.VISIT-CODE", "X", 1),
]
FIELDS_V2 = FIELDS_V1 + [("PATIENT-RECORD.DISCOUNT-FLAG", "X", 1)]

FILES = [
    ("ACCT-FILE", '"patients.dat"', "LINE SEQUENTIAL"),
    ("REPORT-FILE", '"report.txt"', "LINE SEQUENTIAL"),
]


def build(paras, fields, record_len, copybook):
    elements = []
    elements.append({
        "kind": "program", "name": PROGRAM,
        "key": "legacy:%s" % PROGRAM, "id": le2("legacy:%s" % PROGRAM),
        "native": {"source": "BILLBATCH.cbl"},
    })
    for name, line in paras:
        key = "legacy:%s:para:%s" % (PROGRAM, name)
        elements.append({
            "kind": "paragraph", "name": name, "key": key, "id": le2(key),
            "native": {"source": "BILLBATCH.cbl", "paragraph": name,
                       "line_hint": line},
        })
    rkey = "legacy:%s:record:PATIENT-RECORD" % PROGRAM
    elements.append({
        "kind": "record", "name": "PATIENT-RECORD", "key": rkey,
        "id": le2(rkey),
        "attributes": {"record_length": record_len, "copybook": copybook},
        "native": {"source": copybook,
                   "record": "PATIENT-RECORD"},
    })
    for path, pic, length in fields:
        key = "legacy:%s:field:%s" % (PROGRAM, path)
        elements.append({
            "kind": "field", "name": path.split(".")[-1],
            "path": path, "key": key, "id": le2(key),
            "attributes": {"pic": pic, "length": length},
            "native": {"source": copybook, "path": path, "pic": pic},
        })
    for name, assign, org in FILES:
        key = "legacy:%s:file:%s" % (PROGRAM, name)
        elements.append({
            "kind": "file", "name": name, "key": key, "id": le2(key),
            "attributes": {"assign": assign, "organization": org},
            "native": {"source": "BILLBATCH.cbl", "select": name,
                       "assign": assign},
        })
    return elements


v1 = build(PARAS_V1, FIELDS_V1, 34, "patient.cpy")
v2 = build(PARAS_V2, FIELDS_V2, 35, "patient.cpy")
with open(os.path.join(HERE, "elements-v1.json"), "w") as f:
    json.dump({"program": PROGRAM, "version": "v1",
               "elements": v1}, f, indent=2)
with open(os.path.join(HERE, "elements-v2.json"), "w") as f:
    json.dump({"program": PROGRAM, "version": "v2",
               "elements": v2}, f, indent=2)
print("v1 elements: %d, v2 elements: %d" % (len(v1), len(v2)))

by_key = {e["key"]: e["id"] for e in v1 + v2}
diff = {
    "from": "v1",
    "to": "v2",
    "note": "The add is the only legacy diff in v2 (Trap B1 as signed "
            "off). No rate, exemption, band or file change.",
    "changes": [
        {"subject": by_key["legacy:BILLBATCH:para:APPLY-PROMPT-DISCOUNT"],
         "subject_name": "APPLY-PROMPT-DISCOUNT",
         "kind": "paragraph-added", "severity": "non-breaking",
         "flags": []},
        {"subject": by_key["legacy:BILLBATCH:field:PATIENT-RECORD.DISCOUNT-FLAG"],
         "subject_name": "PATIENT-RECORD.DISCOUNT-FLAG",
         "kind": "field-added", "severity": "non-breaking",
         "flags": []},
        {"subject": by_key["legacy:BILLBATCH:record:PATIENT-RECORD"],
         "subject_name": "PATIENT-RECORD",
         "kind": "record-layout-changed",
         "detail": "record length 34 -> 35; VISIT-CODE offset unchanged, "
                   "one field added at the end",
         "severity": "breaking",
         "flags": ["L2"]},
        {"subject": by_key["legacy:BILLBATCH:para:CHARGE-ACCOUNT"],
         "subject_name": "CHARGE-ACCOUNT",
         "kind": "paragraph-statements-changed",
         "detail": "wiring for the add: IF DISCOUNT-FLAG PERFORM "
                   "APPLY-PROMPT-DISCOUNT inside the non-exempt branch",
         "severity": "breaking",
         "flags": []},
    ],
    "not_changes": [
        "Header comments, the (V1)/(V2) run labels and the discount summary "
        "lines live in report-layout lines, which docs/legacy-ids.md does "
        "not cite: no facts.",
        "CALC-LATE-FEE, LOG-EXEMPT, BALANCE, VISIT-CODE, WRITE-TOTALS, "
        "WRITE-DETAIL, TALLY-BAND, VERIFY-TOTALS and both files are "
        "identical: no facts.",
    ],
}
with open(os.path.join(HERE, "legacy-diff.json"), "w") as f:
    json.dump(diff, f, indent=2)
print("legacy changes: %d (%d breaking)"
      % (len(diff["changes"]),
         sum(1 for c in diff["changes"] if c["severity"] == "breaking")))
