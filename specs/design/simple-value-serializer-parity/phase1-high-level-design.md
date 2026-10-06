# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** The issue records the preferred fix and the evidence. `SimpleValue` shares core
  `Value`'s `TypeInfo`s by design, and the two serializers are meant to mirror each other variant
  for variant. The test `every_declared_format_round_trips_or_is_recorded_as_unwritable` measures
  completion: `UNWRITABLE` must become empty.
- **Open questions:** None. Declaring narrower `TypeInfo`s (the alternative) would reintroduce the
  "second, drifting copy" the shared descriptions were adopted to avoid.

## Problem

`SimpleValue::type_descriptions()` (`liquers-lib/src/value/simple.rs`) returns core's descriptions,
but `SimpleValue::as_bytes` refuses 42 declared (type, format) pairs. These are the textual
`css`/`js`/`py`/`rs` formats, `b`/`bin`/`bytes` for `Text`/`Bytes`, and `txt`/`html` for
`Query`/`Key`. The write path checks `supported_data_formats`, so e.g. a `Bytes` value requested as
`data.bin` passes the check and then fails in `as_bytes`.

## Expected behaviour and acceptance

1. `SimpleValue::as_bytes` accepts exactly the (type, format) pairs core `Value::as_bytes` accepts,
   with the same bytes.
2. `SimpleValue::deserialize_from_bytes` reads them back (`txt`-family into `Text`, as core does;
   `bin` into `Bytes`). The round-trip test enforces this.
3. `UNWRITABLE` is empty and is then removed, along with its recording branch. The test then
   asserts plain round-tripping for every declared pair.

## Scope

`SimpleValue`'s serializer and deserializer. Core's are unchanged. `Error::new` uses in the
touched arms are replaced by typed constructors (`Error::general_error` or
`Error::from_error(ErrorType::SerializationError, …)`), per CLAUDE.md, but only in the lines this
change rewrites.

## Design Dependencies

- `type-info-write-only-formats` — **overlaps**. It defines how a deliberately write-only format is
  declared. This design leaves no write-only pair, so it does not depend on it.

## Documentation assessment

None beyond the issue. `VALUE_TYPE_SYSTEM.md` describes the shared descriptions, which are correct
after this change.

## Consolidated Findings

- Mirror core's arms literally (`"txt" | "html" | "rs" | "py" | "css" | "js"` and
  `"bytes" | "b" | "bin"`), mapping each `SimpleValue` variant to the core variant's behaviour.
  Core writes `Query`/`Key` as their encoded strings, and `SimpleValue` has the same variants
  (check names).
- Existing `_ =>` arms in `SimpleValue::as_bytes` and core are pre-existing default arms. The
  touched match should list variants explicitly where practical. If the enum is large, keep the
  final refusal arm only where core has one, so the two stay structurally identical. Note this
  as a deliberate exception in the code comment, since parity is the goal.
