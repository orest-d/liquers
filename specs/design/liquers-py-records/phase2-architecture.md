# Phase 2: Solution & Architecture — liquers-py-records

## Overview

A new `liquers-py/src/records/` module wraps the `Arc`s held by `ExtValue::RecordView` and
`ExtValue::RecordSource` in frozen PyO3 classes. The wrappers read through the traits and never
copy a table.

- `value_to_py` gains a `Scope`: what a source resolves its chunks in.
- A column object holds its `Column`, so its `Arc`ed buffers outlive every `memoryview` of it.
- Arrow is reached through `liquers-records`' IPC writer and reader, which gain a per-batch
  `liquers.origins` key.
- A pure-Python `_records.py` does the pyarrow work: normalizing in, copying metadata out.
- Plain Python data is built in Rust with `ColumnMut`.

Rejected:
- A thread-local "current environment" during conversion, as in liquers-web's `conversion_env`:
  a conversion here runs on any thread, and an explicit argument is checked at every call site.
- Arrow normalization in Rust: it needs kernels `liquers-records` lacks, and pyarrow's `cast` is
  present whenever such a table is.
- The others were decided at Phase 1 (Decisions 1, 4, 5).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `POLARS-BRIDGE-TRUNCATES-SUB-MICROSECOND-DATETIMES` | draft | P3 | `from_polars` cuts nanoseconds | No | No | Eligible for automatic fixing (assessed in its file): a spin-off PR |
| `IPC-READER-CANNOT-READ-ANY-POLARS-STRING-COLUMN` | draft | P2 | `string_view` / `large_string` refused | No | No | `_records.py` casts to `string` / `binary` first |
| `POLARS-BRIDGE-VECTOR-COLUMNS-REFUSED` | draft | P3 | `from_polars` refuses vectors | No | No | Documented open problem |
| `COLUMN-SLICE-COPIES-INSTEAD-OF-SHARING-BUFFER-STORAGE` | draft | P3 | `RecordView::column` copies off a batch | No | No | Lend `as_batch().columns[i]` when present |
| `ERROR-TYPES-TOO-COARSE-AND-GENERAL-OVERUSED` | draft | P2 | AC-15 uses `GeneralError` | No | No | Existing convention |
| `PY-REPRS-SHOW-RUST-DEBUG-FORMAT` (design `py-python-style-reprs`) | in review | P3 | New classes need reprs | No | No | Follow that design's convention when it lands; Rust debug form until then |

## Interfaces

**Conversion** (`liquers-py/src/value.rs`). All sync: a conversion never waits.

```rust
pub struct Scope { origin: Option<SourceOrigin> }          // Clone by hand
impl Scope {
    pub fn env(py: Python<'_>, cell: &Arc<EnvCell>) -> Scope; // Weak to the built environment
    pub fn command(context: &crate::context::Context, envref: &EnvRef<PyEnvironment>, shut: Arc<Shut>) -> Scope;
    pub fn detached() -> Scope;                             // unit tests; a source refuses to traverse
}
pub fn value_to_py(py: Python<'_>, value: &Value, scope: &Scope) -> Result<Py<PyAny>, Error>;
```

