# Phase 3: Examples & Use-cases — liquers-py-records

## Overview Table

| # | Kind | Name | Scenarios |
|---|---|---|---|
| 1 | Example | Read a table, then leave for pandas | AC-1, AC-3, AC-9 |
| 2 | Example | Walk a manifest stream; build a table in Python | AC-6, AC-16, AC-18 |
| 3 | Test | `test_RECORDS_views.py` | AC-1, AC-2, AC-10, AC-12, AC-14 |
| 4 | Test | `test_RECORDS_columns.py` | AC-3, AC-4, AC-5 |
| 5 | Test | `test_RECORDS_sources.py` | AC-6, AC-7, AC-8, AC-13, AC-15 |
| 6 | Test | `test_RECORDS_build.py` | AC-16, AC-17, AC-18 |
| 7 | Test | `test_RECORDS_arrow.py` | AC-9, AC-11 |
| 8 | Test | Rust: `ipc.rs`, `sources.rs`, `liquers-py/src/records/resolver.rs` | AC-2, AC-13, AC-15 |

**The fixture.** `liquers-py/tests/data/records_all_types.feather` is committed. It is written
by the Rust test `ipc_writes_the_python_fixture` and compared byte for byte by
`python_fixture_is_current`. It holds four rows with these fields: `id` (Text, key `Id`), `src`
(UInt, key `Source`), and one each of Bool, Int, Float, Text, Binary, Date, Timestamp and Vector
(`dim` 2). Every field except `id` has a null. It also carries two origins and the chunk id
`-R/fx/part-0.csv`. Python reads it with `ipc_bytes/ns-rec/to_record-feather`, so no pyarrow is
needed.

**Validated queries** (`liquers-validate --command table --command manifest --command part
--command arrow_table --command ipc_bytes`; all `Ok`, and each `encoded` equals the source):
1. `table/ns-rec/to_record-csv`
2. `table/ns-rec/to_record-csv/to_record_source`
3. `manifest/ns-rec/to_record_source`
4. `part-0`
5. `arrow_table/ns-rec/to_json`
6. `ipc_bytes/ns-rec/to_record-feather`
7. `table/ns-rec/to_record-csv/select_columns-amount/row-1`
8. `table/ns-rec/to_record-csv/slice-0-2`
9. `table/ns-rec/to_record-csv/select_columns-id-amount`

## Example 1: read a table, then leave for pandas

```python
import liquers as lq
env = lq.Environment()

@env.first_command
def table() -> bytes:
    return b"id,amount\na,1\nb,2\nc,\n"

view = env.evaluate("table/ns-rec/to_record-csv")          # AC-1
assert isinstance(view, lq.RecordView) and len(view) == 3
assert view.schema.names == ["id", "amount"]
amount = view.column("amount")
assert amount.type == lq.FieldType.Int and amount.null_count == 1
assert memoryview(amount).tolist()[:2] == [1, 2]           # lent, read-only (AC-3)
assert amount.to_list() == [1, 2, None]
assert env.evaluate("table/ns-rec/to_record-csv/select_columns-amount/row-1") == 2   # AC-10
df = view.to_pandas()                                       # AC-9, needs liquers-py[pandas]
assert list(df["id"]) == ["a", "b", "c"]
```

## Example 2: walk a manifest stream, and build a table in Python

The chunks are CSV *bytes* (`filename="part.csv"` gives them the `csv` data format), so the
stream parses each one with the manifest's declared schema. The schema declares `n` as `Float`.
Inference would choose `Int` for `1`, so a `Float` in every yielded view proves that the stream
applied the manifest, which is route 2's reason. `part` counts its calls.

