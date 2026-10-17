# Phase 2: Solution & Architecture — Record and polars command interoperability

## Overview

`ns-rec` gains table operations built on what already exists:
- **Filters** construct a search-language AST node directly and run the existing unranked search
  path. A view gets `search_view`, which produces a lazy `RowIndexView`. A source gets
  `FilteredSource`. A thin `StrictPredicate` turns an unknown column into an error (AC-6).
- **Sort** is a new stable index kernel, `Column::sort_indices`, followed by `RecordView::take`.
- **Lazy source operations** are a new generic `MappedSource` that applies a view-to-view function
  to each chunk.

`ns-pl` stays synchronous. `try_to_polars_dataframe` accepts a `RecordView` through the bridge, and
the `ns-rec` converters accept a DataFrame. Four schema round-trip defects are fixed where they
arise:
- lenient `Deserialize` for `IndexKind` and `Analyzer`;
- `#[serde(default)]` on `FieldRole`;
- JSON-first parsing of the `schema` argument;
- UTF-8 bytes accepted as a `String` argument.

Rejected alternatives:
- **A separate column-predicate engine for the filters.** It would duplicate the kernels: JSON
  any-element matching, date widening and typed comparisons already live in
  `liquers-records/src/search/kernels.rs`.
- **Building the filter as query text and calling `SearchExpr::try_from`.** This brings back the
  escaping problem the design exists to remove, and `f:v` would glob on `*` and `?`.
- **A dedicated `ProjectedSource` and `HeadSource`.** One `MappedSource`, plus `FilteredSource` with
  `Node::Always` and a limit, covers both.
- **Async `ns-pl` over a `LazyFrame`.** Ruled out in Phase 1 (`lazyframe-investigation.md`).
- **Changing how `IndexKind` serializes.** That would change the schema stored in Arrow IPC files.
  Only deserialization is made lenient.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `POLARS-COMMAND-TESTS-BYPASS-COMMANDS` | draft | P2 | The `ns-pl` tests call functions, not commands | No | No | New `ns-pl` tests go through `envref.evaluate` |
| `LINK-IN-VARIADIC-DOES-NOT-EXPAND` | draft | P3 | A linked list given to `is_in` arrives as one value | No | No | Documented as a limitation of `is_in` |
| `POLARS-BRIDGE-VECTOR-COLUMNS-REFUSED` | draft | P3 | A Vector column cannot cross into `ns-pl` (AC-10/11) | No | No | The bridge error stands; a test pins it |
| `RECORD-SOURCE-WRAPPERS-UNSPECIFIED` | draft | P3 | Source of AC-18 | — | — | Resolved by `MappedSource`; closed in Phase 5 |
| `LIBRARY-CODE-USES-UNWRAP-AND-EXPECT` | draft | P2 | Files edited here may contain `unwrap` | No | No | No new `unwrap`; existing ones untouched |
| `AGENT-MEMORY-SERVICE` (design `agent-memory-mvp`) | draft | P2 | Depends on this design | — | — | Required-by; no change |

None blocks. Command namespaces involved: **`rec`** (new commands and extended commands), **`pl`**
(new commands, `sort` signature, record input) and **`search`** (read-only reuse).

## Interfaces

### liquers-records

All of these are sync, because they are CPU-bound and run in memory.

