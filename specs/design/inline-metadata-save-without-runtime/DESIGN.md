---
id: INLINE-METADATA-SAVE-WITHOUT-RUNTIME
kind: design
title: The immediate manager persists metadata without spawning on native targets
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [core/assets]
issues: [METADATA-SAVER-SPAWNS-ON-THE-INLINE-PATH]
created: 2026-10-20
---
# The immediate manager persists metadata without spawning on native targets

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20: Phases 1-4 written; Phase 4
implemented on 2026-10-27 (automatic fix).

## Phase 1: High-Level Design

### Purpose

Make the immediate manager's documented "spawn-free" contract hold for every command, not only for
commands that never log, so a native embedder can drive it with any executor — the first concrete
step of `CORE-TOKIO-REMOVAL` (`design/executor-neutral-core/`).

### Problem Example

```rust
// test-only command `log_greet`: ctx.info("greeting")?; Ok(Value::from("hello"))
let mut env = ImmediateEnvironment::<Value>::new();
/* register log_greet */
let envref = env.to_ref();
futures::executor::block_on(async move {
    envref.get_asset_manager().get_asset(&parse_query("log_greet")?).await?.get().await
});
```

Today (from code): the `LogMessage` reaches `MetadataSaver::save_immediately`, which calls
`tokio::spawn` on any non-wasm target → panic "there is no reactor running". Should: returns
`"hello"`, and the asset's log holds "greeting".

### Scope and Acceptance Criteria

- **AC-1** A logging command runs with no runtime
  WHEN the immediate manager evaluates a non-keyed query whose command calls `context.info` and sets progress, under `futures::executor::block_on`
  THEN it returns the value, and the asset's metadata log contains the message
- **AC-2** A keyed logging command persists its metadata with no runtime
  WHEN the same command is evaluated by key (`log_greet/logged.txt` recipe) under `block_on`
  THEN it returns the value, and the store's metadata for `logged.txt` contains the log entry
- **AC-3** The queued manager keeps its background saver
  WHEN `DefaultAssetManager` evaluates the same commands under tokio
  THEN every existing `manager_parametric` and `assets.rs` test passes unchanged

Non-goals: `AssetRef::new_temporary` (spawns its service loop natively; reached only through the
deprecated `interpreter::evaluate` and `create_temporary_asset`, which nothing in-tree calls) and
`AssetRef::cancel`'s native `tokio::time::timeout`; both stay in `executor-neutral-core`'s inventory.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — a bug fix restoring the documented spawn-free contract, inside
  one private method body in `liquers-core/src/assets.rs`, no signature or structure change, tests
  in the same crate.
- **Leading issue:** None
- **Explanation:** the decision rule already exists in `persist_with_status_tracking` (check
  `EvalMode::Inline`); this applies it to the one other spawn on the inline path.
- **Open questions:** None

### Design Dependencies

- `executor-neutral-core` (`CORE-TOKIO-REMOVAL`) — **required-by**: its inventory counts this site
  as fixed; it does not need to wait for it.
- `inline-run-claim-repair-under-lock-contention` — **overlaps**, weak: same file, different
  functions; no order.

## Phase 2: Architecture

### Solution

In `MetadataSaver::save_immediately` (`liquers-core/src/assets.rs`), replace the target-only fork
with a mode-aware one: on native, read `envref.get_asset_manager().eval_mode()` (synchronous trait
method, already used by `persist_with_status_tracking`); `EvalMode::Inline` takes the existing
inline branch (take `pending`, drop the lock, `set_metadata` when `stored()`), `EvalMode::Queued`
keeps the spawned, debounced worker. wasm stays the inline branch unconditionally. Match `EvalMode`
exhaustively. The inline body is factored into a private `async fn persist_inline(…)` shared by
both targets, so the two copies cannot drift.

Rejected: a `bool` parameter on the `pub` method (interface change); making `MetadataSaver` own an
`EvalMode` (new field set at every construction; the envref already knows).

### Changes

Only `MetadataSaver::save_immediately` and a new private helper. `MetadataSaver::close` already
handles a missing `in_flight` (it is `None` on the inline path). No error-path change: store errors
on this path are ignored today (`let _ =`) and stay so.

### Risks

Inline native persistence writes on every log line instead of debouncing at 100 ms — the
behaviour wasm already has; acceptable for an inline manager. Certainty high; the only test likely
to change is none.

## Phase 3: Examples and Tests

### Examples

The problem example is the test. Recipe for AC-2: `recipes:\n  - query: log_greet/logged.txt`,
queries validated with `liquers-validate --command log_greet`.

### Tests

In `liquers-core/tests/manager_parametric.rs`, beside `immediate_keyed_eval_without_tokio_runtime`,
with a helper `register_log_greet` in `tests/common/manager_scenarios.rs` (command calls
`ctx.info("greeting")?` and `ctx.progress(ProgressEntry::…)?`, returns `"hello"`):

- `immediate_logging_command_without_tokio_runtime` (AC-1): plain `#[test]`, `block_on`; asserts
  the value and that `asset.get_metadata().await?` log contains "greeting".
- `immediate_keyed_logging_command_without_tokio_runtime` (AC-2): store from a `recipe_store`
  variant listing `log_greet/logged.txt`; asserts the value and the stored metadata's log.
- AC-3: the existing suites — `cargo test -p liquers-core --test manager_parametric` and
  `cargo test -p liquers-core --lib assets`.

Run: `cargo test -p liquers-core --test manager_parametric without_tokio_runtime`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-core/tests/common/manager_scenarios.rs` `register_log_greet`, and the two tests
  in `tests/manager_parametric.rs` — confirm they panic at HEAD (reproduction) —
  `cargo test -p liquers-core --test manager_parametric logging_command`
- [x] 2. `liquers-core/src/assets.rs` `MetadataSaver::save_immediately` + private `persist_inline`
  — mode-aware fork as Phase 2 — same command, both tests pass
- [x] 3. Regression: `cargo test -p liquers-core --lib --tests`; `cargo check -p liquers-core
  --target wasm32-unknown-unknown` (the wasm branch now calls the shared helper)

### Validation

Update `specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` (persistence: metadata saves are
inline under the immediate manager on every target) with a History row and `reviewed:` bump; close
`METADATA-SAVER-SPAWNS-ON-THE-INLINE-PATH` with the commit; `python3 scripts/docs_index.py` and
`--check`. If step 1 does not panic, stop: close the issue `closed_not_planned` with the evidence.
