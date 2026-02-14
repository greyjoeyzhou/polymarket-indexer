# Spec: Frontfill Prometheus Metrics Endpoint

## Goal

Add an optional Prometheus-scrapable metrics endpoint to the frontfill ingestion CLI.
The endpoint must expose:

- current processed block number,
- number of logs per minute (total),
- number of logs per minute per event type,
- and additional operational metrics useful for production monitoring.

## Scope

In scope:

- frontfill CLI config surface (`src/main.rs`)
- frontfill runtime instrumentation (`src/frontfill.rs`)
- new metrics module for registration, updates, and serving (`src/metrics.rs`)
- dependency additions and tests

Out of scope:

- backfill metrics endpoint
- external dashboards/alerts provisioning

## Current Baseline

- No metrics crates or `/metrics` server currently exist.
- Runtime observability relies on `tracing` logs.
- Frontfill loop already has clear instrumentation points:
  - log receipt
  - decode result
  - sink write success/failure
  - rotation
  - shutdown path

## Metrics Design

Namespace prefix: `polymarket_frontfill_*`

### Required metrics

1. `polymarket_frontfill_current_block_number` (gauge)
   - latest seen `block_number` from the stream.

2. `polymarket_frontfill_logs_total` (counter)
   - total ingested logs.

3. `polymarket_frontfill_logs_by_type_total{event_type="..."}` (counter vec)
   - cumulative ingested logs by decoded event type.

4. `polymarket_frontfill_logs_per_minute` (gauge)
   - sliding 60-second total logs rate represented as logs/minute.

5. `polymarket_frontfill_logs_per_minute_by_type{event_type="..."}` (gauge vec)
   - sliding 60-second logs/minute by event type.

### Additional valuable metrics

6. `polymarket_frontfill_decode_errors_total` (counter)
7. `polymarket_frontfill_write_errors_total` (counter)
8. `polymarket_frontfill_rotate_total` (counter)
9. `polymarket_frontfill_shutdown_total` (counter)
10. `polymarket_frontfill_sink_write_seconds` (histogram)
11. `polymarket_frontfill_ws_connected` (gauge: 0/1)
12. `polymarket_frontfill_last_log_unix_seconds` (gauge)

## Library and Endpoint Approach

Use a lightweight Prometheus path suitable for a CLI daemon:

- `prometheus` crate for counters/gauges/histograms.
- minimal Tokio-native HTTP exposure for `/metrics` only.

Endpoint behavior:

- path: `/metrics`
- returns Prometheus text exposition format
- binds only when metrics are enabled

## CLI and Env Additions

Add frontfill options:

- `--metrics-enabled` (`METRICS_ENABLED`, default `false`)
- `--metrics-bind` (`METRICS_BIND`, default `127.0.0.1`)
- `--metrics-port` (`METRICS_PORT`, default `9090`)

Config wiring:

- extend `FrontfillConfig` with metrics options
- parse in `main.rs`, pass through to `frontfill::run`

## Runtime Instrumentation Plan

In `frontfill::run`:

1. Initialize metrics registry/state at startup if enabled.
2. Start metrics endpoint task.
3. Update metrics in loop:
   - on log receive: current block, total logs, last log timestamp
   - on decode success: per-type counters and per-minute trackers
   - on decode error: decode error counter
   - around write_event: write latency histogram and write error counter
   - on rotate: rotate counter
4. On shutdown signal:
   - increment shutdown counter
   - mark ws connected gauge to 0
   - stop endpoint task gracefully

## Logs-Per-Minute Computation

To satisfy explicit per-minute metric requirement, compute application-side 60-second windows:

- Maintain timestamped counters for:
  - total logs,
  - per event type logs.
- On each update, evict entries older than now-60s.
- Publish gauges:
  - `logs_per_minute`
  - `logs_per_minute_by_type{event_type}`

Implementation detail:

- Keep metric state behind `Arc<Mutex<...>>` to match current repository pattern.

## Files to Modify

- `Cargo.toml` (metrics dependencies)
- `src/main.rs` (CLI/env options and config mapping)
- `src/frontfill.rs` (instrumentation + endpoint lifecycle)
- `src/lib.rs` (export new module)
- `src/metrics.rs` (new)
- `README.md` (document metrics endpoint options)
- `AGENTS.md` (document frontfill metrics conventions)

## Test Plan

### Unit tests

- CLI parsing defaults and override behavior for metrics args (`src/main.rs`).
- metrics state updates:
  - total/per-type counters,
  - per-minute gauge updates,
  - stale window eviction,
  - error counters.
- endpoint response contains expected metric names.

### Runtime tests (frontfill)

- metrics disabled: no endpoint startup, run behavior unchanged.
- metrics enabled + invalid bind/port: startup error path covered.
- instrumentation branches:
  - decode error increments metric,
  - write error increments metric,
  - rotate increments metric.

## Verification

After implementation:

- `just fmt-check`
- `just check`
- `just clippy`
- `just test`
- `just build`

Manual smoke:

1. run frontfill with metrics enabled
2. `curl http://<bind>:<port>/metrics`
3. verify required metric names appear

## Rollout Notes

- Feature is opt-in by default (`metrics_enabled = false`).
- Existing frontfill behavior remains unchanged when disabled.
- Prometheus can compute rate-based views from counters; app-side per-minute gauges are included to satisfy direct requirement.
