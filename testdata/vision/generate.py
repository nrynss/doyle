#!/usr/bin/env python3
"""Build T9.2 vision fixtures: 3270 green screens, diff pairs, a 60 s recording.

Single source of truth: every PNG/MP4 here is rendered from the element
models below, and the ground-truth JSON files are exported from the same
models. No network, no key, no model calls. Re-run with:
    python3 testdata/vision/generate.py
Outputs land beside this script in testdata/vision/.
"""
import hashlib
import json
import os
import subprocess

from PIL import Image, ImageDraw, ImageFont

HERE = os.path.dirname(os.path.abspath(__file__))
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


class Screen:
    """An 80x24 text grid plus its element list (1-indexed rows/cols)."""

    def __init__(self, name):
        self.name = name
        self.grid = [[" "] * 80 for _ in range(24)]
        self.attrs = [[BG] * 80 for _ in range(24)]
        self.fill = {}
        self.elements = []

    def add(self, row, col, text, kind, color, protected=True):
        r, c = row - 1, col - 1
        for i, ch in enumerate(text):
            self.grid[r][c + i] = ch
            self.attrs[r][c + i] = color
        self.elements.append(
            {
                "id": "E%02d" % (len(self.elements) + 1),
                "row": row,
                "col": col,
                "len": len(text),
                "text": text,
                "kind": kind,
                "protected": protected,
            }
        )

    def rule(self, row):
        r = row - 1
        for c in range(1, 79):
            self.grid[r][c] = "-"
            self.attrs[r][c] = DIM

    def status(self, row, text):
        r = row - 1
        for c in range(80):
            self.fill[(r, c)] = STATUS_BG
        for i, ch in enumerate(text):
            self.grid[r][3 + i] = ch
            self.attrs[r][3 + i] = STATUS_FG
        self.elements.append(
            {
                "id": "E%02d" % (len(self.elements) + 1),
                "row": row,
                "col": 4,
                "len": len(text),
                "text": text,
                "kind": "status",
                "protected": True,
            }
        )

    def text_rows(self):
        return ["".join(r).rstrip() for r in self.grid]

    def ground_truth(self):
        return {
            "screen": self.name,
            "rows": 24,
            "cols": 80,
            "elements": self.elements,
            "screen_text": self.text_rows(),
        }

    def render(self, cursor=None):
        font = ImageFont.truetype(FONT, FS)
        img = Image.new("RGB", (W, H), BG)
        d = ImageDraw.Draw(img)
        for (r, c), color in self.fill.items():
            d.rectangle([c * CELL_W, r * LINE_H,
                         (c + 1) * CELL_W - 1, (r + 1) * LINE_H - 1],
                        fill=color)
        for r in range(24):
            for c in range(80):
                ch = self.grid[r][c]
                if ch == " ":
                    continue
                fg = self.attrs[r][c]
                if fg == STATUS_BG:
                    fg = STATUS_FG
                d.text((c * CELL_W, r * LINE_H + 2), ch, font=font, fill=fg)
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
    for key, _label in keys:
        s.add(23, col, key + " " + _label, "pfkey", DIM)
        col += len(key) + len(_label) + 4


def lookup_screen(code, phone_label, balance, pf5_label="UPDATE"):
    s = Screen("lookup")
    title_block(s, code)
    s.add(3, 4, "PATIENT LOOKUP", "heading", BRIGHT)
    s.add(3, 66, "DATE 06/14/26", "datetime", DIM)
    fields = [
        ("CLINIC", "MAPLE ST CLINIC"),
        ("PATIENT ID", "FWV-004812"),
        ("NAME", "BELLA"),
        ("SPECIES", "CANINE"),
        ("BREED", "GOLDEN RETRIEVER"),
        ("DOB", "03/02/2019"),
        ("OWNER", "A. RIVERA"),
        (phone_label, "555-0142"),
    ]
    row = 5
    for label, value in fields:
        if label in ("PHONE", "MOBILE"):
            r = 12
        else:
            r = row
            row += 1
            if row == 12:
                row = 13
        s.add(r, 4, label, "label", DIM)
        s.add(r, 16, value, "value", NORM)
    s.add(14, 4, "VACCINATIONS", "label", DIM)
    s.add(14, 18, "RABIES 06/01/26  DHPP 05/12/26", "value", NORM)
    s.add(15, 4, "ALLERGIES", "label", DIM)
    s.add(15, 18, "PENICILLIN", "value", NORM)
    s.add(16, 4, "BALANCE", "label", DIM)
    s.add(16, 18, balance, "value", NORM)
    s.add(17, 4, "LATE FEE", "label", DIM)
    s.add(17, 18, "$  0.64", "value", NORM)
    s.add(19, 4, "RECORD FOUND. PF5 UPDATES, PF12 DECOMMISSIONS.", "message", NORM)
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
    s.add(3, 66, "DATE 06/14/26", "datetime", DIM)
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
    s.add(3, 66, "DATE 06/14/26", "datetime", DIM)
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
    s.add(3, 66, "DATE 06/14/26", "datetime", DIM)
    names = ["BELLA", "MAX", "LUNA", "CHARLIE", "COOPER", "DAISY",
             "ROCKY", "MOLLY", "DUKE", "SADIE", "TEDDY", "BAILEY"]
    for i in range(10):
        idx = offset + i
        s.add(5 + i, 4, "FWV-%06d" % (4812 - idx), "value", NORM)
        s.add(5 + i, 16, names[idx % len(names)], "value", NORM)
    s.add(16, 4, "12 MATCHES. PAGE DOWN SCROLLS, PF9 OPENS THE MENU.",
          "message", NORM)
    s.add(20, 4, "COMMAND ===>", "label", DIM)
    s.add(20, 16, "____", "input", BRIGHT, protected=False)
    s.status(21, "STATUS: READY")
    footer(s, [("PF1", "HELP"), ("PF3", "EXIT"), ("PF9", "MENU")])
    return s


