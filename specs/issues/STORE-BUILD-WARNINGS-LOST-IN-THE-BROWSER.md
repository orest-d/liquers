---
id: STORE-BUILD-WARNINGS-LOST-IN-THE-BROWSER
kind: issue
title: Store build warnings go to stderr, which a browser build discards
status: draft
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
