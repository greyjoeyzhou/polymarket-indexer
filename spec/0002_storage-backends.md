# Spec: Pluggable storage backends (Parquet + Kafka Avro)

## Goal

Provide a generic storage interface with two implementations:

1) Parquet files under `output/`, with one subdirectory per event type.
2) Kafka topics with Avro-encoded messages, one topic per event type.

Design must allow additional write destinations in the future.

## Storage Interface

Create a trait in `src/storage.rs`:

```
async fn write_event(
  event_type,
  block_number,
  tx_hash,
  log_index,
  columns
) -> Result

async fn rotate() -> Result
async fn close() -> Result
```

The `columns` parameter is a list of `(column_name, value_string)` pairs.

## Parquet Storage

- Each event type has its own subdirectory under `output/`.
- Schema: `event_type`, `block_number`, `transaction_hash`, `log_index` + event-specific columns.
- Event-specific columns are Utf8 strings in the order provided by `columns`.
- Schema is cached per event type and must not change within a run.
- Rotation closes open writers and starts a new file set.

## Kafka Storage (feature-gated)

- Topic name: `{topic_prefix}.{event_type}`.
- Message encoded as Avro, using a per-event schema.
- Schema fields: metadata columns + event-specific fields (Utf8 string type).
- Schema is cached per event type and must not change within a run.
- Rotation is a no-op for Kafka.
- Close flushes pending messages (await producer futures if needed).
- Feature flag: `kafka` for optional dependencies (`rdkafka`, `apache-avro`).

## CLI / Config

Add CLI options:

- `--sink <parquet|kafka>` (default: `parquet`)
- `--kafka-brokers <list>` (required when sink is kafka)
- `--kafka-topic-prefix <prefix>` (default: `polymarket`)

Also allow env vars via clap `env` feature:

- `SINK`, `KAFKA_BROKERS`, `KAFKA_TOPIC_PREFIX`

## Tests

- Keep unit tests for Parquet schema/row group behavior.
- Kafka tests are skipped unless the `kafka` feature is enabled.
