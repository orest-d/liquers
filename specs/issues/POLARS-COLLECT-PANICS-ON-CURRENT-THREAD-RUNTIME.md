---
id: POLARS-COLLECT-PANICS-ON-CURRENT-THREAD-RUNTIME
kind: issue
title: ns-pl commands panic inside LazyFrame::collect on a current-thread tokio runtime
status: closed
priority: P2
complexity: M
area: [lib/polars]
design:
created: 2026-10-13
github:
---
## Resolution

Fixed 2026-10-14 with the first option below. `liquers-lib/src/polars/util.rs` adds
`CollectOnAnyRuntime::collect_on_any_runtime` for `LazyFrame`:
- on a current-thread Tokio runtime it runs `collect` on a scoped OS thread, which has no Tokio
  context, so polars' `block_in_place` just runs the closure;
- on a multi-threaded runtime, or outside any runtime, it is `collect()` unchanged;
- a panic inside polars is re-raised on the calling thread.

All 16 lazy `collect` calls in `liquers-lib/src/polars/` (aggregation, filtering, sorting, serde)
use it. The registry's `impl_version`s were refreshed.

Tests:
- `polars::util::tests::collect_on_any_runtime_works_on_a_current_thread_runtime`, and the same
  on a multi-threaded runtime and with no runtime;
- `liquers-lib/tests/records_table_ops.rs`: its `ns-pl` tests now run on the default
  current-thread `#[tokio::test]`. They hung before the fix.

## Problem

Every `ns-pl` command that goes through a `LazyFrame` (`filter`, `sort`, the comparison
filters, …; `liquers-lib/src/polars/filtering.rs`, `sorting.rs`) calls `LazyFrame::collect`. At
polars 0.55 `collect` converts the plan with `polars_async::RuntimeManager::block_in_place_on`
(`polars_plan::plans::conversion::dsl_to_ir::fetch_metadata`), which calls Tokio's
`block_in_place` and panics on a current-thread runtime:

```
panicked at polars-async-0.55.2/src/lib.rs:81:13:
can call blocking only when running on the multi-threaded runtime
```

A Liquers command runs on whatever runtime evaluates the query, so an `ns-pl` query evaluated
from a current-thread runtime (`#[tokio::test]`'s default, a `current_thread` binary, and
possibly a wasm-like inline manager) panics. `DataFrame`-only commands such as `pl/shape` and
`pl/schema` are unaffected.

## Impact

`ns-pl` queries cannot be evaluated in a current-thread runtime. Since
`COMMAND-PANIC-HANGS-EVALUATION` was fixed (2026-10-14), the panic fails the asset with
`evaluation panicked: can call blocking only when running on the multi-threaded runtime` instead of
hanging it. The existing
`polars_commands.rs` tests avoid it only because they call polars directly instead of the
commands (`POLARS-COMMAND-TESTS-BYPASS-COMMANDS`). Workaround: evaluate on a multi-threaded
runtime (`#[tokio::test(flavor = "multi_thread")]`, as `liquers-lib/tests/records_table_ops.rs`
does).

## Expected behaviour

An `ns-pl` command works, or fails with an error, on any runtime. Options, not chosen here:
- collect on a dedicated thread (`std::thread::spawn` / a rayon pool) outside the Tokio context,
  so polars' runtime manager sees no current-thread runtime;
- detect `Handle::current().runtime_flavor()` and refuse with a clear error;
- use polars' eager `DataFrame` API (`filter`, `sort`) where it suffices, avoiding the lazy plan.

## Discovery

2026-10-13, implementing `record-filter-commands` Step 10: the first tests that evaluate
`ns-pl/sort`, `ns-pl/eq` and `ns-pl/tail` through `EnvRef::evaluate` under `#[tokio::test]`
panicked with the message above and then hung.
