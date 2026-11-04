# P8: Ship

The submission: the README, the video, the public repository, the form. Everything outward here is the user's to do or approve.

Read first: [`../product.md`](../product.md) (Hackathon rules that shape the build).

---

### T8.1: README
```yaml
requires:   [T7.5]
fixture-ok: yes
size:       S · mid
owns:       README.md
status:     done
```
What Doyle is, how to run the demo from a fresh clone, and a clear line between what was built during the event (Doyle) and what are existing dependencies (Vimanam, Lambo, Muse Code), as the rules require. Usage replaces "planned". The status note goes.

**Done when:** a fresh clone, following only the README, runs the offline demo; the user has read it.

Notes (t81): README rewritten on branch t81 (commit 4c6266f): what Doyle is, fresh-clone offline demo (gate, cite + replay-backed check v1→v2 with exit 1, serve review + /judges, guard deny), new-vs-existing table, at-most numbers quoted from evidence/measures.md, live paths separated. Verified by a real fresh `git clone` of the worktree to /tmp: check.sh green from scratch, cite/check output as documented, all 34 /judges links plus /flag/flag-1 resolve 200, guard denies with the documented reason (exit 0). No README divergence found; no claim beyond the at-most list. Second Done-when clause (user read) is the user's. Surprise: `pkill -f 'doyle serve'` matches the invoking shell itself — kill serve by port instead.

Notes (orchestrator, closing): the user read the README and asked for it polished; the orchestrator rewrote it around both recorded stories, how Doyle works, the offline demo (the clone line now names the public repository), the measured numbers for both stories, the new-or-existing table, the agents that built it, and the name with the 1979 paper in full. It links the video, the docs site and the X thread.

---

### T8.2: Demo video ★
```yaml
requires:   [T7.3]
fixture-ok: no
size:       M · user
owns:       none in the repository (the cut, slides and builder live in the demo-shoot folder, user decision)
status:     done
```
Three minutes, of what was built during the event: the problem, the run without Doyle, the run with Doyle, the review page, the numbers. The agent drafts the script from the recorded runs; the user records and publishes. The cut assembles from the takes T7.2 and T7.3 left in the demo-shoot `takes/`.

**Done when:** the user has published the video and its link is in the README and the judges' page.

Notes (T8.2, in-review):
- Built in `evidence/video/` only: `SCRIPT.md` (about 430 words, five beats plus close, every number indexed to its `evidence/measures.md` line, every claim inside the at-most list), `ASSEMBLY.md` (takes inventory with ffprobe durations, gaps, shot order, recipe), `assemble.sh` (story-order ffmpeg concat into a reel outside the repo; verified here: 10 takes, 92 s, stream copy, no re-encode needed).
- Takes existing: all 12 T7.3-rerun takes (the real run), all 8 T7.3a takes, `2026-10-31-without-doyle-migration.mkv` (3 s, 6 frames), the two T7.2b takes. Takes missing or unusable: no T7.2 seed take (black Xwayland capture, deleted per T7.2 notes); both T7.2b takes are 1 s near-empty frames, so beat 2 needs user-recorded screen time; no approval-click or decommission footage exists in any run. T7.3a takes must not be cut in (self-approved finding, excluded from every claim). Console/training takes are P9 material, out of scope.
- Deliberately no video binary is committed: takes never enter the repo by convention, and the reel builds outside it.
- Recording, publishing, and the README/judges-page link updates remain the user's. Gate passes (video-only files cannot break it; still run).

Notes (orchestrator, closing): the cut was rebuilt from the T9.10 takes instead: 15 animated HTML slides and captioned clips scored to a generated track, 2:50, plus a 2:18 cut for X. The cut table, slides, song and builder live in the demo-shoot folder, outside the repository (user decision). The user published it on YouTube (`cBD3CQrDqUc`). The link is in the README, on the docs site's video page (embedded) and on the judges' page, as text there so every link on that page stays on the local server, as its test requires.

---

### T8.3: Public repository ★
```yaml
requires:   [T8.1]
fixture-ok: yes
size:       S · user
owns:       none
status:     done
```
Before the user makes the repository public: scan the whole history for key values and personal data (`grep -rlF` with each value loaded, never printed), check `.gitignore` covers `.env`, `doyle.toml` and `.doyle/`, and read the dev-diary for anything private. The user flips it public.

**Done when:** the scan is clean and recorded in the handoff log, and the repository is public.


Notes (orchestrator): scan run before the repository went public. Key values (the Baseten and Meta Model API keys from the keyring, the FAL key, the OBS WebSocket password) loaded into a mode-600 patterns file, never printed: no match in the working tree, in any commit of the history, or in the demo-shoot folder. Personal data: every commit uses the GitHub no-reply address; the account email (one keyring line in `AGENTS.md`) and a home path (three files) were removed from the tree; both remain in older commits, left as they are by the user's decision (rewriting would change every commit hash the dev-diary cites). The only other hit is the operating system's default hostname in two session exports, not identifying. `.gitignore` covers `.env`, `doyle.toml` and `.doyle/`. Video-making files moved out of the repository (user decision).

---

### T8.4: Submit ★
```yaml
requires:   [T8.2, T8.3]
fixture-ok: no
size:       S · user
owns:       none
status:     done
```
The agent drafts the submission text from product.md, the measures and the judges' page; the user submits.

**Done when:** the user has submitted, and the handoff log says so.

Notes (orchestrator): the agent drafted the form's answers from the run records (the project description, the use of Meta products, why Meta's models matter, partner feedback in 150 characters), the YouTube details and thumbnail, and the X thread. The user submitted the form with the repository, the video and the X thread.

---

## Exit Criteria

- [x] The README runs a fresh clone through the demo (T8.1).
- [x] The video is published (T8.2).
- [x] The repository is public and clean (T8.3).
- [x] Submitted (T8.4).
