# Phase 5: Documentation - Record and polars command interoperability

## Completion Preconditions

- [x] Implementation is finished and validated (see Validation)
- [x] All user comments are answered or incorporated (gate decisions in Phase 1 §Design Readiness)
- [x] All review comments are answered or incorporated (no PR review yet; no PR opened)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation branch

## Implementation Summary

The work was implemented on branch `claude/kind-goldberg-9oxjza` in ten commits, from `c20a625`
to `b4da9ee` (Phase 4 §Progress lists them per step). It conforms to the approved design; the
deviations are listed below.

**`ns-rec` table operations** (`liquers-lib/src/records/table_ops.rs`):
- the filters `eq`, `ne`, `gt`, `gte`, `lt`, `lte`, `starts_with`, `ends_with`, `contains`,
  `glob`, `is_in`, `is_null`, `is_not_null` and `filter`;
- `sort`, `tail` and `drop_columns`;
- the counts `nrows`, `ncols`, `shape` and `count`;
- `schema`, with `records_schema` kept as an alias.

Each filter builds one node of the search language (the new `SearchExpr::field` and
`FieldTest::prefix` / `suffix` / `substring` constructors). It runs the unranked search path,
behind the new `StrictPredicate`, which turns an unknown column into an error. `select_columns`,
`drop_columns`, the filters and `head` are lazy over a source: `head` uses `FilteredSource` with a
limit, and the projections use the new `MappedSource`.

**`ns-pl`.** Every command accepts a `RecordView` and refuses a `RecordSource`, naming
`ns-rec/materialize`. `sort` takes `ascending: bool` and `nulls_last`, and is stable. New commands:
`is_in`, `is_null`, `is_not_null`, `starts_with`, `ends_with` and `contains`. The counts are `i64`.

**Schema round trip:**
- lenient `IndexKind` / `Analyzer` deserialization (`remote = "Self"`, so serialization is
  unchanged);
- `FieldRole` field defaults;
- a JSON-first `schema` argument;
- UTF-8 bytes accepted as a `String` argument, in liquers-lib and liquers-core.

**Deviations from the approved design:**
- **polars features.** Phase 2 named `is_in` and `strings`. The build needs `strings` and `regex`
  (`str().contains_literal` is behind `regex`). `pl/is_in` is an OR of equalities, so the `is_in`
  feature is not used.
- **Error-checking helper.** `ViewMap` needed a helper trait `ViewMapFn`, because `MaybeSend` and
  `MaybeSync` are not auto traits and cannot be added to `dyn Fn` directly.
- **The test fixture's chunk counter** ignores the empty batch that `MappedSource::new` maps to
  derive its schema.
- **`ns-pl` tests run on a multi-threaded runtime.** polars' `collect` panics on a current-thread
  runtime (filed below).

Nothing was omitted.

## Documentation Delivered

### New Reference Documents
None. The behaviour belongs in the existing records, polars, search and alias references.

### New Guide Documents
None. `RECORD_STREAM_GUIDE.md` §5 was extended instead.

### Existing Documents Reviewed or Updated

Each document in `affects_docs` was reviewed against the code and tests. Each gets a History row
and `reviewed: 2026-10-13`.

| Document | Change |
|---|---|
| `reference/RECORD_STREAMS.md` | the `ns-rec` command tables; the `ns-rec` ↔ `ns-pl` comparison; `MappedSource`; lenient schema deserialization; DataFrame input; the `schema` argument rules |
| `reference/POLARS_COMMAND_LIBRARY.md` | inputs and runtime note; the polars-first conventions table with pandas; `sort`; the implemented string, membership and null filters replace the planned `startswith` / `isnull` / `notnull`. Only these sections were checked |
| `guides/RECORD_STREAM_GUIDE.md` | §5 filter, sort and hand over; §6.5 the query-level bridge |
| `reference/SEARCH.md` | `ns-rec/filter`; constructors and `StrictPredicate`; `FilteredSource` behind `head` |
| `reference/COMMAND_ALIASES.md` | `rec/records_schema` → `rec/schema`; the registered aliases |

