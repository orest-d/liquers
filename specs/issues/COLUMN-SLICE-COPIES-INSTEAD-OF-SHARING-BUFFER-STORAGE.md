---
id: COLUMN-SLICE-COPIES-INSTEAD-OF-SHARING-BUFFER-STORAGE
kind: issue
title: Column slice copies instead of sharing buffer storage
status: draft
priority: P3
complexity: M
area: [records]
design: record-streams
created: 2026-09-26
github:
---
## Problem

`specs/design/record-streams/phase2-architecture.md` (§"Kernels live on `Column`, so views get the
fast path too") documents `Column::slice` as `// zero-copy` and describes `RowRangeView`'s
`column_range` as producing "zero-copy slices, over a batch". But `Buffer<T>` and `AlignedBuffer`
(`liquers-records/src/buffer.rs`, Step 2.2) have no windowed view of their own storage — each holds
exactly one `Arc<[AlignedChunk]>` covering the whole logical byte range, with no start offset — so
there is no way to construct a sub-range that shares the parent's allocation. `Column::slice`
(`liquers-records/src/column.rs`, `slice_buffer`/`slice_bitmap`/`slice_bytes`/`slice_vector`) is
implemented by copying the selected range into a freshly allocated buffer via `Buffer::from_slice`.

## Impact

Every `Column::slice` call — and by extension every zero-length-preserving read through
`RecordView::column_range` on a plain `RecordBatch` — allocates and copies, rather than sharing the
parent's `Arc`. For a UI scrolling a wide window over a large in-memory table (the scenario
phase2-architecture.md's §"Why the required method is a column *range*" specifically motivates:
"rows 1000..1050 of a computed view" should be "cheap"), this is a real cost that the design
explicitly meant to avoid. There is a full workaround already in place — correctness is unaffected,
every `Column` kernel test in Step 2.3 passes — this is purely a missed performance property.

## Expected behaviour

One of:
1. Give `AlignedBuffer`/`Buffer<T>` a `(start, len)` window over their shared `Arc<[AlignedChunk]>`
   (the `bytes::Bytes` idiom Phase 2 already names for `RecordBatchMut::freeze` elsewhere), so
   `Buffer::slice`/`AlignedBuffer::slice` can share storage; `Column::slice` then delegates to it.
2. Or, if arbitrary byte-offset windows conflict with the 64-byte alignment guarantee (a slice
   starting mid-chunk is not itself 64-byte aligned), document that `Column::slice` copies and
   correct phase2-architecture.md's `// zero-copy` comment and the `RowRangeView` table entry to
   match reality, so a future reader does not rely on a guarantee that was never implemented.

## Discovery

Found while implementing Step 2.3 of the record-streams Phase 4 plan
(`liquers-records/src/column.rs`): writing `Column::slice` against the `Buffer`/`AlignedBuffer` API
from Step 2.2 showed neither type exposes a sub-range accessor, so the only available
implementation copies. Phase 3's own test for this
(`column_slice_returns_the_requested_range`, §2.3) already anticipates the gap: its comment reads
"Zero-copy sharing is `Buffer`'s internal representation ..., not something this level exposes an
accessor for."