```python
CALLS = {"part": 0}

@env.first_command(filename="part.csv")
def part(i: int = 0) -> bytes:
    CALLS["part"] += 1
    return f"id,n\nr{i},{i}\n".encode()

@env.first_command
def manifest():
    return {"manifest": "record-stream",
            "uniform_schema": {"fields": [{"name": "id", "data_type": "Text"},
                                          {"name": "n", "data_type": "Float", "label": "Amount"}]},
            "chunks": [{"query": f"part-{i}"} for i in range(3)]}

source = env.evaluate("manifest/ns-rec/to_record_source")
assert CALLS["part"] == 0 and source.schema.field("n").label == "Amount"
for step, chunk in enumerate(source):                 # AC-6
    assert CALLS["part"] == step + 1
    assert [(f.name, f.type) for f in chunk.schema.fields] == [("id", lq.FieldType.Text), ("n", lq.FieldType.Float)]

built = lq.RecordView.from_records([{"id": "a", "n": 1}, {"id": "b"}],
                                   fields={"id": lq.FieldSchema("text", key="id")})   # AC-18
assert built.schema.field("id").key == lq.KeyRole.Id
assert built.to_dict() == {"id": ["a", "b"], "n": [1, None]}                   # AC-16, AC-17
```

## Edge and Error Cases

Each case follows from a Phase 2 risk. A test listed without a scenario is an edge-case test.

| Symptom | Expected | Test |
|---|---|---|
| `item()` on a 2-row view | `ConversionError` naming `2 rows x 2 fields` | `test_item_on_a_larger_view_names_its_shape` (AC-12) |
| A buffer is held while its view is dropped and 200 MB is allocated | Original values | `test_buffer_outlives_its_view` (AC-4) |
| `struct.pack_into("q", column, 0, 0)`; `memoryview(column)[0] = 0` | `BufferError`; `TypeError` (read-only) | `test_buffers_are_read_only` (AC-3) |
| `memoryview(text_column)` | `BufferError`; `.offsets` and `.data` work | `test_text_and_bool_columns_expose_parts` (AC-3) |
| A second `next()` while `part` waits on a `threading.Event` that is set only after the second call returns | `GeneralError` | `test_concurrent_next_is_refused` |
| `for`, `chunk`, `materialize` or `to_arrow` inside an `async def` command | `RuntimeError` from `check_blocking_call` | `test_blocking_calls_inside_async_command_raise` |
| A source kept after its command returned | Iterates through the environment | `test_source_outlives_its_command` |
| `shutdown()` before, and during, a wait | `GeneralError`, and `Cancelled` while waiting; a held view still reads | `test_shutdown_ends_traversal` (AC-15) |
| A source with no environment (unit test, `Scope::detached`) | `GeneralError` | `resolver_refuses_without_environment` |
| Chunks whose schemas differ, then `materialize` | The source's error, naming the field | `test_materialize_of_differing_chunks` |
| pyarrow blocked (`monkeypatch.setitem(sys.modules, "pyarrow", None)`) | `to_arrow` names `[arrow]`, `to_pandas` names `[pandas]`; `from_polars` still works | `test_exits_name_their_extra` (AC-9) |
| `timestamp[ns]` with nanoseconds; `string_view`; an LZ4-compressed feather | Refused naming the column; normalized; read | `test_lossy_timestamp_is_refused`, `test_arrow_table_enters_records` (AC-11) |
| A polars `List` or `Decimal` column | `ConversionError` naming it | `test_polars_unbridgeable_column` |
| Mixed `str` and `int` in one column | Inference error naming field and row | `test_inference_conflict_names_field_and_row` |
| A pandas int column holding `None`; an `Id` with nulls; an int beyond 64 bits | `Float` unless declared; refused; refused | `test_pandas_nullable_int_needs_a_declaration`, `test_id_with_nulls_is_refused`, `test_int_out_of_range_names_field_and_row` (AC-18) |
| Free-threaded: 8 threads read columns | Agree | `test_concurrent_column_reads` (AC-3) |

## Test Plan

Python tests live in `liquers-py/tests/`. Any test that needs pyarrow, pandas or polars calls
`pytest.importorskip` in its body.

- **`test_RECORDS_views.py`**
  - `test_view_reaches_python` (AC-1): through `evaluate` and through `evaluate_state`, whose
    metadata reports `RecordView`.
  - `test_view_round_trips_with_identity` (AC-2): a Python command returns the fixture view. The
    next command sees the same `chunk_id`, `origins` and `Id` key.
  - `test_single_cell_is_a_scalar` (AC-10): query 7 gives `2`, and its `state.record_view()` gives
    the view.
  - `test_item_on_a_larger_view_names_its_shape` (AC-12).
  - `test_narrowing_matches_commands` (AC-14): each step is compared on its own, with
    `evaluate_state(q).record_view().to_list()` for queries 8 and 9. `cell(1, "amount")` is
    compared with query 7.
