---
id: PY-ENVIRONMENT-CANNOT-EVALUATE
kind: issue
title: liquers-py's Environment has no asset manager or recipe provider, so nothing can be evaluated from Python
status: draft
priority: P2
complexity: M
area: [py]
design:
created: 2026-10-19
github:
---
## Problem

`impl liquers_core::context::Environment for Environment` in `liquers-py/src/context.rs` is a stub:
`get_asset_manager`, `get_recipe_provider` and `apply_recipe` are `todo!()`, `get_async_store`
returns `NoAsyncStore`, and `init_with_envref` returns an error ("the Python environment does not
yet provide an asset manager"). No `EnvRef` can be built for it and no query can be evaluated, so
`pycall`, the value serializer and every command registered from Python have no execution path.

`get_asset_manager` and `get_recipe_provider` are `todo!()` behind a trait method a generic caller is
entitled to call: a panic on a reachable path, not merely a missing feature.

## Impact

`liquers-py` can parse queries, build plans and edit metadata, but cannot run anything. Work that
depends on evaluation (the `pycall` bridge, storing Python values) can be verified only in
isolation.

## Expected behaviour

The Python environment holds a store, an asset manager (`DefaultAssetManager`) and a recipe
provider, built the way `liquers-core`'s `SimpleEnvironment` / `EnvironmentBuilder` build theirs,
and exposes `evaluate` to Python, per `specs/design/python-wrapper/` (`EnvRef.evaluate*`). The two
`todo!()` accessors stop being reachable panics.

## Discovery

Found 2026-10-19 while designing `specs/design/py-orphan-modules/` and
`specs/design/py-value-serializer/`: neither can be tested end to end, because no Python environment
can evaluate a query.
