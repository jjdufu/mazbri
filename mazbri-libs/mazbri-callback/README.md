# mazbri-callback

Tenant/product event callback delivery: subscriptions, signed HTTP POSTs, retry, and delivery logs.

## Capabilities (closed for H-25)

| Piece | Behavior |
|-------|----------|
| Subscriptions | CRUD via mgmt API (`/callbacks/...`) |
| Delivery | Gateway / workers enqueue; `CallbackWorker` polls and POSTs |
| Signature | HMAC headers (see signer); timestamp skew window enforced |
| Retry | `retry_policy.max_retries` + exponential backoff |
| Terminal fail | `delivery_logs.status = failed` + error-level log for ops scrape |

## Operator notes

1. Create a subscription with target URL and secret.
2. Events produce delivery rows; worker signs and POSTs.
3. Failures retry until `max_retries`, then mark **failed**.
4. Ops: query `callback_delivery_logs` (or product UI if present) for `status=failed`.
5. Optional: ship failed-log metrics/logs to the same Greptime path used by `mazbri-scripts/alerts` (`WEBHOOK_URL`).

This is **not** a full notification product; it is a reliable callback pipe with audit-friendly failure marking.
