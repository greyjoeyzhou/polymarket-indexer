# Spec: Frontfill Header-Based RPC Auth

## Goal

Update frontfill WebSocket RPC auth to use headers when an auth key is provided, instead of embedding secrets in `RPC_URL`.

## Problem

Frontfill currently receives a single `RPC_URL` and connects directly via `WsConnect::new(&rpc_url)`.
In practice, paid providers are often configured with credentials in URL query params, which is unsafe for logs, traces, and operational tooling.

## Desired Behavior

- Keep `RPC_URL` as a clean WebSocket endpoint.
- If an auth key is present, send it via WebSocket handshake headers.
- If no auth key is present, keep current unauthenticated behavior.

## Configuration

Add/align frontfill auth settings with backfill conventions:

- `RPC_AUTH_KEY` (optional): auth token/key for frontfill RPC.
- `RPC_AUTH_HEADER` (optional, default `Authorization`): header name.
- `RPC_AUTH_SCHEME` (optional, default `Bearer`): optional prefix.

Header value formatting:

- Non-empty scheme: `<scheme> <key>`
- Empty scheme: `<key>`

## Implementation Plan

1. Extend frontfill config surface:
   - `FrontfillConfig` in `src/frontfill.rs`
   - frontfill CLI args/env in `src/main.rs`
2. Add shared helper in frontfill runtime to build optional auth headers.
3. Construct `WsConnect` with headers when `RPC_AUTH_KEY` is provided.
4. Keep connection/filter/stream logic unchanged aside from transport setup.
5. Do not modify backfill auth behavior (already header-based).

## Compatibility Notes

- Existing setups using key-in-URL will continue to work only if users keep it that way.
- Recommended and documented path is header-based auth via env/flags.

## Tests

Unit tests (frontfill module):

- default `Authorization: Bearer <key>` formatting
- custom header + empty scheme formatting
- no key => no auth header

CLI tests (`src/main.rs` tests):

- default values for `RPC_AUTH_HEADER` and `RPC_AUTH_SCHEME`

## Docs Updates

- `README.md`: frontfill auth env vars and guidance to avoid URL-based key placement.
- `AGENTS.md`: add frontfill header-auth convention in logging/CLI/environment section.

## Verification

- `just fmt-check`
- `just check`
- `just clippy`
- `just test`
- `just build`
- LSP diagnostics on modified Rust files
