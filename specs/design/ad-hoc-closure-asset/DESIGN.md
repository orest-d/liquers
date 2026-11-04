---
id: AD-HOC-CLOSURE-ASSET
kind: design
title: The asset manager runs a Rust closure as an ad-hoc asset
form: compact
workflow: liquers-project
status: approved
phase: implementation
readiness: ready
autofix: not-eligible
area: [core/assets, core/context]
issues: [AD-HOC-ASSET-CANNOT-RUN-A-CLOSURE]
affects_docs: [specs/reference/ASSETS.md]
created: 2026-11-04
---
# The asset manager runs a Rust closure as an ad-hoc asset

A core prerequisite of milestone M6 of liquers-py: the top-level template render of
[`liquers-py-templates`](../liquers-py-templates/DESIGN.md) (its Decision 1, through
[`template-render-in-env`](../template-render-in-env/DESIGN.md)) and the top-level async reads of
[`liquers-py-data-nodes`](../liquers-py-data-nodes/DESIGN.md) (`env.data_node`). Lands in its own PR,
before both.

Written on 2026-11-04 under the M6 pre-approval (Orest: "proceed all", which covered "the core
closure primitive designed in its own folder and PR", `liquers-py-templates` Decision 2). Compact
form: the issue was filed as `L`, re-estimated `M` here (one provided trait method, one small type,
one private evaluation body, a `Drop` branch), and each phase was reviewed by its author.

## Phase 1: High-Level Design

### Purpose

Code outside a command that needs a `Context` for its own Rust work has no way to get one inside
the asset lifecycle. An ad-hoc asset can only run a recipe (`AssetManager::apply`). Add a closure
asset: the manager builds an ad-hoc asset, returns its handle, and runs a closure in it through the
same harness as `compute`, so the closure gets the asset's `Context` with status, cancellation,
log, dependency recording and nested evaluation. Never keyed, never cached, never stored.

### Problem Example

Verified at HEAD (`liquers-core/src/assets.rs`, `AssetManager::apply`, `AssetRef::run`,
`AssetRef::compute`): a binding's `env.render("{{a}}{{b}}", state={"a": 1}, data={"b": 2})` needs
two frames and a `Context` for the template's query tags. `apply(recipe, state, payload)` passes
one state and a query, so the data would have to be encoded into the query, and the render returns
`1` with `b` missing. `interpreter::evaluate_pipeline` builds a context on
`AssetRef::new_temporary` plus `Context::new` instead, which runs outside `run`: no status, no
cancellation race, and on wasm no service loop applies the log.

After: the binding calls `manager.closure_asset("template render")?`, keeps
`closure.asset().clone()` to cancel it, and awaits `closure.run(|context| async move { … }, None)`.
The closure renders with both frames through `ContextHost::new(context, cwd)` and returns
`(Value::from("12"), warnings)`: the asset ends `Ready` holding `"12"`, its log holds what the
render logged, and `run` hands back the warnings.

### Scope and Acceptance Criteria

- **AC-1** A closure runs as an ad-hoc asset
  WHEN `closure_asset("t")` is run with a body returning `(Value::from("x"), 7)`
  THEN `run` returns `Ok(Some(7))`, the asset is `Ready` with value `"x"`, its key is `None`, and two closure assets have different ids
- **AC-2** The context is the asset's
  WHEN the body logs `info("hello")` and evaluates `-R/a.txt` through its context
  THEN the asset's log holds `hello` and its metadata records the dependency on `a.txt`
- **AC-3** Cancelling a running closure
  WHEN the body waits forever and `asset().cancel()` is called from another task
  THEN `run` returns, the asset is `Cancelled`, and the body's future was dropped
- **AC-4** Cancelling before the run
  WHEN `asset().cancel()` is called before `run`
  THEN the body is never invoked and the asset is `Cancelled`
- **AC-5** A failing closure
  WHEN the body returns `Err`, and in another asset panics
  THEN both assets are `Error`, `run` returns the error, and the panic's message is named
- **AC-6** A dropped inline run ends cancelled
  WHEN `run_inline` is polled into the body and its future dropped
  THEN the asset is `Cancelled`, never repaired to `Recipe`, and its waiters are released
- **AC-7** Nothing is cached
  WHEN a closure asset has run
  THEN no key or query map holds it, and nothing is written to the store

Non-goals: a closure asset inside a parent's evaluation session (a command already has a
`Context`); queueing (like `apply`, it takes no job-queue slot); a closure that returns metadata
(the asset's metadata is the asset's).

