# Spec: Backfill CLI for Polymarket logs

## Goal

Add a `backfill` CLI binary that ingests Polymarket logs for a block range and writes Parquet files per event type.

## Requirements

- Input: start/end block range.
- Output: Parquet files under `output/<EventType>/`.
- File naming: `{start}-{end}.parquet` where start/end are the block range covered by that file.
- Data sorted by `block_number`, then `log_index` ascending inside each file.

## Controls

- `--chunk-size-by-block-number` (default: 1000)
- `--parallelism` (default: 2)
- `--rate-limit-per-second` (default: 15)

## Implementation

- New binary: `src/bin/backfill.rs`.
- Use `get_logs` with `Filter` from `start` to `end` for each chunk.
- Use topic0 dispatch to decode events.
- For each chunk:
  - Group decoded rows by event type.
  - Sort rows by `block_number`, `log_index`.
  - Write a single Parquet file per event type named `{chunk_start}-{chunk_end}.parquet`.

## Schema

Each file uses a schema with base columns plus event-specific columns:

- Base: `event_type`, `block_number`, `transaction_hash`, `log_index`
- Event columns: per the live indexer schema (Utf8 strings).

## Rate Limiting

Use a token bucket with a per-second refill to cap RPC calls.

## Logging

Use `tracing` for progress logs per chunk.
