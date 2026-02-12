# Polymarket Event Indexer (MVP)

This is a Rust-based indexer that streams live events from Polymarket's smart contracts on Polygon and saves them as Parquet files.

## Features

-   **Live Streaming**: Connects to Polygon via WebSocket.
-   **Event Decoding**: Decodes events from:
    -   Conditional Tokens Framework (CTF)
    -   CTF Exchange
    -   NegRiskAdapter
-   **Parquet Storage**: Writes structured event data to Parquet files, partitioned by event type.

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

## Development Commands

Use `just` as the primary interface for development workflows:

```bash
just               # list available recipes
just check         # fast compile/type validation
just fmt           # format code
just clippy        # lint all targets with warnings denied
just test          # run all tests
just build         # debug build
just build-release # release build
```

## Running

```bash
just run-release
```

The indexer will start listening for events and write them to the `output/` directory.

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

Each Parquet file contains:
-   `event_type`: The name of the event.
-   `block_number`: The block number where the event occurred.
-   `transaction_hash`: The transaction hash.
-   `log_index`: The log index within the block.
-   `data`: The JSON-serialized event data.

## Configuration

The contract addresses and RPC URL are defined in `src/main.rs`. You can modify `RPC_URL` to use your own Polygon RPC provider for better stability.
