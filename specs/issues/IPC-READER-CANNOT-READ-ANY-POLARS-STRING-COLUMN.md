---
id: IPC-READER-CANNOT-READ-ANY-POLARS-STRING-COLUMN
kind: issue
title: The Arrow IPC reader cannot read a String/Binary column from any polars IpcWriter setting
status: draft
priority: P2
complexity: M
area: [records]
design: record-streams
created: 2026-09-27
github:
---

## Problem

`liquers-records/src/formats/ipc.rs`'s `read_ipc` (`decode_arrow_field`) accepts only Arrow's
classic, 32-bit-offset `Utf8`/`Binary` type tags for text/binary columns — the mapping
`specs/design/record-streams/phase2-architecture.md` §"Where our layout meets Arrow's" gives, and
`Column::Text`/`Column::Binary`'s own `Buffer<i32>` offsets. Everything else with text is refused
by name (`Utf8View`/`BinaryView`) or by decision 2's explicit large-offset exclusion (`LargeUtf8`/
`LargeBinary`).

polars 0.55.2's `DataType::String`/`DataType::Binary` cannot be written as plain `Utf8`/`Binary` at
all — `polars_core::datatypes::DataType::try_to_arrow` (`dtype.rs:987`) has exactly two branches:

```rust
String => if compat_level.0 >= 1 { ArrowDataType::Utf8View } else { ArrowDataType::LargeUtf8 },
Binary => if compat_level.0 >= 1 { ArrowDataType::BinaryView } else { ArrowDataType::LargeBinary },
```

`CompatLevel::newest()` (`IpcWriter`'s default) writes `Utf8View`/`BinaryView`; the only other
option, `CompatLevel::oldest()`, writes `LargeUtf8`/`LargeBinary` — never the plain, 32-bit-offset
type this reader accepts. So **no `IpcWriter` setting** produces a string/binary column `read_ipc`
can read; the two column types Phase 2 lists as supported are, in practice, unreachable from
polars.

## Impact

`liquers-lib/tests/records_ipc_polars.rs`'s `ipc_written_by_polars_reads_in_records` cannot include
a `label: Text` column at all — both compat levels were tried; both are refused
(`NotSupported: "... has unsupported Arrow type Utf8View (tag 24)"` and, at `CompatLevel::oldest()`,
`NotSupported: "... is LargeUtf8 (64-bit offsets), which this reader refuses"`). The test now
covers only `Int`/`Float`/`Bool`/`Date`/`Timestamp` in that direction, with a comment pointing here.

This narrows phase2-architecture.md's interop claim ("a file written by polars reads here" —
§"Tier 2 — Arrow IPC file (Feather v2)") to numeric/temporal/boolean columns only: a real
`.arrow`/`.feather` file with any text or binary column, from polars at any settings (or a modern
pyarrow, which defaults to `Utf8View` the same way since Arrow 14), is refused outright rather than
read with reduced fidelity. `ipc_written_by_records_reads_in_polars` (the other direction) is
unaffected — this crate's own writer emits plain `Utf8`, which polars reads without trouble.

## Expected behaviour

One of, decided together since they share a cause:

1. **Accept `LargeUtf8`/`LargeBinary`** by widening `Column::Text`/`Column::Binary` (or a reading
   path that narrows 64-bit offsets to `i32`, fallibly, when they fit) — reverses decision 2's
   explicit exclusion, so needs its own re-review, not a Step 6.1 fix.
2. **Accept `Utf8View`/`BinaryView`** by decoding the view-array layout (inline data for short
   strings; buffer index + offset + length for long ones, spread across the "variadic" buffers
   `RecordBatch.variadicBufferCounts` counts — currently unread by this module) into
   `Column::Text`/`Column::Binary`. More than a rename: the buffer count per field becomes
   variable, unlike every other field this reader decodes.
3. **Document the gap** in phase2-architecture.md's accepted-subset list (§"Still no claim of full
   Arrow support" already names what is excluded — `Union`, dictionary, `Large*` — but not
   `Utf8View`/`BinaryView`) and note in `ipc.rs`'s module doc comment that polars' output cannot
   satisfy the text/binary side of the interop claim without a writer-side compatibility flag this
   reader has no equivalent of.

## Discovery

While implementing Phase 4 Step 6.1's polars interop test: writing a `DataFrame` with a `String`
column through polars' `IpcWriter`, at both its default `CompatLevel::newest()` and the alternative
`CompatLevel::oldest()`, and reading the bytes back with `read_ipc` — refused both times, for
different reasons (`Utf8View` unsupported; then `LargeUtf8` explicitly excluded).
