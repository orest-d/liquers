---
id: SIMPLE-VALUE-UNTYPED-AND-SCALAR-READS
kind: design
title: liquers-lib's base value reads untyped files as Bytes and textual scalars as their type
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [lib/value]
issues: [STORED-UNTYPED-FILE-OF-UNLISTED-FORMAT-CANNOT-BE-READ, SIMPLE-VALUE-READS-TEXT-SCALARS-AS-TEXT]
merged: 2026-10-08
created: 2026-10-08
---
# liquers-lib's base value reads untyped files as Bytes and textual scalars as their type

Produced under [`guides/autonomous_bulk_design.md`](../../guides/autonomous_bulk_design.md) by the
2026-10-08 backlog compaction: Phases 1-4, reviewed without phase approval. Not an approval and not
an implementation. Two sources share this design because both fixes edit the same function,
`SimpleValue::deserialize_from_bytes` (overlap test T3); the leading source is the `P2` one.

## Phase 1: High-Level Design

### Purpose

`SimpleValue::deserialize_from_bytes` (`liquers-lib/src/value/simple.rs` ≈670) has two read rules
that disagree with what its users expect:

1. a stored file with no type identifier (`""`) or `Bytes`, in a format no base arm lists (`csv`,
   `png`, `parquet`), is refused, so a hand-placed CSV cannot be loaded as a resource;
2. a scalar written under a textual format (`txt`, `html`, `rs`, `py`, `css`, `js`) reads back as
   `Text`, where core `Value` reads it back as its own type.

### Problem Example

1. A CSV placed in the store with no type identifier: `-R/data/raw/jan.csv/-/ns-rec/to_record-csv`
   fails with "No recipe found for key data/raw/jan.csv", because `CombinedValue` gets "Unsupported
   format in deserialize_from_bytes: csv" from `SimpleValue` and "Unsupported type identifier" from
   `ExtValue`, and the fast track treats the entry as corrupted. Reproduced 2026-10-08 by the
   ignored test `manifest_over_hand_placed_csv_files_materializes`.
2. `SimpleValue::I32 { value: 7 }` written as `n.txt` is the bytes `7`; reading them with identifier
   `I32` and format `txt` gives `Text("7")`. Core gives `I32(7)`.

### Scope and Acceptance Criteria

- **AC-1** Untyped file of an unlisted format loads as Bytes
  - WHEN `SimpleValue::deserialize_from_bytes(b, "" | "Bytes", fmt)` is called with a format no arm
    handles
  - THEN it returns `SimpleValue::Bytes` holding `b`
- **AC-2** A hand-placed CSV is usable as a resource
  - WHEN a CSV with no type identifier is stored and a manifest or `ns-rec/to_record` reads it
  - THEN the query succeeds, taking the format from the metadata
- **AC-3** Other identifiers still refuse an unlisted format
  - WHEN the identifier is a base type other than `Bytes` (e.g. `I32`) and the format is unlisted
  - THEN the read fails as today, so `CombinedValue` still asks the extension
- **AC-4** Textual scalars read back as their type
  - WHEN identifier `Bool`, `I32`, `I64` or `F64` is read under `txt`, `html`, `rs`, `py`, `css`
    or `js`
  - THEN the result is that scalar, parsed as core `Value` parses it; unparsable text is a
    conversion error, as in core
- **AC-5** `None`, `Text` and `toml` are unchanged
  - WHEN identifier `""`, `Text` or `None` is read under a textual format, or any base identifier
    under `toml`
  - THEN the result is `Text`, as today (core has no `none` read rule either)

Out of scope: core `Value`'s own `_` arm, which also refuses `csv` for `""`. A core-only
environment has no command that consumes a CSV, so the refusal costs nothing there today.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — bug fix in one function of `liquers-lib/src/value/simple.rs`;
  no `pub` item, type, format or command changes.
- **Leading issue:** None
- **Explanation:** Both expected behaviours are stated by their issues and match existing
  references: `RECORD_STREAMS.md` shows `-R/data/orders.csv/-/ns-rec/head-10` working on a stored
  CSV, and core `Value`'s textual rule is the parity target the `simple-value-serializer-parity`
  design adopted. The `None` question in the second issue is answered by keeping today's rule,
  which needs no change to either serializer.
- **Open questions:** None

