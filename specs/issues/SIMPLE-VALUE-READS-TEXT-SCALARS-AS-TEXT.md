---
id: SIMPLE-VALUE-READS-TEXT-SCALARS-AS-TEXT
kind: issue
title: liquers-lib's base value reads a scalar written as text back as Text, where core reads its type
status: closed
priority: P3
complexity: S
area: [lib/value]
design: simple-value-untyped-and-scalar-reads
created: 2026-10-07
github:
---

## Problem

`SimpleValue` and core `Value` share their `TypeInfo`s and now write the same bytes for every
declared (type, format) pair (`simple-value-serializer-parity`). Reading back still differs for
the scalars under the textual formats (`txt`, `html`, `rs`, `py`, `css`, `js`):

- core `Value::deserialize_from_bytes` dispatches on the type identifier, so `"7"` with identifier
  `I32` reads back as `I32(7)`, `Bool` as `Bool`, and so on (`liquers-core/src/value.rs`, the
  `vts7.2` test `scalar_identifiers_round_trip_through_the_serializer`);
- `SimpleValue::deserialize_from_bytes` (`liquers-lib/src/value/simple.rs`) reads every base
  scalar identifier (`None`, `Bool`, `I32`, `I64`, `F64`) as `Text`. Only `Query`, `Key` and
  `Bytes` read back as themselves.

So in a `liquers-lib` environment an integer stored as `n.txt` comes back as `Text("7")`, which
core's own test calls silent corruption.

## Impact

Low today: commands mostly convert their input (`try_into_i64` parses text), so a `Text("7")`
usually works where an `I32` was expected. It matters wherever the type is inspected, such as a
metadata-driven UI or a type check on a reloaded asset.

## Expected behaviour

`SimpleValue` reads a scalar identifier under a textual format as that scalar, as core does. The
round-trip test `every_declared_format_round_trips` then expects the value itself for every
textual pair except `None` (core has no rule to read `none` back, which it records as a known
narrowness). Either keep `None` reading as `Text`, or add a `none` rule to both serializers.

## Discovery

Implementing `specs/design/simple-value-serializer-parity/` on 2026-10-07. The design asked for
write parity and kept the text reading rule (Phase 1: "`txt`-family into `Text`"), so this
divergence was left as found.

## Resolution (2026-10-08)

Fixed by design `simple-value-untyped-and-scalar-reads` (PR #91). Under `txt`, `html`, `rs`,
`py`, `css` and `js`, identifiers `Bool`, `I32`, `I64` and `F64` now read back as
that scalar, parsed as core `Value` parses them (`from_bool_str`, `str::parse`); unparsable text
is a conversion error, as in core. `""`, `None` and `Text` still read as `Text` — core has no
textual read rule for `None` either. `toml` keeps its old rule (every base scalar reads as `Text`),
since core has no `toml` reader to match.

Evidence: `textual_scalars_read_back_as_their_type`, `toml_scalars_still_read_as_text` and the
updated `every_declared_format_round_trips` (`liquers-lib/src/value/simple.rs`).
