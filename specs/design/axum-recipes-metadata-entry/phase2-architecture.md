# Phase 2: Solution and Architecture

## Changes in `liquers-axum/src/recipes/handlers.rs`

```rust
/// The recipe's metadata as the Assets API derives it for a recipe key.
async fn recipe_metadata<E: Environment>(env: &EnvRef<E>, key: &Key) -> Result<Metadata, Error> {
    let info = env.get_recipe_provider().get_asset_info(key, env.clone()).await?;
    Ok(Metadata::MetadataRecord(MetadataRecord::from(info)))
}
```

- `get_metadata_handler`: keep the existence check (`recipe(&key, env)`), then
  `recipe_metadata` → `ApiResponse::ok(metadata_json(&m), "Recipe metadata retrieved")`.
- `get_entry_handler(State, Path, headers: HeaderMap, AxumQuery(params))`: after getting the
  recipe, `entry_response(recipe.to_string().as_bytes(), &metadata, &headers, &params)`.

Imports: `crate::assets::common::{entry_response, metadata_json}`. Make `metadata_json`
`pub(crate)` if it is not already (it is `pub(crate)`).

## Errors

`get_asset_info` errors are reported through the existing `error_to_detail` path with the message
"Failed to get recipe metadata".

## Alternatives

Return `AssetInfo` JSON directly. Rejected: the Assets API returns a `MetadataRecord` for a recipe
key, and the two APIs should agree.

## Known-issue preflight

None.

## Relevant commands

None.

## Documentation architecture

WEB_API_SPECIFICATION Recipes rows, with History and `reviewed:`.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `recipes/handlers.rs`; `tests/recipes_api_routes.rs`; the spec |
| Existing tests | Tests asserting `{}` or CBOR-only change expectation |
| Compatibility | Clients get richer metadata. CBOR remains the default entry format. |
| Recovery | Revert |
| Certainty | High |
