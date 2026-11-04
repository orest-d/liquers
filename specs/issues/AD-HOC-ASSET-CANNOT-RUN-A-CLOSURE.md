---
id: AD-HOC-ASSET-CANNOT-RUN-A-CLOSURE
kind: feature
title: The asset manager cannot run Rust code as an ad-hoc asset, only a recipe
status: draft
priority: P2
complexity: L
area: [core/assets, core/context]
design:
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

Top-level template rendering in the language bindings (`liquers-py-templates`, Decision 1), and any
future top-level helper that needs a context. Workaround: a single-frame render through `apply`.

## Expected behaviour

The asset manager runs a closure (`FnOnce(Context<E>) -> Future<Output = Result<State, Error>>`)
as an ad-hoc asset: never keyed or cached, through the same run path, with status, cancellation,
log and dependency recording, on native and wasm. A design is needed (`L`).

## Discovery

Orest's review of `liquers-py-templates` Phase 1 on 2026-11-04: templates must work outside
commands whenever an environment is available.
