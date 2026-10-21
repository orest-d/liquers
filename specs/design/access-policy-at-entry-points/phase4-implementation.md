# Phase 4: Implementation Plan — Principals and an access policy at the entry points

## Overview

Core vocabulary and error first (they compile alone), then the policy and the walk, then the axum
seam, then the routes family by family, then bindings and documents. Prerequisites: Q1 and Q3
decided; the plan follows the recommended answers. No open issue must be fixed first. Steps 5-8
are independent of each other and can be separate PRs.

## Progress

- [ ] Step 1: `ErrorType::AccessDenied` and its mappings in every crate
- [ ] Step 2: `User` derives and `Display`; context documentation
- [ ] Step 3: `liquers-core/src/access.rs`: vocabulary, `AccessPolicy`, `RulePolicy`
- [ ] Step 4: `plan_access_requests`
- [ ] Step 5: axum seam: `Principal`, `AccessGuard`, `with_access_policy` on the four builders
- [ ] Step 6: Store and Recipes APIs checked
- [ ] Step 7: Query API and Assets `q/` family checked
- [ ] Step 8: Assets `key/`, `admin/` and WebSocket checked
- [ ] Step 9: documents, follow-ups, issue closure

## Implementation Steps

### Step 1: the error variant
- Files / symbols: `liquers-core/src/error.rs` `ErrorType::AccessDenied`, `Error::access_denied`
  (signature takes `&str` principal and target text until Step 3 adds the types, then the typed
  form); `liquers-core/src/assets.rs` persistence `match`; `liquers-axum/src/api_core/error.rs`
  `error_to_status_code` (403), `parse_error_type`, `keyabs15b`; `liquers-py/src/error.rs` both
  directions; `liquers-web/src/error.rs` (`"access_denied"`).
- Change: one variant everywhere an `ErrorType` match is exhaustive (the `configuration-error-kind`
  file list).
- Depends on: none.
- Proof: `cargo test -p liquers-axum --lib keyabs15b access_denied_is_403`; `cargo test -p
  liquers-core --lib`; `bash scripts/test-liquers-py.sh`; liquers-web loop after `cargo clean`
  (Test 18).
- Rollback: revert; nothing uses the variant yet.

### Step 2: the principal type
- Files / symbols: `liquers-core/src/context.rs` `User` (derives, `Display`), module and `User`
  doc comments (identity is checked at entry points; `Context` stays identity-free).
- Depends on: none.
- Proof: `cargo test -p liquers-core --lib`.
- Rollback: revert (additive).

### Step 3: policy model
- Files / symbols: new `liquers-core/src/access.rs`; `lib.rs` `pub mod access`.
- Change: Phase 2 Interfaces except the walk; `Error::access_denied` moves to the typed signature.
- Depends on: Steps 1-2.
- Proof: `cargo test -p liquers-core --lib access` (Tests 3, 4, 5, 7); `cargo check -p liquers-core
  --target wasm32-unknown-unknown`.
- Rollback: revert the module.

### Step 4: the plan walk
- Files / symbols: `access.rs` `plan_access_requests` (+ private `parameter_requests`), mirroring
  `liquers-mcp/src/server.rs` `check_allowed` / `check_parameter` with key steps added; exhaustive
  `match` over `Step` and `ParameterValue`.
- Depends on: Step 3.
- Proof: `cargo test -p liquers-core --lib plan_access_requests` (Test 6).
- Rollback: revert the function.

### Step 5: the axum seam
- Files / symbols: new `liquers-axum/src/api_core/access.rs` (`Principal`, `AccessGuard`);
  `api_core/mod.rs`; `with_access_policy` and the `Extension(AccessGuard)` layer in
  `store/builder.rs`, `assets/builder.rs`, `recipes/builder.rs`, `query/builder.rs`; re-exports in
  `lib.rs`.
- Depends on: Step 4.
- Proof: `cargo test -p liquers-axum --lib --tests` unchanged (AC-5); `acc06` (Test 13).
- Rollback: revert; no handler uses it yet.

### Step 6: Store and Recipes APIs
- Files / symbols: `liquers-axum/src/store/handlers.rs` (18 handlers), `recipes/handlers.rs` (5).
- Change: the Phase 2 table rows; listings filtered with `AccessGuard::readable`.
- Depends on: Step 5.
- Proof: `cargo test -p liquers-axum --test access_policy acc01 acc07`; existing store and
  recipes suites unchanged.
- Rollback: revert the handler edits.

### Step 7: Query API and `q/`
- Files / symbols: `liquers-axum/src/query/handlers.rs`; `assets/query_handlers.rs` (10 handlers),
  the check after `parse_request` so overrides are included.
- Depends on: Step 5.
- Proof: `acc02`, `acc03`, `acc04`, `acc05` (Tests 9-12); existing query and override suites.
- Rollback: revert the handler edits.

### Step 8: `key/`, `admin/`, WebSocket
- Files / symbols: `assets/key_handlers.rs` (24 handlers); `assets/websocket.rs` (principal and
  guard captured in the four upgrade handlers, held by `Session`, checked in `Session::subscribe`).
- Depends on: Step 5.
- Proof: `acc07` (key listing), `acc08`, `acc09` (Tests 14-16); existing WebSocket suites.
- Rollback: revert the handler edits.

### Step 9: documents and follow-ups
- Files / symbols: `WEB_API_SPECIFICATION.md` §3.1, §9, Appendix B; `WEB_API_GUIDE.md` new
  section; `PROJECT_OVERVIEW.md`; `DOC_04`; `POSITIONING.md`; `specs/README.md` capability line;
  `specs/issues/CORE-SESSION-AND-KEY-ACL.md` closed for slice 1 with the remainder (groups,
  sessions as assets) recorded in the filed follow-ups; `MCP-SERVER-APPLIES-NO-ACCESS-POLICY`
  linked.
- Depends on: Steps 6-8.
- Proof: `python3 scripts/docs_index.py --check`.
- Rollback: documents only.

## Testing Plan

Per step as above. Final: `cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-axum
--lib --tests`, `cargo test -p liquers-lib --lib --tests` (no exhaustive `ErrorType` match there at
18732dd; Step 1 re-checks with `grep -rn "ErrorType::" liquers-*/src`), the py and web loops, and
`bash scripts/check-build-matrix.sh` (a new `ErrorType` arm reaches every configuration).

## Rollback Plan

Without a configured policy nothing changes at runtime, so a partial landing (Steps 1-5) is inert.
Steps 6-8 can be reverted per family; a deployment can also drop `with_access_policy` to return to
HEAD behaviour without a code change.

## Documentation Updates

As Step 9, each reference/guide with a History row and `reviewed:` bump; `specs/index.csv`
regenerated; no command registry change.

## Phase 5 Entry Criteria

None — four-phase design.
