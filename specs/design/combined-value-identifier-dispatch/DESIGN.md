---
id: COMBINED-VALUE-IDENTIFIER-DISPATCH
kind: design
title: CombinedValue reads an identifier the extension declares through the extension
form: compact
workflow: liquers-project
status: in_review
phase: high-level
area: [lib/value]
issues: [COMBINED-VALUE-DISCRIMINATION]
created: 2026-10-09
---
# CombinedValue reads an identifier the extension declares through the extension

## Phase 1: High-Level Design

### Purpose

`CombinedValue::deserialize_from_bytes` (`liquers-lib/src/value/extended.rs`) asks the base value
first and the extension only when the base refuses. The base accepts some formats whatever the type
identifier, so an extended value written in those formats reads back as a *base* value of another
type. The declared type identifier must decide which half reads the bytes.

### Problem Example

```rust
use liquers_core::value::{DefaultValueSerializer, ValueInterface};
use liquers_lib::value::{ExtValueInterface, Value};

let value = Value::from_record_view(batch);          // a RecordBatch with 3 rows
let bytes = value.as_bytes("json")?;                 // a JSON array of 3 records
let back = Value::deserialize_from_bytes(&bytes, "RecordView", "json")?;
back.identifier()
```

**Today:** `"Array"`. `SimpleValue::deserialize_from_bytes` (`liquers-lib/src/value/simple.rs`)
reads every `json` identifier it does not name as plain JSON, so `CombinedValue` never asks
`ExtValue`. The same happens to a `RecordView` written as `html` (read as `Text`; `txt`, `html` and
`toml` read as `Text` for any identifier) and to a `RecordSource` manifest written as `yaml` or
`json` (read as `Object`). Every other `RecordView` format reads back correctly only because the
base happens to refuse it. A stored or cached record view written as JSON comes back from the asset
layer as an array, and a command expecting a `RecordView` fails on it.

**Expected:** `"RecordView"`, with 3 rows, because `ExtValue::type_descriptions()` declares
`RecordView`. An identifier the extension declares is the extension's to read, and only the
extension's.

### Scope and Acceptance Criteria

- **AC-1** Extended JSON reads as the extended type
  WHEN a `RecordView` is written as `json` and read back through `Value` with identifier `RecordView`
  THEN the result is a `RecordView` with the same number of rows, not an `Array`
- **AC-2** A manifest reads as a source
  WHEN a `RecordSource` with a manifest is written as `yaml` or `json` and read back through `Value`
  with identifier `RecordSource`
  THEN the result is a `RecordSource`, not an `Object`
- **AC-3** A write-only extended format refuses instead of changing type
  WHEN bytes are read through `Value` with an identifier the extension declares, in a format the
  extension cannot read (`RecordView` as `html`)
  THEN the read fails with the extension's error, and no `Text` value is produced
- **AC-4** Extension identifiers never become base values
  WHEN any identifier in `ExtValue::type_descriptions()` is read through `Value` in any format it
  declares
  THEN the result is either an extended value with that identifier or an error, never a base value
- **AC-5** Base and untyped reads are unchanged
  WHEN a base identifier (`Text`, `I32`, `Array`, `Bytes`, `Query`, …) or the empty identifier is
  read through `Value` in any format
  THEN the result is what it is at HEAD: the existing `SimpleValue` round-trip tests, the untyped
  `csv` → `Bytes` fallback and an extension's format inference for `""` all still pass

**Non-goals.** `liquers_core::value::Value` has no extension and nothing to discriminate; its `json`
identifier dispatch already exists. `liquers-py`'s `Value` is `PY-VALUE-SERIALIZER-IS-A-STUB`.
Making `html` readable for `RecordView`, adding formats, and migrating identifiers older stores
wrote are out of scope. Identifiers neither half declares keep today's base-first order; the asset
layer already degrades them before deserialization (`liquers-core/src/assets.rs`
`deserialize_stored_value`).

**Systems touched.** `liquers-lib` only: `CombinedValue::deserialize_from_bytes`
(`liquers-lib/src/value/extended.rs`), and the comments in `SimpleValue::deserialize_from_bytes`
that defer to this issue. No `liquers-core` change, no command, no query syntax, no stored format.
The issue's `core/value` area is not touched.

**Documentation intent.**
- Reference: `specs/reference/VALUE_TYPE_SYSTEM.md` §Reading gains the dispatch rule (which half
  reads which identifier, and what an unknown or empty identifier does). History row and
  `reviewed:` bump.
- Guide: `specs/guides/TYPE_SYSTEM_GUIDE.md` §Adding a value type step 4 says that the `TypeInfo`
  also routes reading: an extension identifier missing from `type_descriptions()` is offered to the
  base first.
- No new documents. `specs/README.md` links this design in place of the issue.

### Design Dependencies

Overlap triage (`references/overlap.md`), all weak; none is merged into this design:

- **requires (landed)** `simple-value-untyped-and-scalar-reads` (PR #91, merged): set the current
  fallback (an untyped file neither half reads is `Bytes`), which AC-5 preserves. Same function
  (T3), but in implementation and merged (E3).
- **overlaps** `simple-value-serializer-parity` (in documentation): deferred the `txt`/`html`/`toml`
  "any identifier" rule to this issue.
- **overlaps** `DATA-FORMAT-CONSTANTS-AND-TOOLING`: the format side of the same serializers.
- `value-type-system` and `text-value-markdown-format` (complete) supplied `type_descriptions()`
  and the evidence; frozen (E4).

### Open Questions

1. **Proposed resolution — where the dispatch lives.** (a) `CombinedValue` checks whether
   `E::type_descriptions()` declares the identifier and, if so, asks only the extension; otherwise
   it keeps today's order. (b) The issue's suggestion: every `SimpleValue` arm refuses identifiers
   it does not own. (c) Both. **Recommend (a):** one function, generic over every extension (it
   fixes `liquers-web`'s combinations too), and it leaves the base lenient for empty and legacy
   identifiers. (b) spreads the rule over every arm of one base type and still depends on each
   base remembering it.
2. **Proposed resolution — extension declared, extension fails.** Return the extension's error, or
   fall back to the base? **Recommend the error** (AC-3): falling back is the bug. Consequence: a
   `RecordView` cached as `html` now fails to read instead of becoming `Text`.
3. **Implementation detail — cost.** `type_descriptions()` builds a `Vec` per read. Reads go
   through a store, so this is small; Phase 2 decides whether a cheaper membership hook is worth a
   trait method (which would affect automatic-fix eligibility).
4. **Implementation detail.** Whether the `polars.DataFrame` special case in the fallback branch is
   kept; with (a) it is reached only when the `polars` feature is off.
