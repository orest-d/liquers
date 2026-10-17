# LazyFrame over a record stream — investigation (AC-16)

Checked on 2026-10-13 against the sources of `polars-lazy` and `polars-plan` 0.55.2, the versions
in `Cargo.lock`.

## What polars offers

- `polars_plan::plans::AnonymousScan` (re-exported by `polars::prelude`) is the custom-source hook.
  `LazyFrame::anonymous_scan(Arc<dyn AnonymousScan>, ScanArgsAnonymous)` builds a `LazyFrame`
  from it. The trait is public, and polars tests it (`polars-lazy/src/tests/io.rs`,
  `scan_anonymous_fn_with_options`).
- `fn scan(&self, AnonymousScanArgs) -> PolarsResult<DataFrame>` is **synchronous** and returns
  **one whole `DataFrame`**. It does not yield batches. `AnonymousScanArgs` carries `with_columns`
  (projection), `predicate: Option<Expr>` and `n_rows`, and the pushdowns are opt-in through
  `allows_projection_pushdown` and `allows_predicate_pushdown`.
- `schema()` can be supplied, so a source with a declared `uniform_schema` could answer without
  opening a chunk.
- The batch-yielding scan (`PythonScan` / the streaming engine's generator source) exists only
  behind the `python` feature. It is not usable from Rust.

## Consequences

1. **`AnonymousScan` is not used.** `RecordSource::stream` is async, but `scan` runs inside the
   synchronous `collect`. Calling the source from `scan` would need `block_on` inside a Tokio
   runtime, which is blocking in an async context (forbidden by `CLAUDE.md`, and it panics on a
   current-thread runtime). Even if that worked, `scan` returns the whole frame, so nothing is
   gained over loading the chunks first.
2. **Considered and not taken: async commands that build the `LazyFrame` themselves.** An `ns-pl` command
   becomes `async fn …(state, …, context)`. Its input becomes a `LazyFrame` in three ways:
   - a `DataFrame` with `.lazy()`;
   - a `RecordView` through `record_batch_to_dataframe`, then `.lazy()`;
   - a `RecordSource` by awaiting its chunks one by one, converting each, and joining them with
     `polars::prelude::concat`. Its size is bounded by the same materialization limit as
     `ns-rec/materialize`.

   The command applies its operation to the `LazyFrame`, then `collect`s and returns a
   `PolarsDataFrame` value. **The result is always an eager DataFrame.** No `LazyFrame` value
   type is added.
3. **Possible, but it would duplicate `ns-rec`.** A row-local operation (filters,
   `select_columns`, `drop_columns`) can run its lazy plan on each chunk and concatenate the
   results, so peak memory is one chunk plus the output. `sort`, `tail` and the aggregations need
   the whole input.
4. **`collect` is CPU-bound.** Phase 2 decides whether it runs on `spawn_blocking` (native) or
   inline. polars is not built for wasm32, so only native applies.

## Outcome

The aim was for a `LazyFrame` to avoid materializing an incoming stream, so that a column selection
would hold only the selected columns. **At polars 0.55 that is not possible**, because a
`LazyFrame` cannot pull an async stream or push a projection into it. Consequence 2's route
(async loading, then a lazy `concat`) still loads every chunk in full, so it gains nothing over
`ns-rec/materialize`.

**Maintainer decision, 2026-10-13:** `ns-pl` stays synchronous and eager. A stream reaches it
through `ns-rec/materialize` and the bridge. Streaming laziness lives in `ns-rec` (Phase 1 AC-18):
`select_columns`, `drop_columns`, the filters and `head` wrap a source, so whatever is
materialized afterwards holds only what was selected.

Revisit if a polars release exposes a batch-yielding scan to Rust. No issue is filed for that.
