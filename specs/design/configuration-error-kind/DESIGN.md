---
id: CONFIGURATION-ERROR-KIND
kind: design
title: Classify semantic configuration failures
status: in_review
phase: implementation
readiness: needs-decision
area: [core/error, store/config]
issues: [CORE-CONFIGURATION-ERROR-KIND]
created: 2026-08-31
---
# Configuration error kind

## Phase Status

- [x] Phase 1: High-Level Design
- [x] Phase 2: Solution and Architecture
- [x] Phase 3: Examples and Tests
- [x] Phase 4: Implementation Plan

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)

## Review 2026-10-06

Re-verified against HEAD after the latest merges. **Updated: the scope was incomplete.** Findings:

1. `EnvironmentConfig` (`liquers-core/src/environment_config.rs`, added since the design) parses
   whole environment documents and reports malformed YAML/JSON/TOML as `General`
   (`Error::general_error("Failed to parse environment configuration …")`), while store documents
   use `ParseError`. Its semantic failures (`expand_env_vars` through `StoreConfig`) are in scope
   too.
2. A new `ErrorType` variant is matched exhaustively in **five** places beyond core:
   `liquers-web/src/error.rs` (`error_type_name` / `error_type_from_name`),
   `liquers-web/tests/objects_OBJECT.rs` (`ALL_ERROR_TYPES` and the OBJECT06 count assertion,
   which `web-object06-error-type-exhaustiveness` reworks), `liquers-py/src/error.rs`,
   `liquers-axum/src/api_core/error.rs` (HTTP status mapping), and `liquers-lib/src/polars/util.rs`.
   The plan lists them.
3. `expand_env_vars` (`store_config.rs` ≈271) reports an unclosed `${` as `ParseError` and an unset
   variable as `General`. The design keeps the first and migrates the second (unchanged intent,
   now with exact locations).

The taxonomy decision is unchanged. Readiness stays `needs-decision`.
