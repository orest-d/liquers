---
id: RECORD-TABLE-CSV-REFINEMENTS
kind: design
title: A record view accepts the CSV separator refinements a DataFrame accepts
form: compact
status: complete
gh_pr: [211]
readiness: ready
autofix: eligible
area: [records]
issues: [RECORD-TABLE-FORMAT-REFUSES-CSV-REFINEMENTS-POLARS-ACCEPTS]
created: 2026-10-26
---
# A record view accepts the CSV separator refinements a DataFrame accepts

> **Complete on 2026-10-27**: implemented by automatic fix in #211.

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written; Phase 4
implemented on 2026-10-27 (automatic fix).

## Phase 1: High-Level Design

### Purpose

`TableFormat::from_data_format` says its names "match the DataFrame serializer's, so a filename
means the same thing for a `polars_dataframe` and a `RecordView`", but it knows only `csv:comma` and
`csv:tab` of the CSV refinements. The type registry accepts any refinement of a declared base
(`TypeInfo::supports_data_format`), so `csv:semicolon` passes the write-path check for a
`RecordView` and then fails in the codec. Make the record codec accept the CSV separator
refinements `parse_polars_data_format` accepts, with the same meaning.

### Problem Example

Verified by reading HEAD (`70b9005`): `liquers-records/src/formats/mod.rs`
`TableFormat::from_data_format` has arms `"csv" | "csv:comma"` and `"tsv" | "csv:tab"` and refuses
everything else; `liquers-lib/src/polars/serde.rs` `parse_polars_data_format` also maps
`csv:semicolon` → `;`, `csv:pipe` → `|`, and any one-character `csv:<c>` → `c`.
`ExtValue::as_bytes` for a `RecordView` (`liquers-lib/src/value/mod.rs`) calls
`TableFormat::from_data_format(format)`.

A `RecordView` stored with declared data format `csv:semicolon` (from Rust today, or
`…/data_format-csv-semicolon/…` once `query-data-format-command` lands): `set_state` passes the
registry check (base `csv` is declared for `RecordView`), then serialization fails with
"TableFormat::from_data_format: unknown data format 'csv:semicolon'". The same rows as a
`polars.DataFrame` are written as `;`-separated CSV. Should be: the record view is written as
`;`-separated CSV too, and reads back with the same format name.

### Scope and Acceptance Criteria

- **AC-1** Named separator refinements
  WHEN `TableFormat::from_data_format` is given `csv:semicolon` or `csv:pipe`
  THEN it returns `TableFormat::Csv` with separator `;` or `|`
- **AC-2** One-character refinement
  WHEN it is given `csv:<c>` where `c` is one ASCII character that is neither a letter, a digit,
  `"`, CR nor LF (`csv:;`, `csv:|`, `csv: `)
  THEN it returns `TableFormat::Csv` with separator `c`
- **AC-3** Unusable separators are refused by name
  WHEN it is given `csv:xy`, `csv:a`, `csv:"` or `csv:` (empty)
  THEN it fails naming the format, and the message lists the accepted CSV refinements
- **AC-4** Round trip
  WHEN a record batch is written with the format from `csv:semicolon` and read back with the same
  format
  THEN the values are equal and the written bytes use `;` between fields
- **AC-5** Existing names unchanged
  WHEN any name accepted at HEAD is given
  THEN the result is unchanged

Non-goals: the polars-only underscore spellings `csv_comma`, `csv_tab`, `csv_semicolon`,
`csv_pipe` (they are not refinements, so the registry refuses them for a `RecordView` before the
codec runs, and accepting them would also mean declaring them in `liquers-lib`); case
normalisation (polars lowercases the whole name; data formats are lowercase in metadata); one
shared parser for both table types (`data-format-vocabulary` territory: `liquers-records` cannot
depend on `liquers-lib`, and core has no format vocabulary yet).

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — bug fix restoring the behaviour `from_data_format`'s own doc
  comment promises, in one function of `liquers-records/src/formats/mod.rs` with its tests in the
  same file; the signature of the `pub fn` is unchanged, no type, command, feature or dependency
  changes (rules 1-6). Priority P3.
- **Leading issue:** None
- **Explanation:** `TableFormat::Csv { separator }` already carries any separator and the CSV
  reader and writer take it as a parameter (`csv_separator_is_configurable_for_tsv` proves a
  non-comma separator end to end), so only the name table changes.
