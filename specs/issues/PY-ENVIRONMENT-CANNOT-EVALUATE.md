---
id: PY-ENVIRONMENT-CANNOT-EVALUATE
kind: issue
title: liquers-py's Environment has no asset manager or recipe provider, so nothing can be evaluated from Python
status: closed
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

Found 2026-10-19 while designing `specs/design/py-value-serializer/` and an orphaned-module design
(withdrawn once PY-MODULES-NOT-DECLARED-IN-LIB was closed by deletion): neither could be tested end
to end, because no Python environment can evaluate a query.

## Resolution

Found already fixed during bulk-design triage, 2026-10-20 (draft → closed, §4.3). The issue was
filed on 2026-10-19 against a branch that predated `design/liquers-py/` landing on main
(orest-d/liquers#150, #163); its description of `liquers-py/src/context.rs` (a stub
`impl Environment` with `todo!()` accessors) no longer matches the code. At HEAD (18732dd):

- `lq.Environment` wraps an `EnvCell` that builds a `DefaultEnvironment<Value>` through
  `default_environment_builder` (`liquers-py/src/environment.rs`, `EnvCell::build`, commit
  3cc4e0b), so it has `liquers-lib`'s asset manager, its default recipe provider and a store
  (memory by default, or the configured one).
- `Environment.evaluate`, `evaluate_state` and `get_asset` evaluate through `EnvRef::evaluate`;
  `Context.evaluate` does nested evaluation (`liquers-py/src/context.rs`, now the execution
  context, not an `Environment` impl).
- No `todo!()` or `unimplemented!()` remains in `liquers-py/src`.
- Evidence: `liquers-py/tests/test_EVAL_evaluation.py` (`test_EVAL01`–`06`, including
  `test_EVAL_text_resource_read_from_store`) and the `EVAL` row of `liquers-py/README.md`.

The `pycall` bridge the issue mentions no longer exists: Python commands are closures
(`liquers-py/src/command.rs`), per `reference/PYTHON_INTEGRATION.md`.