- **Out.** In `ext_to_py`, a RecordView with one row and one payload field, or no payload field
  and an `Id` or a sole field (liquers-lib's `record_view_is_single_cell`, made `pub`), becomes its `FieldValue` as a
  Python scalar (RECORDS10). Any other view becomes `lq.RecordView`, and a source becomes
  `lq.RecordSource`.
- **In.** `py_to_value` turns an `lq.RecordView` / `RecordSource` back into the same `Arc`
  (RECORDS01). A `pyarrow.Table` / `RecordBatch`, recognized by type name without importing,
  goes through `_records.arrow_to_ipc` and then `read_table`.

**Where a source resolves** (`records/resolver.rs`). `SourceOrigin` holds `Weak<PyEnvironment>`
plus `Arc<Shut>`, never a strong reference. `Scope::env` reads the built environment from the cell
while it holds the interpreter, so the resolver later rebuilds `EnvRef(arc)` on a tokio thread
without attaching to Python. A source captured by a command
closure would otherwise make a cycle the GC cannot see (`RecipeProviders` holds `Weak` for the
same reason). `PyChunkResolver: ChunkResolver` decides on every `evaluate` / `metadata` /
`read_resource` call:
- while the command's context handle is valid, through `ContextResolver`, which records
  dependencies;
- otherwise through `EnvResolver` on the upgraded environment;
  (`crate::context::Context` gains a `pub(crate) fn is_valid(&self) -> bool` for this);
- when the environment is gone or shut, with `shut_down_error()`.

It decides per call, not when the stream opens, so a stream kept past its command stays safe.
`prepare_call` (`command.rs`) now always builds the `crate::context::Context` wrapper first, uses
it for `Scope::command`, and invalidates it after the call, even when the function takes no
`context`.

**Scope plumbing.** These signatures gain a `&Scope`:
- `asset::State::from_core(py, state, scope)`, with `State` keeping the core
  `Arc<dyn RecordView>` for `record_view()`;
- `AssetRef::new(asset, shut, scope)`.

`awaitable` takes a plain `fn` wrapper, so the scope travels inside the future's output: the
`ToPy::{Value, State, Asset}` variants take `(…, Scope)`. A new `ToPy::Next(Result<Option<Arc<dyn
RecordView>>, Error>)` raises `StopAsyncIteration` at the end of a stream, and `ToPy::View(Result<Arc<dyn
RecordView>, Error>)` returns `chunk_async` / `materialize_async` results as `lq.RecordView`,
without the single-cell rule.

**Classes** (`records/{mod,schema,column,buffer,build,arrow,resolver}.rs`; `buffer.rs` holds the
only `unsafe`, each block with a `SAFETY` note). Every one is `#[pyclass(frozen, module = "liquers")]`:

| Class | Holds | Members |
|---|---|---|
| `RecordView` | `Arc<dyn RecordView>` | `schema`, `chunk_id` (`Key \| Query \| None`), `origins`, `len()`, `num_columns`, `column(name \| index)`, `columns()`, `value(row, col)`, `select_columns`, `slice`, `row`, `cell`, `item()`, `to_list()`, `to_dict()`, `to_arrow()`, `to_pandas()`, `to_polars()`; static `from_records`, `from_dict`, `from_pandas`, `from_polars`, `from_arrow` (each `fields=None`) |
| `RecordColumn` | name, `Column` | `name`, `type`, `len()`, `null_count`, `validity`, `offsets`, `data`, `dim`, `to_list()`, `[i]`; buffer protocol for fixed widths only |
| `RecordBuffer` | the column, which part, and its shape and strides | buffer protocol, `format`, `len()` |
| `RecordSource` | `Arc<dyn RecordSource>`, `SourceOrigin` | `chunks()`, `bounded`, `schema`, `manifest`, `__iter__`, `__aiter__`, `chunk(index)`, `chunk_async`, `materialize(max_rows)`, `materialize_async` |
| `RecordStream` | `Mutex<StreamState>`, taken with `try_lock` | `__next__`, `__anext__`; a second call while one waits fails instead of blocking |
| `RecordChunk` | index, kind, query | `index`, `kind` (`"query"`, `"key"`, or `"memory"` from `InMemorySource::is_placeholder_chunk_id`), `query`, `evaluable` |
| `RecordSchema(fields, *, type_identifier=None)` | `Arc<RecordSchema>` | `fields`, `names`, `field(name)`, `len()`, `type_identifier`, `to_dict()` |
| `FieldSchema` | `FieldSchema` | getters `name`, `type`, `key`, `label`, `description`, `nullable` |
| `FieldType`, `KeyRole` | enums | `FieldType.Bool` … `Vector`; `KeyRole.Id`, `Source`, `Data` |
| `ChunkOrigin` | `ChunkOrigin` | `asset`, `chunk` (`lq.Query`), `to_dict()` (adds `info`, `locator`) |

- **Constructors.** `FieldSchema(type, *, name=None, key="data", label=None, description="",
  nullable=True)` takes the type first. Inside `fields=` the dict key supplies the name, and a
  `RecordSchema` requires one. `key` is Rust's `FieldSchema::key` (the structural role, as in
  `liquers.schema` JSON; Rust's separate `role` is index intent and is not exposed in M5). It
  takes a `KeyRole` or `"id"`, `"source"` or `"data"`; `KeyRole.Data` is Rust's `KeyRole::None`,
  because `None` is a Python keyword.
- **Narrowing never collapses.** `select_columns`, `slice` and `row` always return an
  `lq.RecordView`, even when the result is one cell. Only a conversion out of a value
  (`evaluate`, a command's argument) applies the single-cell rule.
- **Buffer formats.** Fixed widths export `q` (Int, and Timestamp in µs), `Q`, `d`, `i` (Date in
  days), and `f` with shape `(rows, dim)` (Vector). Bool data and `validity` export packed `B`
  bytes, whose trailing bits may be unmasked.
- **Blocking calls.** `__next__`, `chunk`, `materialize` and `to_arrow` call
  `check_blocking_call` first, so they raise inside an `async def` command, as
  `Context.evaluate` does. They wait with `environment::block_on`, without holding the
  interpreter lock.
- **`chunk(index)`** takes the position in `chunks()`, as `ns-rec/rowid` does, and is answered by
  a new `liquers-records/src/sources.rs` `pub async fn read_chunk(source: Arc<dyn RecordSource>,
  resolver: Arc<dyn ChunkResolver>, index: usize) -> Result<Arc<dyn RecordView>, Error>`. Through
  a manifest it reads only that chunk with `ManifestSource`'s existing private `read_chunk` logic
  (a keyed chunk through `read_resource`, as iteration does), then `place_chunk`, so `chunk(i)`
  and iteration agree. Without a manifest it walks the stream. Its error texts are neutral. `rowid` (`liquers-lib/src/records/commands.rs`)
  calls it in place of its private `chunk_id_at` / `walk_to_chunk`; for a stored keyed chunk it now
  reads the resource, as iteration does, where it evaluated before.
- **`debug-handles`.** `live_record_handle_count()` is a module function that counts live
  wrappers (RECORDS06).

**Builder** (`build.rs`). Phase 1 question 9 fixes the steps. The details:
- `int` beyond `i64` is a `ConversionError`.
- A declared field is cast by `cast(&FieldValue, FieldType) -> Result<FieldValue, Error>`:
  - numbers convert when no value is lost;
  - `Text` parses through `FieldValue::parse_text`;
  - anything else turns into `Text` through the CSV writer's cell formatter, which becomes public as
    `liquers_records::format_field_value` (today `pub(crate) format_value` in `formats/csv.rs`);
  - each failure names the field and the row.
- `build.rs` checks an `Id` column's `null_count` itself, since `RecordSchema::new` checks only
  that roles are unique.
- A pandas int column holding `None` is `float64`, which arrives as `Float` unless the frame uses
  `Int64` or `fields=` declares `int`. This is documented in the inference table.

**Arrow** (`arrow.rs` and `_records.py`, imported lazily). The exits and entrances:
- `to_arrow` writes IPC with `write_table(view, Ipc, &WriteOptions)` with the lock released. It
  then calls `_records.ipc_to_arrow(data: bytes, chunk_id: str | None, origins: str | None) ->
  pa.Table`, which reads with `pa.ipc.open_file(...).read_all()` and copies `liquers.chunk_id`
  and `liquers.origins` into the schema metadata.
- `to_pandas` is `to_arrow().to_pandas()`. `to_polars` is `polars.read_ipc` on the same bytes.
- `_records` imports pyarrow and pandas inside each function, through `_extras.require`, so a
  test can block a package per call. `_records.arrow_to_ipc(table) -> bytes` writes an
  uncompressed IPC file (`pa.ipc.new_file`; the reader refuses compressed bodies) and first applies Phase 1 question 3's normalization column by
  column:
  - `large_*` and view types to `string` / `binary`;
  - `int8`-`int32` to `int64`, and `uint8`-`uint32` to `uint64`;
  - `float16` and `float32` to `float64`;
  - dictionaries decoded;
  - `date64` to `date32`;
  - timestamps to `us`, refused when that loses a value.

  Anything else raises a `ConversionError` naming the column. Rust reads the result with
  `read_table(…, Ipc, ReadSchema::Infer, &ReadOptions)`.
- `_records.pandas_to_arrow(df) -> pa.Table` is `pa.Table.from_pandas(df, preserve_index=False)`.
- **Missing packages.** Each exit names its own extra, using `_extras.py`'s mapping
  (`liquers-py-package`): `[arrow]` for pyarrow, `[pandas]` for pandas with pyarrow, and
  `[polars]` for polars. `_records.py` catches `require`'s `ModuleNotFoundError` and raises
  `lq.ConversionError` with the same text, as AC-9 says.

**`liquers-records/src/formats/ipc.rs`** (additive). `LIQUERS_ORIGINS_KEY = "liquers.origins"`
holds the JSON of a batch's `sources`. It is written beside `liquers.chunk_id` when the batch has
sources, and read back into each batch's `sources` before `read_ipc` concatenates;
`RecordBatch::concat` already re-bases them. It is dropped with a stale
`liquers.schema`, which can no longer locate the `Source` column.

## Integration Points

- **liquers-py.**
  - `value.rs`, plus the callers of `value_to_py`: `environment.rs`, `asset.rs`, `context.rs`,
    `command.rs`, `aio.rs`.
  - `records/*`.
  - `lib.rs`, which registers the classes.
  - `python/liquers/__init__.py` (exports) and `_records.py`.
  - `scripts/test-liquers-py.sh`, which adds `pyarrow` and `pandas` to its wheel-only install
    loop; a leg without wheels skips.
- **liquers-records.** `formats/ipc.rs`, `formats/csv.rs` (`format_field_value` made public) and `sources.rs` (`read_chunk`).
- **liquers-lib.** `records/commands.rs`: `rowid` calls `read_chunk`; `value/mod.rs`: `record_view_is_single_cell` made `pub`.

## Error Handling

| Case | Constructor |
|---|---|
| A package is missing; a column cannot be normalized, bridged, inferred or cast; an int is too large; `item()` on a larger view | `Error::conversion_error_with_message`, naming the extra, column, row or shape |
| A source with no environment, or used after `shutdown()` | `shut_down_error()` (`GeneralError`) |
| `shutdown()` during a wait | `shut_down_while_waiting()` (`Cancelled`) |
| Over `max_rows`; chunk schemas differ | the source's own errors |
| A second `__next__` while one is waiting | `Error::general_error` |
| A writable buffer is requested | `PyBufferError` |

## Relevant Commands

No new commands. `select_columns`, `slice` and `row` call the view constructors their `ns-rec`
commands call. `ns-rec` has no `cell` command, so `cell(r, c)` is checked against
`…/row-r/select_columns-c`, whose single cell `evaluate` returns as a scalar (AC-14).
`to_record`, `to_record_source` and `to_json` are used by the tests.

## Concurrency

The wrappers hold immutable `Arc`s, so they are free-threading safe without locks. The one
exception is `RecordStream`, which keeps its stream state behind a `Mutex`. IPC encoding and
decoding run under `py.detach`.

## Documentation Architecture

These match `DESIGN.md` `affects_docs`. Each changed reference or guide adds a `## History` row
and bumps `reviewed:`.

| Path | Change |
|---|---|
| `specs/reference/PYTHON_INTEGRATION.md` | New §Records: the classes, `Scope` and resolution, the inference and cast tables, Arrow routes and normalization, buffers and lifetimes, traversal, the open problems. §5 drops the records refusal |
| `specs/guides/PYTHON_GUIDE.md` | New "Records" chapter; the values table in §7 |
| `specs/reference/RECORD_STREAMS.md` | §"Arrow IPC": `liquers.origins`, replacing "`sources` are not written"; §ChunkOrigin; `read_chunk` |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | §RECORDS: what liquers-py does; §6 Python |
| `liquers-py/README.md`, `specs/README.md` | A RECORDS row with the two `NA`s; the design's row |
| API | Docstrings on every class and method; the module docs of `ipc.rs` and `liquers-py/src/records/mod.rs` |

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | Those under Integration Points |
| Crates and workflows affected | liquers-py, liquers-records, liquers-lib (`rowid` only); `scripts/test-liquers-py.sh` |
| Existing tests likely to change | `value.rs` unit tests pass `Scope::detached()`; `rowid`'s tests (`records_end_to_end.rs`, `record_manifest_resource_key.rs`) must still pass |
| New validation | `test_RECORDS_*.py`; IPC origins round trip; `read_chunk` and `PyChunkResolver` unit tests |
| Compatibility / data / concurrency / performance / security | IPC gains an optional key that old readers ignore. Every Arrow exit copies. The buffer protocol is `unsafe` FFI, read-only, and needs 3.11+ in the limited API (wheels are 3.14+) |
| Recovery | Additive. Reverting leaves records refused, as they are today |
| Certainty and open questions | High. The decision log in Phase 1 records this phase's choices. None needs Orest |