Also updated:
- `specs/command_registry.yaml`, regenerated with a CHANGELOG line;
- design notes `conventions.md`, `lazyframe-investigation.md` and `schema-example.md`.

### Links and Capability Map

`specs/README.md` §"Command libraries" gains "Record table operations and `ns-rec` ↔ `ns-pl`
interoperability", linking the two references and the design.

## Issues Filed

- `POLARS-COLLECT-PANICS-ON-CURRENT-THREAD-RUNTIME` (P2, M): every lazy `ns-pl` command panics on a
  current-thread Tokio runtime. Triage: no existing candidate. Not eligible for automatic fixing,
  because the fix has open alternatives.
- `COMMAND-PANIC-HANGS-EVALUATION` (P1, M): a panicking command leaves its asset unfinished, so
  evaluation hangs. Weak overlap with `LIBRARY-CODE-USES-UNWRAP-AND-EXPECT`. Not eligible, because
  the fix has open alternatives and it is P1.

**Closed:**
- `RECORD-AND-POLARS-COMMANDS-NOT-INTEROPERABLE`, with a resolution covering all four expected
  behaviours;
- `RECORD-SOURCE-WRAPPERS-UNSPECIFIED`, by `MappedSource` and `FilteredSource`.

## Important Learning

- **A `-` inside a query value is `~_`.** A raw `*` or `?` does not parse; they are the named
  entities `~nast~` and `~nquest~`. `glob` patterns depend on this.
- **A failed command does not make `.get()` fail.** It returns a state whose value is the error,
  and `state.value()` surfaces it. Error-asserting tests must check `value()`.
- **`#[tokio::test]` and polars.** polars' lazy `collect` needs a multi-threaded runtime. Before
  this work, no test exercised an `ns-pl` command through evaluation
  (`POLARS-COMMAND-TESTS-BYPASS-COMMANDS`).
- **Serialization unchanged, deserialization lenient.** `#[serde(remote = "Self")]` plus a
  `deserialize_any` visitor makes an externally tagged enum accept its JSON map form from YAML
  while serializing exactly as before. Every schema format Liquers reads is self-describing.
- **Laziness lives in `ns-rec`.** A `LazyFrame` cannot pull an async record stream at polars 0.55,
  so the lazy work happens in `ns-rec` and `ns-pl` stays eager
  (`lazyframe-investigation.md`).

## Conformance and Remaining Work

Requested in `RECORD-AND-POLARS-COMMANDS-NOT-INTEROPERABLE`, approved as AC-1 to AC-18, and
implemented:
- AC-1 to AC-15, AC-17 and AC-18, each cited by a passing test (Phase 3);
- AC-16, recorded in `lazyframe-investigation.md`, with no `LazyFrame` value type added (checked).

Nothing remains in scope. The two new issues are outside it.

## Validation

- **Tests:**
  - `cargo test -p liquers-records --all-features --lib --tests`: 465 + 22 passed.
  - `cargo test -p liquers-lib --lib --tests`: 620 passed, across 38 suites.
  - `cargo test -p liquers-core --lib`: 1086 passed.
  - `records_table_ops`: 27 passed by default, and 20 under `--no-default-features --features
    records` (AC-15).
  - `registry_export`: 7 passed.
- **Build matrix:** `bash scripts/check-build-matrix.sh` — result recorded in `DESIGN.md` §Phase
  Status. The wasm32 rows need `rustup target add wasm32-unknown-unknown`, which this environment
  does not have.
- **Docs and queries:** `python3 scripts/docs_index.py --check` reports 0 errors.
  `validate_phase.py` passes for Phases 1–5. Every query in the updated documents was checked with
  `liquers-validate`.
