---
id: STORE-BUILD-WARNINGS-LOST-IN-THE-BROWSER
kind: issue
title: Store build warnings go to stderr, which a browser build discards
status: closed
priority: P3
complexity: S
area: [core/store, store/config, web]
design:
created: 2026-10-15
github:
---
## Problem

`StoreRouterBuilder` warns with `eprintln!` when `read_only: true` wraps a store without data-only
access, because that store's own reads may still write default metadata
(`liquers-core/src/store_factory.rs`). On `wasm32-unknown-unknown` stderr goes nowhere, so a page
configuring `read_only` over such a store never sees the warning.

## Impact

A browser deployment can believe a mount is fully read-only while its store writes metadata on
read. Small today: the browser's built-in stores do not write on read.

## Expected behaviour

Build warnings are returned to the caller (for example a list of warnings beside the built
store) so each integration can surface them: stderr natively, the console or a returned value in
the browser.

## Discovery

Found while documenting `design/store-adapters/` (language integration guide update).

## Resolution (2026-10-16)

Fixed in PR #137 (store-adapters Phase 5 review). Root cause: the builder printed warnings with `eprintln!` and had no way to return them; on `wasm32-unknown-unknown` stderr is discarded. `StoreRouterBuilder::build_store_reporting` (and `…_without_env_expansion`) now return a `BuiltStore { store, warnings }`; `build_store` prints the warnings to stderr as before, and `liquers-web`'s `build_store` writes them to the page console. Test `read_only_over_store_without_data_access_warns` asserts the returned warning.