### Documentation Intent

- Reference: `ASSETS.md` §AssetManager, a paragraph on closure assets (ad hoc, handle first, cancel
  by handle, drop is not cancellation).
- API docs: the module doc of `liquers-core/src/assets.rs` (its "Public API" and "Evaluation entry
  points" tables gain the row; its "Keyed?" table the closure asset).

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** not-eligible: a new public method on the public `AssetManager` trait.
- **Leading issue:** None.
- **Explanation:** every piece exists: `AssetData::new_ext` builds an unkeyed asset,
  `run_with_future` / `run_with_future_inline` are the harnesses, and the evaluation body of
  `evaluate_recipe_outcome` shows how a context's dependencies and volatility are collected.
- **Open questions:** None. Settled from the issue's review notes: the handle is returned first,
  the closure returns the value plus a caller result, `Drop` of an inline run ends `Cancelled`.

### Design Dependencies

- `liquers-py-templates`, `template-render-in-env`, `liquers-py-data-nodes`: **required-by**.

## Phase 2: Architecture

### Solution

A provided method on `AssetManager` builds the asset and wraps it in a `ClosureAsset`, whose
consuming `run` / `run_inline` start the harness with a private body, `AssetRef::compute_closure`.
The body mirrors `compute` and the tail of `evaluate_recipe_outcome`: catch a panic, resolve
volatility, `create_context`, install the payload, call the closure, collect pending dependencies,
`check_resolved_volatility`, and return `RecipeEvaluation { value, dependencies, origin: Computed }`.
The closure's `T` is written to a slot the harness future borrows, so no `'static` bound is
needed and nothing is stored in the asset.

The asset is created `Submitted`, so `AssetRef::cancel` before the run marks it `Cancelled` (a
`Recipe` asset is "not started, nothing to cancel") and the harness's `is_finished` check then
skips the body. `AssetData` gains `runs_closure: bool`; `InlineRunClaim::drop` reads it and marks a
dropped closure run `Cancelled` instead of repairing it to `Recipe`, because the closure is
consumed and the placeholder recipe would run instead.

Rejected:
- Storing the closure in `AssetData` (`Mutex<Option<Box<dyn FnOnce>>>`): the asset type gains a
  boxed future type per target, and a second `run` would find nothing; a consuming `run` makes
  "once" a type error.
- A recipe-shaped closure (a command registered for one call): it pollutes the registry and still
  cannot carry two frames.
- `AssetRef::new_temporary` plus `Context::new`: no harness (the issue).

Known issues: none open blocks. The limits of `AssetRef::cancel` (a native wait bounded at 5 s for
a body that ignores cancellation) apply unchanged.

### Changes

`liquers-core/src/assets.rs`:

```rust
/// An ad-hoc asset that runs a closure (`AssetManager::closure_asset`).
pub struct ClosureAsset<E: Environment> { asset: AssetRef<E> }

impl<E: Environment> ClosureAsset<E> {
    /// The asset, to cancel, follow or read. Clone it before `run` to cancel a running closure.
    pub fn asset(&self) -> &AssetRef<E>;
    /// Runs `body` under the spawning harness (native). `Ok(Some(t))` when the body produced its
    /// value, `Ok(None)` when it never ran or was cancelled, `Err` when it failed. Await it to the
    /// end: dropping it is not cancellation (the service loop task would be left behind).
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn run<T, F, Fut>(self, body: F, payload: Option<E::Payload>) -> Result<Option<T>, Error>
    where F: FnOnce(Context<E>) -> Fut, Fut: Future<Output = Result<(E::Value, T), Error>>;
    /// As `run`, under the single-task harness (wasm32, immediate-mode managers).
    pub async fn run_inline<T, F, Fut>(self, body: F, payload: Option<E::Payload>) -> Result<Option<T>, Error>
    where F: FnOnce(Context<E>) -> Fut, Fut: Future<Output = Result<(E::Value, T), Error>>;
}

// trait AssetManager<E>, provided:
/// A new closure asset: ad hoc (in no map, never reused, not keyed, never stored), `Submitted`,
/// with a placeholder recipe titled `title`. Takes no job-queue slot.
fn closure_asset(&self, title: &str) -> Result<ClosureAsset<E>, Error>;

// impl AssetRef<E>, private:
async fn compute_closure<T, F, Fut>(&self, body: F, payload: Option<E::Payload>, out: &mut Option<T>)
    -> Result<RecipeEvaluation<E::Value>, Error>;
```

- Sync and async: `closure_asset` is sync (it builds `AssetData` with `next_id_for_asset` and
  `try_get_envref`, as the default `apply_queued` does); `run` is async. No `Send` bound is
  written: the future is `Send` exactly when `F`, `Fut` and `T` are, which a native caller that
  spawns it needs and wasm does not.
- `AssetData`: field `runs_closure: bool`, `false` in every constructor, set by `closure_asset`.
- `InlineRunClaim::drop`, arm `Status::Processing | Status::Dependencies`: when `runs_closure`,
  `mark_cancelled` with the cause `"the closure run was dropped"` and send
  `StatusChanged(Cancelled)` and `JobFinished`; otherwise the existing repair. The `try_write`
  failure branch is unchanged (a later claim of an abandoned closure asset sees `runs_closure` and
  cancels instead of running the placeholder recipe: `try_claim_for_run_inline` gains that check).
- Errors: a body error and a panic go through `settle_run` like a command's
  (`Error::general_error("evaluation panicked: …")` for the panic); `closure_asset` fails only
  with `try_get_envref`'s error. No new constructor.
- Docs: the module doc tables and `ASSETS.md` (above). No command, registry or binding change.

### Risks

| Assessment | Finding |
|---|---|
| Files | `liquers-core/src/assets.rs`; a new `liquers-core/tests/closure_asset.rs`; `ASSETS.md` |
| `Submitted` without a queue | the job queue keeps its own list; a status alone queues nothing. AC-4 pins the cancel path |
| Existing tests | none change; `InlineRunClaim` tests keep `runs_closure = false` |
| Compatibility | additive: a provided trait method, a new type, a private field |
| wasm32 | `run` is native-only like `AssetRef::run`; `run_inline` compiles everywhere. `bash scripts/check-build-matrix.sh` (wasm rows) |
| Recovery | revert the commit |
| Certainty | High |

## Phase 3: Examples and Tests

### Examples

```rust
let closure = envref.get_asset_manager().closure_asset("template render")?;
let asset = closure.asset().clone();               // to cancel on an interrupt
let warnings = closure
    .run(|context| async move {
        let text = render_with_frames(&context, template, frames).await?;
        Ok((Value::from(text.text), text.warnings))
    }, None)
    .await?
    .unwrap_or_default();                         // None: cancelled before it produced
let state = asset.get().await?;                   // Ready, "12"
```

### Tests

`liquers-core/tests/closure_asset.rs`, on the in-memory environment of
`async_hellow_world.rs` (`DefaultAssetManager`, native) unless stated:

- `closure01_runs_value_and_result` — AC-1, AC-7
- `closure02_context_logs_and_records_dependencies` — AC-2
- `closure03_cancel_while_running` — AC-3 (the body awaits a `tokio::sync::Notify` that is never
  notified; a drop guard in it records the drop)
- `closure04_cancel_before_run` — AC-4
- `closure05_error_and_panic_end_in_error` — AC-5
- `closure06_dropped_inline_run_is_cancelled` — AC-6 (`run_inline`, polled once with
  `futures::poll!` into a pending body, then dropped)

Run: `cargo test -p liquers-core --test closure_asset`, then `cargo test -p liquers-core --lib`
for the `InlineRunClaim` tests.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/src/assets.rs` `AssetData` — `runs_closure` field; `InlineRunClaim::drop`
  and `try_claim_for_run_inline` branches — `cargo test -p liquers-core --lib`
- [ ] 2. `assets.rs` — `ClosureAsset`, `AssetManager::closure_asset`, `AssetRef::compute_closure`
  — `cargo test -p liquers-core --test closure_asset`
- [ ] 3. Module doc tables of `assets.rs`; `specs/reference/ASSETS.md` §AssetManager (History row,
  `reviewed:`) — `python3 scripts/check_api_docs.py liquers-core/src/assets.rs`
- [ ] 4. Wider checks — `cargo test -p liquers-core`, `cargo test -p liquers-lib --lib --tests`,
  `bash scripts/check-build-matrix.sh`

Rollback: revert the PR's commits; nothing else depends on it until `template-render-in-env` lands.

### Validation

`cargo test -p liquers-core`; the build matrix (wasm32 rows); `check_api_docs.py` on `assets.rs`;
`python3 scripts/docs_index.py --check`. The issue `AD-HOC-ASSET-CANNOT-RUN-A-CLOSURE` is closed
in Phase 5.