def menu_overlay(base):
    OVER_BG = (0, 64, 0)
    s = base
    for r in range(10, 17):
        for c in range(25, 56):
            s.grid[r - 1][c - 1] = " "
            s.attrs[r - 1][c - 1] = BRIGHT
            s.fill[(r - 1, c - 1)] = OVER_BG
    s.add(10, 27, "ACTION MENU", "heading", BRIGHT)
    s.add(12, 29, "U UPDATE RECORD", "value", BRIGHT)
    s.add(13, 29, "D DECOMMISSION", "value", BRIGHT)
    s.add(14, 29, "X EXIT MENU", "value", BRIGHT)
    return s


def save(screen, path):
    img = screen.render()
    img.save(path)
    with open(path.replace(".png", ".json"), "w") as f:
        json.dump(screen.ground_truth(), f, indent=2)
    print("wrote %s (%d elements)" % (path, len(screen.elements)))


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        h.update(f.read())
    return h.hexdigest()


# --- stills ---
lookup_v1 = lookup_screen("FWV-01", "PHONE", "$ 42.50")
save(lookup_v1, os.path.join(HERE, "lookup-v1.png"))

lookup_v2 = lookup_screen("FWV-01", "MOBILE", "$ 44.00", pf5_label="SAVE")
save(lookup_v2, os.path.join(HERE, "lookup-v2.png"))

pair_a = {
    "a": "lookup-v1.png",
    "b": "lookup-v2.png",
    "changes": [
        {"id": "C1", "type": "label-changed", "location": "row 12",
         "before": "PHONE", "after": "MOBILE"},
        {"id": "C2", "type": "value-changed", "location": "row 16",
         "before": "$ 42.50", "after": "$ 44.00"},
        {"id": "C3", "type": "footer-changed", "location": "row 23",
         "before": "PF5 UPDATE", "after": "PF5 SAVE"},
    ],
}
with open(os.path.join(HERE, "pair-a-changes.json"), "w") as f:
    json.dump(pair_a, f, indent=2)

decomm_v1 = decomm_screen("FWV-09", True)
save(decomm_v1, os.path.join(HERE, "decomm-v1.png"))
decomm_v2 = decomm_screen("FWV-09", False)
save(decomm_v2, os.path.join(HERE, "decomm-v2.png"))

pair_b = {
    "a": "decomm-v1.png",
    "b": "decomm-v2.png",
    "changes": [
        {"id": "C1", "type": "message-removed", "location": "row 9",
         "before": "WARNING: DECOMMISSION IS PERMANENT.", "after": "(blank)"},
        {"id": "C2", "type": "confirmation-removed", "location": "row 10",
         "before": "TYPE CLINIC NAME TO CONFIRM: ________________",
         "after": "(blank)"},
        {"id": "C3", "type": "footer-changed", "location": "row 23",
         "before": "PF12 CONFIRM", "after": "PF12 DECOMMISSION"},
    ],
}
with open(os.path.join(HERE, "pair-b-changes.json"), "w") as f:
    json.dump(pair_b, f, indent=2)

# pair C: byte-identical copy; any reported change is invention.
with open(os.path.join(HERE, "lookup-v1.png"), "rb") as f:
    data = f.read()
with open(os.path.join(HERE, "lookup-v1-copy.png"), "wb") as f:
    f.write(data)
print("wrote lookup-v1-copy.png (byte-identical)")
pair_c = {"a": "lookup-v1.png", "b": "lookup-v1-copy.png",
          "changes": []}