```rust
// column.rs — stable; nulls first unless nulls_last; Float NaN orders above every number
// (polars). Text compares by bytes, Bool false < true. Binary and Vector are refused.
impl Column {
    pub fn sort_indices(&self, ascending: bool, nulls_last: bool) -> Result<Vec<u32>, Error>;
}

// sources.rs — a lazy per-chunk view transform (AC-18). Async only through RecordSource's
// BoxFuture methods. chunks / describe_chunk delegate to `inner`.
pub type ViewMap =
    Arc<dyn Fn(Arc<dyn RecordView>) -> Result<Arc<dyn RecordView>, Error> + MaybeSend + MaybeSync>;
#[derive(Clone)]
pub struct MappedSource { inner: Arc<dyn RecordSource>, map: ViewMap, schema: Option<Arc<RecordSchema>> }
impl MappedSource {
    /// `schema` is the inner schema mapped through `map` on an empty batch, when the inner one
    /// is declared.
    pub fn new(inner: Arc<dyn RecordSource>, map: ViewMap) -> Result<Self, Error>;
}
impl Debug for MappedSource { /* inner + "map" */ }
impl RecordSource for MappedSource { /* stream: inner.stream(..) mapped item by item */ }

// batch.rs — a zero-row batch for a schema (Column::empty per field). It is also used by
// RecordStreamExt::materialize for an empty stream.
impl RecordBatch { pub fn empty(schema: Arc<RecordSchema>) -> Result<RecordBatch, Error>; }

// search/syntax.rs — programmatic construction, so liquers-lib needs no `regex` dependency and
// never writes query text. The position is Position::unknown().
impl SearchExpr {
    pub fn field(column: &str, test: FieldTest) -> SearchExpr;
    pub fn any(items: Vec<SearchExpr>) -> SearchExpr;
    pub fn not(inner: SearchExpr) -> SearchExpr;
    pub fn always() -> SearchExpr;
}
impl FieldTest {
    pub fn prefix(text: &str) -> FieldTest;      // Regex("^" + regex::escape(text))
    pub fn suffix(text: &str) -> FieldTest;      // Regex(regex::escape(text) + "$")
    pub fn substring(text: &str) -> FieldTest;   // Regex(regex::escape(text))
}

// search/predicate.rs — wraps a predicate, and bind fails when a required field is missing.
#[derive(Debug)]
pub struct StrictPredicate { inner: Arc<dyn Predicate>, required: Vec<String> }
impl StrictPredicate { pub fn new(inner: Arc<dyn Predicate>, required: Vec<String>) -> Self; }
impl Predicate for StrictPredicate { /* bind: check names, then delegate; the rest delegate */ }

// schema.rs — serialization unchanged; deserialization lenient.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]                       // was: every field required
pub struct FieldRole { pub indexed: Vec<IndexKind>, pub stored: bool, pub fast: bool }
// IndexKind and Analyzer: derive(Serialize) kept. Deserialize becomes a hand-written visitor
// accepting a bare string (unit variants), a single-key map {"FullText": {..}}, which is how
// JSON and serde_yaml maps arrive, and a YAML tag !FullText {..}, which arrives as enum access.
```

### liquers-lib

Every `ns-rec` command below is `async` with owned `State<Value>` and `context`, as all `ns-rec`
commands are, because a `Key` input and a source need the context. File:
`liquers-lib/src/records/table_ops.rs`.

```rust
// Filters: build a SearchExpr, wrap it in StrictPredicate(column), then run the unranked path.
async fn eq(state, column: String, value: String, context) -> result      // ne, gt, gte, lt, lte
async fn starts_with(state, column: String, text: String, case_sensitive: bool = false, context) -> result
                                                          // ends_with, contains, glob(…, pattern: String, …)
async fn is_in(state, column: String, values: Vec<String> multiple, context) -> result
async fn is_null(state, column: String, context) -> result                // is_not_null
async fn filter(state, query: String = "", context) -> result             // lenient, like search
async fn sort(state, column: String, ascending: bool = true, nulls_last: bool = false, context) -> result
async fn tail(state, n: i64 = 5, context) -> result
async fn drop_columns(state, columns: Vec<String> multiple, context) -> result
async fn nrows(state, context) -> result                                  // ncols, shape, count
async fn schema(state, context) -> result      // records_schema becomes an alias of it
```

How each test is built, in `liquers-records/src/search/syntax.rs` terms (`Node::Field { path:
vec![column], test }`):

| Command | `FieldTest` | Case flag | Notes |
|---|---|---|---|
| `eq`…`lte` | `Compare(CompareOp::Eq…Ge, value)` | none: exact, like polars | Literal, never a glob. Typed columns use `FieldValue::parse_text`, a date is widened on a Timestamp, JSON arrays match any element |
| `starts_with` / `ends_with` / `contains` | `FieldTest::prefix` / `suffix` / `substring` | `SearchOptions.case_sensitive` | On a non-text column the existing kernel refuses a regex |
| `glob` | `Glob(pattern)` | as above | |
| `is_in` | `Node::Any` of `Compare(Eq, v)` | none | Empty list: refused |
| `is_null` / `is_not_null` | `Not(Glob("*"))` / `Glob("*")` | — | `Glob("*")` binds to `BNode::Present` (kernels.rs:265) |

### `ns-pl` (sync, unchanged pattern `&State<Value>`)

```rust
fn sort(state, column: String, ascending: bool = true, nulls_last: bool = false) -> result
                                                          // was ascending: String = "t"; adds maintain_order(true)
fn is_in(state, column: String, values: Vec<String> multiple) -> result
fn is_null(state, column: String) -> result               // is_not_null
fn starts_with(state, column: String, text: String, case_sensitive: bool = false) -> result
                                                          // ends_with, contains (literal; case-insensitive by lowercasing both sides)
```

