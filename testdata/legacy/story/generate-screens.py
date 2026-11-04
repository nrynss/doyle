#!/usr/bin/env python3
"""Story console fixtures for T9.4: the Fernwhistle operator console.

Renders the 3270 green screens the story cites (lookup + decommission,
v1 + v2), their ground-truth element lists, the expected screen diffs,
and the short v1 training recording with its step list. No network, no
key, no model calls. Run from this directory:

    python3 generate-screens.py

Outputs land in screens/ (committed) and takes/ (raw .mkv captures for
~/work/demo-shoot/doyle/takes/, never committed).

Layout rules (Trap C1): the lookup screen changes between v1 and v2 by
one cosmetic footer relabel only (PF5 UPDATE -> SAVE), which no S-memory
cites. The decommission screen drops the warning line, the type-the-name
prompt, and PF12 CONFIRM becomes PF12 DECOMMISSION.
"""
import hashlib
import json
import os
import subprocess

from PIL import Image, ImageDraw, ImageFont

HERE = os.path.dirname(os.path.abspath(__file__))
SCREENS = os.path.join(HERE, "screens")
TAKES = os.path.expanduser("~/work/demo-shoot/doyle/takes")
FONT = "/usr/share/fonts/liberation/LiberationMono-Regular.ttf"
FS = 20

BG = (0, 0, 0)
DIM = (0, 170, 0)
NORM = (51, 255, 51)
BRIGHT = (150, 255, 150)
STATUS_BG = (0, 180, 0)
STATUS_FG = (0, 0, 0)

CELL_W = 12
LINE_H = 27
W = 80 * CELL_W
H = 24 * LINE_H

DATE = "06/14/26"


class Screen:
    def __init__(self, name):
        self.name = name
        self.grid = [[" "] * 80 for _ in range(24)]
        self.elements = []

    def add(self, row, col, text, kind, color, protected=True):
        r, c = row - 1, col - 1
        for i, ch in enumerate(text):
            self.grid[r][c + i] = ch
        self.elements.append(
            {
                "row": row,
                "col": col,
                "len": len(text),
                "text": text,
                "kind": kind,
                "protected": protected,
            }
        )

    def rule(self, row):
        for c in range(1, 79):
            self.grid[row - 1][c] = "-"

    def status(self, row, text):
        for i, ch in enumerate(text):
            self.grid[row - 1][3 + i] = ch
        self.elements.append(
            {"row": row, "col": 4, "len": len(text), "text": text,
             "kind": "status", "protected": True}
        )

    def ground_truth(self):
        return {
            "screen": self.name,
            "rows": 24,
            "cols": 80,
            "elements": self.elements,
            "screen_text": ["".join(r).rstrip() for r in self.grid],
        }

    def render(self, cursor=None):
        font = ImageFont.truetype(FONT, FS)
        img = Image.new("RGB", (W, H), BG)
        d = ImageDraw.Draw(img)
        for r in range(24):
            for c in range(80):
                ch = self.grid[r][c]
                if ch == " ":
                    continue
                d.text((c * CELL_W, r * LINE_H + 2), ch,
                       font=font, fill=NORM)
        if cursor is not None:
            cr, cc = cursor
            d.rectangle(
                [(cc - 1) * CELL_W, (cr - 1) * LINE_H + 3,
                 cc * CELL_W - 1, cr * LINE_H - 3],
                fill=BRIGHT,
            )
        return img


def title_block(s, code):
    s.add(1, 27, "FERNWHISTLE VETERINARY GROUP", "title", BRIGHT)
    s.add(1, 74, code, "screen-id", DIM)
    s.rule(2)


def footer(s, keys):
    col = 4
    for key, label in keys:
        s.add(23, col, key + " " + label, "pfkey", DIM)
        col += len(key) + len(label) + 4


