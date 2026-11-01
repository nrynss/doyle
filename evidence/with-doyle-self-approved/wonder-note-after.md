---
type: project
description: Old records service GET /patients exports whole clinic
---

- Old records service: `GET /patients` returns every patient in one response; one call exports the whole clinic. [Superseded by MEMORY.md#2]
- GET /patients returns an array of at most 100 patients per response. To export the whole clinic, paginate using cursor/limit and follow the Link header (present only when more patients remain) until no Link remains, and verify the exported total against GET /stats patient_count.
