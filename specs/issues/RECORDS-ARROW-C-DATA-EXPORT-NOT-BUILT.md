---
id: RECORDS-ARROW-C-DATA-EXPORT-NOT-BUILT
kind: feature
title: Record batches cannot be handed to Arrow consumers in place; every route copies
status: draft
priority: P3
complexity: L
area: [records, py]
design: record-streams
created: 2026-09-27
github:
---
# Record batches cannot be handed to Arrow consumers in place; every route copies

## What is missing

`liquers-records` keeps columns in Arrow's buffer layout, with 64-byte-aligned buffers and LSB-first
validity bitmaps. The point of that layout is a zero-copy hand-off: the Arrow C Data Interface
(`ArrowArray` / `ArrowSchema`, `repr(C)`) could pass a batch to pyarrow, pandas, polars or DuckDB by
pointer. That export was designed in `record-streams` Phase 2 §"Arrow interoperability" and not
built. Today a batch reaches those systems in one of three ways, each of which copies:
- the in-process polars bridge (`liquers-lib/src/records/polars.rs`);
- Arrow IPC bytes;
- Parquet bytes.

There is no `liquers-py` path at all.

## Expected behaviour

A C Data export (and import) for `RecordBatch` in `liquers-records`, with the release callback
keeping the `Arc`-shared buffers alive. Then a pyo3 conversion in `liquers-py` to a `pyarrow`
batch. Phase 2 lists the three places where the layout is not 1:1 with Arrow's
(`Vector`→`FixedSizeList`, `Text` offsets, the dictionary-less `Source`), and those need their
mapping.

## Discovery

Recorded 2026-09-27 in record-streams Phase 5 as design scope deliberately left unbuilt. It was
never scheduled in the Phase 4 plan.
