# Phase 4: Implementation Plan — Accept negotiation on query routes

## Overview

Steps 1-4 deliver format negotiation over the value's own type and need no other design. Steps 5-6
add conversion and wait for `value-conversion-registry`. Record the decisions on Phase 1
questions 1-3 before step 2; under another answer, only `negotiate` (step 2) and its tests change.

## Progress

- [ ] Step 1: re-check the serving paths and tests
- [ ] Step 2: `negotiate.rs` — parsing and the decision without conversion
- [ ] Step 3: wire the Query API and `q/data`
- [ ] Step 4: integration tests for the serialization half
- [ ] Step 5: the conversion branch
- [ ] Step 6: conversion tests
- [ ] Step 7: documentation, manual check and final review

## Implementation Steps

### Step 1: re-check the serving paths and tests
- Files / symbols: `liquers-axum/src/query/handlers.rs` (`serve_query`, `get_query_handler`,
  `post_query_handler`), `assets/query_handlers.rs` (`q_get_data_handler`, `q_post_data_handler`),
  `assets/common.rs` (`asset_bytes`), `axum_integration.rs` (`BinaryResponse`); every test under
  `liquers-axum/tests/` sending `Accept` to a non-entry route.
- Change: none; if `value-serialization-streaming` or `shared-metadata-and-bytes` has landed,
  note where its serializer is called so step 3 inserts the decision before it.
- Depends on: nothing.
- Proof: `cargo check -p liquers-axum`.
- Rollback: n/a.

### Step 2: `negotiate.rs` — parsing and the decision without conversion
- Files / symbols: new `liquers-axum/src/api_core/negotiate.rs` (`MediaRange`, `parse_accept`,
  `MediaRange::matches`, `Negotiated` with all four variants, `negotiate` with step 3b absent,
  `negotiated_bytes`, the `406` builder); `api_core/mod.rs`. Matches over `Negotiated` explicit.
- Depends on: 1.
- Proof: `cargo test -p liquers-axum --lib negotiate` (the five unit tests).
- Rollback: delete the module.

### Step 3: wire the Query API and `q/data`
- Files / symbols: `query/handlers.rs` — both handlers extract `HeaderMap`; `serve_query(env,
  query_path, pairs, body, headers, config)` calls `negotiated_bytes` within the timeout;
  `assets/query_handlers.rs` — `q_get_data_handler` / `q_post_data_handler` extract `HeaderMap`
  and call `negotiated_bytes`. `key_handlers.rs` untouched.
- Depends on: 2.
- Proof: `cargo test -p liquers-axum` — every existing suite passes unchanged (AC-2, AC-8).
- Rollback: restore the two calls to `get_binary` / `asset_bytes`.

### Step 4: integration tests for the serialization half
- Files / symbols: new `liquers-axum/tests/accept_negotiation.rs` — Phase 3 tests 6-11.
- Depends on: 3.
- Proof: `cargo test -p liquers-axum --test accept_negotiation`.
- Rollback: n/a.

### Step 5: the conversion branch
- Files / symbols: `negotiate.rs` — step 3b of the decision and the `Convert` arm of
  `negotiated_bytes` (`get_conversion_registry`, `targets_from`, `convert` with
  `ConversionPolicy::Automatic`); `NotAcceptable.producible` includes automatic targets.
- Depends on: 4 and `value-conversion-registry` merged; re-check its signatures first.
- Proof: `cargo test -p liquers-axum`.
- Rollback: remove step 3b; steps 1-4 stand alone.

### Step 6: conversion tests
- Files / symbols: `accept_negotiation.rs` — `automatic_edge_converts`,
  `lossy_and_fallible_edges_refused` (environment built with `with_conversion_registry`).
- Depends on: 5.
- Proof: `cargo test -p liquers-axum --test accept_negotiation`.
- Rollback: n/a.

### Step 7: documentation, manual check and final review
- Files / symbols: `WEB_API_SPECIFICATION.md`, `WEB_API_GUIDE.md` (History rows, `reviewed:`);
  issue `HTTP-ACCEPT-HEADER-NOT-NEGOTIATED` status and resolution note (`DOCS_STRUCTURE_GUIDE.md`
  §4.3). Run Examples 1-3 by hand against a server built on `liquers-lib`'s environment
  (feature `records`) and record the output in the PR.
- Depends on: 6 (or 4, if the registry has not landed: then the issue stays open for AC-3/AC-4
  with a note naming steps 5-6).
- Proof: `python3 scripts/docs_index.py --check`; diff review for edits outside `liquers-axum`
  and the two documents, and for stdout printing.

## Testing Plan

Focused: `cargo test -p liquers-axum --lib negotiate` and `--test accept_negotiation`. Crate:
`cargo test -p liquers-axum`. Cross-crate: `cargo check -p liquers-lib` is not affected (no shared
code), so it is not required. Manual: Examples 1-3 (step 7).

## Rollback Plan

Each step is one commit. Reverting step 3 restores today's behaviour on every route; the module
from step 2 is then unused. Reverting step 5 leaves format-only negotiation.

## Documentation Updates

`specs/reference/WEB_API_SPECIFICATION.md` (§2.3, §3.1, §7) and `specs/guides/WEB_API_GUIDE.md`
in step 7, each with a History row and a `reviewed:` bump.

## Phase 5 Entry Criteria

Not applicable (no `workflow` marker). Done when questions 1-3 are decided, every proof passes,
and `docs_index.py --check` is clean; the conversion half is done when steps 5-6 pass after
`value-conversion-registry` lands.
