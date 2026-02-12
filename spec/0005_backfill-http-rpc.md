# Spec: Backfill HTTP RPC + auth

## Goal

Backfill uses HTTP `eth_getLogs` (no websocket) and supports paid RPC URLs with auth.

## Configuration

- `RPC_HTTP_URL` (required): base HTTP endpoint.
- `RPC_HTTP_KEY` (optional): API key.

If `RPC_HTTP_URL` contains `{API_KEY}`, replace it with `RPC_HTTP_KEY`.
Otherwise append query param `apiKey=<key>` when a key is provided.

CLI flags:

- `--rpc-http-url` (env: `RPC_HTTP_URL`)
- `--rpc-http-key` (env: `RPC_HTTP_KEY`)

## Tests

- Unit tests for URL building logic:
  - placeholder replacement
  - query param append (with and without existing query)
- Integration test for CLI validation that fails before network access.
