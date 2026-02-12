# AGENTS.md

This guide is for coding agents working in `polymarket-indexer`.
Follow these repo-specific commands and conventions to keep changes consistent.

## Project Snapshot

- Language: Rust (edition 2024)
- Build system: Cargo
- Runtime: Tokio async runtime
- Domain: Polygon event ingestion (frontfill + backfill) with pluggable sinks
- Primary entrypoints: `src/main.rs` (frontfill CLI), `src/bin/backfill.rs` (backfill CLI)
- Shared runtime/library modules: `src/lib.rs`, `src/frontfill.rs`, `src/backfill.rs`, `src/decode.rs`, `src/contracts.rs`, `src/storage.rs`

## Preferred Runtime and Tooling Crates

Use these Rust ecosystem defaults for new work in this repo:

- Logging: `tracing` + `tracing-subscriber` (avoid new `println!`-style operational logs)
- Async runtime: `tokio` and related ecosystem crates (`tokio-stream`, `futures`)
- CLI and env parsing: `clap` with `derive` and `env` features
- `.env` loading: `dotenvy` (preferred over `dotenv` for new code)

## Command Reference

Run all commands from repository root:

```bash
cd /Users/hang/Code/polymarket-indexer
```

Use `just` as the primary interface for development lifecycle commands.

### Just Lifecycle Commands

- List recipes: `just`
- Build debug binary: `just build`
- Build release binary: `just build-release`
- Fast compile/type validation: `just check`
- Format code: `just fmt`
- Check formatting only: `just fmt-check`
- Lint all targets with warnings as errors: `just clippy`
- Run tests: `just test`
- List tests: `just test-list`
- Run indexer: `just run`
- Run release indexer: `just run-release`
- Run backfill CLI: `just run-backfill -- <args>`

### Build and Run

- Debug build: `cargo build`
- Release build: `cargo build --release`
- Run debug binary: `cargo run`
- Run release binary: `cargo run --release`
- Fast compile/type validation: `cargo check`

### Formatting and Linting

- Format code: `cargo fmt`
- Check formatting only: `cargo fmt --check`
- Lint default targets: `cargo clippy`
- Lint all targets: `cargo clippy --all-targets`
- Fail CI on warnings: `cargo clippy --all-targets -- -D warnings`

### Testing

- Run all tests: `cargo test`
- List available tests: `cargo test -- --list`
- Compile tests without running: `cargo test --no-run`
- Run only binary tests: `cargo test --bins`
- Run only library tests (if library exists): `cargo test --lib`

### Test Coverage

Use `just` coverage recipes:

- Summary report: `just coverage`
- LCOV output: `just coverage-lcov` (writes `coverage/lcov.info`)
- HTML report: `just coverage-html` (writes `coverage/html`)

Prerequisite:

- Rust 1.85-compatible install: `cargo install cargo-llvm-cov --version 0.6.21 --locked`

### Single-Test Execution (Important)

Use these forms for targeted test execution:

- By substring match: `cargo test <test_name>`
- Exact test name: `cargo test <test_name> -- --exact`
- Integration test file: `cargo test --test <file_stem>`
- Specific test in an integration file: `cargo test --test <file_stem> <test_name>`
- Show print output: append `-- --nocapture`
- Run serially for flaky async tests: append `-- --test-threads=1`

Example:

```bash
cargo test order_filled -- --exact --nocapture --test-threads=1
```

## Current Repo Reality (Verified)

- The project is expected to pass `just fmt-check`, `just check`, `just clippy`, `just test`, and `just build` after changes.
- Test coverage commands are available through `just coverage*` recipes.

Do not treat existing warnings as permission to introduce new warnings.

## File and Module Conventions

- Keep module layout flat unless complexity clearly requires nesting.
- Current architecture pattern:
  - CLI wrappers only parse args/env and bootstrap runtime:
    - `src/main.rs` for frontfill
    - `src/bin/backfill.rs` for backfill
  - Shared runtime + domain logic belongs in library modules:
    - `src/frontfill.rs`, `src/backfill.rs`, `src/contracts.rs`, `src/decode.rs`, `src/storage.rs`
- Export shared modules via `src/lib.rs` so binaries consume common code.

## Spec Documentation Conventions

