# Phase 5: Documentation - asset-cancellation-outcome

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical

## Implementation Summary

Implemented as approved in Phase 2 (commit `b01da6d`). `AssetRef::cancel` sets a lock-free
`CancellationRequest` shared with the run's `Context`; the run races `compute` against it (compute
polled first) and is the single writer of the terminal status, finalizing once from an in-flight
status. A command that has returned `Ok` ends ready and is persisted; a suspended async command is
dropped; a `Submitted` asset ends `Cancelled` at once. `Context::is_cancelled` / `check_cancelled`
give cooperative checks. Any `ErrorType::Cancelled` error ends its asset `Cancelled` with the cause
in `error_data`; a cancelled dependency cascades unchanged (its `query` names the root cause) with
one warning per dependent; an asset's own cancel wins over a cascade. Replacement uses
`cancel_for_replacement`, which discards the late result. `fail_due_to_dependency` and
`AssetData::set_cancelled` are gone. Conforms to D1–D13 and AC-1 to AC-12.

Added during implementation (recorded in Phase 1 §Design Readiness, no contract change):
`MetadataSaver::close` for a replaced asset; run outcomes (failure, cancellation) are recorded in
memory only; `fail_asset` keeps a finalized progress bar. Nothing approved was omitted.

## Documentation Delivered

### New Reference Documents
None: the contract lives in the existing asset references listed below.

### New Guide Documents
- `specs/guides/COMMAND_DESIGN_GUIDE.md` — cooperative cancellation: what `cancel()` guarantees,
  `is_cancelled` / `check_cancelled`, sync vs async commands, returning and propagating
  `Error::cancelled`, cascade, testing (D9).

### Existing Documents Reviewed or Updated
Every `affects_docs` entry was reviewed against the code and updated, each with a `## History` row
and `reviewed: 2026-10-09`: `reference/ASSETS.md` (status description, cancellation path,
Scenarios 3–5, terminal outcome), `reference/ASSET_SET_OPERATION.md` (flag replaced by discard),
`reference/DEPENDENCIES_STATUS.md` (Flow D cascade), `reference/ASSET_LIFECYCLE.md` (persistence
rows), `reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`, `reference/WEB_API_SPECIFICATION.md`,
`guides/COMMAND_REGISTRATION_GUIDE.md`, `guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md`,
`guides/WEB_API_GUIDE.md`. Rustdoc: `assets.rs` module docs, `AssetRef::cancel`,
`cancel_for_replacement`, `Context::is_cancelled` / `check_cancelled`, `ErrorType::Cancelled`.

### Links and Capability Map
`specs/README.md`: task row "Make a long-running command cancellable", a capability entry under
Commands, and this design's entry moved from designing to built.

## Issues Filed

- `CANCEL-CAN-OVERTAKE-A-RETURNED-COMMAND` (P3, draft) — the point of no return is the end of the
  compute future, not the last command: a cancel can still win if a post-command await suspends.
  Filed rather than fixed: it needs an interpreter-side marker, a design decision of its own.

Closed: `ASSET-CANCEL-DURING-PROCESSING-FINISHES-READY` (resolution recorded in the issue).

## Important Learning

- A queued asset is `Processing` from the moment it is claimed, before its command runs; tests must
  wait for the command itself to start, or the cancel correctly skips the command (AC-4).
- Sync commands block their worker thread; cancellation tests need a multi-threaded runtime.
- Store writes from a replaced asset are a race of their own: the coalescing `MetadataSaver` writes
  in a spawned task, so finishing the replaced asset had to close it, not just skip a call.
- Writing a failed or cancelled keyed asset's metadata leaves a metadata-only entry; the old error
  path avoided it by accident, now it is explicit.

## Conformance and Remaining Work

Requested (Phase 1, widened by the maintainer) = approved (Phase 2) = implemented. The only
remainder is the filed limitation above. Out of scope by Phase 1: preemptive interruption of sync
code, downward cancellation, inline/wasm cancellation (`WEB-CANCELLATION-INERT`).

## Validation

- `cargo test -p liquers-core --lib --tests`: all pass (13 new in `asset_cancellation.rs`).
- `cargo test -p liquers-axum --test assets_api_endpoints`: 65 pass (`aae92`, `aae38` assert `Ready`).
- `cargo test -p liquers-lib --lib --tests`: all pass.
- `cargo check -p liquers-core --target wasm32-unknown-unknown` and `-p liquers-web`: build.
- `python3 scripts/docs_index.py --check`: 0 errors; `validate_phase.py` 3, 4, 5: pass.
