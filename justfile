set shell := ["bash", "-cu"]

default:
    @just --list

build:
    cargo build

build-release:
    cargo build --release

check:
    cargo check

fmt:
    cargo fmt

fmt-check:
    cargo fmt --check

clippy:
    cargo clippy --all-targets -- -D warnings

test:
    cargo test

test-list:
    cargo test -- --list

coverage:
    if ! cargo llvm-cov --version >/dev/null 2>&1; then cargo install cargo-llvm-cov --version 0.6.21 --locked; fi
    cargo llvm-cov --summary-only

coverage-lcov:
    if ! cargo llvm-cov --version >/dev/null 2>&1; then cargo install cargo-llvm-cov --version 0.6.21 --locked; fi
    mkdir -p coverage
    cargo llvm-cov --lcov --output-path coverage/lcov.info

coverage-html:
    if ! cargo llvm-cov --version >/dev/null 2>&1; then cargo install cargo-llvm-cov --version 0.6.21 --locked; fi
    mkdir -p coverage
    cargo llvm-cov --html --output-dir coverage

run:
    cargo run

run-release:
    cargo run --release

run-backfill *args:
    cargo run --bin backfill {{args}}
