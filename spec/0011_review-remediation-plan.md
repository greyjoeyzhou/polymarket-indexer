# 0011 Review Remediation Plan

## Goal

Implement the highest-impact fixes identified during review validation to improve correctness, failure signaling, and runtime operability.

## Scope

### In Scope

1. Backfill should fail fast at end-of-run if any chunk fetch/write/metadata validation fails.
2. Frontfill and backfill must stop fabricating metadata from missing fields.
3. Frontfill should support optional start block for safer handoff from backfill.
4. Frontfill should reconnect on websocket stream termination with bounded backoff.
5. Metrics server should expose `/health` and `/ready` derived from connection + freshness signals.

### Out of Scope

1. Persistent cursor/checkpoint storage.
2. File compaction pipeline.
3. Schema evolution/version migration framework.

## Implementation Plan

### Phase 1 - Backfill correctness signaling

1. Add per-chunk failure tracking in `src/backfill.rs`.
2. Capture failures for:
   - `get_logs` fetch errors
   - parquet write errors
   - missing required metadata (`block_number`, `transaction_hash`, `log_index`)
3. Return `Err(...)` from `backfill::run` when any failure is recorded.

### Phase 2 - Metadata integrity hardening

1. In `src/frontfill.rs`, replace metadata fallbacks with explicit validation and error on missing fields.
2. In `src/backfill.rs`, skip invalid logs and mark chunk failure for end-of-run failure summary.

### Phase 3 - Frontfill resilience and operability

1. Add optional frontfill start block config in `src/main.rs` and `src/frontfill.rs`.
2. Add websocket reconnect loop in `src/frontfill.rs` for stream termination.
3. Add health/readiness endpoints in `src/metrics.rs`:
   - `/health`
   - `/ready`
4. Use existing gauges (`ws_connected`, `last_log_unix_seconds`) for endpoint decisions.

## Verification

Run the standard repo checks after implementation:

1. `just fmt-check`
2. `just check`
3. `just clippy`
4. `just test`

If failures appear, iterate until green or report clear pre-existing issues.
