# doyle check

Old: ~/work/doyle/.claude/worktrees/agent-ac5e63a24d50bb4dc/testdata/contracts/patients-v1.yaml
New: ~/work/doyle/.claude/worktrees/agent-ac5e63a24d50bb4dc/testdata/contracts/patients-v2q.yaml

## Flags (2)

### flag-1 — 49e5492c-3276-4779-83c5-76554984372b (lambo) — review
- Changes: vc2_85021e37503afa3aec3d4b868cd65c691c52ee246db06e7d115220e58b9f9025
- Evidence: GET /patients returns all patients in one response and exports the whole clinic in one call
- Proposal: proposal-1 — correct
  - Rationale: 200 response now limits array to maxItems 100.
  - New text: GET /patients returns an array of Patient objects limited to 100 items per 200 response, so large clinics span multiple responses.

### flag-2 — old-records-service.md#0 (muse-code) — review
- Changes: vc2_85021e37503afa3aec3d4b868cd65c691c52ee246db06e7d115220e58b9f9025
- Evidence: GET /patients returns every patient in a single response allowing full clinic export in one call
- Proposal: proposal-2 — correct
  - Rationale: New 200 response schema adds maxItems 100.
  - New text: The GET /patients operation returns an array of Patient objects with at most 100 entries per response.

## Proposals (2)

- proposal-1 — flag-1 — correct
  - Rationale: 200 response now limits array to maxItems 100.
  - New text: GET /patients returns an array of Patient objects limited to 100 items per 200 response, so large clinics span multiple responses.
- proposal-2 — flag-2 — correct
  - Rationale: New 200 response schema adds maxItems 100.
  - New text: The GET /patients operation returns an array of Patient objects with at most 100 entries per response.
exit=1
