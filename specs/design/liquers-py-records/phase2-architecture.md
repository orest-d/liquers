# Phase 2: Solution & Architecture — liquers-py-records

## Overview

A new `liquers-py/src/records/` module wraps the `Arc`s that `ExtValue::RecordView` and
`ExtValue::RecordSource` hold in frozen PyO3 classes. Each wrapper reads its data through the
`RecordView` / `RecordSource` traits and never copies the table. The conversion that makes a
source learns which environment it serves for: `value_to_py` gains a `Scope`. Column buffers are
lent through the buffer protocol from the column object, which holds the `Column` and with it
the `Arc`s of every buffer. Arrow is reached through `liquers-records`' IPC writer and reader. The
reader and writer gain a per-batch `liquers.origins` key. A small pure-Python module, `_records.py`,
does the pyarrow work: it normalizes a table on the way in and copies metadata on the way out.
Plain Python data is built in Rust with `ColumnMut`, with declared fields cast in Rust.

Rejected:
- **The value is a `pyarrow.Table`** (`py-record-values-to-python`). This was Decision 1.
- **Route 1 traversal as the default.** This was Decision 4.
- **Building plain data through the JSON reader.** That reader sorts columns by name and infers
  types from text (Phase 1 question 9).
- **A thread-local "current environment" during conversion**, as in liquers-web's
  `conversion_env`. A conversion here can run on any thread, and the explicit `Scope` argument is
  checked by the compiler at each of its call sites.
- **Normalizing Arrow in Rust.** That needs Arrow kernels that `liquers-records` does not have.
  pyarrow's `cast` already does this, and pyarrow is present whenever such a table is.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `POLARS-BRIDGE-TRUNCATES-SUB-MICROSECOND-DATETIMES` | draft | P3 | `from_polars` cuts nanoseconds without a word | No | No | Eligible for automatic fixing: a spin-off PR; until it lands, the guide says so |
| `IPC-READER-CANNOT-READ-ANY-POLARS-STRING-COLUMN` | draft | P2 | pyarrow's `string_view` / `large_string` would be refused | No | No | Designed around: `_records.py` casts to `string` / `binary` first (question 3) |
| `POLARS-BRIDGE-VECTOR-COLUMNS-REFUSED` | draft | P3 | `from_polars` of a vector column fails | No | No | Documented open problem |
| `COLUMN-SLICE-COPIES-INSTEAD-OF-SHARING-BUFFER-STORAGE` | draft | P3 | `RecordView::column` copies for a view that is not a batch | No | No | Designed around: read the batch's own column when `as_batch` is `Some` (question 5) |
| `RECORD-BATCH-FIELDS-CAN-BE-MUTATED-PAST-VALIDATION` | draft | P3 | None: wrappers never mutate | No | No | None |
| `ERROR-TYPES-TOO-COARSE-AND-GENERAL-OVERUSED` | draft | P2 | Traversal after shutdown uses `GeneralError` (AC-15) | No | No | Follows today's convention |
| `PY-REPRS-SHOW-RUST-DEBUG-FORMAT` | draft | P3 | New classes need a `repr` | No | No | New classes get Python-style reprs now (`RecordView(rows=2, fields=['id', 'amount'])`) |

## Interfaces

**Rust, `liquers-py/src/value.rs`** (changed). These calls are sync: a conversion never waits.

```rust
/// What a conversion to Python is for: the environment a `RecordSource` resolves its chunks in.
#[derive(Clone)]
pub struct Scope { origin: Option<crate::records::SourceOrigin> }
impl Scope {
    pub fn env(cell: Arc<EnvCell>) -> Scope;                         // evaluate*, State, AssetRef
    pub fn command(context: &crate::context::Context, envref: EnvRef<PyEnvironment>, shut: Arc<Shut>) -> Scope;
    pub fn detached() -> Scope;                                      // unit tests; traversal refuses
}
pub fn value_to_py(py: Python<'_>, value: &Value, scope: &Scope) -> Result<Py<PyAny>, Error>; // + scope
// py_to_value gains the record arms: lq.RecordView / lq.RecordSource → the same Arc (RECORDS01);
// a pyarrow.Table / pyarrow.RecordBatch → RecordView through `_records.arrow_to_ipc` + `read_table`.
```

