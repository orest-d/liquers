# Phase 2: Solution and Architecture - Markdown as a `Text` Data Format

## Chosen Solution

1. **Registry** — `liquers-core/src/value.rs`, `Value::type_descriptions`, the `Text` entry:

   ```rust
   TypeInfo::new("Text")
       .with_type_name("text")
       .with_defaults("txt", "txt", "text/plain", "text.txt")
       .with_data_formats(TEXTUAL)
       .with_data_formats(["md"])
       .with_data_formats(["b", "bin", "bytes"]),
   ```

   (`with_data_formats` appends, as the existing second call shows.)

2. **Core serializer** — `impl DefaultValueSerializer for Value`:
   - `as_bytes`: a new arm `"md" => match self { Value::Text(x) => Ok(x.as_bytes().to_vec()),
     _ => Err(…SerializationError "Serialization to md not supported by {type_name}") }`.
     A separate arm, not an addition to the `"txt" | …` pattern, because that arm also serializes
     scalars, `Query`, `Key` and `Bytes`, which must not be accepted as `md`.
   - `deserialize_from_bytes`: add `"md"` to the text arm's pattern. Its `type_identifier`
     dispatch already returns `Text` for `""` and `"Text"`; the scalar identifiers would parse
     (`"I32"` from `"7"`), but the write path never produces them for `md` because the registry
     refuses it — so widening the read pattern is safe and keeps one text-reading rule.
3. **Lib serializer** — `liquers-lib/src/value/simple.rs`, `impl DefaultValueSerializer for
   SimpleValue`:
   - `as_bytes`: new arm `"md" => match self { SimpleValue::Text { value } => Ok(value.as_bytes()
     .to_vec()), _ => Err(…) }`.
   - `deserialize_from_bytes`: add `"md"` to the `"txt" | "html" | "toml"` arm (it returns
     `Text` unconditionally).
   - Test helper in `every_declared_format_round_trips_or_is_recorded_as_unwritable`: the
     expected-value match becomes `"txt" | "html" | "md" => …`.

## Rejected Alternatives

- **Add `md` to `TEXTUAL`** — declares `md` for nine types, eight of which have no business
  being markdown, and grows `SimpleValue`'s `UNWRITABLE` record. Rejected.
- **Open "any `text/*` media type" rule** — the right long-term shape, but it changes how the
  registry answers `supports_data_format` for every type; owned by
  `DATA-FORMAT-CONSTANTS-AND-TOOLING`.
- **`markdown` alias** — no producer uses it; `media_type.rs` keys on `md`. Not added.

## Files and Symbols

`liquers-core/src/value.rs`: `Value::type_descriptions`, `DefaultValueSerializer::as_bytes`,
`::deserialize_from_bytes`, tests `scalar_identifiers_round_trip_through_the_serializer`,
`supports_data_format_agrees_with_the_registry`.
`liquers-lib/src/value/simple.rs`: `as_bytes`, `deserialize_from_bytes`, test
`every_declared_format_round_trips_or_is_recorded_as_unwritable`.
Not touched: `liquers-py/src/value.rs` (own `type_descriptions`, declares only `txt`/`html`;
its test `declared_formats_are_ones_the_serializer_accepts` stays valid).

## Errors, Ownership, Sync/Async

Errors use the existing `ErrorType::SerializationError` pattern in each file (core uses
`Error::new` there today; new code should prefer `Error::from_error(ErrorType::SerializationError,
…)` per CLAUDE.md). Synchronous, borrowed input, owned output — unchanged.

## API and Compatibility

No signature change. Registry gains one format on one type. Read-side behaviour for an untyped
`md` entry changes from error to `Text` (Phase 1). `ExtValue` and `CombinedValue` need no change:
`CombinedValue` dispatches base identifiers to `SimpleValue`.

## Questions

- **Implementation detail — separate `md` write arm vs widened pattern:** separate arm (above).
- **Implementation detail — whether a scalar identifier may be *read* from `md`:** allowed by the
  shared read arm, unreachable from the write path; no rule added.

## Risk Table

| Aspect | Assessment |
|---|---|
| Likely files | `liquers-core/src/value.rs`, `liquers-lib/src/value/simple.rs` |
| Affected workflows/crates | storing/reading `.md` assets; core and lib value layers |
| Existing-test impact | `simple.rs` round-trip test (expected-value arm must learn `md`); core round-trip/registry tests extended, not broken; `specs/command_registry.yaml` unaffected (no command change) |
| New validation | core + lib round trips for `Text:md`; asset-manager store/read test |
| Compatibility/data | untyped `.md` entries now read as `Text` (intended) |
| Concurrency/performance | none |
| Security | none — bytes are not interpreted |
| Recovery | revert the arms and the one `with_data_formats` call |
| Certainty | high |

## Review

Against Phase 1: criteria 1-2 → serializer arms; 3 → registry; 4 → integration test; 5 →
separate arm and Text-only declaration. Against code: both serializer bodies, the `TypeInfo`
builder, and the lib test's expected-value table were read at HEAD.
