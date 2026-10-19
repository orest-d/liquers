---
id: IPC-READER-POLARS-TEXT-COLUMNS
kind: design
title: The Arrow IPC reader reads polars' text and binary columns (view and large-offset layouts)
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [records]
issues: [IPC-READER-CANNOT-READ-ANY-POLARS-STRING-COLUMN]
created: 2026-10-19
---
# The Arrow IPC reader reads polars' text and binary columns

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented.

## Phase 1: High-Level Design

### Purpose

`specs/design/record-streams/phase2-architecture.md` §"Tier 2" promises that "a file written by
polars reads here". polars 0.55.2 writes a `String` column only as `Utf8View` (the default
`CompatLevel::newest()`) or as `LargeUtf8` (`CompatLevel::oldest()`), and writes `Binary` the same
way with the binary types. `liquers-records/src/formats/ipc.rs::read_ipc` refuses all four, so no
`.arrow` / `.feather` file from polars that has a text or binary column can be read. The reader
should accept these layouts and convert them, while reading, into the existing `Column::Text` /
`Column::Binary`, which keep their `i32` offsets.

### Problem Example

```rust
let mut df = df!("id" => [1i64, 2], "label" => ["short", "a label longer than twelve bytes"])?;
let mut bytes = Vec::new();
IpcWriter::new(&mut bytes).finish(&mut df)?;                     // writes Utf8View
read_table(&bytes, TableFormat::Ipc, ReadSchema::Infer, &ReadOptions::default())
// today: Err(NotSupported "Arrow IPC: field 'label' has unsupported Arrow type Utf8View (tag 24)")
// with .with_compat_level(CompatLevel::oldest()):
// today: Err(NotSupported "Arrow IPC: field 'label' is LargeUtf8 (64-bit offsets), which this reader refuses")
```

The query form is `-R/data/polars_export.arrow/-/ns-rec/to_record` (validated), and it fails the
same way. Expected result: a two-row batch whose `label` is `Text`, holding `"short"` and the long
string, from either file.

Verified at HEAD in `decode_arrow_field`; `ipc_written_by_polars_reads_in_records` omits text for
this reason. polars-arrow 0.55.2's `write_binview` emits validity, views, then one buffer per data
buffer, and sets `RecordBatch.variadicBufferCounts`.

### Scope and Acceptance Criteria

- **AC-1** polars default text reads
  WHEN `read_ipc` reads a file whose `String` column polars wrote as `Utf8View`, with short (≤ 12 bytes, inline) and long values, nulls, `""` and non-ASCII text
  THEN the field is `FieldType::Text` and every row equals what polars wrote
- **AC-2** polars default binary reads
  WHEN the same holds for a `Binary` column written as `BinaryView`
  THEN the field is `FieldType::Binary` and every row matches
- **AC-3** Large-offset layouts read
  WHEN a column is `LargeUtf8` / `LargeBinary` (polars `CompatLevel::oldest()`)
  THEN it reads as `Text` / `Binary` with the same values
- **AC-4** Several data buffers
  WHEN a view column's long values are spread over two or more variadic data buffers
  THEN each value is taken from the buffer its view names
- **AC-5** Malformed views are refused, not panicked on
  WHEN a view names a buffer index past the field's count, an offset + length past its buffer, a negative length, invalid UTF-8 in a `Utf8View` value, a `variadicBufferCounts` entry that is missing or does not match the buffers, or large offsets that decrease or point past the data
  THEN `read_ipc` returns `Err` naming the field
- **AC-6** Too large for an `i32` column
  WHEN a field's text or binary data in one record batch exceeds `i32::MAX` bytes
  THEN the read is `Err(NotSupported)` naming the field and the limit (no truncation, no wrap)
- **AC-7** Schemas unchanged
  WHEN such a field is read with `ReadSchema::Declared` (declared `Text` / `Binary`) or with an embedded `liquers.schema`
  THEN matching works as for `Utf8` / `Binary`, because the layout is invisible above the reader
- **AC-8** Nothing else widens
  WHEN a file has a dictionary-encoded field, a compressed body, or another refused type
  THEN it is still refused by name. The writer still emits plain `Utf8` / `Binary`

