---
type: reference
description: Bellflower Market payments API
---

- POST /charges captures the payment at once; a 200 means the customer paid, fulfill the order
- Creating a payment needs only amount, currency, card_token
- GET /charges/{id} returns 404 for an unknown charge id
- POST /refunds with charge_id and amount returns the money
