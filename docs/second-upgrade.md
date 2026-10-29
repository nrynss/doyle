# A second real upgrade

product.md's open question: a second real public API upgrade with visible
stakes (payments, auth, personal data) where a memory written against the old
version would cause harm. Three candidates below, all verified against live
public sources. The recommendation is A. The final pick is the user's
decision; this document recommends, it does not choose.

## Candidate A (recommended): Stripe Charges → PaymentIntents — payments

- **What it is.** Stripe's cards and Charges API integration migrates to the
  PaymentIntents API, driven by Strong Customer Authentication: a created
  payment needs customer confirmation (3D Secure) before money moves, and
  fulfilment moves from "if charge succeeds" to "on the
  `payment_intent.succeeded` webhook".
- **Public sources of both versions.**
  - Migration guide (the v1 → v2 story): <https://docs.stripe.com/payments/payment-intents/migration>
  - PaymentIntents API (v2): <https://docs.stripe.com/api/payment_intents>
  - Charges API (v1): <https://docs.stripe.com/api/charges>
- **What changed, in Vimanam terms.** The synchronous capture operation is
  removed (`operation_removed`, breaking); new operations for create, confirm
  and read are added (`operation_added`, non-breaking each); the old read path
  stays so both integrations read charges the same way. No pagination.
- **Visible stakes.** Real money movement. SCA non-compliance, lost payments,
  goods shipped for orders that never paid.
- **Plausible stale-memory harm.** "A 200 from the create call means the
  customer paid; fulfill the order." Under v2 a 200 means
  `requires_confirmation`: acting on it ships goods the shop never gets paid
  for, with no payment to refund or dispute. Silent harm — every check passes —
  exactly Doyle's shape.

## Candidate B: GitHub OAuth Authorizations API removal — auth, credentials

- **What it is.** The endpoints that create OAuth tokens with Basic
  authentication (username + password, `POST /authorizations`) were
  discontinued; integrations must use the web application flow.
- **Public sources.**
  - REST reference carrying the removal notice (13 Nov 2020): <https://docs.github.com/en/rest/oauth-authorizations/oauth-authorizations>
- **What changed, in Vimanam terms.** Token-creation operations removed
  (`operation_removed`, breaking); the password-carrying request shapes go
  with them. What remains (grants listing, deletion) is unchanged.
- **Visible stakes.** Credentials in scripts: the old flow teaches agents to
  handle raw usernames and passwords; the new flow never takes them.
- **Plausible stale-memory harm.** "Mint a token with `POST /authorizations`
  and the user's password." Under the new API the endpoint answers 410 Gone,
  so automation built on the memory breaks — loudly, not silently — and every
  retry risks another password in a log. Real, but a loud failure: less
  Doyle-shaped than A, where everything looks right while value is lost.

## Candidate C: Google OAuth out-of-band flow deprecation — auth, account data

- **What it is.** Google ended the OAuth out-of-band (OOB) flow, where the
  user copies a code from the browser into the app (`redirect_uri`
  `urn:ietf:wg:oauth:2.0:oob`), as a phishing risk; desktop apps move to
  loopback or the installed-app flow.
- **Public sources.**
  - Deprecation announcement with dates (new clients blocked 28 Feb 2022, existing clients 3 Oct 2022): <https://developers.googleblog.com/2022/02/making-oauth-flows-safer.html>
- **What changed, in Vimanam terms.** A redirect option on the authorization
  endpoint is rejected: a constraint tightening on one parameter (review or
  breaking depending on modelling), everything else unchanged.
- **Visible stakes.** Account access and the personal data behind it; the OOB
  flow's manual code step is exactly what phishing imitates.
- **Plausible stale-memory harm.** "Authenticate by sending the user to the
  OOB URL and asking for the code." Under the new rules the request is
  blocked with an error page, so sign-in breaks — again loudly — and an agent
  following the memory trains users to paste codes, the habit the removal was
  meant to kill.
- **Why not.** The "contract" is one endpoint parameter on an OAuth flow,
  awkward to capture as a faithful one-screen OpenAPI pair, and the failure
  is loud. Weaker fixture, weaker demo.

## Recommendation

Candidate A. Reasons: the stakes are money, the most legible of the three;
the harm is silent (ship-then-never-paid), which is the case Doyle exists
for; and the change has the same fixture shape as the Quiet Migration — one
breaking removal, additive additions, and unchanged operations as controls —
so the flagging rules in `docs/architecture.md` are tested, not bent. B and C
stay on the list as evaluation cases: both are real, both are auth, and both
fail loudly, which is a useful contrast for the evaluation (flagged, but the
world also tells the agent).

## The captured pair

`testdata/contracts/second/`: `second-v1.yaml` and `second-v2.yaml` are
hand-written summaries in the style of `patients-v*.yaml`, faithful only in
the changed operations, with invented names throughout (Bellflower Market).
`memories.md` holds four notes in Muse Code format; `README.md` in that
directory tells the unpaid-shipment story, the memory table, and the expected
flag set `{S1, S2}`.

## Recorded Vimanam output for the captured pair

Vimanam **2.0.0** (crates.io), installed into a scratch root outside the
repository, never global — the T0.6 pattern:

```
cargo install vimanam --version 2.0.0 --locked --root <scratch>/vimanam
<scratch>/vimanam/bin/vimanam --version        # vimanam 2.0.0
```