Non-goals: writing view or large layouts; a 64-bit or view `Column` layout; other widths polars
may write (`Int32`, `Float32`, …); compression; dictionaries.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. The design is `needs-decision` (rule 5). It also adds a
  private enum (rule 3), and its fixtures and interop tests sit in `liquers-lib` (rule 6).
- **Leading issue:** Open design question: which layouts the reader accepts.
- **Explanation:** fully specified under the recommended answer, private to one module. But the
  read subset is a documented contract (Tier 2 and `RECORD_STREAMS.md` refuse Large\* by name), so
  widening it is the maintainer's call, as the issue itself says.
- **Open questions:**
  1. **Open design: which layouts the reader accepts.** The issue gives three options:
     (a) Large\* only, which still fails on polars' default output, so the interop promise stays
     broken; (b) views only, which fixes polars' default output and leaves `CompatLevel::oldest()`
     and other Large\* writers refused; (c) document the gap. **Recommended: (a) and (b) together.**
     Both convert into `i32` offsets while reading, refusing above `i32::MAX` (AC-6). This keeps the
     in-memory layout decision intact and narrows only the read refusal list. Phases 2-4 assume this
     answer. Under (b) alone, steps 2 and 4's Large\* parts and AC-3 are dropped. Under (c), only
     step 6 remains.
- **Implementation details resolved:** a null row's view is not read; the 4-byte prefix is not
  compared; data is copied into one `AlignedBuffer`, as every column read already copies.

### Design Dependencies

- `record-streams`: the **origin** design (complete and frozen, E4, so it is not edited). This
  design changes its Tier 2 read scope, which is recorded in `RECORD_STREAMS.md` instead.
- `parquet-read-declared-schema`: **overlaps** (weak, polars interop through a different reader).
  There is no ordering between the two.
- `RECORDS-ARROW-C-DATA-EXPORT-NOT-BUILT` (feature): **overlaps**. Export stays plain `Utf8`, and
  nothing here constrains it.

### Consolidated Findings

- Confirmed at HEAD. The issue's pyarrow remark is not relied on (the checked-in
  `pyarrow_multi_batch.arrow` uses plain `Utf8` and reads today).
- Private to `ipc.rs`; schema resolution sees only `FieldType`, so it is untouched (AC-7).
- `variadicBufferCounts` makes the buffer count per field variable: `BodyReader` consumes it in
  view-field order and checks that every count was used.
- Every view is untrusted: index, offset, length and UTF-8 are checked before copying (AC-5).
- Fixtures follow the existing `#[ignore]`d-generator pattern in `records_ipc_polars.rs`.

## Phase 2: Architecture

### Solution

Keep `ArrowField.data_type` as the logical `FieldType` and record the physical layout beside it.
The read for `Text` / `Binary` branches on that layout and always produces the existing `i32`
offsets column.

- `decode_arrow_field`: `TYPE_LARGE_UTF8` maps to `(Text, Offsets64)`, `TYPE_LARGE_BINARY` to
  `(Binary, Offsets64)`, `TYPE_UTF8_VIEW` (24) to `(Text, View)` and `TYPE_BINARY_VIEW` (23) to
  `(Binary, View)`. `TYPE_UTF8` / `TYPE_BINARY` map to `Offsets32`. Every other type is unchanged.
- `Offsets64`: the buffers are validity, `i64` offsets and data. Use `read_fixed::<i64>`, then
  the 32-bit path's checks (first offset ≥ 0, never decreasing). Then, before the data bound,
  refuse `last - first > i32::MAX` (AC-6); then check the data bound, rebase, narrow, and run the
  shared UTF-8 / char-boundary check.
- `View`: the buffers are validity, then `len × 16`-byte views, then `n` data buffers, where `n` is
  the next entry of the header's `variadicBufferCounts`. Two passes. First, for each non-null
  row: `i32` length ≥ 0; ≤ 12 bytes are inline at `[4, 4+len)`, else `buffer_index` (at 8) and
  `offset` (at 12) must name a range inside that data buffer (checked arithmetic); sum the lengths
  and refuse past `i32::MAX` (AC-6) before allocating. Second, copy into one `Vec<u8>` with
  offsets; a null row is an empty slot. Then the shared UTF-8 check for `Utf8View`.