with open(os.path.join(HERE, "pair-c-changes.json"), "w") as f:
    json.dump(pair_c, f, indent=2)

# --- recording: 60 s at 5 fps ---
FPS = 5
DUR = 60.0
N = int(FPS * DUR)
PATIENT_ID = "FWV-004812"
CLINIC_NAME = "MAPLE ST"

lookup_blank = lookup_screen("FWV-01", "PHONE", "")
# blank the retrieved values for the pre-search state
for e in lookup_blank.elements:
    if e["kind"] == "value" and e["row"] in (6, 7, 8, 9, 10, 11, 12,
                                             14, 15, 16, 17):
        r = e["row"] - 1
        for i in range(e["len"]):
            lookup_blank.grid[r][e["col"] - 1 + i] = " "
lookup_blank_filled_id = None


def frame_at(t):
    if t < 7.0:
        s = menu_screen()
        if t >= 6.0:
            # "1" typed into the menu command line
            s.grid[17][15] = "1"
            return s.render(cursor=(18, 17))
        return s.render(cursor=(18, 17))
    if t < 20.0:
        s = lookup_screen("FWV-01", "PHONE", "")
        for e in s.elements:
            if e["kind"] == "value" and e["row"] in (7, 8, 9, 10, 11, 12,
                                                    14, 15, 16, 17):
                r = e["row"] - 1
                for i in range(e["len"]):
                    s.grid[r][e["col"] - 1 + i] = " "
        n = min(len(PATIENT_ID), int((t - 7.0) // 1.0))
        # one char per second starting at t=8
        n = min(len(PATIENT_ID), max(0, int(t - 8.0) + 1) if t >= 8.0 else 0)
        for i in range(n):
            s.grid[5][15 + i] = PATIENT_ID[i]
        pos = 16 + n
        if n >= len(PATIENT_ID):
            pos = 16 + len(PATIENT_ID)
        return s.render(cursor=(6, pos))
    if t < 24.0:
        return results_screen(0).render(cursor=(20, 21))
    if t < 40.0:
        off = min(2, int((t - 24.0) // 8.0 + 1) - 1)
        if t >= 32.0:
            off = 2
        elif t >= 28.0:
            off = 1
        else:
            off = 0
        return results_screen(off).render(cursor=(20, 21))
    if t < 47.0:
        return menu_overlay(results_screen(2)).render(cursor=(20, 21))
    if t < 53.0:
        return decomm_screen("FWV-09", True).render(cursor=(10, 35))
    s = decomm_screen("FWV-09", True)
    n = min(len(CLINIC_NAME), max(0, int((t - 53.0) // 0.8) + 1))
    for i in range(n):
        s.grid[9][33 + i] = CLINIC_NAME[i]
    return s.render(cursor=(10, 35 + n))


steps = [
    {"id": "S1", "description": "Show the main menu",
     "start_s": 0.0, "end_s": 7.0},
    {"id": "S2", "description": "Type the patient ID FWV-004812 on the lookup screen",
     "start_s": 7.0, "end_s": 20.0},
    {"id": "S3", "description": "Submit the lookup; results list appears",
     "start_s": 20.0, "end_s": 24.0},
    {"id": "S4", "description": "Scroll down through the results list",
     "start_s": 24.0, "end_s": 40.0},
    {"id": "S5", "description": "Open the action menu (PF9)",
     "start_s": 40.0, "end_s": 47.0},
    {"id": "S6", "description": "Open the decommission confirmation screen",
     "start_s": 47.0, "end_s": 53.0},
    {"id": "S7", "description": "Type the clinic name MAPLE ST to confirm",
     "start_s": 53.0, "end_s": 60.0},
]
with open(os.path.join(HERE, "recording-60s-steps.json"), "w") as f:
    json.dump({"video": "recording-60s.mp4", "duration_s": DUR, "fps": FPS,
               "steps": steps}, f, indent=2)

mp4 = os.path.join(HERE, "recording-60s.mp4")
proc = subprocess.Popen(
    ["ffmpeg", "-hide_banner", "-loglevel", "error", "-y",
     "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", "%dx%d" % (W, H),
     "-r", str(FPS), "-i", "-",
     "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "30",
     "-movflags", "+faststart", mp4],
    stdin=subprocess.PIPE)
for i in range(N):
    img = frame_at((i + 0.5) / FPS)
    proc.stdin.write(img.tobytes())
proc.stdin.close()
proc.wait()
assert proc.returncode == 0, "ffmpeg failed"
import os as _os
print("wrote %s (%.1f s, %d bytes, sha256 %s)"
      % (mp4, DUR, _os.path.getsize(mp4), sha256(mp4)[:16]))
