# Spec: Backfill Header-Based RPC Auth

## Goal

Improve backfill RPC auth safety by stopping API key injection into request URLs.
When an auth key is provided, send it through HTTP headers.

## Problem

Current behavior places `RPC_HTTP_KEY` into the URL:

- via `{API_KEY}` placeholder replacement, or
- via `apiKey=<key>` query param appending.

This is risky because URLs can be exposed in logs, metrics, traces, and proxy caches.

## Desired Behavior

- Keep `RPC_HTTP_URL` as a clean base endpoint.
- If `RPC_HTTP_KEY` is set, use header-based auth for all backfill RPC calls.
- If `RPC_HTTP_KEY` is not set, behavior remains unauthenticated as today.

## Configuration

- `RPC_HTTP_URL` (required): HTTP JSON-RPC endpoint.
- `RPC_HTTP_KEY` (optional): auth token/key value.
- `RPC_HTTP_AUTH_HEADER` (optional, default `Authorization`): header name to use.
- `RPC_HTTP_AUTH_SCHEME` (optional, default `Bearer`): optional scheme prefix.

Header value rules:

- If scheme is non-empty: `<scheme> <key>` (example: `Bearer abc123`).
- If scheme is empty: `<key>` only.

## Implementation Plan

1. Replace URL-key mutation helper with a transport/client builder that can attach default headers.
2. Keep URL parsing/validation logic, but remove placeholder/query-key injection behavior.
3. Build HTTP provider from base URL and configured headers.
4. Wire new config fields into backfill CLI args + env bindings.
5. Update README and AGENTS guidance for auth behavior.

## Compatibility Notes

- `{API_KEY}` substitution and query-param key injection will be removed.
- Users currently relying on URL key placement must migrate to header-based auth env vars.

## Testing Plan

Unit tests:

- auth header builder uses defaults (`Authorization: Bearer <key>`)
- custom header name and scheme behavior
- empty scheme sends raw key value
- URL parsing remains valid without modifying URL

Integration/CLI tests:

- config parse for new auth fields
- existing block-range validation behavior remains unchanged

Verification commands:

- `just fmt-check`
- `just check`
- `just clippy`
- `just test`
- `just build`

## Rollout

- Implement in one refactor pass focused on backfill HTTP provider setup.
- Keep frontfill behavior unchanged.