- `read_one_record_batch` reads `RB_VT_VARIADIC_BUFFER_COUNTS` (voffset 12, a vector of `i64`) into
  `BodyReader`. After all fields are read, any count left unused is an error (AC-5), and so is a
  `View` field with no count left.

Rejected alternatives:

- `i64`-offset or view `Column` variants: a core layout change touching every kernel; beyond M.
- Re-encoding with polars first: no `IpcWriter` setting yields plain `Utf8`.
- Decoding through polars in `liquers-lib`: `liquers-records` reads IPC without polars (browser).

Known-issue preflight: `RECORDS-ARROW-C-DATA-EXPORT-NOT-BUILT` and
`COLUMN-SLICE-COPIES-INSTEAD-OF-SHARING-BUFFER-STORAGE` are independent. Nothing blocks.

### Changes

`liquers-records/src/formats/ipc.rs`, all private:

```rust
const TYPE_BINARY_VIEW: u8 = 23;           // replace the literal arms in `type_tag_name`
const TYPE_UTF8_VIEW: u8 = 24;
const RB_VT_VARIADIC_BUFFER_COUNTS: u16 = 12;

/// How a Text/Binary field's bytes are laid out in the file; ignored for other types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VarLayout { Offsets32, Offsets64, View }

struct ArrowField<'a> { /* existing fields */ layout: VarLayout }
struct BodyReader<'a> { /* existing fields */ variadic_counts: Vec<i64>, variadic_idx: usize }

impl FbTable<'_> { fn i64_vector_field(&self, voffset: u16) -> Result<Vec<i64>, Error>; }
fn read_large_offsets_and_data(offsets: &[u8], data: &[u8], len: usize, name: &str, utf8: bool)
    -> Result<(Buffer<i32>, AlignedBuffer), Error>;
fn read_views(views: &[u8], data: &[&[u8]], validity: &Option<Bitmap>, len: usize, name: &str,
    utf8: bool) -> Result<(Buffer<i32>, AlignedBuffer), Error>;
fn check_utf8_offsets(offsets: &[i32], data: &[u8], name: &str) -> Result<(), Error>; // split out of read_offsets_and_data
```

`VarLayout` matches are exhaustive; the `u8` tag match keeps its documented catch-all. The module
doc comment drops `Large*` from its refusals and states the `i32::MAX` limit. Writer, `pub` API,
`liquers-lib` code, commands and registry are unchanged.