`util.rs` `try_to_polars_dataframe` gains, under `#[cfg(feature = "records")]`, before the text
attempt:
- a `RecordView` becomes `view.materialize()?`, then `record_batch_to_dataframe`;
- a `RecordSource` is refused with `Error::conversion_error_with_message("RecordSource",
  "polars.DataFrame", "a source is not a table; materialize it first with ns-rec/materialize")`.

The message wording is the one `convert.rs` already uses. `nrows`, `ncols` and `shape` return
`i64` instead of `i32`, which is the same JSON for any table under 2^31 rows.

### Conversions

- `liquers-lib/src/records/convert.rs` `to_record` and `to_record_source` gain a
  `"polars.DataFrame"` arm under `#[cfg(feature = "polars")]`: `dataframe_to_record_batch`, then a
  view, or an `InMemorySource` for `to_record_source`.
- `liquers-lib/src/value/simple.rs` `impl TryFrom<SimpleValue> for String` accepts `Bytes` that are
  valid UTF-8. Invalid UTF-8 keeps `conversion_error`. The same change is made in
  `liquers-core/src/value.rs:950`, so that the two `Value` types agree.
- `parse_schema_argument` tries `serde_json::from_str` first when the trimmed text starts with
  `{`, and otherwise uses `serde_yaml`.

No new `ExtValue` variant, and no new trait.

## Integration Points

| Crate | File | Change |
|---|---|---|
| liquers-records | `column.rs`, `batch.rs`, `sources.rs`, `search/syntax.rs`, `search/predicate.rs`, `search/mod.rs`, `schema.rs`, `lib.rs` | The kernels and types above, plus re-exports |
| liquers-lib | `records/table_ops.rs` (new), `records/mod.rs`, `records/commands.rs` | `ns-rec` commands. `register_records_commands!` registers them, aliases `records_schema` to `schema`, and makes `select_columns` and `head` source-aware |
| liquers-lib | `records/convert.rs` | DataFrame arms |
| liquers-lib | `polars/util.rs`, `polars/sorting.rs`, `polars/filtering.rs`, `polars/info.rs` | Record input, `sort` signature, new filters, `i64` counts |
| liquers-lib | `Cargo.toml` | polars features `is_in`, `strings` (needed by `str()` and `is_in`, checked at Step 1 of Phase 4) |
| liquers-lib, liquers-core | `value/simple.rs`, `value.rs` | UTF-8 bytes to `String` |
| specs | `command_registry.yaml` | Regenerated, with a CHANGELOG line (signatures and `version: auto` bodies change) |

How each command treats a source:

| Command | Source behaviour |
|---|---|
| `select_columns`, `drop_columns` | `MappedSource` per chunk |
| filters | `FilteredSource` |
| `head` | `FilteredSource` with `Node::Always` and `limit = n`, returned as a source |
| `nrows`, `ncols`, `shape`, `count` | Stream, summing per chunk |
| `sort`, `tail` | `source.materialize(ContextResolver, DEFAULT_MATERIALIZE_MAX_ROWS)` first |

Inputs are dispatched through a shared `table_input(state, &context) -> TableInput { View, Source }`,
the `search_input` logic without the folder case.

## Error Handling

| Case | Constructor |
|---|---|
| Unknown column (AC-6), from `StrictPredicate`, `sort` and `drop_columns` | `Error::general_error(format!("no field named '{c}' (available: …)"))`, the `select_columns` wording |
| Empty `is_in` / `select_columns` / `drop_columns` list | `Error::general_error` naming the command |
| `drop_columns` of the Id or Source field | `Error::general_error("… cannot drop the Id field '…'")` |
| Value not parseable to the column type | the existing `FieldValue::parse_text` error |
| `sort` on Binary or Vector | `Error::general_error` naming the type |
| Source in `ns-pl` | `Error::conversion_error_with_message` (above) |
| Over `max_rows` when `sort` or `tail` materializes | the existing `RecordStreamExt::materialize` error |
| Schema text invalid | `Error::from_error(ErrorType::SerializationError, e)`. When both parsers fail, the JSON error is reported if the text starts with `{` |
| polars failure | `Error::general_error(format!("… failed: {e}"))`, the existing `ns-pl` pattern |

## Relevant Commands

- **New `rec`:** `eq ne gt gte lt lte starts_with ends_with contains glob is_in is_null
  is_not_null filter sort tail drop_columns nrows ncols shape count schema`.
