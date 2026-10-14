---
id: POLARS-COLLECT-PANICS-ON-CURRENT-THREAD-RUNTIME
kind: issue
title: ns-pl commands panic inside LazyFrame::collect on a current-thread tokio runtime
status: draft
priority: P2
complexity: M
area: [lib/polars]
design:
created: 2026-10-13
github:
---
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
