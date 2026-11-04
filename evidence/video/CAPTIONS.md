# Demo video: captions and cut (T8.2)

A 2:50 music video: no voice, animated slides between live clips, captions burned into the clips, scored to the Doyle song (v4, progressive rock, `~/work/demo-shoot/doyle/music/doyle-v4.mp3`). This table is the cut: `build-reel.py` reads it, so edit it and rebuild.

- **Take**: `slide N` (the Nth slide of `~/work/demo-shoot/doyle/slides/slides.html`, recorded with its animation), or a take name (`~/work/demo-shoot/doyle/takes/2026-11-04-t9.10-<take>.mkv`).
- **From–to**: the part of the take used, in seconds; it is sped up or slowed to fill the row's time. Empty for slides.
- **Caption**: shown along the bottom of a clip. Keep it under about 3 words per second of the row. Slides need none.
- Every number in a caption or slide is in `evidence/legacy/` (the run READMEs).

| Start | End | Take | From–to | Caption |
|---|---|---|---|---|
| 0 | 6 | slide 1 | | |
| 6 | 12 | slide 2 | | |
| 12 | 17 | slide 3 | | |
| 17 | 20 | slide 4 | | |
| 20 | 32 | wo-session | 60–1050 | Without Doyle, Wonder checks its port against the batch, finds the exemption itself, and fixes it. |
| 32 | 37 | wo-after | 18–35 | Shipped and retired. But the memory still says "no exceptions". |
| 37 | 43 | slide 5 | | |
| 43 | 46 | slide 6 | | |
| 46 | 53 | slide 7 | | |
| 53 | 60 | slide 8 | | |
| 60 | 66 | slide 9 | | |
| 66 | 78 | wd-flags | 0–513 | Doyle cites each memory, diffs the COBOL, and runs both programs on all 4,812 accounts. |
| 78 | 86 | wd-flags-reads | 280–347 | Wonder reads the console screens and training video for Doyle. |
| 86 | 92 | slide 10 | | |
| 92 | 98 | wd-guard | 0–35 | The guard, live: ship and retire denied, reading the code allowed. |
| 98 | 116 | wd-session1 | 0–968 | Session one: Wonder is denied the rewrite, works out the exemption, and stops. |
| 116 | 121 | slide 11 | | |
| 121 | 127 | slide 12 | | |
| 127 | 137 | wd-review-decide | 0–87 | The review page: each flag with its evidence, decided. |
| 137 | 142 | slide 13 | | |
| 142 | 148 | wd-memory | 0–20 | The correction, in Muse Code and in Lambo. |
| 148 | 154 | wd-session2 | 0–900 | Session two, from the corrected memory: a two-line fix, shipped. |
| 154 | 159 | wd-converge | 14–39 | Doyle runs both programs again: 0 mismatches. |
| 159 | 164 | slide 14 | | |
| 164 | 170 | slide 15 | | |

## Notes for the edit

- Slide 12 says "A person's sign-off, which the agent cannot give itself"; its small print says that in this recording the operator clicked, at the user's instruction (`evidence/legacy/with-doyle/README.md`).
- The three false alarms are real (S2 twice from one wobbly screen read, L4 once from a curator draw) and counted on slides 12 and 14.
- Session two did not reach the console retirement (a rig fault, fixed). Nothing here claims it did.
