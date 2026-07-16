# Phase 1: High-Level Design - Asset Terminal Outcome Contract (WP-2)

## Feature Name

Asset Terminal Outcome Contract (WP-2 — Asset failure & message-lifecycle contract, F-2/F-3)

## Purpose

A finished asset has **exactly one observable terminal `State`**, and that `State` carries
its own `Status` and, if it failed, its `Error` — a `State` never holds both a value and an
error at once. `get()` therefore returns the same failure to every caller regardless of
polling vs. notification timing or notification overwrites, and a failure never erases the
metadata audit trail. Post-finish service messages are ignored and logged.

## Design stance (clarification vs. WP-2 as written)

WP-2 proposed a new `AssetOutcome<E>` enum plus `poll_outcome()` and a separate
`AssetData.error` field. Analysis shows this is largely **redundant**: `State` already
encodes the full terminal outcome — `Metadata` carries `Status` and the typed error
(`MetadataRecord.error_data: Option<Error>`, serializable), and `State` already exposes
`status()`, `error_result()`, `is_error()`, and `from_error()`. The design adopted here makes
**`State` (backed by metadata) the single source of truth** and drops `AssetOutcome`:

- **Invariant:** value **XOR** error. A terminal `State` has data (`Ready/Source/Override/
  Volatile/Directory`) or an error (`Error`/`Cancelled`), never both.
- **`poll_state() -> Option<State>`:** `None` iff not finished; `Some(state)` for every
  finished status, where the `State` faithfully carries data **or** the typed error. This
  replaces `poll_outcome()`. (Today it fabricates a `Some(none-value)` error-state — the value
  side is what must change, not the return type.)
- **`get() -> Result<State, Error>` is kept** (see Open Question 1): `Err` for computed
  failure (Error/Cancelled) and for infrastructure failure (not-running/hang/closed channel),
  preserving `?` ergonomics; `Ok(state)` iff the state holds data.
- Cancellation becomes a **typed** error (new `ErrorType::Cancelled`) so "cancelled" survives
  as an error, not a generic message.

## Core Interactions

### Asset System
Core of the change. `AssetRef::get()`/`poll_state()`, `finish_run_with_result`, and the
`evaluate_and_store` error branch unify into one `fail_asset(e)` routine that preserves
metadata (`metadata.with_error(e)`, not `Metadata::from_error` which replaces the record).
`process_service_messages` gains a post-finish phase: mutating messages are logged at `debug`
and dropped; `Cancel` after finish is a no-op (resolves the "meaningless send" FIXME).

### State / Value Types
No new `ExtValue` variants. `State` is affirmed as the terminal-outcome carrier; the
value-XOR-error invariant is documented and enforced at asset finalization.

### Command System
No new commands. Cancellation gains a typed `Error` constructor (`Error::cancelled(..)` +
`ErrorType::Cancelled`).

### Web/API
`liquers-axum` asset handlers already `match asset_ref.get().await`; audited so a failed asset
yields the real error (not a 200 with a none-value).

### UI
`AssetViewElement::from_asset_ref` currently reads the error only from the transient
`ErrorOccurred` notification (lossy). It migrates to reading the error from `poll_state()`'s
terminal `State` (`state.error_result()`), so overwritten notifications cannot lose the error.

### Python bindings
`liquers-py` `State`/`Metadata` wrappers audited so failed assets surface the error;
`cargo check -p liquers-py` gates the change.

## Crate Placement

**liquers-core** (`assets.rs`, `state.rs`, `metadata.rs`, `error.rs`) — primary. Downstream
audits/migration in **liquers-lib** (UI), **liquers-axum** (handlers), **liquers-py**.

## Open Questions

1. **`get()` signature — `Result<State>` (recommended) vs. bare `State`.** Returning bare
   `State` (the proposal under evaluation) unifies the model but loses `?`-based propagation
   and the `#[must_use]` guarantee, re-creating the "caller forgot to check" bug in new
   clothes, and leaves infrastructure errors (not-running/hang/closed channel) with no clean
   channel. Recommendation: keep `Result<State, Error>` as the ergonomic boundary but derive
   its `Err` from the terminal `State` (`state.error_result()`), and add
   `get_state() -> Result<State, Error>` returning `Ok(error_state)` for callers (UI,
   WebSocket) that want to *render* the failure rather than propagate it. → Decide before
   Phase 2 freezes signatures.
2. Should `Partial` be pollable (serve partial data) under this contract, or stay `None` until
   terminal? WP-2 leaves `Expired` to WP-3; confirm `Partial` scope here. → Phase 2.
3. Exact `ErrorType::Cancelled` semantics and whether existing cancellation paths already
   attach an error we can reuse. → Phase 2 (caller audit).

## References

- `plan20260707.md` WP-2 (F-2, F-3); `specs/ISSUES.md` → ASSET-MESSAGE-LIFECYCLE-ROBUSTNESS
- Code: `assets.rs` `poll_state` (:596), `get` (:1990), `finish_run_with_result` (:1326/:1359),
  `poll_state` error arm (:612); `state.rs` `from_error`/`error_result`/`is_error`;
  `metadata.rs` `with_error` (:1050), `error_result` (:1192), `error_data` field (:795)
- `specs/ASSETS.md` (Terminal outcome contract section to be added)
