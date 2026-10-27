# Phase 3: Examples & Use-cases — records-arrow-c-data-export

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Export a batch and read it back | the Rust round trip | AC-1, AC-2 |
| 2 | Example | A record view reaches pyarrow | Part B | AC-7 |
| 3 | Test | `liquers_records::cdata::tests::round_trip_every_column_type` | schema, columns, `chunk_id` after export + import | AC-1 |
| 4 | Test | `liquers_records::cdata::tests::export_points_at_the_batch_buffers` | pointer identity | AC-2 |
| 5 | Test | `liquers_records::cdata::tests::export_outlives_the_batch_and_release_frees_it` | `Arc` counts | AC-3 |
| 6 | Test | `liquers_records::cdata::tests::moved_child_survives_parent_release` | consumer moves a child | AC-3 |
| 7 | Test | `liquers_records::cdata::tests::format_strings_follow_the_mapping` | every format, `+w` child, `+s` root | AC-4 |
| 8 | Test | `liquers_records::cdata::tests::unsupported_formats_are_refused_and_released` | `i`, `U`, `vu`, dictionary | AC-5 |
| 9 | Test | `liquers_records::cdata::tests::import_honours_a_nonzero_offset` | offset 3, bit offset 3 | AC-6 |
| 9a | Test | `liquers_records::cdata::tests::export_refuses_an_inconsistent_batch` | mutated public fields | AC-10 |
| 10 | Test | `liquers-py/tests/test_VALUE_records_cdata.py::test_record_view_crosses_through_c_data` | pyarrow 14+ | AC-7 |
| 11 | Test | `liquers-py/tests/test_VALUE_records_cdata.py::test_old_pyarrow_uses_the_ipc_route` | version fallback | AC-8 |
| 12 | Check | `scripts/check-build-matrix.sh` rows with and without `cdata`, plus `grep -rn "unsafe" liquers-records/src` showing only `cdata.rs` and doc comments | AC-9 | AC-9 |

## Example 1: Export a batch and read it back

A Rust consumer (here the importer itself, standing in for pyarrow) receives a batch by pointer.

```rust
use liquers_records::cdata::{export_record_batch, import_record_batch};
let batch = /* RecordBatch: id Int (Id role), name Text with a null, emb Vector dim 4 */;
let (schema, array) = export_record_batch(&batch)?;
drop(batch);                                   // the export keeps the buffers alive
let back = import_record_batch(&schema, array)?;  // copies, then releases `array`
assert_eq!(back.schema.fields[0].key, KeyRole::Id); // roles came back through liquers.schema
```

The root has format `+s` and three children `l`, `u` and `+w:4`. `+w:4` has one `f` child of length
`4 × len`. The `name` child's `buffers[1]` is the address of the original offsets buffer, so no
data was copied on the way out (AC-2).

## Example 2: A record view reaches pyarrow

The fixture of `py-record-values-to-python` (a first command returning CSV text):

```python
@env.first_command(volatile=True)
def table():
    return "a,b\n1,x\n2,\n"

table = env.evaluate("table/ns-rec/to_record")      # validated with --command table
assert isinstance(table, pyarrow.Table)              # the type py-record-values-to-python chose
assert table.column("b").null_count == 1
assert b"liquers.schema" in table.schema.metadata
```

What this adds beyond Example 1: the pair crosses as PyCapsules, and pyarrow's import calls our
`release` when `table` is garbage collected.

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| Empty batch, zero columns | root `+s`, `n_children` 0, length 0; imports to an empty batch | 3 |
| Zero rows with columns | buffers of length 0 (non-null dangling pointers are valid) | 3 |
| `Vector` with `dim` 0 | `+w:0`, child length 0, rows carried by validity | 3, 7 |
| Column without validity | `buffers[0]` null, `null_count` 0 | 4 |
| Field name with NUL | export error naming the field | 7 |
| A released array passed to import | `general_error`, nothing freed twice | 8 |
| `tsm:` or `tsn:` timestamp | refused as unsupported (subset) | 8 |
| Stale `liquers.schema` (field renamed by the consumer) | ignored; schema derived from the Arrow fields | 3 |
| Batch dropped before export release | buffers readable; counts return to 1 after release | 5 |
| Python: `__arrow_c_array__` called twice | `ValueError` | 10 |

