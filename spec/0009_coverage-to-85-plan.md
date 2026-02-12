# Spec: Unit Coverage Improvement Plan to 85%

## Goal

Raise project line coverage from current baseline to at least **85%** using primarily unit tests and lightweight module-level tests, while avoiding dependence on heavy integration tests.

## Baseline (Current)

Measured via `just coverage` (`cargo llvm-cov --summary-only`):

- Total line coverage: **46.15%**
- By module:
  - `src/storage.rs`: 95.42%
  - `src/main.rs`: 87.27%
  - `src/bin/backfill.rs`: 100%
  - `src/frontfill.rs`: 34.91%
  - `src/backfill.rs`: 21.01%
  - `src/decode.rs`: 0.00%
  - `src/contracts.rs`: 0.00%

Largest gaps are concentrated in `decode.rs`, `backfill.rs`, and `frontfill.rs`.

## Coverage Strategy

1. Prioritize high-branch, pure logic modules first (`decode.rs`, helper functions in `backfill.rs`, auth helpers in `frontfill.rs`).
2. Add deterministic unit tests for error paths and boundary conditions.
3. Keep runtime/event-loop tests scoped to validation and helper behaviors; avoid network-dependent tests.
4. Track coverage after each phase and adjust targets by module.

## Module-Level Targets

- `src/decode.rs`: 0% -> **90%+**
- `src/backfill.rs`: 21% -> **85%+**
- `src/frontfill.rs`: 35% -> **80%+**
- `src/contracts.rs`: 0% -> **80%+**
- Preserve existing high coverage in `storage.rs`, `main.rs`, `bin/backfill.rs`.

## Phased Plan

### Phase 1: Quick Unit Wins (fast ROI)

Scope:

- `src/contracts.rs`
- `src/decode.rs` (`topic0`, `value_to_string`)
- `src/backfill.rs` (`chunk_ranges`, auth/url helper edge cases)
- `src/frontfill.rs` (remaining auth builder branches)

Tests to add:

- `contracts::contract_addresses()` returns 3 valid addresses in expected order.
- `decode::topic0()` returns first topic and handles empty topic list.
- `decode::value_to_string()` for string, numeric, and structured values.
- `backfill::chunk_ranges()` for exact chunk fit, remainder chunk, and single-block span.
- `backfill::build_auth_headers()` invalid header name/value error paths.
- `frontfill::build_ws_auth()` custom scheme branch and empty key behavior.

Expected outcome:

- Lift total coverage into ~55-60% range.

### Phase 2: Decode Router Deep Coverage

Scope:

- `src/decode.rs`

Tests to add:

- `decode_log()` returns `Ok(None)` when:
  - no `topic0`
  - unknown event topic
- Happy-path decode tests for each supported event type:
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
- Verify decoded event type and ordered output columns for each.

Expected outcome:

- `decode.rs` reaches 85-90%+, total coverage ~68-72%.

### Phase 3: Backfill Core Runtime Logic (unit-focused)

Scope:

- `src/backfill.rs`

Tests to add:

- Validation failures in `run()`:
  - `start_block > end_block`
  - `chunk_size_by_block_number == 0`
  - `parallelism == 0`
  - `rate_limit_per_second == 0`
- `write_parquet_file()` happy path with temp dir:
  - file generated
  - expected columns present
  - row count sanity check
- `write_parquet_file()` failure cases:
  - invalid output path / create dir failure
  - schema/data mismatch case (if constructible)
- `RateLimiter` behavioral test:
  - `acquire()` blocks/refills as expected with controlled timing.

Expected outcome:

- `backfill.rs` reaches 75-85%, total coverage ~78-82%.

### Phase 4: Frontfill Runtime + Sink Init Error Paths

Scope:

- `src/frontfill.rs`

Tests to add:

- Validation failure for `flush_blocks == 0`.
- Sink initialization branches:
  - parquet path success setup
  - kafka config missing brokers error
  - kafka feature-disabled error branch
- Auth validation branch:
  - non-Authorization header rejected when key provided.

Implementation note:

- Keep tests unit-level by asserting early-return and setup behavior; avoid live websocket networking.

Expected outcome:

- `frontfill.rs` reaches 75-85%, total coverage >=85%.

## Test Design Practices

- Use table-driven test cases for helper functions (`chunk_ranges`, auth builders, value conversion).
- Prefer deterministic data builders for synthetic logs in decode tests.
- Test both happy path and explicit error path for every fallible helper.
- For async tests, use `#[tokio::test]` and bounded `tokio::time::timeout`.
- Keep integration tests minimal; focus on isolated module tests.

## Coverage Measurement Workflow

After each phase:

1. `just fmt-check`
2. `just check`
3. `just clippy`
4. `just test`
5. `just coverage`

Milestone gates:

- End of Phase 1: >=55%
- End of Phase 2: >=70%
- End of Phase 3: >=80%
- End of Phase 4: >=85%

## Risks and Mitigations

- Risk: constructing valid synthetic logs for all events is tedious.
  - Mitigation: shared test fixtures/builders in `decode.rs` test module.
- Risk: async runtime tests become flaky.
  - Mitigation: avoid network, assert early-return branches and deterministic helpers.
- Risk: chasing tiny edge cases with low ROI late in cycle.
  - Mitigation: reevaluate per-module report after each phase and prioritize largest uncovered blocks.