**Rust, `liquers-py/src/records/`** (new): `mod.rs` (the classes), `schema.rs`, `build.rs` (plain
data and casts), `arrow.rs` (IPC both ways), `buffer.rs` (the buffer protocol, the only `unsafe`,
each block with a `SAFETY` comment).

```rust
pub enum SourceOrigin {
    /// From `Environment.evaluate*`: rebuild-safe, re-resolved at each traversal; refuses once shut.
    Env(Arc<EnvCell>),
    /// From a command's arguments: `ContextResolver` while the command's context is valid
    /// (dependencies recorded), then `EnvResolver` on that environment; refuses once shut.
    Command { context: crate::context::Context, envref: EnvRef<PyEnvironment>, shut: Arc<Shut> },
}
#[pyclass(frozen, name = "RecordView", module = "liquers")]   pub struct RecordView   { inner: Arc<dyn records::RecordView> }
#[pyclass(frozen, name = "RecordSource", module = "liquers")] pub struct RecordSource { inner: Arc<dyn records::RecordSource>, origin: Option<SourceOrigin> }
#[pyclass(frozen, name = "RecordColumn", module = "liquers")] pub struct RecordColumn { name: String, column: Column }   // Column's buffers are Arc-shared
#[pyclass(frozen, name = "RecordBuffer", module = "liquers")] pub struct RecordBuffer { column: Py<RecordColumn>, part: Part }  // offsets, data, validity, bitmap
#[pyclass(frozen, name = "RecordStream", module = "liquers")] pub struct RecordStream { state: Mutex<StreamState> }       // one traversal
#[pyclass(frozen, eq, name = "FieldType", module = "liquers")] pub enum FieldType { Bool, Int, UInt, Float, Text, Binary, Date, Timestamp, Vector }
#[pyclass(frozen, eq, name = "KeyRole", module = "liquers")]   pub enum KeyRole { Id, Source, Data }   // `Data` is Rust's KeyRole::None
#[pyclass(frozen, name = "FieldSchema" | "RecordSchema" | "ChunkOrigin" | "RecordChunk", module = "liquers")]  // wrap the owned Rust values
#[cfg(feature = "debug-handles")] #[pyfunction] pub fn live_record_handle_count() -> usize;  // RECORDS06
```

The extension adds one class, `RecordBuffer`, to question 4's list, plus the stream and chunk
classes the iteration needs. It is the object `memoryview` reads for one part of a column. It has
no methods beyond the buffer protocol, `format` and `len`.

**Python surface** (Phase 3 tests it; `liquers-py-stubs` declares it):

| Class | Members |
|---|---|
| `RecordView` | `schema`, `chunk_id` (an `lq.Query` or `None`), `origins`, `len()`, `num_columns`, `column(name_or_index)`, `columns()`, `value(row, col)`, `select_columns(names)`, `slice(offset, length)`, `row(i)`, `cell(row, col)`, `item()`, `to_list()`, `to_dict()`, `to_arrow()`, `to_pandas()`, `to_polars()`. Static: `from_records(rows, *, fields=None)`, `from_dict(columns, *, fields=None)`, `from_pandas(df, *, fields=None)`, `from_polars(df, *, fields=None)`, `from_arrow(table, *, fields=None)` |
| `RecordColumn` | `name`, `type`, `len()`, `null_count`, `validity`, `offsets` / `data` (Text, Binary; `data` is the bitmap for Bool), `dim` (Vector), `to_list()`, `[i]`, and the buffer protocol for fixed widths (`q`, `Q`, `d`, `i` for Date days, `q` for Timestamp µs, `f` shaped `(rows, dim)` for Vector) |
| `RecordSource` | `chunks()` → `list[RecordChunk]` (`index`, `kind` = `"query"` / `"key"` / `"memory"`, `query`, `evaluable`), `bounded`, `schema`, `manifest` (a dict or `None`), `__iter__` / `__aiter__` → `RecordStream`, `chunk(index)` / `chunk_async(index)`, `materialize(max_rows)` / `materialize_async(max_rows)` |
| `RecordStream` | `__next__` (waits without holding the interpreter lock, via `environment::block_on`), `__anext__` (via `aio::awaitable`); a second call while one is still waiting raises `GeneralError` |
| `RecordSchema(fields, *, type_identifier=None)`, `FieldSchema(name, type, *, role=KeyRole.Data, label=None, description="", nullable=True)` | `names`, `fields`, `field(name)`, `len()`; `FieldSchema` getters for every argument |
| `ChunkOrigin` | `asset`, `chunk` (each an `lq.Query`), `to_dict()` (adds `info` and `locator`) |
| `State` | adds `record_view()`, which returns the view whatever its shape (AC-10) and raises `ConversionError` otherwise |

