# Phase 4: Implementation Plan — Error type taxonomy

## Overview

Six PRs. **PR 1** (Steps 1-3) changes the vocabulary and must land whole: making `Error::new`
private breaks `liquers-lib` until its sites move. **PRs 2-5** (Steps 4-7) migrate one crate group
each, in any order, after PR 1. **PR 6** (Steps 8-9) adds the guard and closes the issues; it
needs PRs 2-5. Written under the recommended answers to Q1-Q7; a different answer to Q1, Q2 or Q4
changes only the tables in Step 1-2, not the order. Nothing must be fixed first.

## Progress

- [ ] Step 1: `liquers-core` vocabulary, constructors, private `Error::new`, persistence split
- [ ] Step 2: bindings and HTTP mapping
- [ ] Step 3: `liquers-lib` `Error::new` sites and the `SerializationError` split
- [ ] Step 4: `liquers-core` `General` sites
- [ ] Step 5: `liquers-records` `General` sites
- [ ] Step 6: `liquers-lib` `General` sites and the command registry
- [ ] Step 7: `liquers-template`, `-web`, `-py`, `-mcp`, `-agent`, `-store` `General` sites
- [ ] Step 8: the guard script, its test and the CI step
- [ ] Step 9: documents and issue closure

## Implementation Steps

### Step 1: core vocabulary
- Files / symbols: `liquers-core/src/error.rs` — `ErrorType` (8 variants added,
  `CacheNotSupported` removed, each with a doc comment stating its rule from Phase 2),
  `ErrorCategory` (5 added) and `ALL`, `ErrorType::ALL`, `categories()` per the Phase 2 table,
  the constructors of Phase 2 Interfaces, `fn new` private, `not_available(message)`,
  `cache_not_supported` removed; tests `error_type_all_lists_every_variant`,
  `categories_match_the_taxonomy_table`, `typed_constructors_set_their_type`.
  `liquers-core/src/assets.rs` `classify_persistence_error` (`FormatNotSupported` →
  `NonSerializable`, the rest `NotPersisted`) and its tests.
  `liquers-core/src/value.rs` the 5 `Error::new` arms → `format_not_supported` (refusals),
  `json` read failures stay `serialization_error`; `plan.rs` `ParameterValue::to_result` →
  `Error::argument_missing(error()).with_position(position)`. Every other core
  `SerializationError` producer re-read under the split (`metadata.rs`, `store_config.rs`,
  `environment_config.rs`, `store/*`).
- Depends on: none.
- Proof: `cargo test -p liquers-core --lib`; `cargo check -p liquers-core --all-features`.
- Rollback: revert PR 1.

### Step 2: bindings and HTTP
- Files / symbols: `liquers-axum/src/api_core/error.rs` `error_to_status_code`,
  `parse_error_type`, `keyabs15b` over `ErrorType::ALL`, new `taxonomy_status_codes`;
  `liquers-py/src/error.rs` pyclass `ErrorType`, both `From` impls, `category_name`, `Error::new`
  pymethod built with `from_error`; `liquers-py/python/liquers/_errors.py`
  `_CATEGORY_EXTRA_BASES["InvalidInput"] = (ValueError,)`; `liquers-py/src/value.rs`
  `as_bytes` split (Phase 2 Error Handling); `liquers-web/src/error.rs` both name tables and
  `error_category_name`; `liquers-web/src/default_value.rs` `is_serialization_refusal` →
  `FormatNotSupported`; `liquers-web/src/value.rs` refusal arm; `liquers-web/tests/objects_OBJECT.rs`
  list. Python tests of Phase 3; any `.d.ts` comment listing type names (`check-stubs.sh`).
- Depends on: Step 1.
- Proof: `cargo test -p liquers-axum --lib`; `bash scripts/test-liquers-py.sh`; after `cargo
  clean`, both web Node loops and `./liquers-web/scripts/check-stubs.sh --build`.
- Rollback: with Step 1.

### Step 3: `liquers-lib` construction sites
- Files / symbols: `liquers-lib/src/value/simple.rs` `as_bytes` (12 sites: the `json` arm's
  serde failures → `serialization_error`, the unknown-format fallback → `format_not_supported`);
  `image/serde.rs`, `polars/serde.rs` `serialization_error` helpers → `Error::serialization_error`
  or `format_not_supported` per call; every other `liquers-lib` `SerializationError` producer
  re-read under the split.
- Depends on: Step 1.
- Proof: `cargo test -p liquers-lib --lib --tests`; `bash scripts/check-build-matrix.sh` (PR 1 is
  a `match` and `cfg` change across crates).
- Rollback: with Step 1.

### Step 4: `liquers-core` `General` sites (152)
- Files / symbols: `assets.rs`, `metadata.rs`, `expiration.rs`, `commands.rs`, `recipes.rs`
  (`DefaultRecipeProvider::recipe`, `recipe_plan`, `TrivialRecipeProvider`,
  `RecipeProviderChain::no_recipe_error` → `recipe_not_found`), `plan.rs`, `context.rs`,
  `interpreter.rs`, `parse.rs`, `query.rs`, `type_system.rs`, `environment_builder.rs`,
  `environment_config.rs`, `store_config.rs`, `store/handlers.rs`, `store/test_support.rs`,
  `store_conformance/report.rs`, `validate/*`, `bin/liquers_validate.rs` — each site by the
  Phase 2 rule; messages unchanged. Tests `missing_recipe_is_recipe_not_found`,
  `failed_dependency_is_dependency_failed`; existing asserts on `General` updated.
