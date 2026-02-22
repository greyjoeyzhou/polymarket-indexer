# Polymarket Indexer - Codebase Review

## 1. Architecture, Components, and Interactions

### High-Level Overview

The polymarket-indexer is a Rust-based blockchain event indexer for Polymarket contracts deployed on Polygon. It operates in two distinct modes:

- **Frontfill**: Real-time event streaming via WebSocket subscriptions
- **Backfill**: Historical event ingestion via HTTP `eth_getLogs` RPC calls

The architecture follows a **thin-binary + shared-library** pattern. CLI binaries handle argument parsing and bootstrapping, while all domain logic lives in reusable library modules.

### Component Map

```
                        ┌──────────────────────────┐
                        │    Polygon Blockchain     │
                        │    (3 Smart Contracts)    │
                        └────────┬────────┬────────┘
                                 │        │
                        WebSocket│        │HTTP RPC
                                 │        │
                   ┌─────────────┴─┐  ┌───┴──────────────┐
                   │  main.rs      │  │  bin/backfill.rs  │
                   │  (CLI args)   │  │  (CLI args)       │
                   └───────┬───────┘  └───────┬───────────┘
                           │                  │
                   ┌───────┴───────┐  ┌───────┴───────────┐
                   │ frontfill.rs  │  │   backfill.rs      │
                   │ (WS stream    │  │ (HTTP chunked      │
                   │  + rotation)  │  │  fetch + parallel)  │
                   └───────┬───────┘  └───────┬───────────┘
                           │                  │
                   ┌───────┴──────────────────┴───────────┐
                   │            decode.rs                  │
                   │   (topic0-based event routing)        │
                   └───────┬──────────────────┬───────────┘
                           │                  │
                   ┌───────┴───────┐  ┌───────┴───────────┐
                   │ contracts.rs  │  │   storage.rs       │
                   │ (ABI defs +   │  │ (EventSink trait)  │
                   │  addresses)   │  │                    │
                   └───────────────┘  ├──────────┬─────────┤
                                      │ Parquet  │  Kafka  │
                                      │  Sink    │  Sink   │
                                      └──────────┴─────────┘
                   ┌──────────────────────────────────────┐
                   │         metrics.rs (optional)         │
                   │   Prometheus /metrics HTTP endpoint   │
                   └──────────────────────────────────────┘
```

### Module Breakdown

| Module | Lines | Purpose |
|--------|-------|---------|
| `contracts.rs` | 71 | Solidity event ABI declarations via `sol!` macro; contract address constants |
| `decode.rs` | 492 | Topic0-based event routing; decodes raw logs into normalized `DecodedEvent` structs |
| `frontfill.rs` | 438 | WebSocket-based live streaming runtime with rotation and shutdown handling |
| `backfill.rs` | 509 | HTTP-based historical ingestion with chunking, parallelism, and rate limiting |
| `storage.rs` | 421 | `EventSink` trait + `ParquetStorage` and `KafkaStorage` (feature-gated) implementations |
| `metrics.rs` | 387 | Prometheus metric registration, rolling rate calculation, Axum HTTP server |
| `main.rs` | 128 | Frontfill CLI wrapper (Clap + env vars) |
| `bin/backfill.rs` | 87 | Backfill CLI wrapper (Clap + env vars) |

### Monitored Contracts and Events

Three Polymarket contracts on Polygon are monitored:

| Contract | Address | Events |
|----------|---------|--------|
| CTF (Conditional Tokens Framework) | `0x4D97DCd97eC945f40cF65F87097ACe5EA0476045` | `ConditionPreparation`, `PositionSplit`, `PositionsMerge`, `PayoutRedemption` |
| CTF Exchange | `0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E` | `OrderFilled`, `OrdersMatched`, `OrderCancelled` |
| NegRiskAdapter | `0xd91E80cF2E7be2e162c6513ceD06f1dD0dA35296` | `MarketPrepared`, `QuestionPrepared`, `PositionsConverted` |

### Data Flow