`chunk(index)` takes the position in `chunks()`, as `ns-rec/rowid` does, rather than the id. A
`RecordChunk` carries its query, so an id is never needed. The same code serves both: the body of
`rowid` that fetches one chunk moves to a public
`liquers-lib/src/records/commands.rs` `pub async fn read_chunk(source: Arc<dyn RecordSource>,
resolver: Arc<dyn ChunkResolver>, index: usize) -> Result<Arc<dyn RecordView>, Error>`. Through
the source's manifest it fetches only that chunk, and otherwise it walks the stream. `rowid` calls
it. This is a small `liquers-lib` change, recorded in Phase 1's scope changes.

**The builder** (`build.rs`). The steps:

1. Read the rows, or the columns, once. Record the key order as first seen, and give a missing key
   a null.
2. Infer each column's type from the Python values, using the table in Phase 1 question 9. A `bool`
   is tested before `int`. An `int` outside `i64` raises `ConversionError`.
3. Apply `fields=`. The values accepted for it:
   - `None`;
   - a dict mapping a name to an `lq.FieldType`, a lowercase name or a `FieldSchema`;
   - a `RecordSchema`.
4. Cast each declared column, with `cast(value: &FieldValue, to: FieldType) -> Result<FieldValue,
   Error>`:
   - between the numeric types when no value is lost;
   - `Text` parsed by `FieldValue::parse_text`;
   - anything to `Text` by its display form;
   - `Date` and `Timestamp` from ISO text.

   A failure is `ConversionError` naming the field and the row.
5. `ColumnMut::push`, then `freeze`, then `RecordBatch::new`. `RecordSchema::new` checks the roles,
   and an `Id` holding a null is refused.

Dataframes take the same `fields=` step after they are read.
- `from_pandas`: `_records.pandas_to_arrow`, then `from_arrow`.
- `from_polars`: the DataFrame crosses as it does today, through Parquet into a Rust `DataFrame`,
  then `liquers_lib::records::polars::dataframe_to_record_batch`.
- `from_arrow`: `_records.arrow_to_ipc` (normalize, then write IPC), then
  `read_table(bytes, TableFormat::Ipc, ReadSchema::Infer)`.

**Arrow exits** (`arrow.rs`, `_records.py`). `to_arrow`:
1. `view.materialize()`.
2. `write_table(Ipc)` writes the bytes in Rust with the GIL released.
3. `_records.ipc_to_arrow(bytes, chunk_id, origins)` reads them with `pyarrow.ipc.open_file(...).read_all()`.
4. It copies `liquers.chunk_id` and `liquers.origins` into the table's schema metadata, beside the
   `liquers.schema` that is already there.

`to_pandas` is `to_arrow().to_pandas()`. `to_polars` is `polars.read_ipc(BytesIO(bytes))`.

**`liquers-records/src/formats/ipc.rs`** (changed, additive). `LIQUERS_ORIGINS_KEY =
"liquers.origins"` is the JSON `Vec<ChunkOrigin>` of a batch's `sources`. It is written beside
`liquers.chunk_id` when the vector is not empty, and read back into `RecordBatch::new`'s `sources`.
`RecordBatch::concat` re-bases it as it does today. It is dropped with a stale `liquers.schema`,
because a schema that no longer matches cannot locate its `Source` column.

## Integration Points

- `liquers-py/src/value.rs`: `Scope`; the record arms in `ext_to_py` and `py_to_ext`.
- The callers of `value_to_py`, which pass a scope:
  - `environment.rs` (`evaluate`, `get` / `store_get`);
  - `asset.rs` (`State::from_core`, `AssetRef`);
  - `context.rs` (`evaluate`);
  - `command.rs` `prepare_call` (`Scope::command` from the core context it is about to wrap);
  - `aio.rs` (`ToPy::Value` and `ToPy::State` carry a `Scope`).