- Depends on: Step 1.
- Proof: `cargo test -p liquers-core --lib --tests`; `python3 scripts/check_error_construction.py
  --crate liquers-core` once Step 8 exists, otherwise `grep -rn general_error liquers-core/src`
  showing only test modules.
- Rollback: revert PR 2; types fall back to `General`.

### Step 5: `liquers-records` `General` sites (207)
- Files / symbols: `formats/{ipc,csv,ndjson,markdown,shapes,parquet,mod}.rs`,
  `column.rs`, `views.rs`, `batch.rs`, `mutable.rs`, `buffer.rs`, `schema.rs`, `manifest.rs`,
  `sources.rs`, `provider.rs` (its "No recipe found" → `recipe_not_found`), `search/*`. Tests of
  Phase 3 (`truncated_file_is_serialization_error`, `declared_schema_contradiction_is_invalid_data`,
  `non_finite_float_is_serialization_error`, `row_out_of_range_is_out_of_range`,
  `select_missing_field_is_item_not_found`).
- Depends on: Step 1.
- Proof: `cargo test -p liquers-records --all-features --lib --tests`; `cargo test -p
  liquers-records --lib --tests`.
- Rollback: revert PR 3.

### Step 6: `liquers-lib` `General` sites (223) and the registry
- Files / symbols: `polars/*`, `image/*`, `ui/*`, `egui/widgets.rs`, `mem/*`, `records/*`,
  `search/*`, `template/commands.rs`, `commands.rs` (`Command not found` →
  `from_error(ErrorType::ActionNotRegistered, …)`), `store_config.rs`,
  `bin/export_command_registry.rs`; new `liquers-lib/tests/error_taxonomy.rs` (feature-gated per
  test). Then `cargo run -p liquers-lib --features cli --bin export-command-registry -- --format
  yaml -o specs/command_registry.yaml` and a CHANGELOG line (51 `impl_version` changes).
- Depends on: Step 1; Step 5 for records glue asserts.
- Proof: `cargo test -p liquers-lib --lib --tests` (includes `registry_export`); the per-feature
  `--no-default-features` runs of `CLAUDE.md`; `bash scripts/check-build-matrix.sh`.
- Rollback: revert PR 4 together with the registry file.

### Step 7: the remaining crates (54)
- Files / symbols: `liquers-template/src/{name,render}.rs`, `query/{ast,instantiate}.rs`,
  `mustache/render.rs`; `liquers-web/src/{environment,records}.rs`, `store/{builder,local_storage,js_store}.rs`;
  `liquers-py/src/{runtime,command,metadata,environment,context}.rs`; `liquers-mcp/src/{server,lib}.rs`;
  `liquers-agent/src/{lib,main}.rs`; `liquers-store/src/store_factory.rs` (the
  `opendal_store.rs` hit is inside a comment). Asserts updated.
- Depends on: Step 1.
- Proof: `cargo test -p liquers-template --lib --tests`; `cargo test -p liquers-mcp`; `cargo test
  -p liquers-agent` (Rust 1.96); `bash scripts/test-liquers-py.sh`; web loops after `cargo clean`.
- Rollback: revert PR 5.

### Step 8: the guard
- Files / symbols: new `scripts/check_error_construction.py` — scans `liquers-*/src/**/*.rs`
  (binaries included), ignoring each file from its first `#[cfg(test)]` module on; flags
  `general_error(` and a `from_error(` whose first argument is `ErrorType::General` (multi-line);
  exit 1 with `file:line`. New `scripts/test_check_error_construction.py` (fixtures of Phase 3).
  `.github/workflows/build-matrix.yml` gains a first step running both. `CLAUDE.md` Error Handling:
  the rule's short form; "never `Error::general_error` in workspace code"; `Error::new` is private.
- Depends on: Steps 4-7 (the check reports zero).
- Proof: `python3 scripts/test_check_error_construction.py`; `python3
  scripts/check_error_construction.py` exits 0.
- Rollback: remove the CI step; the script is inert.

### Step 9: documents and closure
- Files / symbols: `specs/reference/WEB_API_SPECIFICATION.md` §3.1; `PYTHON_INTEGRATION.md`;
  `PROJECT_OVERVIEW.md` error handling; `api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` persistence;
  `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`, `PYTHON_GUIDE.md` — each with a History row and
  `reviewed:` bump. Both source issues `closed` with a resolution note; this design's status;
  `python3 scripts/docs_index.py` then `--check`.
- Depends on: Step 8.
- Proof: `python3 scripts/docs_index.py --check`.
- Rollback: documentation only.

## Testing Plan

Per step as above. PR 1 additionally runs `bash scripts/check-build-matrix.sh` and the web and
Python loops, because it changes exhaustive matches in five crates. A final diff review per PR:
messages unchanged (only types), no `General` re-introduced, no unrelated edits, registry diff
limited to `impl_version` and the CHANGELOG.

## Rollback Plan

PRs 2-6 revert independently (their sites fall back to `General` or lose the guard). PR 1 reverts
only with every later PR, since they use its constructors.

## Documentation Updates

Listed in Step 9 and Step 8 (`CLAUDE.md`). `specs/command_registry.yaml` in Step 6.

## Phase 5 Entry Criteria

Not applicable (no `workflow: liquers-project`); the design ends at Phase 4. Implementation
starts only after Q1-Q5 are answered and the design is approved.
