# Phase 2: Solution & Architecture — value-summary

## Overview

A small serializable `ValueSummary` in `liquers-core::metadata`, an optional `value_summary` field
on `MetadataRecord` and `AssetInfo`, and a `value_summary(&self) -> Option<ValueSummary>` hook
(default `None`) on `ValueInterface`, `ValueExtension` and `ForeignValue`.
`State::sync_metadata_with_value` writes it next to the type fields, so every state constructor
keeps it current. `liquers-lib` implements the hook for images, polars frames and record views; core
`Value` and `SimpleValue` for text, bytes, arrays and objects.

Rejected: computing on demand in clients (needs the value — the problem); storing it in the
application attribute map (Phase 1 question 3); a separate capability trait with its own registry
(the hook is per instance, and `State<V>` needs it generically, so a defaulted method is enough);
computing only at the asset layer's finish point (states built by commands would carry stale
summaries).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` (`metadata-application-attributes`) | draft | P2 | Adds another optional field to the same structs | no | no | Either order; trivial rebase |
| `CORE-VALUE-INTERFACE-CAPABILITY-SPLIT` | accepted | P2 | Same traits | no | no | Prefer after |
| `SHAREABLE-METADATA-AND-BYTES-ARE-DEEP-COPIED` | draft | P3 | The summary is cloned with the record | no | no | Small (a short map) |
| `STORE-HANDLER-FIELD-ACCESS-HARDCODED` | draft | P3 | Store handler specs cannot address the new field | no | no | None needed |

## Interfaces

```rust
// liquers-core/src/metadata.rs
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct ValueSummary {
    /// One line for display: "2 rows × 2 columns".
    pub summary: String,
    /// Structured facts; consumers ignore keys they do not know.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, serde_json::Value>,
}
impl ValueSummary {
    pub fn new(summary: impl Into<String>) -> Self;
    pub fn with(self, key: &str, value: impl Into<serde_json::Value>) -> Self;
}
// MetadataRecord and AssetInfo:
#[serde(default, skip_serializing_if = "Option::is_none")]
pub value_summary: Option<ValueSummary>,
impl MetadataRecord { pub fn with_value_summary(&mut self, s: Option<ValueSummary>) -> &mut Self }
impl Metadata { pub fn with_value_summary(&mut self, s: Option<ValueSummary>) -> &mut Self } // legacy: insert/remove key

// liquers-core/src/value.rs, liquers-lib/src/value/{extended,foreign}.rs
fn value_summary(&self) -> Option<ValueSummary> { None }
```

`CombinedValue` delegates by arm; `ExtValue::Foreign` delegates to the payload. Synchronous: sizes
and schemas in hand. `MetadataRecord::get_asset_info` copies the field.

Summaries (keys are the contract; text in `summary` is display only):

| Type | `summary` | `properties` |
|---|---|---|
| `Text` | "N bytes of text" | `bytes` |
| `Bytes` | "N bytes" | `bytes` |
| `Array` / `Object` | "N items" / "N keys" | `len` |
| `Image` | "W × H COLOR image" | `width`, `height`, `color_type` (`DynamicImage::color()` Debug name) |
| `polars.DataFrame` | "R rows × C columns" | `rows`, `columns`, `fields`, `dtypes` |
| `RecordView` | "R rows × C columns" | `rows` (`len()`), `columns`, `fields` (schema order) |
| scalars, `None`, `Query`, `Key`, metadata types, UI, egui, `RecordSource` | none | — |

## Integration Points

- `liquers-core/src/metadata.rs` — `ValueSummary`; fields; `with_value_summary` on record and enum;
  `get_asset_info`; `AssetInfo::new`.
- `liquers-core/src/state.rs` `sync_metadata_with_value` — `metadata.with_value_summary(value.value_summary())`.
- `liquers-core/src/value.rs` — hook and core `Value` impl.
- `liquers-lib/src/value/simple.rs`, `extended.rs` (`ValueExtension` hook, `CombinedValue`),
  `foreign.rs` (hook), `mod.rs` (`ExtValue`, gated arms).
- Struct literals of `AssetInfo` / `MetadataRecord` that list every field (`liquers-axum` tests and
  `value_description.rs`) gain the field (the compiler finds them).
- `liquers-py`, `liquers-web`: no change (metadata JSON carries it).

## Error Handling

The hook is infallible by contract: a type that cannot summarize returns `None`. A failing size
query (none at HEAD) would be mapped to `None`, never to an error that breaks state construction.

## Relevant Commands

None.

## Documentation Architecture

| Path | Change |
|---|---|
| `specs/reference/VALUE_TYPE_SYSTEM.md` | new §Value summary (model, table, cost rule, compatibility note); History, `reviewed:` |
| `specs/guides/TYPE_SYSTEM_GUIDE.md` | optional step "summarize it"; History, `reviewed:` |

Proposed `affects_docs`: those two.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `metadata.rs`, `state.rs`, `value.rs` (core); `simple.rs`, `extended.rs`, `foreign.rs`, `mod.rs` (lib); axum literal sites |
| Crates and workflows affected | every state construction; HTTP metadata and listings gain a key |
| Existing tests likely to change | tests comparing whole metadata JSON documents for image/frame/record/text states (e.g. axum endpoint snapshots) gain the key |
| New validation | Phase 3 tests; build matrix (gated arms) |
| Compatibility / data / concurrency / performance / security | Older builds read new records as `LegacyMetadata` (`deny_unknown_fields`); accepted. Cost per state construction: O(columns) for tables, O(1) otherwise. No data exposure beyond schema and size, which listings already imply |
| Recovery | Revert; records written meanwhile keep an extra key that the reverted build reads as legacy |
| Certainty and open questions | High; model and names open (Phase 1) |
