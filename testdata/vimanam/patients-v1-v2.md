# API Diff: Fernwhistle Veterinary Group patient records 1.0.0 → 2.0.0

**Summary:** 0 endpoints added, 0 removed, 2 changed; 1 breaking, 6 non-breaking, 1 to review

## Breaking changes (1)

| Change | Endpoint | Detail |
|--------|----------|--------|
| Response schema changed | `GET /patients` | 200 `/type` changed `array` → `object` |

## Non-breaking changes (6)

| Change | Endpoint | Detail |
|--------|----------|--------|
| Parameter added | `GET /patients` | `cursor` (query), optional |
| Parameter added | `GET /patients` | `limit` (query), optional |
| Response schema changed | `GET /patients` | 200 `/properties/items` added |
| Response schema changed | `GET /patients` | 200 `/properties/next_cursor` added |
| Response schema changed | `GET /patients` | 200 `/properties/items` added to `required` |
| Response schema changed | `GET /stats` | 200 `/properties/generated_at` added |

## Needs review (1)

| Change | Endpoint | Detail |
|--------|----------|--------|
| Response schema changed | `GET /patients` | 200 `/items` removed |