- **Extended `rec`:** `select_columns` and `head` accept a source. `records_schema` becomes an alias.
- **New `pl`:** `is_in is_null is_not_null starts_with ends_with contains`.
- **Changed `pl`:** `sort` takes `ascending: bool` and `nulls_last`. `nrows`, `ncols` and `shape`
  return `i64`. Every `pl` command accepts a record view.
- **`search`:** unchanged; `filter` equals `search` with `rank=false`.

## Documentation Architecture

| Path | Kind | Change |
|---|---|---|
| `specs/reference/RECORD_STREAMS.md` | reference | The `ns-rec` command table; the rec ↔ pl comparison table (AC-9); `MappedSource` and `StrictPredicate`; lenient schema deserialization |
| `specs/reference/POLARS_COMMAND_LIBRARY.md` | reference | Record inputs; new filters; the `sort` signature; the conventions table from `conventions.md`, including the pandas column |
| `specs/guides/RECORD_STREAM_GUIDE.md` | guide | "Filter, sort and hand over to polars": lazy over sources, and `materialize` then `ns-pl` |
| `specs/reference/SEARCH.md` | reference | `ns-rec/filter` as the unranked form |
| `specs/reference/COMMAND_ALIASES.md` | reference | `rec/records_schema` → `rec/schema` |

Proposed `affects_docs`: the five above. Phase 5 adds the design to `specs/README.md`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | About 16, listed under Integration Points |
| Crates and workflows affected | liquers-records, liquers-lib, and liquers-core (one conversion arm); the build matrix (`polars` × `records`); the registry export |
| Existing tests likely to change | `polars_commands.rs` (`sort` flag), `registry_export` (regeneration), tests asserting `head`/`select_columns` refuse a source (`records/commands.rs` tests), and any test pinning `i32` counts |
| New validation | Phase 3 tests per AC; `scripts/check-build-matrix.sh` for the new `cfg(all(records, polars))` arms; an IPC round trip proving schema serialization is unchanged |
| Compatibility / data / concurrency / performance / security | `pl/sort-col-t` still parses, because `bool` reads `t`/`f`. An empty `ascending` argument, which used to mean true, is now an error. Stored IPC schemas are unchanged (serialize side untouched). Bytes-to-`String` widens every `String` argument; that is intended, since `try_into_string` already accepted bytes. `ns-pl` on a view now copies through `materialize` (one batch). No new concurrency |
| Recovery | Each step is additive and reverts independently. The serde leniency and the bytes conversion are separate commits |
| Certainty and open questions | High for the filters and the bridge, which reuse tested code. Medium for the polars feature names, which Phase 4 Step 1 checks. No open questions |

## Review (2026-10-13)

Author pass over the four concerns. The `rust-best-practices` lens was applied.

- **Conformity.** Every AC has a mechanism: AC-1–4 the filter table; AC-5/18 `FilteredSource` and
  `MappedSource`; AC-6 `StrictPredicate`; AC-7 `sort_indices`; AC-8 the existing JSON kernels;
  AC-9/17 the names and signatures above; AC-10 `try_to_polars_dataframe`; AC-11 the convert
  arms; AC-12/13 `table_ops`; AC-14 the four schema fixes; AC-15 `table_ops` has no polars
  dependency; AC-16 already recorded.
  - **Clarification:** `tail` and `sort` materialize a source, because they need the whole input.
    AC-12 did not say so.
- **Codebase.** Signatures were taken from the code at HEAD, through a survey and then spot checks:
  views.rs:736, value.rs:35, search_command.rs:29, simple.rs:518, column.rs:1172,
  schema.rs:100, selection.rs:142 and kernels.rs:265. No existing sort kernel exists (checked).
  `Column::empty` exists as `pub(crate)` (column.rs:1247), so `RecordBatch::empty` can reuse it.
- **Soundness.**
  - `ViewMap` is `Arc<dyn Fn … + MaybeSend + MaybeSync>`, which is implicitly `'static`, as
    `RecordSource: 'static` requires.
  - `StrictPredicate` can derive `Debug` because `Predicate: Debug`.
  - There is no `_ =>` on Liquers enums: identifier dispatch matches on strings, as `convert.rs`
    does today.
  - The `cfg` gates: `table_ops` is under `records`, the DataFrame arms in `records/convert.rs`
    under `polars`, and the record arm in `polars/util.rs` under `records`.
  - No `unwrap`, and no blocking: `sort` and `tail` await `materialize`.
- **Records.** `affects_docs` is to be set on `DESIGN.md` at this gate. The size is over the
  three-page guide, because the filter table and the `ns-pl` changes are both needed by Phase 3.
