# Polymarket Event Indexer (MVP)

This is a Rust-based indexer for Polymarket contracts on Polygon with two ingestion modes:

- Frontfill (live streaming via WebSocket)
- Backfill (historical range ingestion via HTTP `eth_getLogs`)

Shared ingestion/decode/storage logic lives in the library, while binaries are thin CLI wrappers.

## Features

-   **Live Streaming**: Connects to Polygon via WebSocket.
-   **Historical Backfill**: Fetches logs over block ranges via HTTP RPC.
-   **Event Decoding**: Decodes events from:
    -   Conditional Tokens Framework (CTF)
    -   CTF Exchange
    -   NegRiskAdapter
-   **Pluggable Sinks**: Generic sink interface with Parquet support and optional Kafka support.

## Prerequisites

-   Rust (latest stable)
-   `just` (command runner for development lifecycle)
-   `libssl-dev` and `pkg-config` (for OpenSSL)

## Setup

1.  Clone the repository.
2.  Install dependencies:
    ```bash
    just build
    ```

## Architecture

-   **Thin binaries**
    - `src/main.rs`: frontfill CLI wrapper
    - `src/bin/backfill.rs`: backfill CLI wrapper
-   **Shared library modules**
    - `src/frontfill.rs`: live ingestion runtime
    - `src/backfill.rs`: backfill runtime
    - `src/contracts.rs`: event declarations and contract constants
    - `src/decode.rs`: topic0 routing and event normalization
    - `src/storage.rs`: sink trait + implementations

## Development Commands

Use `just` as the primary interface for development workflows:

```bash
just               # list available recipes
just check         # fast compile/type validation
just fmt           # format code
just clippy        # lint all targets with warnings denied
just test          # run all tests
just coverage      # coverage summary
just coverage-lcov # write coverage/lcov.info
just coverage-html # write coverage/html
just build         # debug build
just build-release # release build
```

## Running Frontfill

```bash
just run-release
```

Starts live streaming and writes output according to sink configuration.

## Running Backfill

```bash
just run-backfill -- --start-block <START> --end-block <END>
```

Example:

```bash
just run-backfill -- --start-block 68000000 --end-block 68001000 --output-dir output
```

## Output Structure

The output is organized by event type:

```
output/
  ├── ConditionPreparation/
  │   └── <timestamp>.parquet
  ├── OrderFilled/
  │   └── <timestamp>.parquet
  ├── PositionSplit/
  │   └── <timestamp>.parquet
  ...
```

Each Parquet file contains normalized metadata columns and event-specific columns:

-   `event_type`: The name of the event.
-   `block_number`: The block number where the event occurred.
-   `transaction_hash`: The transaction hash.
-   `log_index`: The log index within the block.
-   Event-specific decoded columns (for example `order_hash`, `maker`, `taker`, etc).

## Configuration

Common configuration is exposed through CLI flags and env vars (Clap `env` bindings). Key examples:

- `RPC_URL` for frontfill WebSocket RPC
- `RPC_AUTH_KEY`, `RPC_AUTH_HEADER` (default `Authorization`), and `RPC_AUTH_SCHEME` (default `Bearer`) for frontfill WebSocket header auth
- `METRICS_ENABLED` (`false` by default), `METRICS_BIND` (`127.0.0.1` by default), and `METRICS_PORT` (`9090` by default) for optional frontfill Prometheus endpoint
- `RPC_HTTP_URL` and optional `RPC_HTTP_KEY` for backfill HTTP RPC
- `RPC_HTTP_AUTH_HEADER` (default `Authorization`) and `RPC_HTTP_AUTH_SCHEME` (default `Bearer`) for header-based auth
- `OUTPUT_DIR` for Parquet output
- `FLUSH_BLOCKS` (frontfill rotation)
- `CHUNK_SIZE_BY_BLOCK_NUMBER`, `PARALLELISM`, and `RATE_LIMIT_PER_SECOND` (backfill)

When `RPC_HTTP_KEY` is set for backfill, the key is sent via HTTP headers (not URL query params).
When `RPC_AUTH_KEY` is set for frontfill, the key is sent via the WebSocket `Authorization` header.

When metrics are enabled for frontfill, scrape:

```bash
curl "http://127.0.0.1:9090/metrics"
```