CLI (run from the repository root):

```
vimanam diff testdata/contracts/second/second-v1.yaml testdata/contracts/second/second-v2.yaml --format json -o testdata/contracts/second/second-v1-v2.json
vimanam diff testdata/contracts/second/second-v1.yaml testdata/contracts/second/second-v2.yaml -o testdata/contracts/second/second-v1-v2.md
```

Library: a scratch Cargo project outside the repository (not committed) with
`vimanam = { version = "=2.0.0", default-features = false }`. It calls
`vimanam::parse_openapi(path)` for the `ApiDocumentation`,
`vimanam::contract::openapi::parse(path)` for the `Contract` (entities), and
`vimanam::contract::openapi::diff(&old, &new)` for the changes. Output in
`testdata/contracts/second/`: `entities-v1.json` (29 entities),
`entities-v2.json` (47 entities), `changes-v1-v2.json`.

The library diff and the CLI JSON agree: the same 8 change IDs in the same
order. Summary: 3 endpoints added, 1 removed, 0 changed; 1 breaking,
6 non-breaking, 1 review.

### The change facts

Rows are in the order the CLI prints them. IDs abbreviated to 13 characters;
full IDs are in `second-v1-v2.json`.

| # | Change | Subject | Kind | Severity | Native kind |
|---|---|---|---|---|---|
| 1 | `vc2_cf9ac926b…` | `createCharge` (`POST /charges`) | `operation_removed` | breaking | `endpoint_removed` |
| 2 | `vc2_ef929def3…` | `createPayment` (`POST /payments`) | `operation_added` | non_breaking | `endpoint_added` |
| 3 | `vc2_cb29fb3d9…` | `confirmPayment` (`POST /payments/{id}/confirm`) | `operation_added` | non_breaking | `endpoint_added` |
| 4 | `vc2_f67305928…` | `getPayment` (`GET /payments/{id}`) | `operation_added` | non_breaking | `endpoint_added` |
| 5 | `vc2_e84251ba4…` | `ChargeRequest` (type) | `type_removed` | review | `surface_changed` |
| 6 | `vc2_23137fa1e…` | `PaymentRequest` (type) | `type_added` | non_breaking | `surface_changed` |
| 7 | `vc2_e8777102c…` | `ConfirmRequest` (type) | `type_added` | non_breaking | `surface_changed` |
| 8 | `vc2_aed11226a…` | `Payment` (type) | `type_added` | non_breaking | `surface_changed` |

**Which changes flag which memories** (under `docs/architecture.md`):

- S1 (`createCharge`, its 200 response) is flagged by row 1, breaking, subject the operation S1 cites.
- S2 (the create-call request shape) is flagged by row 5, review, subject `ChargeRequest`, the type S2's claim rests on.
- `getCharge`, `Charge`, `createRefund`, `RefundRequest` and `Refund` have no change facts in their subtrees, so S3 and S4 flag nothing. `Charge` is referenced by the unchanged `getCharge` response; the removal belongs to the operation, not the type — the shared-`$ref` trap from `testdata/README.md`, mirrored.
- Rows 2–4 and 6–8 are additive and flag nothing on their own.

**Markdown vs JSON, again.** The CLI Markdown report prints "1 breaking,
3 non-breaking, 0 to review" and lists only the endpoint rows. The JSON
summary says 6 non-breaking and 1 review: the Markdown omits the four
type-level facts (rows 5–8). Same gap T0.6 filed for the patients pair — the
JSON is the source of truth for the flagging code.

### Entity IDs for the later tasks

Stable across v1 and v2. New v2 entities marked (*).

| Name | Kind | ID |
|---|---|---|
| `createCharge` | operation, `POST /charges` (removed in v2) | `ve2_c48f48c1b04af8187da6a8b1e7323dfa4f119dab72ba947f8385262e464862f7` |
| `createPayment` (*) | operation, `POST /payments` | `ve2_896cbf231c3bf5fc7957730d1d54b7dce14926e3bc17895f095d52c956377884` |
| `confirmPayment` (*) | operation, `POST /payments/{id}/confirm` | `ve2_80b3aaf586772b3de81d40be7b577422e5d17d30ee59b8836ab5778d0ad40f98` |
| `getPayment` (*) | operation, `GET /payments/{id}` | `ve2_9701d0fd289ff395e704c587882826e87b209ea2fbc145d2db150e298a0de73d` |
| `getCharge` | operation, `GET /charges/{id}` | `ve2_3ac3b84b85fc19acb662f0525768c123b1f62b1aa25401d93a4bbe86bfee3bc5` |
| `createRefund` | operation, `POST /refunds` | `ve2_77adf5d29d567ad79d84f58999ccf59e09c6c35620fbd6a3d0fe9c5dae96304b` |
| `Payment` (*) | type | `ve2_3a9dab31baa21d9cb1e7393a662fe4ed92888395de26a5c317ae95c291a6e82f` |
| `Charge` | type | `ve2_767cf91906c38274325472d9c1feeaf7bc09c4ba22ce73279055c0830880a5d7` |

## Open issues

- The Markdown/JSON gap above repeats T0.6's Vimanam finding on a second pair; the handoff log carries the issue text.
- The user still picks the evaluation upgrade (product.md open question). Recommended: A.
