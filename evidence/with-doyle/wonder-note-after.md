# MEMORY.md (created by Doyle's write-back)

---
type: reference
---

- [old-records-service](old-records-service.md) — Old records service GET /patients behavior
- GET /patients returns an array of Patient objects limited to 100 items per 200 response, so large clinics span multiple responses.
- The GET /patients operation returns an array of Patient objects with at most 100 entries per response.

# old-records-service.md

---
type: project
description: Old records service GET /patients behavior
---

On the old records service, GET /patients returns every patient in one response; one call exports the whole clinic. [Superseded by MEMORY.md#2] [Superseded by MEMORY.md#3]
