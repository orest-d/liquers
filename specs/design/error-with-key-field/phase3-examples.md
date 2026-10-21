# Phase 3: Examples & Use-cases — Structured Error Context

Written 2026-10-20 under Phase 1's recommended answers (readiness `needs-decision`). If Q2 or Q3 is
answered differently, the expected frames below change and this phase is revised; the test list
stays.

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Nested failure through a keyed recipe | the full frame list | AC-3, AC-4 |
| 2 | Example | A command failing inside a link's evaluation | flat fields keep the inner location | AC-5 |
| 3 | Test | `error::tests::with_key_sets_key_not_query` | builder fix | AC-1 |
| 4 | Test | `error::tests::keyed_constructors_set_key` | four constructors | AC-2 |
| 5 | Test | `error::tests::store_constructors_add_a_resource_frame` | store name in a frame | AC-6 |
| 6 | Test | `error::tests::context_collapses_duplicates_and_is_bounded` | dedup, 16-frame bound | AC-9 |
| 7 | Test | `error::tests::context_serde_round_trip` | JSON, omitted when empty | AC-8 |
| 8 | Test | `interpreter::tests::nested_link_failure_records_the_path` | Example 1 end to end | AC-3, AC-4 |
| 9 | Test | `interpreter::tests::outer_action_does_not_overwrite_inner_position` | Example 2 | AC-5 |
| 10 | Test | `assets::tests::query_asset_failure_names_its_query` | asset frame for a query asset | AC-3 |
| 11 | Test | `assets::tests::failed_keyed_asset_keeps_an_inner_key` | flat `key` set only when empty | AC-3 |
| 12 | Test | `tests/asset_cancellation.rs` (updated asserts) | root cause in `key` / `query` | AC-7 |
| 13 | Test | `metadata::tests::log_entry_from_key_only_error_has_a_query` | log projection | AC-10 |
| 14 | Test | `liquers-axum` `api_core::error::tests::error_detail_carries_context` | HTTP body | AC-8 |
| 15 | Test | `liquers-py` `tests/test_ERROR_errors.py::test_ERROR_context_survives_evaluation` | Python | AC-8 |
| 16 | Test | `liquers-web` `tests/error_ERROR.rs` `error_context_is_exposed` | JavaScript | AC-8 |

## Example 1: Nested failure through a keyed recipe

A memory store, `DefaultRecipeProvider` with `reports/recipes.yaml` defining `out.txt` as
`use_link-~X~-R/missing.txt~E` (validated with `liquers-validate --command use_link`), and `use_link`
registered as in `interpreter.rs`'s existing test. Nothing is stored at `missing.txt`.

```rust
let state = envref.get_asset_manager().get(&parse_key("reports/out.txt")?).await?.get().await?;
let e = state.value_error().expect("failed");
assert_eq!(e.key.as_deref(), Some("missing.txt"));             // where it was raised
let roles: Vec<_> = e.context.iter().map(|f| f.role).collect();
assert_eq!(roles, [Asset, Link, Asset]);
assert_eq!(e.context[0].key.as_deref(), Some("missing.txt"));
assert_eq!(e.context[1].query.as_deref(), Some("-R/missing.txt"));
assert!(!e.context[1].position.is_unknown());                   // inside out.txt's query
assert_eq!(e.context[2].key.as_deref(), Some("reports/out.txt"));
```

The same error is what `error_data` holds and what the store-backed `get_asset_info` reports.

## Example 2: A command failing inside a link's evaluation

Query `use_link-~X~fail_here~E` with `fail_here` a test command returning an error. The inner
asset (query `fail_here`) fails in action `fail_here` at its own position; `command_key` is set
there. The outer `use_link` link boundary adds a `Link` frame with the link's position; the outer
query asset adds `Asset{query: use_link-…}`. Flat `position` and `command_key` stay `fail_here`'s
— today they are overwritten by the link's position, which points into a different query.

## Edge and Error Cases (Corner Cases)

- Two boundaries producing the same frame (a delegated keyed asset re-failing) → one frame (AC-9).
- A pathological recursion depth → 16 frames, innermost kept (AC-9).
- An error with no frames → serialized without `context`; old `error_data` without the key
  deserializes (AC-8).
- A cancellation cascading through three assets → every level reports the root asset in `key`
  (keyed) — `asset_cancellation.rs` `cascade_names_the_root_cause_at_every_depth` (AC-7).
- Python → Rust → Python: a command raising `lq.ParseError(…, key="k/x")` keeps `key`, and its
  `context` gains the asset frame (AC-8; `test_ERROR02` comment updated).

## Test Plan

- Unit, `liquers-core/src/error.rs`: tests 3-7 (AC-1, AC-2, AC-6, AC-8, AC-9).
- Unit, `liquers-core/src/interpreter.rs`: tests 8-9 (AC-3, AC-4, AC-5), reusing the `use_link`
  registration of the existing link test.
- Unit, `liquers-core/src/assets.rs`: tests 10-11 (AC-3); the cancellation cascade test asserts `key`.
- Integration, `liquers-core/tests/asset_cancellation.rs`: the six `query` assertions become
  `key` for keyed roots and stay `query` for query roots (AC-7); `plan.rs`
  `walk_detects_recipe_cycle` asserts `key` (AC-2).
- Unit, `liquers-core/src/metadata.rs`: test 13 (AC-10).
- `liquers-axum`: test 14; `liquers-py`: test 15; `liquers-web`: test 16 (AC-8).
- Commands: `cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-axum`;
  `bash scripts/test-liquers-py.sh`; after `cargo clean`,
  `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`.

## Documentation and Learning Log

Documents: Phase 2 Documentation Architecture; this phase adds none.


- `with_key` writing `query` was not a typo in isolation: the cancellation contract was built on it,
  so the fix and the contract move together (step 2 of Phase 4).
- The run-failure path is the one place every asset error passes; no `?`-site audit is needed for
  AC-3.
