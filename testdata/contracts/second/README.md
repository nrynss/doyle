# testdata/contracts/second

The second-upgrade candidate, captured for T0.7. Every name here is invented:
the shop, the service, the repository and the sessions. The payment flow it
echoes is real (Stripe's Charges → PaymentIntents migration); see
`docs/second-upgrade.md` for the public sources. Nothing here is copied from
any spec: both contracts are hand-written summaries in the style of
`testdata/contracts/patients-v*.yaml`, faithful only in the changed
operations. No keys, no personal data, no private paths.

## Files

| Path | What it is for |
|---|---|
| `second-v1.yaml` | Bellflower Market payments before the change (OpenAPI 3.0). `createCharge` (`POST /charges`) captures the payment at once and returns a `Charge` with `status: succeeded`. |
| `second-v2.yaml` | The same service after it. `POST /charges` is gone. `createPayment` (`POST /payments`) returns a `Payment` with `status: requires_confirmation` and a `client_secret`; `confirmPayment` (`POST /payments/{id}/confirm`, body needs `return_url`) completes it. `getCharge` (`GET /charges/{id}`) and `createRefund` (`POST /refunds`) are identical in both versions: the read path for old charges, and refunds. |
| `memories.md` | Four notes an earlier shop session saved against v1, in Muse Code format (`docs/muse-code.md`): terse, no source. S1 is false in v2 in a harmful way; S2 is false in a milder way; S3 and S4 stay true (the additive/unchanged controls). |
| `second-v1-v2.json` | Vimanam 2.0.0 CLI JSON diff of v1 → v2 (the recorded output). |
| `second-v1-v2.md` | Vimanam 2.0.0 CLI Markdown diff of v1 → v2 (it omits the type-level facts the JSON carries). |
| `entities-v1.json`, `entities-v2.json` | Library parse of v1 and v2 (`Contract` entities). |
| `changes-v1-v2.json` | Library diff of v1 → v2 (same 8 change IDs as the CLI JSON). |

## The story: the unpaid shipment

1. **Setting.** Bellflower Market sells made-to-order goods. Muse Wonder works in the shop's ops repository, `market-ops`.
2. **Where S1 came from.** While the payments service was still on v1, a checkout session (`muse-wonder-checkout`) saved four notes. S1: `POST /charges` captures at once; a 200 means paid, fulfill. S2: creating a payment needs only amount, currency, card_token. S3: `GET /charges/{id}` 404s for an unknown id. S4: `POST /refunds` with charge_id and amount returns the money.
3. **The change.** The service moves to v2. Creating a payment now returns `requires_confirmation`, and the money moves only after `confirmPayment` with a `return_url`. Nothing tells the memories.
4. **Fulfilment.** The fulfilment session (`muse-wonder-fulfil`) takes a customer's order, creates a payment, gets a 200, and ships the goods the same day, because of S1. The customer never completes the confirmation step, so no money ever moves. The goods are gone and the shop has no payment to refund or dispute.
5. **With Doyle.** `doyle check` on v1 → v2 flags S1 (and S2) and nothing else. At the fulfil step, the guard denies shipping on a `requires_confirmation` payment and says why: v2 pays only after confirm. The review page shows the change, the proposed correction (fulfill only on `succeeded`, after confirm), and the sessions that relied on S1. A person approves, and the correction lands in Muse Code and in Lambo (`against` S1).

## The memories

| Id | Text | Rests on |
|---|---|---|
| S1 | POST /charges captures the payment at once; a 200 means the customer paid, fulfill the order | `createCharge`, its 200 response |
| S2 | Creating a payment needs only amount, currency, card_token | `createCharge` request, `ChargeRequest` |
| S3 | GET /charges/{id} returns 404 for an unknown charge id | `getCharge`, its 404 response |
| S4 | POST /refunds with charge_id and amount returns the money | `createRefund` |

S1 and S2 were relied on by `fulfil-1` (fulfil, confirm). S3 was relied on by `checkout-2` (check a receipt row). S4 was relied on by `fulfil-1` (standing refund policy). Sessions are listed in the order they ran: `checkout-1` (wrote S1–S4, against v1), `checkout-2`, `fulfil-1` (against v2).

## Expected flags, v1 → v2

| Memory | Flagged? | Why |
|---|---|---|
| S1 | **Yes** | `createCharge` (`POST /charges`) is removed (breaking: `operation_removed`). The memory is false in v2, and acting on it ships unpaid goods. |
| S2 | **Yes** | The request it describes (`ChargeRequest`: amount, currency, card_token) is removed with the operation (review: `type_removed`), and the replacement confirm step needs a `return_url` the memory does not know. Confirmations attempted on S2 fail. |
| S3 | No | `getCharge`, its parameter, its 200 and 404 responses, and `Charge` are identical in both versions. |
| S4 | No | `createRefund`, `RefundRequest` and `Refund` are identical in both versions. |

The flag set is `{S1, S2}`: two flags and no false alarms.

One trap for the flagging code, mirroring `testdata/README.md`: `Charge` is referenced by v2's unchanged `getCharge` 200 response. The change belongs to the removed `createCharge` operation, not to `Charge`. A memory citing `Charge` or `getCharge` must not be flagged, even though the removed request type sat beside it in v1.