1. **Log Acquisition**: Raw EVM logs are fetched from the RPC provider (WebSocket subscription for frontfill, `eth_getLogs` batches for backfill)
2. **Event Decoding**: `decode.rs` matches the `topic0` hash against 10 known event signatures, then uses Alloy's type-safe `log_decode()` to extract fields
3. **Normalization**: Decoded fields are serialized to strings via `value_to_string()` (JSON for non-string types), producing a flat `Vec<(&str, String)>` column list
4. **Storage**: The `EventSink` trait dispatches to either Parquet files (directory-per-event-type) or Kafka topics (topic-per-event-type)

---

## 2. Implementation Details

### Frontfill Runtime (`frontfill.rs`)

- Connects to a Polygon WebSocket RPC endpoint with optional `Authorization` header auth
- Subscribes to logs from all three contract addresses starting from the latest block
- Processes logs sequentially in a `loop` with `tokio::select!` for graceful Ctrl+C shutdown
- **Rotation**: Every `flush_blocks` blocks (default 1000), closes current Parquet writers and opens new file sets
- **Metrics integration**: When enabled, observes block numbers, log counts, per-type rates, write latency, and WebSocket connection status
- **Error behavior**: Decode errors are fatal (terminates the loop and shuts down metrics). Write errors are also fatal. This is a deliberate fail-fast strategy

### Backfill Runtime (`backfill.rs`)

- Splits the `[start_block, end_block]` range into fixed-size chunks (default 1000 blocks)
- Processes chunks concurrently using `futures::stream::for_each_concurrent` with configurable parallelism (default 2)
- **Rate limiting**: Semaphore-based with a background refill task that restores permits every second. `permit.forget()` is used to consume permits without tying them to scope
- **Per-chunk processing**: Fetch logs, decode, group by event type, sort by (block_number, log_index), write one Parquet file per event type per chunk
- **Error behavior**: Fetch and decode failures are logged as warnings and skipped (non-fatal). Parquet write failures are also warnings. The backfill always runs to completion

### Storage Layer (`storage.rs`)

**`EventSink` trait** defines three async operations:
- `write_event()` - Write a single decoded event
- `rotate()` - Start a new output file set
- `close()` - Flush and close all writers

**`ParquetStorage`**:
- Organizes output into `{base_path}/{event_type}/` directories
- Creates ArrowWriter instances lazily on first write per event type
- File naming: `{timestamp_millis}_set{file_set_id}.parquet`
- Schema is auto-discovered from the first event of each type and then validated for consistency
- All event-specific columns are stored as `Utf8` (strings)
- Each `write_event()` call writes a single-row RecordBatch (no internal buffering)

**`KafkaStorage`** (behind `kafka` feature flag):
- Produces Avro-encoded messages to `{prefix}.{event_type}` topics
- Key: transaction hash
- Each message is self-contained (single Avro record per produce call)
- `rotate()` and `close()` are no-ops

### Decode Layer (`decode.rs`)

- Uses Alloy's `sol!` macro-generated `SIGNATURE_HASH` constants for topic0 matching
- All 10 event types have their own decode branch with explicit column mapping
- `value_to_string()` converts Alloy primitive types to strings: raw strings pass through, everything else gets JSON-serialized
- Returns `Ok(None)` for unrecognized topic0 values (graceful skip)

### Metrics (`metrics.rs`)

- 12 Prometheus metrics covering block tracking, throughput, errors, latency, and connection status
- Rolling 60-second window for per-minute rate calculation using `VecDeque<Instant>` with pruning
- Background task recalculates rates every 1 second
- Axum HTTP server on configurable bind/port exposes `/metrics` in Prometheus text format
- Both the rate updater and HTTP server support graceful shutdown via oneshot channels

### Configuration

All configuration is dual-sourced from CLI flags and environment variables via Clap's `env` binding:

