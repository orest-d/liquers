# Phase 4: Implementation Plan — Record and polars command interoperability

## Overview

The order is: kernels in `liquers-records` first, since they have no dependents; then the
conversion fixes; then the `ns-rec` commands; then the `ns-pl` changes; then the integration tests
and the registry. Each step compiles and tests on its own. No prerequisite issue needs fixing first
(Phase 2 preflight).

## Progress

- [x] Step 1: Enable and verify the polars features — c20a625, d0f8e3c (`strings`, `regex`; `is_in` not needed)
- [x] Step 2: `Column::sort_indices` — c20a625
- [x] Step 3: `RecordBatch::empty` and `MappedSource` — 40a0d7a
- [x] Step 4: Search constructors and `StrictPredicate` — e02af0a
- [x] Step 5: Lenient schema deserialization — 20169c9
- [x] Step 6: UTF-8 bytes as a `String` argument — 56584d7
- [x] Step 7: Schema argument JSON-first; DataFrame into `ns-rec` converters — 56584d7
- [x] Step 8: `ns-rec` table operations and registration — 56584d7
- [x] Step 9: `ns-pl` record input, `sort`, new filters, `i64` counts — d0f8e3c
- [x] Step 10: Integration tests `records_table_ops.rs` — 7428608, b4da9ee
- [x] Step 11: Regenerate `specs/command_registry.yaml` — f52a4d2
- [ ] Step 12: Final checks

## Implementation Steps

### Step 1: Enable and verify the polars features
- Files / symbols: `liquers-lib/Cargo.toml` `polars` dependency features.
- Change: add `is_in` and `strings`, and `regex` only if `str().contains` needs it. Confirm the
  `Expr::is_in(other, nulls_equal)` and `col(..).str().starts_with` paths compile.
- Depends on: none.
- Proof: `cargo check -p liquers-lib`.
- Rollback: revert `Cargo.toml`.

### Step 2: `Column::sort_indices`
- Files / symbols: `liquers-records/src/column.rs` `impl Column`.
- Change: `pub fn sort_indices(&self, ascending: bool, nulls_last: bool) -> Result<Vec<u32>, Error>`.
  It is a stable `sort_by` over row indices, with a per-type comparator: `f64::total_cmp` puts NaN
  last among numbers, Text compares by bytes, and Binary and Vector are refused. Nulls go into a
  separate partition, placed first or last.
- Depends on: none.
- Proof: `cargo test -p liquers-records --lib column::tests::sort_indices` (the two Phase 3 unit
  tests).
- Rollback: delete the method.

### Step 3: `RecordBatch::empty` and `MappedSource`
- Files / symbols: `liquers-records/src/batch.rs` `RecordBatch::empty` (via `Column::empty`, which
  is `pub(crate)`); `liquers-records/src/sources.rs` `ViewMap` and `MappedSource`; re-exports in
  `lib.rs`.
- Change: as in Phase 2 §Interfaces. `stream` maps each item with `map`, and `schema` is computed
  once in `new` by mapping `RecordBatch::empty(inner_schema)`. `RecordStreamExt::materialize` keeps
  its own empty-batch path.
- Depends on: none.
- Proof: `cargo test -p liquers-records --lib sources::tests::mapped_source`.
- Rollback: delete the type.

### Step 4: Search constructors and `StrictPredicate`
- Files / symbols: `liquers-records/src/search/syntax.rs` `SearchExpr::{field, any, not, always}`
  and `FieldTest::{prefix, suffix, substring}`; `search/predicate.rs` `StrictPredicate`; exports in
  `search/mod.rs`.
- Change: as in Phase 2. `StrictPredicate::bind` checks each required name against
  `schema.fields`, then delegates. The other methods delegate.
- Depends on: none.
- Proof: `cargo test -p liquers-records --lib search::` (the two unit tests).
- Rollback: delete them.

### Step 5: Lenient schema deserialization
- Files / symbols: `liquers-records/src/schema.rs` `FieldRole` (`#[serde(default)]`), and
  `IndexKind` and `Analyzer` (a hand-written `Deserialize`; `Serialize` stays derived).
- Change: the visitor accepts `visit_str` for unit variants, `visit_map` with a single key, and
  `visit_enum` for YAML tags.
- Depends on: none.
- Proof: `cargo test -p liquers-records --all-features --lib --tests` (the three schema unit tests,
  plus the existing IPC and manifest tests, unchanged).
- Rollback: restore the derives.

### Step 6: UTF-8 bytes as a `String` argument
- Files / symbols: `liquers-lib/src/value/simple.rs` `impl TryFrom<SimpleValue> for String`;
  `liquers-core/src/value.rs` (the equivalent impl near line 950).
- Change: add a `Bytes` arm, `String::from_utf8(..)` mapped to `Error::conversion_error`.
- Depends on: none.
- Proof: `cargo test -p liquers-core --lib value::` and
  `cargo test -p liquers-lib --lib value::simple`.
- Rollback: remove the arm.

### Step 7: Schema argument JSON-first; DataFrame into `ns-rec` converters
- Files / symbols: `liquers-lib/src/records/commands.rs` `parse_schema_argument`;
  `liquers-lib/src/records/convert.rs` `to_record` and `to_record_source`.