- `liquers-py/src/lib.rs`: registers the classes, and `live_record_handle_count` under
  `debug-handles`.
- `liquers-py/python/liquers/__init__.py`: exports the classes. New `_records.py` (pyarrow and
  pandas helpers, imported lazily).
- `liquers-records/src/formats/ipc.rs`: origins. `liquers-lib/src/records/commands.rs`: `read_chunk`.

## Error Handling

| Case | Constructor |
|---|---|
| pyarrow, pandas or polars absent | `Error::conversion_error_with_message(what, "Python", "needs liquers-py[arrow]")` (the existing `py_conversion_error` shape) |
| A column that cannot be normalized or bridged; a cast or inference failure; an `int` too large; `item()` on a larger view | `Error::conversion_error_with_message`, naming the column (and row, and the view's shape) |
| A source with no environment, or used after `shutdown()` | `Error::general_error` (AC-15), or the existing `shut_down_error()` |
| A row over `max_rows`; schemas that differ | the source's own errors, passed through |
| A second `__next__` / `__anext__` while one waits | `Error::general_error("RecordStream: a previous call is still waiting")` |
| A lent buffer requested writable | `BufferError` (PyO3 `PyBufferError`), the buffer protocol's own error |

## Relevant Commands

No new commands. Interacts with `ns-rec` (`to_record`, `to_record_source`, `select_columns`,
`slice`, `row`, `rowid`, `to_json`). The view methods call the same `liquers-records` view
constructors these commands call, so AC-14 compares them directly. The namespace is `rec`.

## Concurrency

All the classes are frozen. Their data is immutable `Arc`s, so they are safe on a free-threaded
build with no lock, except the one `Mutex` inside `RecordStream`. Waiting on a stream releases the
interpreter lock (`block_on` and `py.detach`). The IPC writing and reading of `to_arrow` and
`from_arrow` run under `py.detach`. A source consumed inside a sync command waits as `Context.evaluate` does.

## Documentation Architecture

| Path | Kind | Change |
|---|---|---|
| `specs/reference/PYTHON_INTEGRATION.md` | reference, py | New §Records: the classes, `Scope` and where a source resolves, the builder's inference and cast tables, the Arrow routes, buffers and their lifetime, traversal, the open problems. §5 drops the records refusal |
| `specs/guides/PYTHON_GUIDE.md` | guide, py | New "Records" chapter; the values table in §7 |
| `specs/reference/RECORD_STREAMS.md` | reference, records | §"Arrow IPC" adds `liquers.origins`; §ChunkOrigin |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | guide | §RECORDS: what liquers-py does; §6 Python: what cost time |
| `liquers-py/README.md` | readme | A RECORDS row with the two `NA`s |
| `specs/README.md` | map | The design's row |

`affects_docs` is unchanged from Phase 1.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-py/src/{value,environment,asset,context,command,aio,lib}.rs`, `liquers-py/src/records/*`, `liquers-py/python/liquers/{__init__,_records}.py`, `liquers-records/src/formats/ipc.rs`, `liquers-lib/src/records/commands.rs` |
| Crates and workflows affected | liquers-py; liquers-records (`ipc` feature); liquers-lib (one function moves); `liquers-py-tests.yml` installs pyarrow and pandas where wheels exist |
| Existing tests likely to change | `test_VALUE_value_bridge.py`: the "records are refused" case becomes the AC-1 test. The Rust unit tests in `value.rs` pass `Scope::detached()` |
| New validation | `test_RECORDS_*.py` (Phase 3); IPC origins round-trip tests in `ipc.rs`; a `read_chunk` unit test |
| Compatibility / data / concurrency / performance / security | IPC files gain an optional key. Old readers ignore it, and a file without it reads as before. Every Arrow exit copies, which is documented. The buffer protocol is `unsafe` FFI, confined to `buffer.rs` and read-only. It needs the limited API at 3.11 or above, or a non-abi3 build |
| Recovery | Every change is additive. Reverting the Python module leaves records refused, as they are today |
| Certainty and open questions | High. One implementation detail is settled here: `chunk(index)` takes a position. No user decision is open |
