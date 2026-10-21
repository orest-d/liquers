---
id: SCHEMA-ARGUMENT-STRUCTURED-LINK
kind: design
title: A linked .yaml or .json file binds to a String argument as its JSON text
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [records, lib/value]
issues: [YAML-SCHEMA-FILE-CANNOT-BE-LINKED-AS-SCHEMA-TEXT]
created: 2026-10-21
---
# A linked .yaml or .json file binds to a String argument as its JSON text

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented.

## Phase 1: High-Level Design

### Purpose

Make the documented way of passing a record schema — a link to a schema file in the `schema`
argument of `ns-rec/to_record` and `ns-rec/from_json` — work for any `.yaml` or `.json` schema file,
not only for one whose YAML tags happen to stop it from being parsed into an object.

### Problem Example

A store holds `data/orders.schema.yaml` with plain YAML (no `!Tag`s):

```yaml
fields:
  - { name: order_id, data_type: Text, nullable: false }
  - { name: amount, data_type: Float, nullable: true }
```

`-R/data/orders.csv/-/ns-rec/to_record-csv-t-~X~-R/data/orders.schema.yaml~E` (validated: Ok) fails
at HEAD with `ConversionError: Can't convert 'Object' to string`. The resource is deserialized by
extension (`SimpleValue::deserialize_from_bytes`, `"yaml" | "yml"` arm,
`liquers-lib/src/value/simple.rs`) into `SimpleValue::Object`; `CommandArguments::get`
(`liquers-core/src/commands.rs`) binds a resolved link through `String::try_from(value)`, and
`TryFrom<SimpleValue> for String` accepts text, numbers and UTF-8 bytes only. A `.json` schema — what
`ns-rec/schema` prints and the reference recommends saving — fails the same way (`from_plain_json`
gives an `Object`). The only passing test (`schema_round_trips_through_the_schema_argument`,
`liquers-lib/tests/records_table_ops.rs`) works because its schema uses `!FullText`, which the
object reader refuses, so the file falls back to bytes. Should: the link binds as the object's JSON
text, which `parse_schema_argument` already accepts.

### Scope and Acceptance Criteria

