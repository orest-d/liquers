# Phase 5: Documentation - liquers-py-records

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical (PR #240)

## Implementation Summary

Record views and sources now cross into Python as wrappers over the same `Arc`s
(`liquers-py/src/records/`). The design is implemented as approved: Phase 4 Steps 1-8, commits
`36cead9`, `ad45f3a`, `af95ce1`, `1ec82f1` and `0d0d8c7`. AC-1 to AC-18 each have a test.

- **Classes.**
  - `lq.RecordView`, with schema, narrowing, `to_list`/`to_dict`, `item`;
  - `RecordColumn` and `RecordBuffer`, which lend columns through the read-only buffer protocol;
  - the schema classes `FieldType`, `KeyRole`, `FieldSchema`, `RecordSchema` and `ChunkOrigin`;
  - `RecordSource`, `RecordStream` and `RecordChunk`.
- **Crossing.**
  - A single-cell view reaches Python as its scalar (`record_view_is_single_cell`).
  - Wrappers go back as the same `Arc`.
  - A `pyarrow.Table` or `RecordBatch` enters as a view.
- **Building.**
  - `from_records` and `from_dict` infer field types, or apply `fields=` declarations with
    lossless casts.
  - `from_arrow`, `from_pandas` and `from_polars` copy data in; `to_arrow`, `to_pandas` and
    `to_polars` copy it out.
  - `to_arrow` keeps roles, chunk id and origins in the table's metadata.
- **Sources.**
  - Every conversion carries a `Scope`. `PyChunkResolver` reads chunks through the command's
    context while that context is valid, then through the environment, which it holds weakly.
  - Traversal is one chunk per step, synchronous or async. `chunk(i)` opens one chunk only.
    `materialize` is bounded.
  - A concurrent step is refused, and shutdown ends traversal.
- **Core changes, in `liquers-records`.**
  - The Arrow IPC writer and reader carry `sources` under `liquers.origins`.
  - `read_chunk` is shared with `ns-rec/rowid`, which now uses it.
  - `format_field_value` is public, so the cast to text can use it.

Deviations, all recorded in Phase 4's Progress:
- writing to a read-only buffer raises `TypeError` from `struct.pack_into`, not `BufferError`;
- a view with no fields has no rows;
- `liquers.chunk_id` is the serde form of the key;
- an unbridgeable polars dtype is a `ConversionError`;
- the resolver's context path is tested through a dependency cycle;
- the test manifests' chunks are views.

## Documentation Delivered

### New Reference Documents
None. The contract is a new section of the existing Python reference, which owns the binding.

### New Guide Documents
None. The usage is a new section of the existing Python guide.

### Existing Documents Reviewed or Updated
`affects_docs`, each with a History row and `reviewed: 2026-11-01`:
- `specs/reference/PYTHON_INTEGRATION.md`:
  - new §11 Records;
  - §1 gains the `_records.py` row and the `liquers-records` link;
  - §2 the record classes;
  - §5 the conversion rows, and the records refusal is removed;
  - §8 the records limitation.
- `specs/reference/RECORD_STREAMS.md`:
  - §Arrow IPC: `liquers.origins`;
  - §Rows: `read_chunk`;
  - §ChunkOrigin;
  - the Table formats row.
- `specs/guides/PYTHON_GUIDE.md`:
  - §7 Records, as a subsection of Values rather than a new chapter, so the § numbers cited from
    the code stay valid;
  - the values table;
  - §2 the class list;
  - §10.
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`: §RECORDS "What `liquers-py` does", and §6 Python.
- `liquers-py/README.md`: the `RECORDS` row with its two `NA`s.
- `specs/README.md`: the M5 row.
- API docs: the module docs of `liquers-records/src/formats/ipc.rs` and
  `liquers-py/src/records/mod.rs`, and a docstring on every class and member
  (`test_DOCS_docstrings.py`).

### Links and Capability Map
`specs/README.md`'s M5 row now links PYTHON_INTEGRATION §11, PYTHON_GUIDE §7 Records and
RECORD_STREAMS §Arrow IPC.

## Issues Filed

- `BYTES-VALUE-HAS-NO-NAMED-DATA-FORMAT` (P3, draft): a bytes value cannot be serialized as `csv`,
  so a command returning raw CSV bytes cannot be a manifest chunk. Found while writing the
  `RECORDS07` tests, which return views instead. Not fixed here: it is a core serialization
  question.
- Closed: `PY-RECORD-VALUES-CANNOT-CROSS-TO-PYTHON` and
  `PY-RECORD-SOURCES-CANNOT-BE-TRAVERSED-FROM-PYTHON`.

## Important Learning

- A source's resolution scope has to travel with the wrapper, not be looked up when it is used.
  The same `lq.RecordSource` is read inside a command, where its chunks must become dependencies,
  and after the command, where it must not keep the environment alive. A weak environment handle
  plus an invalidated `Context` gives both behaviours.
- PyO3's buffer protocol is the cheap half of an Arrow-style hand-off. It is zero-copy and needs
  no third-party package, which makes it the right primitive for numpy. Arrow, pandas and polars
  then copy through IPC bytes, which already carry the schema with its roles.
- The limited API has no `PyDate`; build dates with `date.fromordinal` through the module.

## Conformance and Remaining Work

The requested, approved and implemented scope match, including the scope changes Orest approved
(construction from Python data and dataframes, `to_list`/`to_dict`, declared or inferred fields).
`RECORDS09` and `RECORDS11` are `NA` with their reasons. Nothing remains in this design. The
defect found is filed as an issue.

## Validation

- Python suite on CPython 3.14: 416 passed, 10 skipped. CI runs 3.10 and 3.14t.
- `liquers-records`, all features: 479 tests. The `rowid` tests pass.
- `scripts/check-build-matrix.sh`: every native configuration, and every wasm32 row once the
  target was installed.
- `python3 scripts/docs_index.py --check`: clean after regeneration.
- The guide's examples run under `test_GUIDE_examples.py`.