### Design Dependencies

- `overlaps` `STORE-KEY-FORMAT-SEEDING` (weak: a seeded data format is what this reader receives).
- Follows `SIMPLE-VALUE-SERIALIZER-PARITY` (complete in code; Phase 5 awaiting approval).

## Phase 2: Architecture

### Solution

Two local edits to `SimpleValue::deserialize_from_bytes`:

1. The final `_ =>` format arm returns `Bytes` when `type_identifier` is `""` or `"Bytes"`, and
   keeps the "Unsupported format" error otherwise.
2. In the textual arm, split `"" | "None" | "Bool" | "I32" | "I64" | "F64" | "Text"` into
   `"" | "None" | "Text"` → `Text`, and one arm each for `Bool` (`SimpleValue::from_bool_str`, the
   `ValueInterface` default), `I32`, `I64`, `F64` (`str::parse`, mapped with
   `Error::conversion_error_with_message` exactly as `liquers-core/src/value.rs` ≈1028 does). The
   per-scalar parsing applies to `txt`, `html`, `rs`, `py`, `css` and `js` only, as in core; `toml`
   is split out of the arm and keeps today's rule (every base identifier reads as `Text`), since no
   acceptance criterion covers it and core has no `toml` rule to match.

Rejected: falling back to `Bytes` in `CombinedValue::deserialize_from_bytes`
(`liquers-lib/src/value/extended.rs` ≈602) — it would also swallow refusals for real identifiers
such as `Image`, hiding a broken stored image as bytes.

### Changes

- `liquers-lib/src/value/simple.rs` `SimpleValue::deserialize_from_bytes` only.
- No commands, no registry change, sync code only.
- Documents: no reference document states these read rules (searched 2026-10-08); the doc
  comment on `every_declared_format_round_trips` is the statement, and is updated with the test.

### Risks

A stored scalar whose text does not parse (identifier `I32`, content `abc`) now errors instead of
reading as `Text`; core already behaves this way. The fast track would then treat it as corrupted
and recompute. AC-2 depends on the records resolver turning a `Bytes` value back into bytes plus
metadata; if `manifest_over_hand_placed_csv_files_materializes` still fails after step 1, stop and
re-assess (the remaining cause is outside this function). Certainty: high for AC-1, AC-3-AC-5;
medium for AC-2.

## Phase 3: Examples and Tests

### Examples

The two Problem Examples. Secondary: `deserialize_from_bytes(b"\x89PNG…", "", "png")` is
`Bytes`; `deserialize_from_bytes(b"true", "Bool", "html")` is `Bool(true)`.

### Tests

In `liquers-lib/src/value/simple.rs` tests:

- `untyped_unlisted_format_reads_as_bytes` — `""` and `Bytes` with `csv`, `png`, `parquet`; AC-1
- `typed_unlisted_format_still_refuses` — `I32` with `csv`; AC-3
- `textual_scalars_read_back_as_their_type` — AC-4, including an unparsable `I32` error
- `toml_scalars_still_read_as_text` — `I32` under `toml` reads as `Text`; AC-5
- `every_declared_format_round_trips` (existing) — change the expectation so textual scalars read
  back as `value.clone()` except `None`, which stays `Text`; AC-4, AC-5

In `liquers-lib/tests/records_manifest_over_csv_files.rs`:

- `manifest_over_hand_placed_csv_files_materializes` (existing, `#[ignore]`d with this design's
  leading source) — remove the `#[ignore]`; AC-2

Command: `cargo test -p liquers-lib --lib value::simple` and `cargo test -p liquers-lib --test
records_manifest_over_csv_files`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `simple.rs` `_` format arm — `Bytes` for `""`/`Bytes` — `cargo test -p liquers-lib --test
  records_manifest_over_csv_files -- --include-ignored` (the hand-placed test must pass)
- [ ] 2. `simple.rs` textual arm — per-scalar parsing — `cargo test -p liquers-lib --lib value::simple`
- [ ] 3. Tests above; un-ignore the hand-placed test — `cargo test -p liquers-lib --lib --tests`
- [ ] 4. Both issues' resolutions and `status: closed`;
  `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-lib --lib --tests`; `cargo test -p liquers-lib --no-default-features --lib
--tests` (base value without extensions). Rollback: revert the commit.