def lookup_screen(code, pf5_label):
    s = Screen("lookup")
    title_block(s, code)
    s.add(3, 4, "PATIENT LOOKUP", "heading", BRIGHT)
    s.add(3, 66, "DATE " + DATE, "datetime", DIM)
    rows = [("CLINIC", "MAPLE ST CLINIC"), ("PATIENT ID", "FWV-004812"),
            ("NAME", "BELLA"), ("SPECIES", "CANINE"),
            ("BREED", "GOLDEN RETRIEVER"), ("DOB", "03/02/2019"),
            ("OWNER", "A. RIVERA"), ("PHONE", "555-0142")]
    for i, (label, value) in enumerate(rows):
        r = 5 + i
        s.add(r, 4, label, "label", DIM)
        s.add(r, 16, value, "value", NORM)
    s.add(14, 4, "VACCINATIONS", "label", DIM)
    s.add(14, 18, "RABIES 06/01/26  DHPP 05/12/26", "value", NORM)
    s.add(15, 4, "ALLERGIES", "label", DIM)
    s.add(15, 18, "PENICILLIN", "value", NORM)
    s.add(16, 4, "BALANCE", "label", DIM)
    s.add(16, 18, "$ 42.50", "value", NORM)
    s.add(17, 4, "LATE FEE", "label", DIM)
    s.add(17, 18, "$  0.64", "value", NORM)
    s.add(19, 4, "RECORD FOUND. PF5 UPDATES, PF12 DECOMMISSIONS.",
          "message", NORM)
    s.add(20, 4, "COMMAND ===>", "label", DIM)
    s.add(20, 16, "____", "input", BRIGHT, protected=False)
    s.status(21, "STATUS: READY")
    footer(s, [("PF1", "HELP"), ("PF3", "EXIT"), ("PF5", pf5_label),
               ("PF9", "MENU"), ("PF12", "DECOMMISSION")])
    return s


def decomm_screen(code, with_confirmation):
    s = Screen("decommission")
    title_block(s, code)
    s.add(3, 4, "DECOMMISSION CLINIC", "heading", BRIGHT)
    s.add(3, 66, "DATE " + DATE, "datetime", DIM)
    s.add(5, 4, "CLINIC", "label", DIM)
    s.add(5, 16, "MAPLE ST CLINIC", "value", NORM)
    s.add(6, 4, "PATIENTS", "label", DIM)
    s.add(6, 16, "4812", "value", NORM)
    s.add(7, 4, "LAST EXPORT", "label", DIM)
    s.add(7, 16, "49 PAGES, 4812 RECORDS", "value", NORM)
    if with_confirmation:
        s.add(9, 4, "WARNING: DECOMMISSION IS PERMANENT.", "message", NORM)
        s.add(10, 4, "TYPE CLINIC NAME TO CONFIRM:", "label", DIM)
        s.add(10, 34, "________________", "input", BRIGHT, protected=False)
    s.add(12, 4, "EXPORT MUST MATCH PATIENT COUNT BEFORE CONFIRMING.",
          "message", DIM)
    s.add(20, 4, "COMMAND ===>", "label", DIM)
    s.add(20, 16, "____", "input", BRIGHT, protected=False)
    s.status(21, "STATUS: AWAITING CONFIRMATION")
    if with_confirmation:
        footer(s, [("PF1", "HELP"), ("PF3", "EXIT"),
                   ("PF9", "MENU"), ("PF12", "CONFIRM")])
    else:
        footer(s, [("PF1", "HELP"), ("PF3", "EXIT"),
                   ("PF9", "MENU"), ("PF12", "DECOMMISSION")])
    return s


def menu_screen():
    s = Screen("menu")
    title_block(s, "FWV-00")
    s.add(3, 4, "MAIN MENU", "heading", BRIGHT)
    s.add(3, 66, "DATE " + DATE, "datetime", DIM)
    for i, opt in enumerate(["PATIENT LOOKUP", "BILLING BATCH",
                             "VACCINATION REPORT", "DECOMMISSION"]):
        s.add(6 + i * 2, 8, "%d" % (i + 1), "label", DIM)
        s.add(6 + i * 2, 12, opt, "value", NORM)
    s.add(16, 4, "TYPE A NUMBER AND PRESS ENTER.", "message", NORM)
    s.add(18, 4, "COMMAND ===>", "label", DIM)
    s.add(18, 16, "_", "input", BRIGHT, protected=False)
    s.status(21, "STATUS: READY")
    footer(s, [("PF1", "HELP"), ("PF3", "EXIT")])
    return s


