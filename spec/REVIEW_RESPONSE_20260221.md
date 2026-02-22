# Review Response

This document responds to findings in `CODEBASE_REVIEW_20260216.md` with a validated triage and an execution scope.

## Validated Triage

### Must Fix

1. Backfill chunk failures are logged but do not fail the run (`src/backfill.rs`), which can hide data gaps.
2. Missing log metadata is currently defaulted in both runtimes (`unwrap_or` / `unwrap_or_default`), which can fabricate values.

### Should Fix

1. Frontfill starts from `latest` only and has no configurable handoff block.
2. Frontfill does not reconnect when the subscription stream ends.
3. No health/readiness endpoint is exposed; only `/metrics` is available.

### Nice to Have

1. Backfill rate limiter refill task has no explicit shutdown signal.
2. Small-file proliferation from current chunk/rotation strategy.
3. Schema evolution strategy beyond strict mismatch rejection.

### Clarifications on Review Claims

1. Backfill block fallback is `chunk_start`, not `0`.
2. Kafka `Timeout::After(0s)` is a high-risk fail-fast timeout configuration; it is not strictly equivalent to guaranteed silent drops.

## Execution Scope

This execution focuses on the highest-risk correctness and operability items:

1. Make backfill return an error when chunks fail.
2. Reject logs with missing required metadata rather than fabricating values.
3. Add frontfill start-block configuration and websocket reconnection.
4. Add `/health` and `/ready` endpoints based on websocket state and log recency.

Out-of-scope for this pass: persistent cursor storage, compaction tooling, and schema versioning.
