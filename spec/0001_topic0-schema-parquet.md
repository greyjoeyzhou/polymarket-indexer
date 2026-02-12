# Spec: Topic0-based dispatch + schemaed parquet columns

## Goal

Update event decoding to choose the event type based on `topic0` (event signature hash), and store decoded fields in dedicated Parquet columns instead of a single JSON payload column.

## Scope

- Replace address-based branching with topic0 matching in `src/main.rs`.
- Keep metadata columns: `event_type`, `block_number`, `transaction_hash`, `log_index`.
- Add per-event columns for decoded fields (Utf8 string columns) in `src/storage.rs`.
- Continue to write one Parquet file per event type and rotation set.
- Keep existing rotation + graceful shutdown behavior.

## Event Routing

Match `log.topics()[0]` (topic0) against each event signature hash and decode with `log.log_decode::<Event>()`.

Events:

- `ConditionPreparation`
- `PositionSplit`
- `PositionsMerge`
- `PayoutRedemption`
- `OrderFilled`
- `OrdersMatched`
- `OrderCancelled`
- `MarketPrepared`
- `QuestionPrepared`
- `PositionsConverted`

Unknown `topic0` values should be ignored (no write).

## Column Schema

All event-specific columns are stored as Utf8 strings.
Use serde conversion for values:

- If the serialized value is a JSON string, store its inner string.
- Otherwise store the JSON representation (numbers, arrays, objects).

### ConditionPreparation

Columns: `condition_id`, `oracle`, `question_id`, `outcome_slot_count`

### PositionSplit

Columns: `collateral_token`, `parent_collection_id`, `condition_id`, `partition`, `amounts`, `amount`

### PositionsMerge

Columns: `collateral_token`, `parent_collection_id`, `condition_id`, `partition`, `amounts`, `amount`

### PayoutRedemption

Columns: `redeemer`, `collateral_token`, `parent_collection_id`, `condition_id`, `index_sets`, `amount`

### OrderFilled

Columns: `order_hash`, `maker`, `taker`, `maker_fill_amount`, `taker_fill_amount`, `maker_fee`, `taker_fee`

### OrdersMatched

Columns: `taker_order_hash`, `maker_order_hash`

### OrderCancelled

Columns: `order_hash`

### MarketPrepared

Columns: `condition_id`, `creator`, `market_id`, `data`

### QuestionPrepared

Columns: `question_id`, `condition_id`, `creator`, `data`

### PositionsConverted

Columns: `condition_id`, `stakeholder`, `amount`, `fee`

## Storage Behavior

- `ParquetStorage::write_event` accepts ordered columns for the event.
- Schema for an event type is created once and reused; mismatched columns should be an error.
- The old `data` JSON column is removed from the schema.

## Tests

- Unit tests in `src/storage.rs` should validate row group count and rotation.
- Update tests to use event types that exist with the new column list.