def results_screen(offset):
    s = Screen("results")
    title_block(s, "FWV-02")
    s.add(3, 4, "PATIENT SEARCH RESULTS", "heading", BRIGHT)
    s.add(3, 66, "DATE " + DATE, "datetime", DIM)
    names = ["BELLA", "MAX", "LUNA", "CHARLIE", "COOPER", "DAISY",
             "ROCKY", "MOLLY", "DUKE", "SADIE"]
    for i in range(10):
        idx = offset + i
        s.add(5 + i, 4, "FWV-%06d" % (4812 - idx), "value", NORM)
        s.add(5 + i, 16, names[idx % len(names)], "value", NORM)
    s.add(16, 4, "10 MATCHES. PAGE DOWN SCROLLS, PF9 OPENS THE MENU.",
          "message", NORM)
    s.add(20, 4, "COMMAND ===>", "label", DIM)
    s.add(20, 16, "____", "input", BRIGHT, protected=False)
    s.status(21, "STATUS: READY")
    footer(s, [("PF1", "HELP"), ("PF3", "EXIT"), ("PF9", "MENU")])
    return s


def save(screen, path):
    img = screen.render()
    img.save(path)
    with open(path.replace(".png", ".json"), "w") as f:
        json.dump(screen.ground_truth(), f, indent=2)
    print("wrote %s (%d elements)" % (path, len(screen.elements)))


def encode(frames, path, fps, crf):
    proc = subprocess.Popen(
        ["ffmpeg", "-hide_banner", "-loglevel", "error", "-y",
         "-f", "rawvideo", "-pix_fmt", "rgb24",
         "-s", "%dx%d" % (W, H), "-r", str(fps), "-i", "-",
         "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", str(crf),
         path],
        stdin=subprocess.PIPE)
    for img in frames:
        proc.stdin.write(img.tobytes())
    proc.stdin.close()
    proc.wait()
    assert proc.returncode == 0, "ffmpeg failed on " + path


