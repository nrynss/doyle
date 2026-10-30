#!/usr/bin/env python3
"""Score T9.2 step and diff runs. Deterministic; no network.

Step mapping (GT step -> reported indices) is the one semantic judgment, made
by reading the replies; everything else is computed. Writes
runs/step-scores.json and runs/diff-scores.json.
"""
import json
import os
import re

HERE = os.path.dirname(os.path.abspath(__file__))
RUNS = os.path.join(HERE, "runs")
TOL = 5.0


def norm(t):
    return re.sub(r"\s+", " ", t.strip().upper())


GT_STEPS = json.load(open(os.path.join(HERE, "recording-60s-steps.json")))["steps"]
GT_DIFFS = {
    "a": json.load(open(os.path.join(HERE, "pair-a-changes.json")))["changes"],
    "b": json.load(open(os.path.join(HERE, "pair-b-changes.json")))["changes"],
    "c": [],
}

STEP_MAP = {
    1: {"S1": [0, 1], "S2": [2], "S3": [3], "S4": [4, 5], "S5": [6],
        "S6": [7], "S7": [8]},
    2: {"S1": [0, 1], "S2": [2], "S3": [3], "S4": [4, 5], "S5": [6],
        "S6": [7], "S7": [8]},
    3: {"S1": [0, 1], "S2": [2, 3], "S3": [4], "S4": [5, 6], "S5": [7],
        "S6": [8], "S7": [9]},
}


def load_steps(run):
    body = json.load(open(os.path.join(RUNS, "steps-run%d.response.json" % run)))
    return json.loads(body["choices"][0]["message"]["content"])["steps"]


def load_changes(pair, run):
    body = json.load(
        open(os.path.join(RUNS, "diff-%s-run%d.response.json" % (pair, run))))
    return json.loads(body["choices"][0]["message"]["content"])["changes"]


def main():
    step_out = {}
    for run, mapping in STEP_MAP.items():
        rep = load_steps(run)
        per_step = {}
        for g in GT_STEPS:
            idxs = mapping[g["id"]]
            starts = [rep[i]["start_s"] for i in idxs]
            ends = [rep[i]["end_s"] for i in idxs]
            lo, hi = min(starts), max(ends)
            d_start = round(lo - g["start_s"], 1)
            d_end = round(hi - g["end_s"], 1)
            per_step[g["id"]] = {
                "reported": idxs, "union": [lo, hi],
                "d_start_s": d_start, "d_end_s": d_end,
                "within_tol": abs(d_start) <= TOL and abs(d_end) <= TOL,
            }
        step_out["run%d" % run] = {
            "reported_count": len(rep), "gt_count": len(GT_STEPS),
            "steps": per_step,
        }
    with open(os.path.join(RUNS, "step-scores.json"), "w") as f:
        json.dump(step_out, f, indent=2)

    diff_out = {}
    for pair, gt in GT_DIFFS.items():
        for run in (1, 2, 3):
            rep = load_changes(pair, run)
            caught = []
            for g in gt:
                b = norm(g["before"]).strip("()")
                a = norm(g["after"]).strip("()")
                hit = [c for c in rep
                       if b in norm(c["before"] + " " + c["after"]) or
                       a in norm(c["before"] + " " + c["after"])]
                # require both sides present across the matched set
                both = [c for c in hit
                        if b in norm(c["before"]) and a in norm(c["after"])]
                caught.append({"id": g["id"], "caught": bool(both or hit)})
            invented = len(rep) - sum(1 for c in caught if c["caught"])
            # extras that match nothing at all
            unmatched = []
            for c in rep:
                blob = norm(c["before"] + " " + c["after"])
                if not any(norm(g["before"]).strip("()") in blob or
                           norm(g["after"]).strip("()") in blob for g in gt):
                    unmatched.append(c)
            diff_out["%s-run%d" % (pair, run)] = {
                "reported": len(rep), "gt": len(gt),
                "caught": [c["id"] for c in caught if c["caught"]],
                "missed": [c["id"] for c in caught if not c["caught"]],
                "invented": unmatched,
            }
    with open(os.path.join(RUNS, "diff-scores.json"), "w") as f:
        json.dump(diff_out, f, indent=2)
    print(json.dumps(step_out, indent=1))
    print(json.dumps(diff_out, indent=1))


if __name__ == "__main__":
    main()
