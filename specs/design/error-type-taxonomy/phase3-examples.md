# Phase 3: Examples & Use-cases — Error type taxonomy

## Overview Table

| # | Scenario | Before | After | AC |
|---|---|---|---|---|
| 1 | `ns-img/erode-0` | `General`, 500 | `ParameterError`, 400 | AC-1 |
| 2 | `ns-rec/select_columns-nosuch` | `General`, 500 | `ItemNotFound`, 422 | AC-7 |
| 3 | Python catches by category | only `LiquersError` | `NotFoundError`, `InvalidInputError` / `ValueError` | AC-4, AC-7 |
| 4 | Value with no byte form persisted | `NonSerializable` | unchanged, via `FormatNotSupported` | AC-6 |
| 5 | NDJSON write of `NaN` while persisting | `General` → `NotPersisted` | `SerializationError` → `NotPersisted` | AC-6 |
| 6 | Recipe provider asked for an unknown key | `General` "No recipe found" | `RecipeNotFound`; evaluation still `AssetNotFound` | AC-8 |
| 7 | Dependency ends in error | `General`, 500 | `DependencyFailed`, 424 | AC-9 |
| 8 | Truncated Arrow file; file field contradicts declared schema | `General` | `SerializationError`; `InvalidData` | AC-11 |
| 9 | New `general_error` call in `liquers-lib/src` | compiles, ships | CI check fails | AC-2 |
| 10 | `Error::new` outside `error.rs` | compiles | compile error | AC-3 |

Queries validated with `liquers-validate` (all `Ok`): `-R/photo.png/-/ns-img/erode-0`,
`-R/data/x.csv/-/ns-rec/select_columns-nosuch`, `-R/data/x.csv/-/ns-pl/select_columns-nosuch`.

## Example 1: an argument outside its domain (primary)

```text
GET /liquer/q/-R/photo.png/-/ns-img/erode-0
→ 400
{"status": "ERROR", "error": {"type": "ParameterError",
  "message": "Erosion radius must be greater than 0", "query": "-R/photo.png/-/ns-img/erode-0"}}
```

JavaScript: `e.errorType === "parameter_error"`, `e.categories` is `["invalid_input"]`.

## Example 2: catching by category in Python

```python
import liquers as lq

env = lq.Environment()           # a store with data/x.csv (columns a, b)
try:
    env.evaluate("-R/data/x.csv/-/ns-rec/select_columns-nosuch")
except lq.NotFoundError as e:    # also lq.InvalidInputError, ValueError and LookupError
    assert type(e) is lq.ItemNotFoundError
    assert e.type.categories() == ["NotFound", "InvalidInput"]
```

`lq.ErrorType.all_categories()` is `["Store", "KeyedAsset", "NotFound", "InvalidInput",
"Unsupported", "Conflict", "Dependency", "Data"]`.

## Example 3: persistence of a value with no byte form, and of a failed encode

A Python object with the pickle codec off, evaluated under a key: `as_bytes` answers
`FormatNotSupported`; the asset is `Ready`, `persistence_status` is `NonSerializable`, and no error
is logged (as today). A records table holding `NaN` persisted as `ndjson`: `write_table` answers
`SerializationError` ("float value NaN is not finite"); the asset is `Ready`, `persistence_status`
is `NotPersisted`, and the error is in its log (today the same outcome is reached through
`General`; after the split it is reached by type, and a `SerializationError` from any encoder
behaves the same way).

## Edge and Error Cases

- **Multiple categories:** `ItemNotFound` is in `NotFound` and `InvalidInput`; `KeyNotAbsolute` in
  `Store` and `InvalidInput`. `is()` answers `true` for each; Python's MRO has both bases.
- **Recipe miss vs asset miss (AC-8):** `DefaultRecipeProvider::recipe(&key)` for a key with no
  recipe → `RecipeNotFound` with `key` set; `env.evaluate("-R/data/missing.txt")` → `AssetNotFound`
  as `key-without-recipe-answer` defined.
- **Removed `CacheNotSupported` (AC-10):** `lq.ErrorType.CacheNotSupported` raises
  `AttributeError`; axum's `parse_error_type("CacheNotSupported")` is an unknown type (500).
- **Mapping arms are not construction:** `ErrorType::General => …` in axum, py and web tables is
  allowed by the guard; `from_error(ErrorType::General, e)` spread over two lines is caught.