def sha16(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        h.update(f.read())
    return h.hexdigest()[:16]


os.makedirs(SCREENS, exist_ok=True)

lookup_v1 = lookup_screen("FWV-01", "UPDATE")
save(lookup_v1, os.path.join(SCREENS, "lookup-v1.png"))
lookup_v2 = lookup_screen("FWV-01", "SAVE")
save(lookup_v2, os.path.join(SCREENS, "lookup-v2.png"))
lookup_changes = {
    "a": "lookup-v1.png",
    "b": "lookup-v2.png",
    "changes": [
        {"id": "C1", "type": "footer-changed", "location": "row 23",
         "before": "PF5 UPDATE", "after": "PF5 SAVE",
         "cited_by": []},
    ],
}
with open(os.path.join(SCREENS, "lookup-changes.json"), "w") as f:
    json.dump(lookup_changes, f, indent=2)

decomm_v1 = decomm_screen("FWV-09", True)
save(decomm_v1, os.path.join(SCREENS, "decomm-v1.png"))
decomm_v2 = decomm_screen("FWV-09", False)
save(decomm_v2, os.path.join(SCREENS, "decomm-v2.png"))
decomm_changes = {
    "a": "decomm-v1.png",
    "b": "decomm-v2.png",
    "changes": [
        {"id": "C1", "type": "message-removed", "location": "row 9",
         "before": "WARNING: DECOMMISSION IS PERMANENT.", "after": "(blank)",
         "cited_by": ["S1"]},
        {"id": "C2", "type": "confirmation-removed", "location": "row 10",
         "before": "TYPE CLINIC NAME TO CONFIRM: ________________",
         "after": "(blank)", "cited_by": ["S1"]},
        {"id": "C3", "type": "footer-changed", "location": "row 23",
         "before": "PF12 CONFIRM", "after": "PF12 DECOMMISSION",
         "cited_by": ["S1"]},
    ],
}
with open(os.path.join(SCREENS, "decomm-changes.json"), "w") as f:
    json.dump(decomm_changes, f, indent=2)

# --- training recording: 45 s at 5 fps, v1 console only ---
FPS = 5
DUR = 45.0
N = int(FPS * DUR)
PATIENT_ID = "FWV-004812"
CLINIC_NAME = "MAPLE ST CLINIC"


def frame_at(t):
    if t < 6.0:
        s = menu_screen()
        if t >= 5.0:
            s.grid[17][15] = "1"
            return s.render(cursor=(18, 17))
        return s.render(cursor=(18, 17))
    if t < 16.0:
        s = lookup_screen("FWV-01", "UPDATE")
        for e in s.elements:
            if e["kind"] == "value" and e["row"] in (6, 7, 8, 9, 10, 11,
                                                     12, 14, 15, 16, 17):
                r = e["row"] - 1
                for i in range(e["len"]):
                    s.grid[r][e["col"] - 1 + i] = " "
        n = min(len(PATIENT_ID), max(0, int(t - 7.0) + 1) if t >= 7.0 else 0)
        for i in range(n):
            s.grid[5][15 + i] = PATIENT_ID[i]
        return s.render(cursor=(6, 16 + n))
    if t < 20.0:
        return results_screen(0).render(cursor=(20, 21))
    if t < 28.0:
        off = 1 if t >= 24.0 else 0
        return results_screen(off).render(cursor=(20, 21))
    if t < 33.0:
        return decomm_screen("FWV-09", True).render(cursor=(10, 35))
    s = decomm_screen("FWV-09", True)
    n = min(len(CLINIC_NAME), max(0, int((t - 33.0) / 0.8) + 1))
    for i in range(n):
        s.grid[9][33 + i] = CLINIC_NAME[i]
    return s.render(cursor=(10, 35 + n))


steps = [
    {"id": "T1", "description": "Show the main menu",
     "start_s": 0.0, "end_s": 6.0},
    {"id": "T2", "description": "Type the patient ID FWV-004812 on the lookup screen",
     "start_s": 6.0, "end_s": 16.0},
    {"id": "T3", "description": "Submit the lookup; results list appears",
     "start_s": 16.0, "end_s": 20.0},
    {"id": "T4", "description": "Scroll down through the results list",
     "start_s": 20.0, "end_s": 28.0},
    {"id": "T5", "description": "Open the decommission confirmation screen",
     "start_s": 28.0, "end_s": 33.0},
    {"id": "T6", "description": "Type the clinic name MAPLE ST CLINIC to confirm",
     "start_s": 33.0, "end_s": 45.0},
]
with open(os.path.join(HERE, "training-v1-steps.json"), "w") as f:
    json.dump({"video": "training-v1.mp4", "duration_s": DUR, "fps": FPS,
               "steps": steps}, f, indent=2)

frames = [frame_at((i + 0.5) / FPS) for i in range(N)]
mp4 = os.path.join(HERE, "training-v1.mp4")
encode(frames, mp4, FPS, 30)
print("wrote %s (%.1f s, %d bytes, sha %s)"
      % (mp4, DUR, os.path.getsize(mp4), sha16(mp4)))

os.makedirs(TAKES, exist_ok=True)
encode(frames, os.path.join(TAKES, "2026-10-31-training-v1.mkv"), FPS, 18)
stills_v1 = [lookup_v1.render()] * 30 + [decomm_v1.render()] * 30
stills_v2 = [lookup_v2.render()] * 30 + [decomm_v2.render()] * 30
encode(stills_v1, os.path.join(TAKES, "2026-10-31-console-v1.mkv"), 5, 18)
encode(stills_v2, os.path.join(TAKES, "2026-10-31-console-v2.mkv"), 5, 18)
for name in ["2026-10-31-training-v1.mkv", "2026-10-31-console-v1.mkv",
             "2026-10-31-console-v2.mkv"]:
    p = os.path.join(TAKES, name)
    print("take %s (%d bytes, sha %s)" % (p, os.path.getsize(p), sha16(p)))