## Test Plan

- Unit (`liquers-records/src/cdata.rs` `#[cfg(test)] mod tests`, `cfg(feature = "cdata")`):
  - `round_trip_every_column_type` (AC-1): one column per variant with and without nulls, `Id` and
    `Source` roles, a `chunk_id`, the dim-0 and empty cases; schema, columns, `len` and
    `chunk_id` of `import(export(b))` equal `b`'s, `rows` is one default run, `sources` is empty.
  - `export_points_at_the_batch_buffers` (AC-2): for every child, compares `buffers[i]` with the
    column's `as_bytes().as_ptr()` / bitmap `as_bytes().as_ptr()`, and the `+w` child with `data`.
  - `export_outlives_the_batch_and_release_frees_it` (AC-3): `AlignedBuffer::strong_count` rises by
    one per exported node holding it, stays above zero after `drop(batch)`, and returns to its
    original value after dropping the pair.
  - `moved_child_survives_parent_release` (AC-3): `ptr::read`s child 0 out of the root, marks the
    slot released, drops the root, reads the moved child's values, then drops it; counts as above.
  - `format_strings_follow_the_mapping` (AC-4): reads each child's `format` and `flags`; also the
    NUL-name export error.
  - `unsupported_formats_are_refused_and_released` (AC-5): builds foreign arrays by hand (a test
    producer in the test module) with formats `i`, `U`, `vu` and a non-null `dictionary`; the error
    names the field and format; the test producer's release counter shows each was released once.
    Also a `tsm:` and a `tsn:` child (refused), and an already released array (`general_error`,
    the counter unchanged: nothing is released twice).
  - `export_refuses_an_inconsistent_batch` (AC-10): after `RecordBatch::new`, sets `batch.len`
    one higher, replaces a validity with a deserialized `Bitmap` whose bytes are short, and an
    offsets buffer whose last entry passes the data; each export fails naming the field.
  - `import_honours_a_nonzero_offset` (AC-6): a hand-built `l` child with offset 3, a `u` child
    with offset 3 and a validity bitmap at bit offset 3; the imported rows are the last `len`.
- Python (`liquers-py/tests/test_VALUE_records_cdata.py`, skipped when pyarrow is absent, as the
  polars tests are):
  - `test_record_view_crosses_through_c_data` (AC-7): Example 2, and equality with
    `pyarrow.ipc.open_file` of the same view written as `.arrow`. It also calls the private
    export object's `__arrow_c_array__` twice through a `pyarrow.record_batch` retry and expects
    `ValueError` on the second.
  - `test_old_pyarrow_uses_the_ipc_route` (AC-8): monkeypatches `pyarrow.__version__ = "13.0.0"`;
    the result is still equal.
- Build (AC-9): `bash scripts/check-build-matrix.sh`; and
  `cargo test -p liquers-records --features cdata --lib`, `cargo test -p liquers-records --lib`.
- Commands to run: `cargo test -p liquers-records --features cdata --lib cdata`,
  `bash scripts/test-liquers-py.sh -k records_cdata`, and optionally
  `cargo +nightly miri test -p liquers-records --features cdata --lib cdata`.

## Learning Log

- The C Data Interface is self-contained enough that our own importer is a faithful test consumer.
  pyarrow is needed only for the binding test.
- `__arrow_c_array__` lets liquers-py stay free of `unsafe`. The capsule destructor drops our
  struct, and its `Drop` releases it when the consumer has not moved it out.
- Queries validated with `target/debug/liquers-validate`: `table/ns-rec/to_record` (with
  `--command table`) and `-R/data/table.csv/-/ns-rec/to_record`, both `Ok`.