- **Test modules exempt:** a `general_error` inside `#[cfg(test)] mod tests` is not flagged.
- **`version: auto` commands:** a re-typed `erode` changes its `impl_version`; the regenerated
  registry differs only in `impl_version` lines and the CHANGELOG.
- **Binding with a runtime type:** `lq.LiquersError(lq.ErrorType.General, "x")` still works
  (`liquers-py` builds it with `from_error`).

## Test Plan

Stage 1 — `liquers-core` (`cargo test -p liquers-core --lib`):

- `error::tests::error_type_all_lists_every_variant` — 34 entries, no duplicates, exhaustive match
  over `ALL` (AC-4, AC-10).
- `error::tests::categories_match_the_taxonomy_table` — the Phase 2 table, row by row (AC-4).
- `error::tests::typed_constructors_set_their_type` — each new constructor; `recipe_not_found`
  and `dependency_failed` set `key` (AC-3, AC-8, AC-9).
- `value::tests::as_bytes_unsupported_format_is_format_not_supported` and
  `deserialize_unknown_identifier_is_format_not_supported` (AC-3, AC-6).
- `assets::tests::format_not_supported_persists_as_non_serializable` and
  `serialization_failure_persists_as_not_persisted` — a test value type answering each error
  (AC-6).
- AC-3 compile proof: `cargo check --workspace --exclude liquers-web` plus the web loop after the
  constructor is private.

Stage 1 — bindings and HTTP:

- `liquers-axum` `keyabs15b` iterates `ErrorType::ALL`; new `taxonomy_status_codes` asserts the
  Phase 2 status column (AC-5, AC-1, AC-7, AC-9) — `cargo test -p liquers-axum --lib`.
- `liquers-py/tests/test_ERROR_errors.py`: `test_ERROR_category_classes` expects the eight
  categories (AC-4); new `test_ERROR_invalid_input_is_value_error` (AC-1, AC-7) and
  `test_ERROR_cache_not_supported_is_gone` (AC-10); `test_VALUE_pickle_off_*` expects
  `FormatNotSupportedError` (AC-6) — `bash scripts/test-liquers-py.sh`.
- `liquers-web/tests/objects_OBJECT.rs`: the `error_types!` list from `ErrorType::ALL`'s order;
  new `object_error_categories_cover_the_taxonomy` (AC-4, AC-10) — web Node loop after
  `cargo clean`.

Stages 2-5 — one representative test per new type, plus the updated asserts of re-typed sites:

- `liquers-core`: `recipes::tests::missing_recipe_is_recipe_not_found` (default, trivial, chain;
  AC-8); `assets::tests::failed_dependency_is_dependency_failed` (AC-9); the existing
  `missing_key_without_recipe_is_asset_not_found` stays green (AC-8).
- `liquers-records` (`cargo test -p liquers-records --all-features --lib --tests`):
  `formats::ipc::tests::truncated_file_is_serialization_error`,
  `formats::ipc::tests::declared_schema_contradiction_is_invalid_data` (AC-11),
  `formats::ndjson::tests::non_finite_float_is_serialization_error` (AC-6),
  `views::tests::row_out_of_range_is_out_of_range`, `views::tests::select_missing_field_is_item_not_found` (AC-7).
- `liquers-lib` (`cargo test -p liquers-lib --lib --tests`): new `tests/error_taxonomy.rs` with
  `erode_zero_is_parameter_error` (AC-1, gated `image-support`),
  `polars_select_missing_column_is_item_not_found` (AC-7, gated `polars`),
  `rec_select_missing_field_is_item_not_found` (AC-7, gated `records`); `registry_export` passes
  after regeneration.
- `liquers-template`, `liquers-mcp`, `liquers-web`, `liquers-py`: existing asserts on `General`
  updated where a site is re-typed.

Stage 6 — guard (AC-2): `python3 scripts/test_check_error_construction.py` (fixtures: a flagged
`general_error`, a flagged two-line `from_error(ErrorType::General`, an allowed mapping arm, an
allowed call inside `#[cfg(test)] mod tests`), then
`python3 scripts/check_error_construction.py` on the workspace reports zero.

## Learning Log

- The guard is a script because clippy's `disallowed-methods` cannot see a `General` passed to
  `from_error`, and the build has no clippy run yet.
- Counting `Error::new` by text over-counts (`syn::Error::new`, `nom::error::Error::new` are other
  types), so the script does not check it: after Stage 1 the compiler does (AC-3).
