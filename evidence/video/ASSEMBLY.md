# Assembly notes and manifest (T8.2)

Takes live outside the repository by convention
(`~/work/demo-shoot/doyle/takes/`, per T7.2/T7.3 briefs) and are never
committed. This file is the cut assembly: what exists, what is missing, the
shot order for the script, and the exact recipe that builds the reel. The
reel itself is built outside the repo; only this manifest, `SCRIPT.md` and
`assemble.sh` are committed here.

## Takes inventory (all durations from ffprobe)

Usable for the video — the real with-Doyle run (T7.3 rerun, static renders
of real output; `evidence/with-doyle/README.md` "Takes"):

| File | Dur | Shows |
|---|---|---|
| `2026-11-01-t7.3-rerun-guard-precheck.mkv` | 10 s | pre-check: M1 flagged review, probe denied |
| `2026-11-01-t7.3-rerun-denial.mkv` | 14 s | SessionStart notice + denials |
| `2026-11-01-t7.3-rerun-migration-1.mkv` | 12 s | session-one output ("migration paused") |
| `2026-11-01-t7.3-rerun-review-open-desktop.mkv` | 5 s | review page, 2 open (desktop) |
| `2026-11-01-t7.3-rerun-review-open-phone.mkv` | 5 s | review page, 2 open (phone) |
| `2026-11-01-t7.3-rerun-review-lambo-write-failed-desktop.mkv` | 5 s | flag-1 mid-saga: Muse Code corrected, Lambo 404 |
| `2026-11-01-t7.3-rerun-review-settled-desktop.mkv` | 5 s | 0 open, 2 settled (desktop) |
| `2026-11-01-t7.3-rerun-review-settled-phone.mkv` | 5 s | 0 open, 2 settled (phone) |
| `2026-11-01-t7.3-rerun-review-flag-1-corrected-desktop.mkv` | 5 s | "Every copy holds the correction" |
| `2026-11-01-t7.3-rerun-corrected-memory.mkv` | 14 s | MEMORY.md + Lambo recall after |
| `2026-11-01-t7.3-rerun-migration-2.mkv` | 16 s | session-two snapshot, output, measures |
| `2026-11-01-t7.3-rerun-final-counts.mkv` | 8 s | counts old 4,812 / new 4,812 |

Without-Doyle material (thin — see gaps):

| File | Dur | Shows |
|---|---|---|
| `2026-10-31-without-doyle-migration.mkv` | 3 s | T7.2 migration, 6 genuine frames (start + final report) |
| `2026-10-31-without-doyle-quiet-seed.mkv` | 1 s | T7.2b seed start frame only (near-empty, 1,398 bytes) |
| `2026-10-31-without-doyle-quiet-migration.mkv` | 1 s | T7.2b migration start frame only (near-empty, 1,398 bytes) |

Not for this video:

- `2026-11-01-t7.3-*` (no `rerun`): the T7.3a first attempt, where Wonder
  approved its own correction. Published as a finding and explicitly
  excluded from every claim (measures.md L176–184). Do not cut these in.
- `2026-10-31-console-v1/v2.mkv`, `2026-10-31-training-v1.mkv`: P9 screen
  evidence (T9.4). Out of scope until the user decides the video covers it.

## Gaps (for the user's recording session)

1. No T7.2 seed take exists: the fullscreen capture came back black
   (empty Xwayland framebuffer; the black file was deleted, per T7.2 notes).
   Seed record is transcript + export only.
2. The T7.2b takes are 1-second near-empty frames — unusable as footage.
   Beat 2 needs the user to record fresh screen time (scrolling the T7.2b
   README counts section, or the counts files) or to hold a title card.
3. No footage of the person's approval click itself; the settled-page takes
   are the record. The user may re-enact the click on a copy of the state
   (T7.5 notes the committed state serves read-only for this).
4. No live decommission footage exists in any run (T7.2's executed after
   approval; T7.2b/T7.3's waited unanswered to the limit). The script does
   not show one.
5. Total raw footage is about 104 s against a 180 s script: the user reads
   beats 1 and 5 over title/numbers cards they create (SCRIPT.md names
   them), and paces the reel takes under beats 3–4.

## Shot order (SCRIPT.md codes)

- SHOT-WO: `2026-10-31-without-doyle-migration.mkv` + user-recorded T7.2b
  counts scroll (gap 2).
- SHOT-DENY: rerun `guard-precheck` → `denial` → `migration-1` (0:00–0:36
  of the reel).
- SHOT-REVIEW: rerun `review-open-desktop` (desktop cut; phone variant
  held as alternate).
- SHOT-SETTLED: rerun `review-settled-desktop` (+ `flag-1-corrected`
  as the "every copy" beat).
- SHOT-MEMORY: rerun `corrected-memory`.
- SHOT-MIG2: rerun `migration-2`.
- SHOT-COUNTS: rerun `final-counts`.
- Optional, if the 3 minutes need the saga's honesty: `lambo-write-failed`
  (5 s) with one line — "the write-back failed three times for real Doyle
  defects, each since fixed" (`evidence/with-doyle/README.md`, "The
  write-back saga"). Not in the script; the user's call.

## Recipe

`assemble.sh` (committed beside this file) concatenates the story-order
takes with ffmpeg into `doyle-reel.mkv` next to the takes, outside the repo.
Verified in this worktree with `ffmpeg`/`ffprobe`: all takes share codec
and size conventions from the same render path, so the script first tries
stream concat and falls back to re-encode; it prints each input's duration
and the reel total. Expected reel: about 99 s from the 12 rerun takes
(about 104 s with the without-Doyle takes prepended).
