# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** The issue's first option maps directly onto an existing `AsyncStore` method,
  `get_asset_info`, whose `AssetInfo.media_type` is the effective string. Exposing it is additive
  and mirrors `getMetadata`.
- **Open questions:** None. The `effectiveMediaType(key)` helper is unnecessary once
  `getAssetInfo` exists. A page reads `.media_type`.

## Problem

`LiquersStore.getMetadata` (`liquers-web/src/store/wrapper.rs`) returns the raw `MetadataRecord`,
whose `media_type` holds only a declared override (metadata level model). A page sees
`media_type: null` for `input.csv` and has no call that returns `text/csv`.

## Expected behaviour and acceptance

1. `store.getAssetInfo("input.csv")` resolves to an object with `media_type: "text/csv"`,
   `filename`, `data_format`, `is_dir`, and the other `AssetInfo` fields in their serde shape.
2. A directory key resolves to `is_dir: true`.
3. An absent key rejects with a `LiquersError` of type `KeyNotFound`, as `getMetadata` does.
4. `getMetadata` is unchanged (the raw record, no derived values written).
5. The TypeScript declarations include `getAssetInfo(key: string): Promise<…>` (wasm-bindgen
   generates the method, and the stub usage file gains a line).

## Scope

`LiquersStore` wrapper. The JS delegate interface (`JsStore` in `typescript.rs`) is unchanged.

## Design Dependencies

- `axum-store-upload-metadata` — **overlaps** (same level-model reasoning on the server side).

## Documentation assessment

- `liquers-web/README.md`, the store API list: add `getAssetInfo`.
- Reference: the web/JS API section in `specs/reference/` if one lists `LiquersStore` methods
  (search `getMetadata` in `specs/reference`).

## Consolidated Findings

- Serialize `AssetInfo` with the same JS conversion used for metadata. `metadata_to_js_value`
  lives in `liquers-web/src/store/mod.rs`. Add an `asset_info_to_js_value` beside it using the
  same serializer (likely `serde_wasm_bindgen` with the crate's settings), so maps become plain
  objects consistently.
