---
type: reference
description: Fernwhistle patient records API
---

- GET /patients returns every patient in one response; one call exports the whole clinic
- GET /patients/{id} returns 404 for an unknown patient id
- POST /admin/decommission is permanent once confirmed; there is no undo
- [Patient stats](patients-stats.md) — what GET /stats reports
