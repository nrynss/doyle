#!/usr/bin/env python3
"""Score T9.2 element-list runs against ground truth. Deterministic; no network.

Reads testdata/vision/lookup-v1.json (GT) and runs/screen-run{1,3,4}.response.json,
writes runs/element-scores.json. Step/diff scoring is manual (see docs/wonder-vision.md)
because N is small and the judgments are semantic; the raw replies are the evidence.
"""
import json
import os
import re

HERE = os.path.dirname(os.path.abspath(__file__))
RUNS = os.path.join(HERE, "runs")


def norm(t):
    return re.sub(r"\s+", " ", t.strip().upper())


def punct_only(t):
    # Separator rules (long runs of dashes etc.) are decorative, not elements.
    # Short underscore runs are 3270 input fields and count.
    return len(t) > 10 and re.fullmatch(r"[\W_]+", t) is not None


def load_case(run):
    body = json.load(open(os.path.join(RUNS, "screen-run%d.response.json" % run)))
    content = body["choices"][0]["message"]["content"]
    assert content is not None, "run %d has null content" % run
    return json.loads(content)


def main():
    gt = json.load(open(os.path.join(HERE, "lookup-v1.json")))["elements"]
    gt_texts = {}
    for e in gt:
        gt_texts.setdefault(norm(e["text"]), []).append(e)
    out = {}
    for run in (1, 3, 4):
        rep = load_case(run)["elements"]
        real = [e for e in rep if not punct_only(e["text"])]
        skipped = len(rep) - len(real)
        used = set()
        matched = 0
        positioned = 0
        kind_ok = 0
        prot_ok = 0
        for e in real:
            key = norm(e["text"])
            cands = [g for g in gt_texts.get(key, []) if id(g) not in used]
            if not cands:
                continue
            g = cands[0]
            used.add(id(g))
            matched += 1
            if abs(e["row"] - g["row"]) <= 1 and abs(e["col"] - g["col"]) <= 2:
                positioned += 1
            if e["kind"] == g["kind"]:
                kind_ok += 1
            if e["protected"] == g["protected"]:
                prot_ok += 1
        out["run%d" % run] = {
            "reported": len(rep),
            "punctuation_only_skipped": skipped,
            "gt_total": len(gt),
            "matched": matched,
            "recall": round(matched / len(gt), 4),
            "precision": round(matched / len(real), 4) if real else 0.0,
            "positioned_frac": round(positioned / matched, 4) if matched else 0.0,
            "kind_frac": round(kind_ok / matched, 4) if matched else 0.0,
            "protected_frac": round(prot_ok / matched, 4) if matched else 0.0,
        }
    with open(os.path.join(RUNS, "element-scores.json"), "w") as f:
        json.dump(out, f, indent=2)
    print(json.dumps(out, indent=2))


if __name__ == "__main__":
    main()
