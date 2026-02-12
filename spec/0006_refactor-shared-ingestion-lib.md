# Refactor Plan: Shared Ingestion Library + Thin CLIs

## Goal

Refactor the current codebase to extract common blockchain ingestion logic (log extraction, event parsing, and data writing orchestration) into reusable library modules. Then keep `frontfill` and `backfill` binaries as thin wrappers for CLI parsing and runtime bootstrapping.

## Current Pain Points

- Event definitions are duplicated across binaries.
- Topic0-based parsing logic is duplicated.
- Address parsing/filter assembly logic is duplicated.
- Runtime pipelines are embedded directly inside binaries, making extension and testing harder.

## Target Architecture

Introduce shared modules under `src/` and expose them via `lib.rs`:

1. `contracts.rs`
   - Own all `sol!` event definitions.
   - Provide canonical contract address constants.
   - Provide helper to parse/return `Vec<Address>` for filtering.

2. `decode.rs`
   - Own topic0 routing and decode logic.
   - Produce normalized decoded event payloads:
     - event type name
     - ordered `(column_name, value)` pairs

3. `frontfill.rs`
   - Shared frontfill runtime logic moved from `main.rs`.
   - Accept config struct and execute live WS subscription pipeline.
   - Reuse `storage::EventSink` implementations.

4. `backfill.rs` (library module, not bin)
   - Shared backfill runtime logic moved from `src/bin/backfill.rs`.
   - Accept config struct and execute chunked HTTP `eth_getLogs` pipeline.
   - Reuse shared decode + contract address helpers.

5. Existing `storage.rs`
   - Continue as sink abstraction + implementations.
   - Used by frontfill runtime and kept extension-ready for additional sinks.

## CLI Refactor Strategy

1. `src/main.rs` (frontfill binary)
   - Keep only:
     - CLI args parsing (`clap`)
     - env loading (`dotenvy`)
     - logging setup (`tracing_subscriber`)
     - config mapping + invocation of `polymarket_indexer::frontfill::run(...)`

2. `src/bin/backfill.rs` (backfill binary)
   - Keep only:
     - CLI args parsing
     - env loading
     - logging setup
     - config mapping + invocation of `polymarket_indexer::backfill::run(...)`

## Incremental Phases

### Phase 1: Shared Contract + Decode Extraction

- Add `contracts.rs` and move event declarations/constants there.
- Add `decode.rs` and move reusable parsing/value serialization there.
- Update consumers to use new modules without behavior change.

### Phase 2: Shared Runtime Modules + Thin Binaries

- Add `frontfill.rs` library runtime with config struct.
- Add `backfill.rs` library runtime with config struct.
- Rewrite binaries to be wrappers calling library runtimes.

## Verification Plan

Run after each phase and again at the end:

- `just fmt-check`
- `just check`
- `just clippy`
- `just test`
- `just build`

Plus LSP diagnostics on all modified Rust files.

## Commit Plan

1. `spec`: add this plan file.
2. `phase 1`: extract contracts + decode shared modules.
3. `phase 2`: add shared runtime modules and thin CLI wrappers.
4. `verify/docs adjustments` (if needed for resulting command or module updates).

Each phase will be committed incrementally.
