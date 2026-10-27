---
id: CORE-VALUE-JSON-READ-IGNORES-STRUCTURED-IDENTIFIERS
kind: issue
title: Core Value reads structured variants written as JSON back as a different variant
status: closed
priority: P2
complexity: S
area: [core/value]
design: core-value-json-identifier-dispatch
created: 2026-10-19
github:
---
## Problem

`liquers_core::value::Value` is `#[serde(untagged)]`, and the `json` arm of its
`DefaultValueSerializer::deserialize_from_bytes` (`liquers-core/src/value.rs`) consults the type
identifier only for `F64` and `Bytes`. Every other identifier falls through to an untagged
`serde_json::from_slice`, which picks the **first** variant whose shape matches:

| Written as `json` | Identifier | Reads back as |
|---|---|---|
| `Value::Metadata(_)` | `Metadata` | `Object` (a map matches `Object` before `Metadata`) |
| `Value::Recipe(_)`, `Value::CommandMetadata(_)` | `Recipe`, `CommandMetadata` | `Object` |
| `Value::AssetInfo(_)` | `AssetInfo` | `Array` |
| `Value::Query(_)`, `Value::Key(_)` | `Query`, `Key` | `Object` (both derive `Serialize` as structs) |
| `Value::I64(5)` | `I64` | `I32(5)` |

All of these types declare `json` in `type_descriptions()`, and for the first five it is their
default format, so the declared default does not round-trip. The existing test `vts7.2`
(`scalar_identifiers_round_trip_through_the_serializer`) covers scalars only and sidesteps the `I64`
case by using `-9_000_000_000`. Not verified against a build; derived from reading the arm and the
enum's variant order.

## Impact

A stored metadata record, recipe, command metadata, asset-info list, query or key is reloaded as a
plain JSON object or array, so a command expecting the typed value fails on reload (or, for `I64`,
silently narrows the type). `liquers-lib`'s `CombinedValue` inherits it for base identifiers, and
`liquers-py` will once its serializer delegates to core (`design/py-value-serializer/`).

## Expected behaviour

The `json` arm dispatches on every identifier core owns — deserialize the payload type directly
(`serde_json::from_slice::<MetadataRecord>` and so on, `I64` as `i64`) — and keeps the untagged
fallback only for an empty identifier and the genuinely shape-typed ones (`None`, `Bool`, `Text`,
`Array`, `Object`). Extend `vts7.2` to every variant and every declared format.

## Discovery

Found 2026-10-19 while designing `specs/design/py-value-serializer/`, which delegates liquers-py's
codec to core's and needed to know which round trips core actually provides.

## Update 2026-10-20 — confirmed at HEAD, designed

Still reproduces: the `"json"` arm of `Value::deserialize_from_bytes` (`liquers-core/src/value.rs`)
dispatches only `F64` and `Bytes`; the file's last change (`8fde04a`) did not touch it. One
correction to the table above: `Key` is a newtype over `Vec<ResourceName>`, so it serializes as an
array and reads back as `Array`, not `Object`. Design: `specs/design/core-value-json-identifier-dispatch/`
(every core identifier read as its own payload; the untagged parse kept for an empty or foreign
identifier). `liquers-lib`'s `SimpleValue` has the mirror defect for `I32`, filed separately as
`SIMPLE-VALUE-I32-JSON-READS-BACK-AS-I64`.

## Resolution

Fixed 2026-10-27 by `design/core-value-json-identifier-dispatch/`. Core `Value::deserialize_from_bytes` now reads every core identifier in `json` as its own
variant's payload (private helper `json_payload`), and refuses a payload of another shape with a
`SerializationError` naming the identifier; only an empty or foreign identifier is read by shape.
Tests in `liquers-core/src/value.rs`: `structured_identifiers_round_trip_as_json`,
`untyped_json_reads_by_shape`, `mismatched_json_payload_is_refused`,
`every_declared_json_identifier_round_trips`, and the extended `vts7.2` case.