- Change: try JSON first when the trimmed text starts with `{`. Add a `"polars.DataFrame"` arm
  under `#[cfg(feature = "polars")]`, using `super::polars::dataframe_to_record_batch`.
- Depends on: Step 5.
- Proof: `cargo test -p liquers-lib --lib records::`.
- Rollback: revert both functions.

### Step 8: `ns-rec` table operations and registration
- Files / symbols: new `liquers-lib/src/records/table_ops.rs` (`TableInput`, `table_input`, and
  the commands in Phase 2); `records/mod.rs` (`pub mod table_ops`); `records/commands.rs`
  `register_records_commands!`.
- Change: register the commands. `register_alias(rec/records_schema → rec/schema)`. In
  `select_columns` and `head`, dispatch on `TableInput` so that a source is wrapped lazily.
  Existing tests asserting that a source is refused are updated.
- Depends on: Steps 2–4 and 7.
- Proof: `cargo test -p liquers-lib --lib records::` and `cargo test -p liquers-lib --test
  records_end_to_end --test search_commands`.
- Rollback: drop the module and its registrations.

### Step 9: `ns-pl` record input, `sort`, new filters, `i64` counts
- Files / symbols: `liquers-lib/src/polars/util.rs` `try_to_polars_dataframe`; `sorting.rs`
  `sort` and its registration; `filtering.rs` `is_in`, `is_null`, `is_not_null`, `starts_with`,
  `ends_with`, `contains` and their registration; `info.rs` `shape`, `nrows` and `ncols`.
- Change: as in Phase 2. `sort` uses
  `SortMultipleOptions::default().with_order_descending(!ascending).with_nulls_last(nulls_last).with_maintain_order(true)`.
- Depends on: Step 1.
- Proof: `cargo test -p liquers-lib --test polars_commands` (updated for the `sort` flag) and `cargo
  check -p liquers-lib --no-default-features --features polars`.
- Rollback: revert the four files.

### Step 10: Integration tests `records_table_ops.rs`
- Files / symbols: `liquers-lib/tests/records_table_ops.rs`.
- Change: the Phase 3 tests and fixture.
- Depends on: Steps 8 and 9.
- Proof: `cargo test -p liquers-lib --test records_table_ops`, and the same test under
  `--no-default-features --features records`.
- Rollback: delete the file.

### Step 11: Regenerate `specs/command_registry.yaml`
- Change: `cargo run -p liquers-lib --features cli --bin export-command-registry -- --format yaml
  -o specs/command_registry.yaml`, and a dated CHANGELOG line.
- Depends on: Steps 8 and 9.
- Proof: `cargo test -p liquers-lib --test registry_export`.
- Rollback: `git checkout specs/command_registry.yaml`.

### Step 12: Final checks
- Proof:
  - `cargo test -p liquers-records --all-features --lib --tests`
  - `cargo test -p liquers-lib --lib --tests`
  - `cargo test -p liquers-core --lib`
  - `bash scripts/check-build-matrix.sh`
- Rollback: none; it only checks.

Steps 2–6 are independent and can run in any order.

## Testing Plan

Each step runs its own proof. Step 10 runs the integration suite in the default configuration and
in the records-only configuration. Step 12 runs the crate suites and the feature matrix, which is
required because new `cfg(feature)` arms are added in `convert.rs` and `util.rs`. Run `cargo
clean` before the matrix if disk is low (CLAUDE.md §"When a build fails…").

## Rollback Plan

Each step is a separate commit and reverts on its own, in reverse order. If only Steps 2–6 have
landed, they are dormant additions: no command uses them, and behaviour changes only through the
serde leniency and the bytes conversion, both of which only widen what is accepted. If the work
stops partway, the unticked steps become an issue (§5.6).

## Documentation Updates

Phase 5 updates the documents in Phase 2 §Documentation Architecture:
- `RECORD_STREAMS.md`, `POLARS_COMMAND_LIBRARY.md`, `RECORD_STREAM_GUIDE.md`, `SEARCH.md` and
  `COMMAND_ALIASES.md`, each with a History row and a `reviewed:` bump;
- `specs/README.md`, linking the design;
- `specs/index.csv` (`python3 scripts/docs_index.py`);
- the `specs/command_registry.yaml` changelog (Step 11);
- closing `RECORD-AND-POLARS-COMMANDS-NOT-INTEROPERABLE` and `RECORD-SOURCE-WRAPPERS-UNSPECIFIED`.

## Phase 5 Entry Criteria
- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour

## Consistency Pass (2026-10-13)

All phases were read together.
- **Every AC has a test.** AC-16 is covered by a review check, because it records an
  investigation.
- **No contradictions.** Phase 1 AC-18 says `head` over a source returns a source, and Phase 2
  builds it with `FilteredSource` and a limit, as Phase 3 tests. `tail` and `sort` materialize,
  which Phase 2 states and Phase 3 tests.
- **Every step has a proof.** The risks map to tests: old `sort` spelling, serialization
  unchanged, and the bridge's Vector error.
- **One assumption is left to check:** the polars feature names, checked first in Step 1.
- **Decision log:** empty. Nothing needs the maintainer before implementation.
