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
- **Core design principle: a `State` is *always potentially* an error.** Every site that
  consumes a `State` as a value MUST check it (`value_state()` / `error_result()` / an
  error-checked value extractor) before using the value. This is an invariant of the model,
  not an optional ergonomic. Any existing code that consumes a `State` as a value without
  checking is a **bug to be fixed under this WP** — including WP-1's dependency-propagation
  path (`wait_for_dependency` `:2293` and the recipe-delegation copy path) where a
  computed-failed dependency must fail the parent, not be consumed as a valid value.
- **`poll_state() -> Option<State>`:** `None` iff not finished; `Some(state)` for every
  finished status, where the `State` faithfully carries data **or** the typed error. This
  replaces `poll_outcome()`. (Today it fabricates a `Some(none-value)` error-state — the value
  side is what must change, not the return type.)
- **Two failure axes, not one.** *Computed failure* = the asset evaluated to an error
  (`Error`/`Cancelled`); a rich terminal `State` exists (log, query, typed `error_data`) and
  should be **returned**, not discarded. *Delivery failure* = the terminal `State` could not be
  obtained (store I/O, closed channel, hang guard, uninitialized env; confirmed reachable at
  `try_fast_track` `store.get().await?`, `assets.rs:471`); no faithful `State` exists → `Err`.
- **Single `get()`, ergonomics built into `State`** (resolves Open Question 1). One
  outcome-returning method; the value projection is opt-in on `State`, so the rich `State` can
  never be *accidentally discarded* by picking the wrong method:
  - `poll_state() -> Option<State>` — sync; `None` iff not finished, else the rich terminal
    `State` (value **or** error+log). Replaces `poll_outcome()`.
  - `get() -> Result<State, Error>` — async; `Ok(state)` for **any** obtained terminal outcome
    *including a computed-error state* (caller inspects `status()`/`error_result()` or projects
    via `value_state()`); `Err` reserved strictly for **delivery** failure (store I/O, closed
    channel, hang, uninit env). Preserves the log on computed failure. *(This is what the
    interim design called `get_state`; the separate ergonomic `get` is dropped.)*
  - `State::value_state(self) -> Result<State, Error>` — `{ self.error_result()?; Ok(self) }`.
    Ergonomic path is `get().await?.value_state()?`: first `?` = delivery failure, second `?` =
    computed failure. Reuses existing `error_result()` (typed error from `Metadata.error_data`).
- **Safety net (load-bearing).** A single rich `get()` flips the risk from "discard the State"
  to "consume an error-State as a value." To keep the forgotten-projection path fail-safe:
  (a) value extraction on `State` (`try_into_string`, `as_bytes`, a new `value()`/`data()`
  accessor) calls `error_result()?` first — extracting a *value* from an *error* `State`
  returns the error; (b) `State.data` stops being `pub` (existing `// TODO: remove pub`) so the
  guard cannot be bypassed. Render-the-error callers use `poll_state`/`status`/`error_result`
  and never hit the value extractors, so they are unaffected.
- Cancellation becomes a **typed** error (new `ErrorType::Cancelled`) so "cancelled" survives
  as an error, not a generic message.
- **`Err`-vs-error-`State` classification is itself part of the work.** The current code blurs
  the two axes: some sites emit `Err` for what is really a *computed* failure that should put
  the asset into `Status::Error` and yield an error-carrying `State`. Phase 2 must audit every
  `Err`-returning site in the get/evaluate/finish/fast-track paths and classify each as
  delivery (stays `Err`) or computed (must become an error `State`). See Phase 2 scope below.

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

## Phase 2 scope (mandatory deliverables carried from clarification)

- **`Err`-vs-error-`State` classification audit.** Enumerate every `Err`-returning site in the
  asset get/evaluate/finish/fast-track paths (e.g. `get` "finished but no data" `assets.rs:2034`;
  `finish_run_with_result` `:1354`; `process_service_messages` error/join paths; `try_fast_track`
  I/O `:471`). For each, decide: **delivery** (framework could not produce/obtain a State →
  stays `Err`) or **computed** (this asset's own failed outcome → must set `Status::Error` +
  `metadata.with_error` and surface as an error-carrying `State`). Produce a table; the
  reclassification set is a first-class output feeding the implementation plan. *This audit is
  now doubly critical:* with a single rich `get()`, `Err` means **only** delivery, so any
  computed failure still returning `Err` silently loses its `State`/log.
- **`get()` caller migration audit (enforces the "always check the State" principle).** Because
  `get()` now returns `Ok(error_state)` on computed failure (not `Err`), every value-wanting
  caller must check the `State` (`.value_state()?` or an error-checked extractor). Enumerate and
  fix: `get_binary` (`:2057`), `wait_for_dependency` (`:2293`, dependency-error propagation —
  **WP-1 overlap; fix here if WP-1 does not**), the recipe-delegation copy path, axum handlers
  (×2), `interpreter.rs`, `liquers-py`. The `State` value-extraction guard is the backstop for
  any missed site; the audit is the actual fix. Where a consuming site legitimately wants to
  *forward* an error-`State` (e.g. delegation copying a child's terminal state), that is allowed
  precisely because the `State` still carries the error — the rule is "check, don't silently
  treat as a value," not "always convert to `Err`."

## Open Questions

1. Should `Partial` be pollable (serve partial data) under this contract, or stay `None` until
   terminal? WP-2 leaves `Expired` to WP-3; confirm `Partial` scope here. → Phase 2.
2. Exact `ErrorType::Cancelled` semantics and whether existing cancellation paths already
   attach an error we can reuse. → Phase 2 (caller audit).
3. Naming: `get()` returning `Ok` on computed error slightly mismatches the intuitive "get the
   value" reading; confirm `get`/`value_state` names vs. alternatives. → Phase 2, low-risk.
4. Policy: a persisted `Status::Error` state is currently **not** fast-tracked (`:473` loads only
   `Ready/Source/Override`) → it re-evaluates. Confirm this is the intended behavior under the
   contract. → Phase 2.

*(Resolved during clarification: single `get() -> Result<State, Error>` with `Err` reserved
strictly for delivery failure and computed failures preserved as rich error-`State`s;
ergonomics via `State::value_state()` + error-checked value extraction and a private `data`
field. Separate ergonomic `get`, `AssetOutcome`, `poll_outcome`, and a separate
`AssetData.error` are all dropped as redundant with `State` + `Metadata.error_data`.)*

## References

- `plan20260707.md` WP-2 (F-2, F-3); `specs/ISSUES.md` → ASSET-MESSAGE-LIFECYCLE-ROBUSTNESS
- Code: `assets.rs` `poll_state` (:596), `get` (:1990), `finish_run_with_result` (:1326/:1359),
  `poll_state` error arm (:612); `state.rs` `from_error`/`error_result`/`is_error`;
  `metadata.rs` `with_error` (:1050), `error_result` (:1192), `error_data` field (:795)
- `specs/ASSETS.md` (Terminal outcome contract section to be added)