| Variable | Mode | Default | Purpose |
|----------|------|---------|---------|
| `FLUSH_BLOCKS` | frontfill | 1000 | Blocks between Parquet file rotations |
| `RPC_URL` | frontfill | `wss://polygon-bor-rpc.publicnode.com` | WebSocket RPC endpoint |
| `RPC_AUTH_KEY` | frontfill | none | Bearer token for WS auth |
| `SINK` | frontfill | `parquet` | Output backend (`parquet` or `kafka`) |
| `METRICS_ENABLED` | frontfill | `false` | Enable Prometheus endpoint |
| `START_BLOCK` / `END_BLOCK` | backfill | required | Block range to index |
| `CHUNK_SIZE_BY_BLOCK_NUMBER` | backfill | 1000 | Blocks per RPC request |
| `PARALLELISM` | backfill | 2 | Concurrent chunk fetches |
| `RATE_LIMIT_PER_SECOND` | backfill | 15 | RPC request rate cap |
| `RPC_HTTP_URL` | backfill | `https://polygon-bor-rpc.publicnode.com` | HTTP RPC endpoint |

---

## 3. Potential Issues

### 3.1 Data Freshness

**Frontfill starts from `Latest` block only**
- The filter is set to `BlockNumberOrTag::Latest`, meaning any events between the last backfill and frontfill startup are missed. There is no mechanism to specify a starting block for frontfill, and no overlap/gap detection between backfill and frontfill runs.
- *Impact*: Data gaps between backfill end and frontfill start are silent and undetectable without external monitoring.

**No WebSocket reconnection logic**
- When the WebSocket stream ends (returns `None`), the frontfill loop simply breaks and exits cleanly (lines 161-164 of `frontfill.rs`). There is no automatic reconnection.
- *Impact*: Any transient WebSocket disconnection causes the indexer to stop. In production, this requires an external supervisor (systemd, Kubernetes) to restart the process, but events between disconnect and restart are lost.

**Rate limiter refill task runs forever**
- The `spawn_refill()` task in `backfill.rs` creates an infinite loop with no shutdown mechanism. It runs until the tokio runtime is dropped.
- *Impact*: Minimal since backfill is a bounded operation, but it's a resource leak in principle.

### 3.2 Data Consistency

**Backfill silently skips failed chunks**
- When `provider.get_logs()` fails for a chunk, the error is logged as a warning and the chunk is skipped (lines 242-248 of `backfill.rs`). The `run()` function still returns `Ok(())`.
- *Impact*: A completed backfill run may have gaps in coverage. There is no retry mechanism, no failure manifest, and no way to detect missing chunks without externally scanning the output directory.

**No deduplication between backfill and frontfill**
- If backfill and frontfill block ranges overlap (e.g., frontfill starts before backfill finishes), duplicate events will be written to separate Parquet files.
- *Impact*: Downstream consumers must handle deduplication, typically by `(block_number, transaction_hash, log_index)` composite key.

**Single-row RecordBatch writes in ParquetStorage**
- Each `write_event()` call creates a 1-row RecordBatch and writes it to the ArrowWriter. While ArrowWriter internally buffers row groups, this pattern generates significant per-row overhead in Arrow array creation.
- *Impact*: Frontfill write performance is suboptimal. Backfill avoids this by batch-writing all rows for an event type in a single RecordBatch.

**Schema mismatch is a hard error**
- If a contract upgrade changes event fields, `get_or_create_schema()` will return an error on the first log with mismatched columns, halting the frontfill entirely.
- *Impact*: Contract upgrades require indexer redeployment. There is no schema evolution or versioning strategy.

**Parquet file naming collision risk**
- Frontfill uses `{timestamp_millis}_set{file_set_id}.parquet`. If the process restarts within the same millisecond (unlikely but possible in tests or rapid restarts), files may collide.
- *Impact*: Very low probability in production, but could cause data loss on file overwrite.

### 3.3 Runtime Stability

**`unwrap_or(0)` for block metadata**
- `block_number`, `log_index`, and `transaction_hash` are extracted with `unwrap_or(0)` / `unwrap_or_default()` (frontfill.rs lines 176-178, backfill.rs lines 262-264).
- *Impact*: If the RPC returns a log with missing metadata (e.g., pending transactions), the event will be stored with fabricated metadata (block 0, log index 0, zero hash). This creates silent data corruption that is hard to detect.

