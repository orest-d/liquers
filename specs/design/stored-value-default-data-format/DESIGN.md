---
id: STORED-VALUE-DEFAULT-DATA-FORMAT
kind: design
title: A stored value with no declared data format is read in its type's default format
form: compact
status: complete
readiness: ready
autofix: eligible
area: [core/assets]
issues: [STORED-VALUE-WITHOUT-DATA-FORMAT-READ-AS-BIN]
created: 2026-10-20
---
# A stored value with no declared data format is read in its type's default format

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written; implemented
2026-10-21 under automatic fixing (see Phase 5).

## Phase 1: High-Level Design

### Purpose

Make the read path agree with the write path about the format of a stored value whose metadata
declares none, so a value written at a key without an extension reads back as itself.

### Problem Example

Reproduced at HEAD (18732dd) by reading the code; no test covers it. In one environment:

```rust
// Metadata::new() declares no data_format; "counter" has no extension to seed one.
manager.set_state(&parse_key("counter")?, State::from_value_and_metadata(Value::I64(42), Arc::new(Metadata::new()))).await?;
```

`set_state` serializes with `State::as_bytes`, i.e. the value's own default format (`json`, bytes
`42`), and `AsyncStore::finalize_metadata` seeds no format from the extensionless key, so the record
is stored with `data_format: None` and `type_identifier: "I64"`. In a fresh environment over the
same store, `-R/counter` fast-tracks: `try_fast_track` passes `metadata.get_data_format()` — the
constant `bin` (`MetadataRecord::get_data_format`) — to `deserialize_stored_value`, and
`Value::deserialize_from_bytes(b"42", "I64", "bin")` answers `Value::Bytes(b"42")`. The query
returns two bytes instead of the integer 42, with no error. Worse than the issue feared ("fails as
corrupted"): it is a silently wrong value. A recipe result at an extensionless key takes the same
path (`AssetRef::serialize_to_binary` stores the state's metadata, format undeclared).

Should be: the read uses `TypeInfo::default_data_format` of the stored type (`json` for `I64`), and
`-R/counter` returns `I64(42)`.

### Scope and Acceptance Criteria

- **AC-1** Undeclared format reads in the type's default
  WHEN a stored value's metadata has no `data_format` and its `type_identifier` is registered
  THEN the fast track deserializes it with that type's `TypeInfo::default_data_format` and yields the original value
- **AC-2** Same rule on the recovery read
  WHEN `AssetManager::get_any_status` reads such a key from the store
  THEN it returns the original value, not bytes
- **AC-3** Declared formats and raw files unchanged
  WHEN the metadata declares a format, or the type identifier is empty or not registered
  THEN the format used is the declared one, or `bin` as today

Non-goals: filling `data_format` on the write side (`declared_data_format() == None` deliberately
means "nobody chose", `metadata.rs`); other `get_data_format` callers that have no type in hand
(media type, soft warnings); legacy metadata.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — bug fix restoring the level-1 seeding rule
  (`effective_data_format`), private function in `liquers-core/src/assets.rs`, tests in the same
  file, no public item changed. Priority raised to P1 (a wrong value without an error), so it is
  listed for the maintainer to authorize rather than started.
- **Leading issue:** None
- **Explanation:** the write side's rule is already explicit (`State::effective_data_format`); the
  read side only needs the same rule from the type registry. One private signature changes.
- **Open questions:** None

### Design Dependencies

- `key-without-recipe-answer` — **overlaps**: it extracts a degraded-type test out of
  `deserialize_stored_value`, the function this design changes. Independent; whichever lands second
  rebases a few lines. Kept separate by overlap exclusion E1 (this one is eligible, that one is not).

## Phase 2: Architecture

### Solution

`deserialize_stored_value` takes the *declared* format instead of an effective one and resolves the
effective format itself, after its two early exits (raw-bytes identifiers, degraded types), where
the registry is already consulted:

```rust
fn deserialize_stored_value<E: Environment>(
    binary: &[u8],
    type_identifier: &str,
    declared_data_format: Option<&str>,
    registry: &crate::type_system::TypeRegistry,
) -> Result<Option<E::Value>, Error>
```

Effective format: `declared`, else `registry.get(type_identifier)`'s `default_data_format`, else
`"bin"` (an empty identifier: a raw file with no metadata, exactly as today). Callers pass
`metadata.declared_data_format().as_deref()`: `AssetData::try_fast_track` and
`AssetManager::get_any_status` (the trait default in `assets.rs`).

Rejected: a new `Metadata::effective_data_format_for(&TypeRegistry)` (a public item for one private
caller); writing the format on store (changes what "undeclared" means and leaves existing stores
broken).

Known-issue preflight: `DEGRADED-STORED-VALUE-WITHOUT-RECIPE-REPORTS-NO-RECIPE` edits the same
function's neighbourhood (see Design Dependencies); `FREE-FUNCTION-EVALUATE-BAKES-A-BIN-DATA-FORMAT`
is closed. Nothing blocks. No command, query syntax or serialized format changes.

### Changes

`liquers-core/src/assets.rs`: `deserialize_stored_value` (signature above), its two callers.
Documents: `specs/reference/VALUE_TYPE_SYSTEM.md` §Reading gains one sentence (an undeclared format
reads as the type's default), History row, `reviewed:` bump.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A store written by an older build relied on `bin` for a registered non-bytes type | That read was already wrong (bytes for an `I64`); now correct | high |
| `TypeInfo` default and the value's own default disagree | `liquers-core/src/value.rs` already tests that every built-in `TypeInfo` supports its default; same source (`default_extension`) | high |
| An extension value type (polars, images) declares a default its reader cannot read | It is what its own writer used; covered by the type's existing round-trip tests | medium |

Existing tests: none assert the `bin` fallback for a typed value. Recovery: revert one commit.

## Phase 3: Examples and Tests

### Examples

The Problem Example, end to end: write `I64(42)` at `counter` with `Metadata::new()`, build a fresh
environment over the same `AsyncMemoryStore`, evaluate `-R/counter`, get `I64(42)`.

### Tests

Unit tests in `liquers-core/src/assets.rs` `mod tests`, beside
`test_asset_loading_binary_type_uses_raw_bytes`:

- `fast_track_reads_an_undeclared_format_as_the_type_default` — store `b"42"` with
  `type_identifier "I64"`, `status Source`, no `data_format`, key `counter`; `try_fast_track` is
  `true` and the value is `Value::I64(42)`: AC-1
- `fast_track_reads_undeclared_text_as_text` — `b"hello"`, `Text`, key `note`: `Value::Text`: AC-1
- `get_any_status_reads_an_undeclared_format_as_the_type_default` — the same record through
  `envref.get_asset_manager().get_any_status(&key)`: AC-2
- `stored_value_without_type_or_format_stays_bytes` — empty identifier, no format: `Value::Bytes`;
  and a declared `txt` on an `I64` still reads with `txt`: AC-3

Run: `cargo test -p liquers-core --lib assets::tests::` with the four names.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-core/src/assets.rs` `deserialize_stored_value` — take `declared_data_format: Option<&str>`, resolve the effective format after the degraded checks — `cargo check -p liquers-core`. Rollback: revert.
- [x] 2. Same file, `AssetData::try_fast_track` and `AssetManager::get_any_status` — pass `metadata.declared_data_format().as_deref()` — `cargo check -p liquers-core`. Depends on 1.
- [x] 3. Same file `mod tests` — the four tests of Phase 3 — `cargo test -p liquers-core --lib assets::tests::` (each name). Depends on 2.
- [x] 4. `VALUE_TYPE_SYSTEM.md` §Reading sentence, History row, `reviewed:`; close `STORED-VALUE-WITHOUT-DATA-FORMAT-READ-AS-BIN` with a resolution note — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests` (extension
value types read through the same function); diff review: one source file plus the two documents.

## Phase 5: Implementation Notes (2026-10-21)

Implemented as planned: `deserialize_stored_value` takes the declared format and falls back to the
type's `TypeInfo::default_data_format`, then to `bin`; both callers pass
`metadata.declared_data_format()`. The four Phase 3 tests are in `liquers-core/src/assets.rs`.

Deviation: the `I64` tests assert the number (`try_into_i64() == 42`), not the variant. Core
`Value` reads JSON `42` with identifier `I64` back as `I32`, because its `"json"` arm does not
dispatch on integer identifiers. That is the separate, already designed
`CORE-VALUE-JSON-READ-IGNORES-STRUCTURED-IDENTIFIERS` (`core-value-json-identifier-dispatch`), not
this fix; the assertion can tighten to `Value::I64(42)` once it lands.
