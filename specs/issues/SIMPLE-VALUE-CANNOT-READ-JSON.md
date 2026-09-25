---
id: SIMPLE-VALUE-CANNOT-READ-JSON
kind: issue
title: liquers-lib's base value writes JSON but cannot read it back
status: draft
priority: P2
complexity: S
area: [lib/value]
design:
created: 2026-09-25
github:
---
# `liquers-lib`'s base value writes JSON but cannot read it back

## Problem

`SimpleValue` — the base half of `liquers-lib`'s `Value = CombinedValue<SimpleValue, ExtValue>` —
serializes to `json` (`liquers-lib/src/value/simple.rs:563`), but its `deserialize_from_bytes`
accepts only `txt`, `html` and `toml` (`simple.rs:637-647`) and refuses everything else.
`CombinedValue::deserialize_from_bytes` then tries `ExtValue`, which matches type identifiers only
(`liquers-lib/src/value/mod.rs:328-346`), so a JSON document cannot be deserialized at all.

Two consequences:

- **A JSON value stored under a key does not load back.** It is written as `json`; on the next load
  the fast-track's deserialization fails and the asset takes the corrupted-data path
  (`METADATA-ONLY-ENTRY-RELOADS-AS-CORRUPTED` describes that path) and is recomputed — or fails, for
  a key with no recipe.
- **A hand-placed `.json` file is not a value.** `-R/data/export.json` cannot yield the JSON object
  or array a command such as `record-streams`' `ns-rec/from_json` expects.

`liquers-core`'s own `Value` does read JSON; the gap is in the `liquers-lib` value that every
`liquers-lib` environment uses.

## Expected behaviour

`SimpleValue::deserialize_from_bytes` accepts `json` (and `yaml`, which it also has the dependency
for) and builds the value through the existing `try_from_json_value` (`simple.rs:284`). A round-trip
test per format `SimpleValue` declares in its `TypeInfo` would have caught this and would catch the
next one — the same class `DATA-FORMAT-CONSTANTS-AND-TOOLING` records for `toml`, in the other
direction.

## Discovery

Found 2026-09-25 while designing `from_json` for `specs/design/record-streams/` Phase 2 ("JSON shapes
are conversions"), checking whether `-R/data/export.json/-/ns-rec/from_json` could receive a JSON
value. What exactly a type-less `.json` resolves to after the failed fast-track was not traced; the
deserializer's refusal was verified.
