# Phase 3: Examples and Tests - Markdown as a `Text` Data Format

## Use Case

A command returns `Value::from("# Notes\n- one\n")`; a recipe stores it as `notes/today.md`.
`-R/notes/today.md` then yields a `Text` whose metadata reads `data_format: md`,
`media_type: text/markdown`.

## Tests to Add or Change

### `liquers-core/src/value.rs` (`mod tests`)

| Test | Change |
|---|---|
| `scalar_identifiers_round_trip_through_the_serializer` | `Text` row formats become `["json","txt","html","md"]` |
| `supports_data_format_agrees_with_the_registry` | format loop gains `"md"` |
| new `markdown_is_text_only` | `Value::Text(..).supports_data_format("md")`; for every other sample value `!supports_data_format("md")` and `as_bytes("md")` is `Err` with `SerializationError`; `deserialize_from_bytes(b"# x", "", "md")? == Value::Text("# x")` |

### `liquers-lib/src/value/simple.rs` (`mod tests`)

| Test | Change |
|---|---|
| `every_declared_format_round_trips_or_is_recorded_as_unwritable` | expected-value arm `"txt" \| "html" \| "md"`; `UNWRITABLE` unchanged (asserted by the test itself) |
| new `simple_value_text_round_trips_as_markdown` | `SimpleValue::Text` → `as_bytes("md")` → `deserialize_from_bytes(.., "Text", "md")` equal; `SimpleValue::I32` `as_bytes("md")` is `Err` |

### `liquers-core/tests/` — new `text_markdown_storage.rs`

`#[tokio::test] async fn text_value_is_stored_and_read_as_markdown()`:
`SimpleEnvironment<Value>` with `AsyncMemoryStore::new(&Key::new())`; build a `MetadataRecord`
with `with_type_identifier("Text")` and `set_filename("today.md")`; call
`envref.get_asset_manager().set_state(&parse_key("notes/today.md")?, State::from_value_and_metadata(…))`
(the existing `set_state` used in `assets.rs` tests — follow the nearest existing `set_state`
test for the exact constructor); assert `store.get(&key)` returns the text bytes and metadata
`get_data_format() == "md"`; then `get_asset_manager().get(&key)` → `try_into_string()` equals
the original. Before the fix, `set_state` fails hard validation — this is the regression proof.

## Edge Cases

- Non-UTF-8 bytes under `md` read lossily, as `txt` does.
- `Text` default stays `txt`: `every_default_is_in_supported_formats` unchanged.

## Setup

Memory store, no commands. No Liquers query strings are evaluated except the key above, which is
a plain key, not a query.

## Coverage Review

Criteria 1-3 → unit tests both crates; 4 → integration test; 5 → `markdown_is_text_only`.
The Phase 2 risk on the lib round-trip test's expected-value table is covered by changing it.