- **`test_RECORDS_columns.py`**
  - `test_lent_and_copied_columns_agree` (AC-3): on the fixture, buffer values, `validity` bits
    and `to_list()` agree for every type.
  - `test_buffers_are_read_only`, `test_text_and_bool_columns_expose_parts` and
    `test_concurrent_column_reads` (AC-3).
  - `test_buffer_outlives_its_view` (AC-4).
  - `test_record_handles_are_released` (AC-5): `live_record_handle_count()` returns to its
    baseline after `del` and `gc.collect()`.
- **`test_RECORDS_sources.py`**
  - `test_stream_reads_one_chunk_per_step` (AC-6): Example 2.
  - `test_async_stream_yields_same_rows` (AC-7).
  - `test_materialize_is_bounded` (AC-8): sync and async. Over the limit, the error contains
    `max_rows`.
  - `test_chunks_list_without_evaluation` (AC-13):
    - `chunks()` leaves `CALLS["part"] == 0`;
    - an in-memory source from query 2 lists one `kind == "memory"` chunk that is not
      `evaluable`;
    - `chunk(1)` is `r1`, with the manifest's `Float`.
  - `test_shutdown_ends_traversal` (AC-15), and the edge-case tests above.
- **`test_RECORDS_build.py`**
  - `test_constructors_agree` (AC-16): runs `from_records`, `from_dict`, `from_pandas` (with `n`
    as `Int64`), `from_polars` and `from_arrow` on the AC-16 rows.
  - `test_to_list_and_to_dict_round_trip` (AC-17).
  - `test_fields_inferred_declared_or_given[from_records|from_pandas]` (AC-18): four cases for each
    route.
  - The build edge cases.
- **`test_RECORDS_arrow.py`**
  - `test_exits_carry_rows_and_metadata` (AC-9): on the fixture, the table's decoded metadata
    equals `view.chunk_id`, `view.origins` and the schema's keys.
  - `test_exits_name_their_extra` (AC-9). This needs `_extras.py`, from `liquers-py-package`,
    which lands first (Phase 4).
  - `test_arrow_table_enters_records` and `test_lossy_timestamp_is_refused` (AC-11).
- **Rust**
  - `ipc.rs`: `ipc_round_trips_origins` (AC-2), `ipc_without_origins_reads_as_before`,
    `stale_schema_drops_origins`, `ipc_writes_the_python_fixture` and
    `python_fixture_is_current`.
  - `sources.rs`: `read_chunk_reads_only_the_addressed_manifest_chunk` and
    `read_chunk_walks_an_in_memory_source` (AC-13).
  - `resolver.rs`: `resolver_uses_the_context_while_valid_then_the_environment` and
    `resolver_refuses_after_shutdown` (AC-15), plus `resolver_refuses_without_environment`.
- **Commands**
  - `bash scripts/test-liquers-py.sh -k RECORDS`
  - `cargo test -p liquers-records --all-features --lib`
  - `cargo test -p liquers-lib --lib --tests rowid`, which runs `records_end_to_end.rs` and
    `record_manifest_resource_key.rs` through the name filter.

`RECORDS09` and `RECORDS11` stay as skips marked `na(...)`.

## Learning Log

- A manifest given as a Python dict is recognized by `manifest: record-stream`, so a test needs no
  store. Its unkeyed chunks are evaluated, never read from a store, so AC-6 counts evaluations.
- A one-cell query collapses to a scalar, so comparing a narrowed view with a command goes
  through `evaluate_state(...).record_view()`.
- A manifest's `uniform_schema` only *checks* a chunk that is already a view (names, types,
  not-null), and an `Int` view fails a `Float` declaration. Byte chunks are *parsed* with it, so
  the example's chunks are bytes. The proof that the schema was applied is then a type inference
  would not choose. Implementation confirms that `filename=` gives the bytes the `csv` data format;
  if it does not, the command returns the bytes from a `.csv`
  key in a memory store and the chunk is `-R/fx/part-0.csv`.
- A fixture committed from Rust keeps AC-2 and AC-3 free of pyarrow, so they run on PyPy too.