**Frontfill decode errors are fatal**
- A single decode error terminates the entire frontfill process (lines 200-212 of `frontfill.rs`). This is intentional fail-fast behavior, but a malformed log from the RPC (not a contract issue, but a provider bug) would halt indexing.
- *Impact*: Aggressive failure mode may cause unnecessary downtime. Consider whether decode errors for unexpected logs (not matching any known event) should be non-fatal.

**Mutex contention on shared sink**
- The frontfill uses `Arc<Mutex<Box<dyn EventSink>>>` for the storage backend. The mutex is held during both rotation checks and event writes (lines 184-255 of `frontfill.rs`).
- *Impact*: Since frontfill processes logs sequentially, this is not a bottleneck currently. However, if the design evolves toward concurrent log processing, the mutex becomes a serialization point.

**Kafka timeout of 0 seconds**
- `KafkaStorage` uses `Timeout::After(Duration::from_secs(0))` for produce calls (storage.rs line 314). This means the producer will not wait for delivery confirmation.
- *Impact*: Under Kafka broker pressure, messages may be silently dropped. The `FutureProducer::send()` returns immediately, and delivery failures surface only if the internal queue is full.

**No health check endpoint**
- The metrics server provides `/metrics` but no `/health` or `/ready` endpoint.
- *Impact*: Kubernetes liveness/readiness probes must rely on the metrics endpoint or port connectivity rather than application-level health status (e.g., WebSocket connected, recent log received).

### 3.4 Operational Concerns

**No persistent cursor or checkpoint**
- Neither frontfill nor backfill persists a cursor (last processed block number). On restart, frontfill starts from the latest block, and backfill requires explicit block range arguments.
- *Impact*: Operational overhead to manage block ranges externally. Gap-free ingestion requires careful orchestration.

**Backfill always returns success**
- Even if every chunk fails, `backfill::run()` returns `Ok(())`. The caller (CLI) exits with code 0.
- *Impact*: CI/CD pipelines or monitoring that rely on exit codes cannot detect backfill failures.

**Parquet files are never compacted**
- Frontfill creates a new file set every `flush_blocks` blocks, and backfill creates one file per event type per chunk. Over time, this generates many small files.
- *Impact*: Query performance degrades with many small Parquet files. Downstream systems need compaction or merge jobs.

**No backpressure on frontfill**
- If Parquet writes become slow (e.g., disk I/O saturation), the WebSocket subscription buffer will grow unbounded until the provider disconnects.
- *Impact*: Memory usage can spike under write pressure, potentially leading to OOM before graceful shutdown.

---

## 4. Summary

### Strengths

- Clean separation of concerns with thin CLIs and focused library modules
- Type-safe event decoding via Alloy's `sol!` macros eliminates ABI parsing errors
- Pluggable sink interface makes adding new backends straightforward
- Comprehensive unit and integration tests (including Kafka with testcontainers)
- Modern Rust practices: 2024 edition, structured tracing, async/await, no unsafe code
- Well-documented configuration via Clap env bindings with sensible defaults

### Key Risks to Address

1. **Data gaps**: No overlap detection or cursor persistence between backfill and frontfill
2. **Silent failures in backfill**: Failed chunks are skipped without affecting the exit code
3. **No reconnection**: WebSocket disconnections require external process supervision
4. **Fabricated metadata**: `unwrap_or(0)` on block metadata masks missing data
5. **Small file proliferation**: No compaction strategy for Parquet output

### Recommendations (Priority Order)

1. Add cursor/checkpoint persistence to enable gap-free restarts
2. Make backfill return an error (or non-zero exit code) when any chunks fail
3. Add WebSocket reconnection with configurable retry policy
4. Validate log metadata presence before writing (reject or flag logs with missing block_number/tx_hash)
5. Add a `/health` endpoint that reflects WebSocket connection state and recency of last log
6. Consider batching frontfill writes (buffer N events before flushing to Parquet)
7. Add a compaction utility or integrate with downstream merge jobs
