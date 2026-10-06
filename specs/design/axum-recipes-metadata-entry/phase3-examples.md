# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| E1 | HTTP | `GET /liquer/api/recipes/metadata/data/report.txt` → `result.title`/`filename` equal the recipe's |
| E2 | HTTP | `GET /liquer/api/recipes/entry/data/report.txt?format=json` → `Content-Type: application/json`, body `{data: [...yaml bytes...], metadata: {...}}` |
| E3 | HTTP | Same with `Accept: application/cbor` and no `format` → CBOR |
| T1 | regression | Unknown recipe key → error status unchanged |

Tests in `liquers-axum/tests/recipes_api_routes.rs`, reusing its environment fixture (a recipe
provider with one recipe `report.txt` in `data/` with `title: "Report"`). Use the same `send`
helper pattern as the Assets API tests. Names: `recipe_metadata_returns_recipe_asset_info`,
`recipe_entry_honours_format_parameter`, `recipe_entry_honours_accept_header`.
