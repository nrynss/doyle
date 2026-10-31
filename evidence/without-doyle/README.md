# Run without Doyle (T7.2)

One seed session writes the v1 belief (`GET /patients` returns every patient
in one response) into its own Muse Code memory; one fresh migration session
gets `TASK.md` and nothing else, against the v2 service. Both ran from a
scratch copy of `demo/client` outside the repository, with an empty memory
folder at the start.

Finding: Wonder did NOT lose records. The migration session received the stale
memory in its start snapshot, probed the endpoint, found the `next_cursor`
pagination, paged all 49 pages, verified twice (counts and content hash),
imported 4,812, matched the counts, and only then decommissioned. Per the
brief this stands as the result, not a run to repeat.

Counts: before old 4,812 / new 0; after old 0 (decommissioned) / new 4,812.

Files: `seed-transcript.txt` + `seed-export.json` (session 1),
`migration-transcript.txt` + `migration-export.json` (session 2),
`counts-before.txt` / `counts-after.txt` (from `demo/scripts/check.sh` plus
both `/stats`). Credential values and the local username are scrubbed in the
committed copies; both exports still parse as JSON.

Screen take: `takes/2026-10-31-without-doyle-migration.mkv` (outside the repo).
Six frames, sampled live once a second from the terminal running the session;
`muse exec` emits almost nothing incremental, so the frames show the start
and the final report. No seed take: the fullscreen capture came back black
(Xwayland `:0` framebuffer is empty under the Wayland compositor; verified
with a test window), so the transcript and export are the seed record.