Documents: `specs/reference/RECORD_STREAMS.md` §Arrow IPC (subset and refusals; drop the "no
polars-written text column" sentence), §Limitations ("Arrow subset", "Interop"), and the §Layout
row (memory stays `i32`; the reader converts). History row, `reviewed:` bump.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| Hostile view or offset values cause a panic or a huge allocation | checked arithmetic, slices only through `get`, capacity bounded by the body length, the AC-5 hostile tests | high |
| `variadicBufferCounts` misread with several view fields | the fixtures have two view fields (`label`, `blob`) | high |
| polars output differs from our reading of the spec | fixtures from real polars 0.55.2; the interop test reads live output | high |
| wasm / no-`ipc` builds | existing `#![cfg(feature = "ipc")]` module; build matrix | high |

Recovery: reverting the reader commit restores today's refusals.

## Phase 3: Examples and Tests

### Examples

The Problem Example, read back, gives `value(0, 1) == FieldValue::Text("short")` for both compat
levels. `-R/data/polars_export.arrow/-/ns-rec/to_record` then yields the same table.

### Tests

Fixtures come from a new `#[ignore]`d `generate_polars_text_fixtures` in
`liquers-lib/tests/records_ipc_polars.rs`, run with
`cargo test -p liquers-lib --test records_ipc_polars -- --ignored`. It writes
`polars_utf8view.ipc` and `polars_large_utf8.ipc` to `liquers-records/tests/fixtures/`. Each has
columns `id: i64`, `label: String` = `["a", "a label longer than twelve bytes", null, "", "žluťoučký kůň"]`
and `blob: Binary`, with matching short, long, null and empty values.

`liquers-records/src/formats/ipc.rs` tests module:

- `feather_reads_polars_utf8view_fixture`: covers AC-1, AC-2.
- `feather_reads_polars_large_utf8_fixture`: covers AC-3.
- `read_views_takes_each_value_from_its_named_buffer`: calls `read_views` directly with two data
  buffers. Covers AC-4.
- `feather_refuses_view_with_buffer_index_out_of_range`,
  `feather_refuses_view_past_its_data_buffer`, `feather_refuses_view_with_negative_length`,
  `feather_refuses_utf8view_with_invalid_utf8` and `feather_refuses_missing_variadic_buffer_counts`.
  Each patches the fixture (`find_unique`) or calls `read_views` directly; likewise
  `feather_refuses_decreasing_large_offsets`. Covers AC-5.
- `read_large_offsets_refuses_data_past_i32_max` (offsets `[0, i32::MAX + 1]`, short buffer) and
  `read_views_refuses_total_past_i32_max` (2 049 views of one shared 1 MiB buffer): both refuse
  before allocating. Covers AC-6.
- `feather_polars_utf8view_with_declared_schema`: declared `Text` / `Binary` match, and declared
  `Int` for `label` refuses as today. Covers AC-7.
- `feather_read_refuses_dictionary_encoded_batches` and `feather_read_refuses_compressed_bodies`
  (existing, unchanged), plus the existing writer round-trip tests. Covers AC-8.

`liquers-lib/tests/records_ipc_polars.rs`:

- `ipc_written_by_polars_reads_in_records`: add the `label` and `blob` columns and remove the
  comment pointing at the issue. Covers AC-1, AC-2.
- `ipc_written_by_polars_oldest_compat_reads_in_records`: covers AC-3.

Run with `cargo test -p liquers-records --all-features --lib --tests`, then
`cargo test -p liquers-lib --test records_ipc_polars`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-lib/tests/records_ipc_polars.rs`: add `generate_polars_text_fixtures` (`#[ignore]`)
  and run it, then commit the two fixtures. Proof:
  `cargo test -p liquers-lib --test records_ipc_polars -- --ignored generate_polars_text_fixtures`.
  Rollback: delete the fixtures.
- [ ] 2. `ipc.rs`: add the constants, `VarLayout`, the `ArrowField.layout` field and the
  `decode_arrow_field` arms; `type_tag_name` uses the new constants. Proof: the crate builds, and
  `type_tag_name_covers_the_supported_tags` is extended to 23/24.
- [ ] 3. `ipc.rs`: add `fb::FbTable::i64_vector_field` and the `BodyReader` variadic fields, and fill
  and check them in `read_one_record_batch`. Proof:
  `feather_refuses_missing_variadic_buffer_counts`.
- [ ] 4. `ipc.rs`: split `check_utf8_offsets` out of `read_offsets_and_data`, add
  `read_large_offsets_and_data` and `read_views`, and dispatch on `layout` in `BodyReader::column`.
  Proof: every Phase 3 `ipc.rs` test, with
  `cargo test -p liquers-records --all-features --lib --tests`.
- [ ] 5. `liquers-lib/tests/records_ipc_polars.rs`: extend `ipc_written_by_polars_reads_in_records`
  and add the oldest-compat test. Proof: `cargo test -p liquers-lib --test records_ipc_polars`.
- [ ] 6. Docs: the `ipc.rs` module comment and `RECORD_STREAMS.md` (§Arrow IPC, §Limitations, the
  §Layout wording, a History row and a `reviewed:` bump). Close
  `IPC-READER-CANNOT-READ-ANY-POLARS-STRING-COLUMN` with a resolution note. Proof:
  `python3 scripts/docs_index.py --check`.

### Validation

Run `cargo test -p liquers-records --all-features --lib --tests`,
`cargo test -p liquers-records --lib --tests` (without `ipc` the module is not compiled),
`cargo test -p liquers-lib --lib --tests`, `bash scripts/check-build-matrix.sh` (including the
wasm rows) and `python3 scripts/docs_index.py --check`. Review the diff for writer changes (there
should be none), debug output, and fixture sizes (a few hundred bytes each).
