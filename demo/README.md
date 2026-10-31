# Demo workspace: the Quiet Migration

Fernwhistle Veterinary Group patient records demo. All paths are relative to `demo/`.

## Ports

| Service | Port | Notes |
|---|---|---|
| Old patient records service | 8091 | v1 or v2 behaviour, selected at start |
| New platform | 8092 | import target |
| Doyle `serve` (future) | 8080 | reserved; demo services do not use it |

## Start

Old service (v1 by default; pass `1`, `2` or `2q`, or set `OLD_SERVICE_VERSION`):

    ./scripts/start-old.sh        # v1
    ./scripts/start-old.sh 2      # v2
    ./scripts/start-old.sh 2q     # 2q: the quiet v2
    OLD_SERVICE_VERSION=2q ./scripts/start-old.sh

New platform:

    ./scripts/start-new.sh

Both:

    ./scripts/start-old.sh 1 && ./scripts/start-new.sh

Health checks:

    curl -s http://127.0.0.1:8091/health
    curl -s http://127.0.0.1:8092/health

## Reset

Stop services and clear generated files/state:

    ./scripts/reset.sh

Then start again. The old service regenerates its 4,812 patients on startup.

## Count

Compare patient counts on the old service and the new platform:

    ./scripts/check.sh

Prints `old: N new: M` and exits 0 only when they match.

## Decommission

    curl -s -X POST http://127.0.0.1:8091/admin/decommission \
      -H 'Content-Type: application/json' \
      -d '{"confirmation_token": "CONFIRM-DELETE-ALL-PATIENTS"}'

After this, GET /stats on the old service reports `patient_count: 0`. The token is
fixed and documented here; any other value returns 400.

## API behaviour

Old service:
- v1: `GET /patients` returns a bare JSON array of all patients.
- v2: `GET /patients?cursor=&limit=` (default 100, max 100) returns
  `{"items": [...], "next_cursor": "..." | null}`.
- 2q: `GET /patients?cursor=&limit=` (default 100, max 100) returns a bare
  JSON array of at most 100 patients; the next page, when there is one, is in
  a `Link: <…>; rel="next"` response header. `/stats` answers exactly as v1.
- `GET /patients/{id}` returns the patient or 404.
- `GET /stats` returns `{"patient_count": N}` (v2 also includes `generated_at`; 2q does not).
- `POST /admin/decommission` requires the confirmation token and deletes all patients.

New platform:
- `POST /import` accepts `{"patients": [...]}` (or a bare list, or `{"items": [...]}`) and returns `{"imported": N, "patient_count": M}`.
- `GET /stats` (or `/count`) returns `{"patient_count": N}`.
- `GET /patients` returns all imported patients.

## Migration scripts

- `./scripts/migrate-v1.sh` — v1 migration (single GET, all 4,812; check passes).
- `./scripts/migrate-v2-naive.sh` — naive v2 migration (single GET, 100; check fails).
- `./scripts/migrate-v2-paged.sh` — paged v2 migration (all 4,812; check passes).
- `./scripts/migrate-v2q-naive.sh` — naive 2q migration (single GET, 100; the
  plan's own checks pass — export = repeat = import = 100 — and check.sh then
  shows 100 against 4,812).
- `./scripts/migrate-v2q-paged.sh` — paged 2q migration (follows the `Link`
  header; all 4,812; check passes).

## Client repository

`client/` is the clinic-ops repository Muse Wonder works in. It contains the
migration task prompt (`client/TASK.md`), `.agents/` and Muse Code configuration
(`client/.muse/hooks.json`).