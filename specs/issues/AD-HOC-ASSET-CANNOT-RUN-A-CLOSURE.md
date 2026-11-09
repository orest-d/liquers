---
id: AD-HOC-ASSET-CANNOT-RUN-A-CLOSURE
kind: feature
title: The asset manager cannot run Rust code as an ad-hoc asset, only a recipe
status: closed
priority: P2
complexity: M
area: [core/assets, core/context]
design: ad-hoc-closure-asset
created: 2026-11-04
github:
---

## Problem

An ad-hoc asset runs a recipe: `AssetManager::apply(recipe, state, payload)` and
`Context::apply_ad_hoc` (`liquers-core/src/assets.rs`, `context.rs`). Code outside a command that
needs a `Context` for its own Rust work (status, cancellation, a log, dependencies, nested
evaluation) has no way to get one inside the asset lifecycle. Building one on
`AssetRef::new_temporary` plus `Context::new`, as `interpreter::evaluate_pipeline` does, runs outside
`run`: no status transitions, and on wasm no service loop processes its messages
(`template-render-in-env`, rejected alternatives).

**Example.** A Python script calls `env.render(template, state={"a": 1}, data={"b": 2})` for
`{{a}}{{b}}`. The render needs two frames and a `Context` for the template's query tags. Through
`apply`, only the state can be passed (the data would have to be encoded into the query), so the
call cannot return `12` as the same render inside a command does.

## Impact

Top-level template rendering in the language bindings (`liquers-py-templates`, Decision 1), each
async read of a top-level data-node view (`liquers-py-data-nodes`, `env.data_node`), and any future
top-level helper that needs a context. Workaround: a single-frame render through `apply`.

## Expected behaviour

The asset manager runs a closure as an ad-hoc asset: never keyed or cached, through the same run
path, with status, cancellation, log and dependency recording, on native and wasm. Points a design
must settle (found in review, 2026-11-04):

- **Shape.** A sibling of `compute` over `run_with_future` / `run_with_future_inline`
  (`liquers-core/src/assets.rs`): build the context with `create_context`, moving the payload
  (`Option<E::Payload>`) in, call the closure, then `check_resolved_volatility` and
  `take_pending_dependencies`, as `compute` does. The closure returns the asset's value
  (`E::Value`, the metadata belongs to the asset), plus an optional caller result `T` handed back
  beside the `AssetRef`.
- **Bounds.** `FnOnce(Context<E>) -> Fut + Send` with a `Send` future natively (the trait is
  `async_trait`); `MaybeSend` or a cfg'd boxed alias on wasm. A default trait method builds the
  asset itself, as `apply_queued` does.
- **Cancellation.** The handle is returned before the run, so a caller whose wait is interrupted
  cancels the asset and polls until it settles; dropping a running `run` would leak its
  service-loop task.
- **Drop.** A closure asset whose inline run is dropped ends `Cancelled` or `Error`, never
  repaired to `Recipe` (`InlineRunClaim::drop`), because the closure is consumed and the
  placeholder recipe would be evaluated instead.

A design is needed (`L`).

## Discovery

Orest's review of `liquers-py-templates` Phase 1 on 2026-11-04: templates must work outside
commands whenever an environment is available.

## Resolution

Closed 2026-11-05 by [`design/ad-hoc-closure-asset/`](../design/ad-hoc-closure-asset/DESIGN.md):
`AssetManager::closure_asset` and `ClosureAsset::run` / `run_inline` (commit `2b8348c4`); tests
`closure01`–`closure06` in `liquers-core/tests/closure_asset.rs`.
