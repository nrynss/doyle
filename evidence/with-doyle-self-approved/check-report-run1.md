# doyle check

Old: ~/work/doyle/.claude/worktrees/agent-a93abf3a80a51927d/testdata/contracts/patients-v1.yaml
New: ~/work/doyle/.claude/worktrees/agent-a93abf3a80a51927d/testdata/contracts/patients-v2q.yaml

## Flags (2)

### flag-1 — MEMORY.md#1 (muse-code) — review
- Changes: vc2_85021e37503afa3aec3d4b868cd65c691c52ee246db06e7d115220e58b9f9025
- Evidence: GET /patients returns every patient in one response and one call exports the whole clinic
- Proposal: proposal-1 — correct
  - Rationale: New response schema adds maxItems 100 to GET /patients.
  - New text: GET /patients returns an array of Patient objects with at most 100 items in a response, so a single call covers up to 100 patients.

### flag-2 — 758303dc-94e4-447b-90a3-5d988f729591 (lambo) — review
- Changes: vc2_85021e37503afa3aec3d4b868cd65c691c52ee246db06e7d115220e58b9f9025
- Evidence: GET /patients returns every patient in one response allowing full clinic export in one call

## Proposals (1)

- proposal-1 — flag-1 — correct
  - Rationale: New response schema adds maxItems 100 to GET /patients.
  - New text: GET /patients returns an array of Patient objects with at most 100 items in a response, so a single call covers up to 100 patients.
