# Phase 1: High-Level Design - Markdown as a `Text` Data Format

## Design Readiness

- **Readiness:** ready
- **Automatic fixing:** not-eligible — adds a data format to `Text`'s `TypeInfo` (rule 4) across
  three crates (rule 6)
- **Leading issue:** None
- **Explanation:** `md` is a plain-text encoding that both base-value serializers can handle with
  the code they already use for `txt`; the type registry gates the write path, so declaring it on
  `Text` and adding the serializer arms is the whole fix. The open-ended "any textual format"
  alternative belongs to the already-filed `DATA-FORMAT-CONSTANTS-AND-TOOLING`.
- **Open questions:** None

## Problem and Evidence

`Value::type_descriptions()` (`liquers-core/src/value.rs` ≈407) declares `Text` with
`TEXTUAL = ["txt","html","css","js","py","rs","json"]` plus `b`/`bin`/`bytes`. `md` is absent,
and neither `DefaultValueSerializer for Value` (core, `as_bytes` ≈944 /
`deserialize_from_bytes` ≈1010) nor `DefaultValueSerializer for SimpleValue`
(`liquers-lib/src/value/simple.rs`, `"txt" | "html"` arms ≈556 / ≈649) has an `md` arm.
`validate_metadata_hard` therefore refuses a `Text` value whose metadata declares
`data_format: "md"`, and a `Text` evaluated under a `.md` key cannot be serialized for storage —
although `media_type.rs` maps `md` → `text/markdown` and `icons.rs` has an icon for it.
`SimpleValue` reuses core's `TypeInfo` list, so the declaration fixes both registries at once.

## Expected Behaviour and Acceptance Criteria

1. `Value::Text(s).as_bytes("md") == s.as_bytes()`, and the same for `SimpleValue::Text`.
2. `deserialize_from_bytes(b, "Text", "md")` and `(b, "", "md")` return `Text` (lossy UTF-8, as
   for `txt`), in both serializers.
3. `Text`'s `TypeInfo` lists `md`; `supports_data_format("md")` is true for a `Text` and the
   registry agrees.
4. Storing a `Text` under `notes/today.md` through the asset manager writes the text bytes and
   reads back a `Text` with `data_format` `md` and media type `text/markdown`.
5. No other base type gains `md`; `Text`'s default format stays `txt`.

## Affected Users, Workflows and Systems

Anyone storing markdown documents — the agent-memory corpus and notes (`AGENT-MEMORY-SERVICE`),
`POST /api/assets/data` (`axum-assets-endpoints`) with a `.md` key. Systems: core value
serializer, lib `SimpleValue` serializer, the shared type registry. No query syntax change.

## Scope and Non-Goals

In scope: `md` for `Text` in both serializers and the registry. Non-goals: `yaml`, `toml`, `csv`
as plain text (each already has a *structured* reading in `liquers-lib` — `yaml` parses to a
`SimpleValue` tree, `csv` is a record/polars format — so declaring them as text would make the
same extension mean two things); a media-type-driven open rule (tracked by
`DATA-FORMAT-CONSTANTS-AND-TOOLING`); `liquers-py`'s stub serializer
(`PY-VALUE-SERIALIZER-IS-A-STUB`); other scalars as `md`.

## Compatibility, Migration, Data Format

Additive for writes. One read-side change: a stored entry with an **empty** type identifier and
data format `md` used to fail deserialization ("Unsupported format in from_bytes:md") and is now
read as `Text` — the same treatment `txt` and `html` already get. Entries stored as `Bytes` are
untouched (the `bytes` identifier short-circuits in `deserialize_stored_value`).

## Documentation Assessment

`specs/reference/VALUE_TYPE_SYSTEM.md` lists base-type formats if it enumerates them — review
and add `md` to `Text`; `guides/TYPE_SYSTEM_GUIDE.md` review only. Close the issue.

## Design Dependencies

- `overlaps` none-design issue `DATA-FORMAT-CONSTANTS-AND-TOOLING` (no design folder): the
  generic textual rule would subsume this; this design does not block or pre-empt it.
- `overlaps` issue `SIMPLE-VALUE-WRITES-FEWER-FORMATS-THAN-DECLARED` (no design): its recorded
  `UNWRITABLE` list in `simple.rs` must not grow; adding `md` to `SimpleValue`'s text arm keeps
  `Text:md` writable.

## Consolidated Findings

- `md` must be added to `Text` only, as a separate `.with_data_formats(["md"])`, **not** to the
  shared `TEXTUAL` constant: `TEXTUAL` also declares formats for `None`, `Bool`, `I32`, `I64`,
  `F64`, `Query`, `Key`, and adding it there would make `SimpleValue`'s
  `every_declared_format_round_trips_or_is_recorded_as_unwritable` fail for every scalar unless
  its writer arm also widened for all of them.
- Both serializers must change together: core `Value` for core-only environments, `SimpleValue`
  for every `liquers-lib` environment (`CombinedValue` delegates base values to it).
- The `simple.rs` round-trip test computes the expected value for text formats with
  `"txt" | "html"` — it must include `"md"` or it would compare against the unchanged value and
  still pass by accident only for `Text`.
- The read-side change for untyped `.md` entries is intended and matches `txt`.

## Review

Coherent, two files of code, testable with existing round-trip harnesses on both sides.