- **AC-1** Plain YAML schema file links
  WHEN a `.yaml` schema file without tags is linked into `ns-rec/to_record`'s `schema`
  THEN the read is schema-aware with that schema (the declared types and `Id` role are in the result's `ns-rec/schema`)
- **AC-2** JSON schema file links
  WHEN the text `ns-rec/schema` printed is stored as `orders.schema.json` and linked into `schema` of `ns-rec/to_record` and `ns-rec/from_json`
  THEN both read with the declared schema
- **AC-3** Tagged YAML still works
  WHEN the schema file uses YAML tags (`!FullText`)
  THEN it binds as before (bytes to text) and the existing round-trip test passes unchanged
- **AC-4** Structured value to String is its JSON text
  WHEN any `Object` or `Array` value is converted with `String::try_from` (lib `SimpleValue`, and core `Value` under Q1)
  THEN the result is its compact JSON text, which `serde_json` parses back to an equal value
- **AC-5** Other refusals unchanged
  WHEN a value that is neither scalar, text, bytes, object nor array (`Metadata`, `Recipe`, an `ExtValue`) is bound to a `String` argument
  THEN the conversion error is unchanged

Non-goals: a `Value`-typed command argument; `ValueInterface::try_into_string` (the state
conversion, a different path); YAML tags inside an `Object` value (they never reach it).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — it changes the argument-binding contract of every `String`
  argument (rule 4), and under Q1 touches two crates (rule 6).
- **Leading issue:** Q1, whether core's `Value` changes too.
- **Explanation:** the fix is one match arm per value type and the schema parser already accepts
  JSON text; what remains is the scope of a binding rule that every command sees.
- **Open questions:**
  1. **Open design - scope:** change `TryFrom<SimpleValue> for String` in `liquers-lib` **and**
     `TryFrom<Value> for String` in `liquers-core` (recommended: the two were changed together for
     bytes and their comments match; a core-only environment linking a JSON file should behave the
     same), or `liquers-lib` only (one crate; core keeps refusing).
  2. **Proposed resolution - general, not schema-specific:** a structured value bound to any
     `String` argument becomes its JSON text (recommended: a new argument type would change
     `specs/command_registry.yaml` from `String` to `Any` and needs macro support for `Value`
     arguments). Consequence: a command whose `String` argument is linked to a JSON/YAML object
     now receives text instead of an error.

### Design Dependencies

- `parquet-read-declared-schema` (complete, E4) — **overlaps**: its tests use `.txt` schema files
  to avoid this bug; they may switch to `.yaml` afterwards. Not extended.
- `record-streams` (complete) — origin of the `schema: String` spelling (Step 5.5). Not extended.
- None open.

### Consolidated Findings

- Root cause is the binding conversion, not `to_record`: both `schema` arguments (`to_record`,
  `from_json`) are fixed at once with no command change and no registry change.
- `.json` schema files are affected too, which the issue did not say; AC-2 covers them.
- The behaviour change is error → success for every `String` argument linked to a structured
  value; nothing in the workspace relies on the error (checked: no test expects it).
- Documents: `RECORD_STREAMS.md` (schema argument paragraph), `RECORD_STREAM_GUIDE.md` §2.5,
  `VALUE_TYPE_SYSTEM.md` if it lists String conversions; the test comment "A .yaml file arrives as
  bytes" is corrected.

## Phase 2: Architecture

### Solution

Add two arms to the `String` conversions: `Object` and `Array` serialize to compact JSON through the
existing `try_into_json_value` (`ValueInterface`), then `serde_json::to_string`. `parse_schema_argument`
(`liquers-lib/src/records/commands.rs`) already parses text starting with `{` as JSON, so nothing in
the records glue changes except its doc comment. Rejected: a `SchemaArgument` newtype with
`TryFrom<Value>` (registry type becomes `Any`, macro maps unknown types to `Any`); deserializing YAML
resources as bytes (breaks every `.yaml` data file read as an object); YAML text output (the parser
would then take the YAML branch; JSON is canonical and is what `ns-rec/schema` prints).

Known-issue preflight: none blocks. No command namespace changes (`rec` only gains behaviour).

### Changes

- `liquers-lib/src/value/simple.rs` `impl TryFrom<SimpleValue> for String`: arms
  `SimpleValue::Object { .. } | SimpleValue::Array { .. }` → `value.try_into_json_value()` then
  `serde_json::to_string`, error mapped with `Error::from_error(ErrorType::SerializationError, e)`.
  The explicit arms keep the trailing `_` arm for the remaining refusals as today.
- `liquers-core/src/value.rs` `impl TryFrom<Value> for String` (Q1): same arms for `Value::Object`
  and `Value::Array`.
- `liquers-lib/src/records/commands.rs` `parse_schema_argument` doc: a linked `.yaml`/`.json` file
  arrives as JSON text.
- Synchronous, no new types, no registry change.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A command relied on the refusal to detect a wrong link | none found; AC-5 pins the remaining refusals | high |
| JSON number formatting differs from the file (e.g. `1.0` → `1.0`) | schema fields are names and enums; numbers irrelevant | high |
| Large object linked into a String argument now succeeds and is big | same as a large text file today | high |

Existing tests unchanged; the round-trip test's comment is corrected.

## Phase 3: Examples and Tests

### Examples

Problem Example query succeeds. Also
`-R/data/orders.json/-/ns-rec/from_json-records-~X~-R/data/orders.schema.json~E` (validated: Ok).

### Tests

In `liquers-lib/tests/records_table_ops.rs` (fixture gains `data/orders.plain.schema.yaml` and
`data/orders.schema.json`, the latter written from the tagged schema's `ns-rec/schema` output):

- `plain_yaml_schema_file_links_into_the_schema_argument` — AC-1
- `json_schema_file_links_into_to_record_and_from_json` — AC-2
- existing `schema_round_trips_through_the_schema_argument` unchanged — AC-3

Unit tests:

- `liquers-lib/src/value/simple.rs` `string_from_object_and_array_is_json_text` — AC-4
- `liquers-lib/src/value/simple.rs` `string_from_metadata_is_still_refused` — AC-5
- `liquers-core/src/value.rs` `string_from_object_is_json_text` (Q1) — AC-4

Run: `cargo test -p liquers-lib --lib --tests string_from` and
`cargo test -p liquers-lib --test records_table_ops`; `cargo test -p liquers-core --lib string_from`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-lib/src/value/simple.rs` `TryFrom<SimpleValue> for String` — Object/Array arms; unit tests — `cargo test -p liquers-lib --lib string_from`. Rollback: revert.
- [ ] 2. `liquers-core/src/value.rs` `TryFrom<Value> for String` — same (only if Q1 = both) — `cargo test -p liquers-core --lib string_from`.
- [ ] 3. `liquers-lib/tests/records_table_ops.rs` — fixture files, two tests, corrected comment; `parse_schema_argument` doc — `cargo test -p liquers-lib --test records_table_ops`. Depends on 1.
- [ ] 4. Docs: `RECORD_STREAMS.md` (schema argument: a linked `.yaml`/`.json` file binds as JSON text), `RECORD_STREAM_GUIDE.md` §2.5; History rows, `reviewed:`. Close `YAML-SCHEMA-FILE-CANNOT-BE-LINKED-AS-SCHEMA-TEXT` — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-lib --lib --tests`; `cargo test -p liquers-core --lib`;
`cargo test -p liquers-lib --no-default-features --features records --lib --tests` (the test file
is `records`-gated); diff review: no command signature or registry change.
