---
id: CORE-LEGACY-METADATA-ACCESSORS-RETURN-JSON
kind: issue
title: Metadata accessors return JSON-quoted strings for legacy metadata
status: accepted
priority: P2
complexity: S
area: [core/value]
design:
created: 2026-08-09
github:
---

## Problem

`Metadata::get_media_type` reads the `LegacyMetadata` branch with `serde_json::Value::to_string()`
(`liquers-core/src/metadata.rs:1574-1582`), which *serializes* the value rather than extracting it.
For a JSON string the result carries the quotes:

```rust
Metadata::from_json(r#"{"media_type":"text/plain"}"#)?.get_media_type()
// == "\"text/plain\""   — not "text/plain"
```

Observed while adapting a JavaScript store: a page supplying `{media_type: 'text/plain'}` produced
a media type of `"\"text/plain\""`, which matches nothing downstream.

The correct extraction is `as_str()` with a fallback, as the `MetadataRecord` branch of the same
method does. Neighbouring accessors on the same enum should be checked for the same mistake — the
`mimetype` key one line above has it, and other `LegacyMetadata` readers in the file may too. This
is a small, mechanical fix, but it needs a sweep rather than a single-line patch.

**A second, related trap made this hard to see.** `Metadata::from_json` falls back to
`LegacyMetadata` whenever the document does not deserialize as a `MetadataRecord`
(`liquers-core/src/metadata.rs:1524-1533`), and `MetadataRecord` does not accept a partial
document — `{"media_type":"text/plain"}` alone is not enough. So any caller passing partial
metadata silently lands on the legacy branch and then gets a quoted string out of it. The fallback
is deliberate and useful; the silence is what turns a small formatting bug into a mystery.

## Impact

Any metadata that arrives as a partial or foreign JSON document reads back with quoted values.
Media type drives content negotiation and deserialization dispatch, so the effect is a value that
compares unequal to every media type in the table while *looking* correct in a log line.

Reachable from every language integration that accepts metadata as a plain object, and from any
store holding metadata written by an older or external writer. `liquers-web`'s `JsStore` worked
around it by normalizing partial metadata itself before handing it to core; nothing else does.

## Expected behaviour

`get_media_type` — and every other `LegacyMetadata` accessor — extracts with `as_str()` and only
falls back to `to_string()` for values that genuinely are not strings.

Worth considering alongside: whether `MetadataRecord` should accept a partial document, so that
`{"media_type":"text/plain"}` deserializes into a record with defaults rather than dropping to the
legacy branch. That would remove the trap rather than just the symptom, but it is a wider change
and belongs to `CORE-METADATA-FORMAT-TYPE-CONSISTENCY` if it is taken up.

## Discovery

Found on 2026-08-09 by a `JsStore` test in `specs/design/liquers-web-store/` M5: a fixture store
returning `{media_type: 'text/plain'}` produced `"\"text/plain\""`. Verified by reading
`metadata.rs:1574`; not fixed here because the sweep touches `liquers-core` accessors used by every
consumer, which is outside that milestone.