- Write implementation specs under the `spec/` directory.
- Name each new spec file using a zero-padded sequence number plus a short slug: `NNNN_short-description.md`.
- Start numbering from `0001` and increment for each new spec (for example: `0001_topic0-schema-parquet.md`, `0002_storage-backends.md`).
- Use lowercase kebab-case for the slug portion, separated from the numeric prefix by an underscore.

## Imports

Match the existing style and let `rustfmt` finalize ordering.

- Prefer grouped imports with braces for related symbols.
- Keep one import per crate path unless grouping improves clarity.
- Avoid wildcard imports like `use foo::*;` in production code.
- Remove unused imports in touched code.

## Naming

- Constants: `SCREAMING_SNAKE_CASE` (e.g., contract addresses).
- Functions, variables, modules: `snake_case`.
- Structs, enums, event types: `PascalCase`.
- Use descriptive names tied to domain concepts (`block_number`, `log_index`).

## Types and Signatures

- Prefer explicit return types on public and async functions.
- Use `anyhow::Result<T>` for fallible flows and `anyhow!` for contextual errors.
- Use borrowed `&str` for input parameters where ownership is unnecessary.
- Use owned `String` for persisted or stored values.
- Avoid `as` casts unless unavoidable and safe.

## Async and Concurrency Patterns

- Use Tokio runtime (`#[tokio::main]`) for entrypoint async execution.
- Prefer Tokio-native primitives (`tokio::sync`, `tokio::signal`, `tokio::time`).
- Use `.await` directly at async call sites; avoid hidden blocking work.
- For shared mutable async state, current pattern is `Arc<Mutex<T>>`.
- Keep lock scope small; do not hold mutex guards across unrelated operations.
- Prefer `StreamExt` + `while let Some(item)` for stream processing.

## Logging, CLI, and Environment

- Initialize structured logging with `tracing_subscriber` at startup.
- Use `tracing::{info, warn, error, debug}` for runtime observability.
- Define CLI args with `clap::Parser` derive-based structs.
- Use Clap `env` bindings for env-backed defaults (e.g., RPC URLs, output paths).
- For frontfill paid RPC auth, use header-based auth (`RPC_AUTH_KEY` + `RPC_AUTH_HEADER`/`RPC_AUTH_SCHEME`), sent via the WebSocket `Authorization` header.
- For backfill paid RPC auth, prefer header-based auth (`RPC_HTTP_KEY` + `RPC_HTTP_AUTH_HEADER`/`RPC_HTTP_AUTH_SCHEME`) instead of URL query key injection.
- Load local environment files via `dotenvy::dotenv()` during startup.

## Error Handling

- Propagate errors with `?`; avoid `.unwrap()` in production paths.
- Add context before returning when failures are ambiguous.
- Do not swallow errors in empty `match` arms or empty `if let Err(_)` blocks.
- Return early on fatal setup failures.
- Keep error paths deterministic and observable.

## Event and Data Handling

- Keep topic0-based event matching centralized in shared decode logic (`src/decode.rs`).
- Preserve ordered event columns for each event type when normalizing decoded data.
- Persist normalized metadata fields (`event_type`, `block_number`, `transaction_hash`, `log_index`) plus event-specific columns.
- Keep sink interfaces generic (`EventSink`) so backends remain extensible.

## Agent Workflow Expectations

- Make minimal, targeted changes.
- Read files before editing; do not guess repository structure.
- After edits, run at least:
  - `cargo fmt` (or `cargo fmt --check` in CI mode)
  - `cargo check`
  - `cargo test` (or targeted test command)
- If a command fails due to pre-existing repo issues, report it clearly.

## Cursor and Copilot Rules

Checked for additional instruction sources:

- `.cursorrules`: not present
- `.cursor/rules/`: not present
- `.github/copilot-instructions.md`: not present

If any of these files are added later, treat them as higher-priority local policy and merge their constraints into this guide.

## Quick Start for New Agents

1. Read `README.md` and `Cargo.toml`.
2. Inspect `src/lib.rs`, `src/frontfill.rs`, `src/backfill.rs`, `src/decode.rs`, and `src/storage.rs` for architecture and shared patterns.
3. Implement the smallest viable change.
4. Run `just fmt-check`, `just check`, `just clippy`, and `just test`.
5. Report changes and verification results, including pre-existing failures.
