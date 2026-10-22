# Phase 1: High-Level Design — data-format vocabulary

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — complexity `L`, adds a `pub` module to `liquers-core`, a
  serialized `pub` field to `TypeInfo` and a `WarningSource` variant (rules 1, 3, 4), across
  `liquers-core` and `liquers-lib` (rule 6).
- **Leading issue:** **Open design - scope split.** The source feature asks for six things; this
  design delivers points 1-2 and the write-only declaration, and proposes the rest as follow-up
  issues.
- **Explanation:** the slice is additive and checked against HEAD. Phases 3-4 assume the
  recommended answers.
- **Open questions:**
  1. **Open design - scope split:** deliver the vocabulary, the tooling and write-only formats
     here; move the generic serde path (point 3) to `SERDE-TYPES-HAVE-NO-GENERIC-DATA-FORMATS`,
     the format-choosing command (point 4) to `QUERY-CANNOT-CHOOSE-A-DATA-FORMAT`, the compact
     binary format (point 5) to `NO-COMPACT-BINARY-DATA-FORMAT`, and the declaration syntax
     (point 6) to `VALUE-TYPE-DEFINITION-MACRO`. Recommended: yes; points 3-5 each add a
     dependency or a command with its own open choices, and together would make this `XL`.
  2. **Open design - write-only representation:** a subset list
     `TypeInfo::write_only_data_formats` beside `supported_data_formats`, which keeps meaning
     "writable" (recommended: additive; every reader of `supported_data_formats` keeps working).
     Alternative: per-format capability records — serde breaks are allowed, but it edits every
     `TypeInfo` consumer in five crates for no extra expressiveness.
  3. **Proposed resolution - no normalization on the data path:** aliases stay distinct strings in
     metadata and declarations; the vocabulary validates and suggests only (codecs do not accept
     the same aliases: polars has no `arrow` at HEAD).
  4. **Proposed resolution - unknown is a warning, not a refusal:** a validator warning for a
     filename, a hint in the write path's existing refusal; registration does not refuse (tests
     guard the built-in types).
  5. **Proposed resolution - canonical spellings:** the spelling Liquers writes by default
     (`b` for bytes, `ipc` for Arrow IPC, `md`, `jpg`, `tif`, `ndjson`, `yaml`), with the other
     accepted spellings as aliases.

## Purpose

Data formats are bare literals in every codec: the sets drift, a typo looks like an unsupported
format, and a write-only format cannot be declared. Give them one vocabulary in `liquers-core`,
reachable offline by `liquers-validate`, and let a type declare write-only formats (VISION:
"N transformations × M output formats" needs a shared format name space).

## Problem Example

```
-R/data/sales.csv/-/ns-pl/head-5/data.jsonn
```

Today `liquers-validate` returns `Ok` with no warning (checked), and evaluation fails late with
"Type 'polars.DataFrame' cannot be serialized as 'jsonn'; supported formats: [csv, …]" — the same
message as for a real format the type lacks. Should be: a validator warning "data format 'jsonn'
of filename 'data.jsonn' is not known; did you mean 'json'?", and the same hint in the write error.

Write-only: `RecordView`'s `TypeInfo` (`liquers-lib/src/value/mod.rs`) declares `html` in its only
format list, so a stored `table.html` is claimed loadable; on reload `read_table` refuses it, the
fast track logs "failed to deserialize stored value (treated as corrupted)" and recomputes. Should
be: `html` declared write-only, and the reload goes straight to recomputation with no corruption
report.

Drift, at HEAD: `TEXTUAL` is a local const in `Value::type_descriptions` (`liquers-core/src/value.rs`);
`polars.DataFrame` declares `csv_comma`, `RecordView` does not; polars parses `csv_pipe`, records does
not; records accepts `arrow`, polars does not; `SimpleValue` reads `toml`/`yaml`/`yml` but declares
and writes neither. A refinement cannot be written in a query (`abc/x-csv:comma` fails to parse,
checked with `liquers-validate`).

## Scope and Acceptance Criteria

- **AC-1** A typo in a filename is reported offline
  WHEN `liquers-validate` checks `-R/data/sales.csv/-/ns-pl/head-5/data.jsonn`
  THEN the result is a warning naming `jsonn` and suggesting `json`, exit code 0; with `data.json`
  there is no warning
- **AC-2** The vocabulary answers
  WHEN `is_known_data_format`, `canonical_data_format` and `suggest_data_format` are asked about
  `csv:comma`, `jsonl`, `feather`, `jsonn` and `JSON`
  THEN they report known/`csv`, `ndjson`, `ipc`, unknown with suggestion `json`, and suggestion `json`
