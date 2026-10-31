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
status:     not-started
```
What Doyle is, how to run the demo from a fresh clone, and a clear line between what was built during the event (Doyle) and what are existing dependencies (Vimanam, Lambo, Muse Code), as the rules require. Usage replaces "planned". The status note goes.

**Done when:** a fresh clone, following only the README, runs the offline demo; the user has read it.

---

### T8.2: Demo video ★
```yaml
requires:   [T7.3]
fixture-ok: no
size:       M · user
owns:       evidence/video/
status:     not-started
```
Three minutes, of what was built during the event: the problem, the run without Doyle, the run with Doyle, the review page, the numbers. The agent drafts the script from the recorded runs; the user records and publishes. The cut assembles from the takes T7.2 and T7.3 left in the demo-shoot `takes/`.

**Done when:** the user has published the video and its link is in the README and the judges' page.

---

### T8.3: Public repository ★
```yaml
requires:   [T8.1]
fixture-ok: yes
size:       S · user
owns:       none
status:     not-started
```
Before the user makes the repository public: scan the whole history for key values and personal data (`grep -rlF` with each value loaded, never printed), check `.gitignore` covers `.env`, `doyle.toml` and `.doyle/`, and read the dev-diary for anything private. The user flips it public.

**Done when:** the scan is clean and recorded in the handoff log, and the repository is public.

---

### T8.4: Submit ★
```yaml
requires:   [T8.2, T8.3]
fixture-ok: no
size:       S · user
owns:       none
status:     not-started
```
The agent drafts the submission text from product.md, the measures and the judges' page; the user submits.

**Done when:** the user has submitted, and the handoff log says so.

---

## Exit Criteria

- [ ] The README runs a fresh clone through the demo (T8.1).
- [ ] The video is published (T8.2).
- [ ] The repository is public and clean (T8.3).
- [ ] Submitted (T8.4).