- **Open questions:** None. Resolved in Phase 2 as implementation details under the documented
  promise: letters, digits, `"`, CR and LF are refused as separators (each would make the CSV
  unparseable or, for letters, mean something else after polars' lowercasing), so every name the
  record codec accepts means the same separator in both codecs.

### Design Dependencies

- `query-data-format-command` — **overlaps** (weak): lets a query declare `csv:semicolon`; it
  lists this issue as the reason a refinement can still fail late. No order.
- `data-format-vocabulary` — **overlaps** (weak): a future shared vocabulary could own the CSV
  refinement table; this fix does not pre-empt it.
- `polars-dataframe-ipc` — **overlaps** (weak): edits the IPC arm of `parse_polars_data_format`
  to match `TableFormat`'s aliases; different arm, different crate.
- `soft-consistency-refined-format` — **overlaps** (weak): the other refinement defect from the
  same design pass.

### Consolidated Findings

- The registry already declares `csv` for `RecordView`, and refinements match by base, so no
  `liquers-lib` change is needed for `csv:semicolon` to reach the codec.
- `liquers-records/src/sources.rs` also calls `from_data_format` (reading a source by its
  metadata); it gains the same names, which is the intent.
- `RECORD_STREAMS.md` §Table formats lists the accepted names and changes with the fix.

## Phase 2: Architecture

### Solution

In `TableFormat::from_data_format` (`liquers-records/src/formats/mod.rs`) add arms
`"csv:semicolon" => Csv { separator: b';' }` and `"csv:pipe" => Csv { separator: b'|' }`, and
before the final refusal handle `other` starting with `csv:` through a private helper:

```rust
/// The separator of a one-character `csv:<c>` refinement, when `c` can separate CSV fields.
fn csv_separator_refinement(refinement: &str) -> Option<u8>;
```

It returns `Some(b)` when `refinement` is exactly one ASCII byte that is not alphanumeric and not
`"`, `\r` or `\n`. The refusal message for any other `csv:` name names it and the accepted refinements, e.g. for
`csv:a`: "TableFormat::from_data_format: unknown CSV refinement 'csv:a'; use csv:comma, csv:tab,
csv:semicolon, csv:pipe or csv: followed by one punctuation character". Other names keep today's
message.
Update the doc comment to list the refinements and to say the underscore spellings are
DataFrame-only.

Rejected: copying `parse_polars_data_format` verbatim (it lowercases first, so `csv:X` means `x`,
and it accepts `"` and newlines); a shared parser (no crate both can depend on today).

### Changes

- `liquers-records/src/formats/mod.rs`: `TableFormat::from_data_format` arms and doc comment;
  private `csv_separator_refinement`; tests. Sync, `Error::general_error` as today.
- `specs/reference/RECORD_STREAMS.md` §Table formats: CSV row lists `csv:semicolon`, `csv:pipe`
  and `csv:<c>`; History row, `reviewed:` bump.
- No `liquers-lib` change; `specs/command_registry.yaml` unchanged.

### Risks

| Risk | Handling | Certainty |
|---|---|---|
| Likely files | `formats/mod.rs`, `RECORD_STREAMS.md` | high |
| Existing tests | `table_format_from_data_format_resolves_every_alias` and `…_refuses_unknown_names` (`xlsx`) stay green: no existing name changes (AC-5) | high |
| A separator that breaks the CSV writer | refused by the helper (AC-3); quoting already covers any other byte (`needs_quoting` checks the separator) | high |
| Divergence from polars | every accepted name maps to the same byte as polars; the record codec accepts a strict subset | high |
| Recovery | revert the arms | high |

## Phase 3: Examples and Tests

### Examples

- `TableFormat::from_data_format("csv:semicolon")` → `Csv { separator: b';' }`
- `TableFormat::from_data_format("csv:;")` → `Csv { separator: b';' }`
- `TableFormat::from_data_format("csv:a")` → error naming `csv:a` and the accepted refinements

### Tests

In `liquers-records/src/formats/mod.rs` `tests`:

- `table_format_from_data_format_resolves_csv_separator_refinements` — `csv:semicolon`,
  `csv:pipe`, `csv:;`, `csv:|`, `csv: ` resolve to the expected separators. Covers AC-1, AC-2.
- `table_format_from_data_format_refuses_unusable_csv_refinements` — `csv:xy`, `csv:a`, `csv:7`,
  `csv:"`, `csv:` are errors whose message contains the given name and `csv:semicolon`. Covers AC-3.
- `semicolon_refinement_round_trips` — build a two-column batch as
  `csv_separator_is_configurable_for_tsv` (`formats/csv.rs`) does; `write_table` with
  `from_data_format("csv:semicolon")?`; the bytes contain `;` and no `,` between fields;
  `read_table(…, ReadSchema::Infer, …)` with the same format gives equal values. Covers AC-4.
- Existing `table_format_from_data_format_resolves_every_alias`,
  `…_refuses_unknown_names` and `…_resolves_ipc_and_parquet_aliases` unchanged. Covers AC-5.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-records --lib formats`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `formats/mod.rs` tests — add the three tests; the first and third fail at HEAD —
  `cargo test -p liquers-records --lib table_format_from_data_format`
- [x] 2. `formats/mod.rs` `TableFormat::from_data_format` — two named arms, the `csv:` fallback
  through `csv_separator_refinement`, the refusal message, the doc comment — step-1 tests pass:
  `cargo test -p liquers-records --lib formats`
- [x] 3. `RECORD_STREAMS.md` §Table formats CSV row, History row, `reviewed:`; issue
  `status: closed` with a resolution note — `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-records --all-features --lib --tests` and `cargo test -p liquers-records
--lib --tests` (no format features); `cargo test -p liquers-lib --test record_typeinfo --test
record_value_round_trip` to confirm nothing in the lib relied on the refusal. Diff review: no `println!`, no `unwrap` in library code.
