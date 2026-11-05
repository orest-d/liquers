# Phase 4: Implementation Plan — liquers-py-data-nodes

## Overview

M6 lands in this order, each in its own PR: `foreign-value-data-node` (PR #250, open),
`ad-hoc-closure-asset`, `template-render-in-env`, then this design, then `liquers-py-templates`,
then `liquers-py-pipelines`. This design's PR is cut from `main` after the first three have merged:
it needs only the first two, but Step 5 edits `inner_node_of` in `liquers-lib/src/template/host.rs`,
which `template-render-in-env` also changes, so this design rebases on it. Inside it:
1. The adapter and the hook, testable through `lq.data_node` and the existing template commands.
2. Lazy `State`, which changes where a conversion error surfaces; the existing suite shows what
   moves.
3. The view with its three roots, then the awaitable closure run for `Env` reads.

Every step that adds a Python name exports it from `python/liquers/__init__.py` (and `__all__`)
with a docstring, and adds it to `__init__.pyi` / `_native.pyi`, since `test_DOCS_docstrings.py`
and stubtest check every exported name.

## Progress

- [x] Step 1: `PyNode` and `node_of` (classification, lookups, `to_json`)
- [x] Step 2: the `ForeignValue::data_node` hook on `PyObjectValue`
- [x] Step 3: opt-in protocols (`__liquers_data__`)
- [x] Step 4: lazy `lq.State`
- [x] Step 5: `context_node_kind` in liquers-lib (its test, `context_node_kind_agrees_with_node_of`, sits in `liquers-lib/tests/template_top_level.rs` beside the other host tests rather than in `host.rs`)
- [x] Step 6: `lq.NodeKind`, `lq.DataNode`, `lq.data_node`, sync roots (`Node`, `Command`)
- [x] Step 7: `closure::awaitable_run` and the `Env` root (cancelling the awaitable also cancels the private asset; Phase 2 updated)
- [x] Step 8: conformance row, docs, module docs

## Implementation Steps

### Step 1: `PyNode` and `node_of`
- Files / symbols: `liquers-py/src/data_node.rs` (new) `PyNode`, `Shape`, `node_of`, `impl DataNode
  for PyNode`; `liquers-py/src/value.rs` `attach_off_worker` made `pub(crate)`, and the out-of-range
  integer error, inline in `py_to_simple` today, factored into `int_out_of_range_error` (new), shared
  by both; `lib.rs` `mod data_node`.
- Change: the classification table and the standard-type lookups of Phase 2; `to_json` with the
  identity set.
- Depends on: none.
- Proof: step-local Rust unit tests in `data_node.rs` `tests` (attach to an embedded interpreter,
  as the existing `value.rs` tests do) for kinds, missing keys, the integer and NaN edges and a
  cycle; `cargo test -p liquers-py --no-default-features --features debug-handles --lib data_node`
  (the form `scripts/test-liquers-py.sh` uses, since `extension-module` does not link a test).
- Rollback: delete the module.

### Step 2: the hook
- Files / symbols: `value.rs` `impl ForeignValue for PyObjectValue` `data_node`.
- Change: `fn data_node(self: Arc<Self>) -> AnyNode`, building `node_of` under `attach_off_worker`.
- Depends on: Step 1; PR #250 merged.
- Proof: `test_DATANODE_render.py::test_retained_value_renders_in_place` (plain `dict` variant),
  `test_worker_lookup_does_not_deadlock`.
- Rollback: remove the method; the core default answers again.

### Step 3: opt-in protocols
- Files / symbols: `data_node.rs` `node_of` (the opt-in branch), `Shape::ProtocolMap`,
  `ProtocolList`, protocol scalars.
- Change: type-level lookup of `__liquers_data__`; `True` and method forms; raising entries opaque.
- Depends on: Step 1.
- Proof: `test_DATANODE_protocols.py` (all), the two other variants of
  `test_retained_value_renders_in_place`, `test_reentrant_getitem_completes` (it needs an opted-in
  `__getitem__`) and `test_large_render_cost` (timing recorded).
- Rollback: the branch returns `Opaque`.

### Step 4: lazy `lq.State`
- Files / symbols: `liquers-py/src/asset.rs` `State` (`core`, `scope`, `PyOnceLock` value),
  `State::from_core`, `value`, `record_view`.
- Change: conversion on first access of `value`.
- Depends on: none (can run in parallel with Steps 1-3).
- Proof: the whole existing suite, `bash scripts/test-liquers-py.sh`; any test that expected a
  conversion error from `evaluate_state` / `get_state` is moved to `.value` and listed in the
  commit message; `test_DATANODE_views.py::test_state_value_is_lazy`.
- Rollback: revert the commit; `State` converts eagerly again.

### Step 5: `context_node_kind`
- Files / symbols: `liquers-lib/src/template/host.rs` `context_node_kind`, `inner_node_of`.
- Change: the function and its use in `inner_node_of`.
- Depends on: none.
- Proof: `cargo test -p liquers-lib --lib template` (`context_node_kind_agrees_with_node_of`);
  `cargo test -p liquers-lib --no-default-features --features templates --lib --tests`.
- Rollback: revert; `inner_node_of` decides inline again.

### Step 6: the view, sync roots
- Files / symbols: `data_node.rs` `NodeKind`, `DataNode`, `Root::{Node, Command}`, `Step`,
  `data_node` (module function); `asset.rs` `State.data_node`; `context.rs` `Context.data_node`;
  `lib.rs` registration.
- Change: the members of Phase 2; `Command` root built with `clone_handle()`.
- Depends on: Steps 1, 4, 5.
- Proof: `test_DATANODE_mapping.py` (all), `test_DATANODE_views.py::test_frame_cell_without_conversion`,
  `test_command_view_expires`, `test_state_value_is_lazy`.
- Rollback: remove the classes; the hook stays.

### Step 7: `awaitable_run` and the `Env` root
- Files / symbols: `liquers-py/src/closure.rs` (new) `awaitable_run`; `data_node.rs`
  `Root::Env`, the `_async` members; `environment.rs` `Environment.data_node`; `aio.rs` a
  `ToPy::Node` variant for an awaited view.
- Change: the `_async` members of every root; each `Env` async read is one
  `closure_asset("data node read")` run.
- Depends on: Step 6; `ad-hoc-closure-asset` merged.
- Proof: `test_DATANODE_views.py::test_async_nodes_in_a_command` (the `Command` root's `_async`
  members land here), `test_async_nodes_at_top_level`, `test_env_view_after_shutdown`,
  `test_env_read_cancelled_at_shutdown` (shutdown during a read: `lq.CancelledError`, the asset
  ends `Cancelled`),
  `test_cancelled_await_cancels_the_read`.
- Rollback: `Environment.data_node` without async reads (sync accessors only).

### Step 8: records and docs
- Files / symbols: `liquers-py/README.md` DATANODE row; the documents below; module docs.
- Depends on: Steps 1-7.
- Proof: `python3 scripts/check_api_docs.py` on each touched module; `python3 scripts/docs_index.py --check`.
- Rollback: documentation only.

## Testing Plan

Each step's proof runs as it lands. At the end: `bash scripts/test-liquers-py.sh` (CPython 3.14
and, in CI, 3.10 and 3.14t), `cargo test -p liquers-lib --lib --tests`, and
`bash scripts/check-build-matrix.sh` for the `templates` rows (Step 5 touches a `templates` module).
`test_large_render_cost` records its timing in Phase 5's learning section.

## Rollback Plan

The PR reverts as a whole. If only Steps 1-3 land, Python data renders in place and nothing else
changes. Step 4 is independent and can be reverted alone.

## Documentation Updates

- `specs/reference/PYTHON_INTEGRATION.md`: new §Data nodes; §5 lazy `State.value` (History row,
  `reviewed:`).
- `specs/guides/PYTHON_GUIDE.md`: "Data nodes" section (History row, `reviewed:`).
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §DATANODE, §6 Python; `specs/guides/TEMPLATE_GUIDE.md`
  one line (History rows, `reviewed:`).
- `liquers-py/README.md` conformance row; `specs/README.md` M6 entry links.
- Module docs per Phase 2's table, checked with `check_api_docs.py`; `specs/index.csv` regenerated.
- The user guide chapters `python3 scripts/docs_index.py chapters-for <affects_docs>` lists are
  reviewed in Phase 5.

## Phase 5 Entry Criteria

- [x] Implementation finished and validated (`bash scripts/test-liquers-py.sh`: 446 passed)
- [x] User and review comments answered
- [x] Documentation checkable against implemented and tested behaviour