- **AC-3** Every declared format is known
  WHEN every `TypeInfo` of core `Value` and of `liquers_lib::value::Value` (each feature set) is checked
  THEN each supported and write-only format is known to the vocabulary
- **AC-4** A write-only format is declared and reported
  WHEN `RecordView`'s `TypeInfo` and the registry are asked about `html` and `csv`
  THEN `html` is writable and not readable, `csv` is both
- **AC-5** A write-only stored value is recomputed, not reported corrupted
  WHEN a keyed `RecordView` stored as `table.html` is reloaded from the store
  THEN deserialization is not attempted, the value is recomputed, and a recovery read
  (`get_any_status`) returns an error naming the write-only format
- **AC-6** Writing a write-only format still works
  WHEN a `RecordView` is persisted as `html`
  THEN the write path accepts it and stores the bytes
- **AC-7** The write error names an unknown format
  WHEN a state with data format `jsonn` is stored
  THEN the `SerializationError` says `jsonn` is not a known data format and suggests `json`
- **AC-8** Constants change no behaviour
  WHEN core `Value` and `SimpleValue` write and read every declared pair
  THEN the existing round-trip tests pass unchanged
- **AC-9** `TypeInfo` serializes compactly
  WHEN a `TypeInfo` without write-only formats is serialized, and one with them is round-tripped
  THEN the first has no `write_only_data_formats` key, the second reads back equal

Non-goals: the generic serde path, YAML/CBOR/TOML writers and the `SimpleValue` `toml`/`yaml`
asymmetry (follow-up, question 1); a format-choosing command and query-writable refinements
(follow-up); a binary format (follow-up); macro syntax (`VALUE-TYPE-DEFINITION-MACRO`); runtime
registration of formats by integrations; replacing literals inside `liquers-records`, image and
polars codecs (guarded by AC-3 through their `TypeInfo`s instead).

## Core Interactions

Value types (`TypeInfo`, core `Value`, `SimpleValue`, `ExtValue` descriptions); Assets (load path
for write-only formats, write-path error text); Query validation (`liquers-validate` warnings).

## Crate Placement

`liquers-core` owns the vocabulary, `TypeInfo` and the validator, so `liquers-validate` needs no
`liquers-lib`. `liquers-lib` only declares its formats with it.

## Design Dependencies

- `type-info-write-only-formats` (abandoned, E4) — input: subset list and load-path check adopted.
- `VALUE-TYPE-DEFINITION-MACRO` (issue, no design) and the three follow-ups filed with this design
  (`SERDE-TYPES-HAVE-NO-GENERIC-DATA-FORMATS`, `QUERY-CANNOT-CHOOSE-A-DATA-FORMAT`,
  `NO-COMPACT-BINARY-DATA-FORMAT`) — **required-by**.
- `polars-dataframe-ipc`, `core-value-json-identifier-dispatch` — **overlaps**, weak: literals this
  design turns into constants; the table already lists the IPC spellings. Either order.
- `serialization-lossy-write-warnings`, `value-serialization-writer` — **overlaps**, weak: same
  files, different functions.
- `deserialization-with-metadata` — **overlaps**, weak: both edit `deserialize_stored_value`
  (a metadata parameter there, a write-only branch here) — different statements, so T3 was judged
  not to hold; the second to land rebases.
- `TYPE-REGISTRY-NOT-REALM-AWARE` (issue) — **overlaps**, weak: per-realm formats come later.

## Documentation Intent

- Reference: extend `specs/reference/VALUE_TYPE_SYSTEM.md` (§Refinements → the vocabulary;
  §The registry → `write_only_data_formats`; §The record identifiers and §Reading → write-only).
- Guide: extend `specs/guides/TYPE_SYSTEM_GUIDE.md` step 4 (constants, write-only builder).
- Other documents: `.claude/skills/liquers-validate/SKILL.md` (the unknown-format warning).
- Documents to update: the three above; both source issues closed or re-scoped.

## Consolidated Findings

- Nothing stored changes. `supported_data_formats` already means "writable" at every use; only
  its doc comment says "and read" (fixed in step 2).
- The write-only check sits in `deserialize_stored_value` beside the "no data format" degraded
  branch, so the fast track and `get_any_status` both get it; `degraded_reason` gains the format.
- `RecordView`'s `parquet` is write-only too when `records-parquet` is on and `polars` off; the
  new `cfg` needs `scripts/check-build-matrix.sh`.
- Core `None` written as text is not readable either; left as is (`SimpleValue` reads it as text).
- `pickle` (`liquers-py`) is in the vocabulary, so AC-3 also holds for integration types.
