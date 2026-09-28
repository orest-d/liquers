---
id: AXUM-ASSETS-WEBSOCKET-ROUTE-PANICS
kind: issue
title: AssetsApiBuilder::build panics whenever the WebSocket route is enabled
status: closed
priority: P1
complexity: S
area: [axum]
design: axum-assets-endpoints
created: 2026-09-27
github:
---
## Problem

`liquers-axum/src/assets/builder.rs` registers the WebSocket route as
`format!("{}/*query", ws_path)`. axum 0.8 rejects a path segment starting with `*`
(`validate_v07_paths` in `axum-0.8.9/src/routing/path_router.rs`), and `Router::route` panics on
that error. `AssetsApiBuilder::new(..)` enables the WebSocket route by default, so
`AssetsApiBuilder::new(..).build()` panics, and so does the `assets_recipes_basic` example, which
sets the path explicitly.

## Impact

No server can mount the Assets API without first calling `.without_websocket()`. No test builds
the router today (the builder tests inspect fields only), which is why it went unnoticed. The
`axum-assets-endpoints` handler tests all build the router and would panic.

## Expected behaviour

`build()` registers `format!("{}/{{*query}}", ws_path)`, and a `--lib` test calls `build()` with
the default configuration so a route-table panic fails the unit loop.

## Discovery

Final cross-phase review of `specs/design/axum-assets-endpoints/`, 2026-09-27, while checking
whether the Phase 3 router tests could run. The fix is planned in that design's Phase 4, Step 7.

## Resolution

The WebSocket routes use axum 0.8 wildcard syntax (`{ws}/q/{*query}`, `{ws}/key/{*key}`, plus the
bare `{ws}/q` and `{ws}/key`), so `AssetsApiBuilder::build()` no longer panics. Evidence:
`liquers-axum/tests/assets_websocket.rs` AWS14a/AWS14b, and every router test in
`assets_api_endpoints.rs`.

Fixed on branch `claude/fervent-cori-ew4kvn` (design `axum-assets-endpoints`, 2026-09-28).
