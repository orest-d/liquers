# Phase 3: Examples & Use-cases — recipe-provider-chain-config

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | A configured `liquers-lib` build keeps manifests | Absent, `default` and an explicit list | AC-1, AC-3, AC-4 |
| 2 | Example | Placing an integration's provider | A crate supplies an entry type | AC-5, AC-12 |
| 3 | Test | `liquers-core/src/recipes.rs` `recipes_config_tests` | serde forms, canonical output | AC-2, AC-4, AC-10, AC-12 |
| 4 | Test | `liquers-core/src/recipes.rs` `recipe_provider_chain_tests` (extended) | every-member rule | AC-1, AC-7, AC-11 |
| 5 | Test | `liquers-core/tests/recipe_provider_chain_config.rs` | builder resolution, errors, setters | AC-1, AC-2, AC-3, AC-5, AC-6, AC-8 |
| 6 | Test | `liquers-lib/tests/recipe_provider_chain_config.rs` | `LibKind` lists, `manifest`, setters | AC-3, AC-5, AC-6, AC-8, AC-9, AC-11 |
| 7 | Test | `liquers-records/src/provider.rs` (changed test) | no self-made clash check | AC-11 |
| 8 | Test | `liquers-py/tests/test_ENVIRON_environment.py` | Python accepts every value | AC-9 |

## Example 1: A configured `liquers-lib` build keeps manifests

A memory store with no prefix holds `data/recipes.yaml` (`hello/repeat-3/triple.txt`) and
`data/chunks.manifest.yaml` (`manifest: record-stream`, chunk query `greet/greet.csv`). The test
registers its own commands `hello`, `repeat` and `greet` on `builder.command_registry` before
`build()`, as `liquers-lib/src/environment.rs`'s existing manifest test does; fixtures are written
after `build()` through `envref.get_async_store()`.

```rust
for recipes in ["", "recipes: default\n", "recipes: [recipes_yaml, manifest]\n"] {
    let config = EnvironmentConfig::from_yaml(&format!("{STORE_YAML}{recipes}"))?;
    let mut builder = default_environment_builder::<Value, ()>()
        .with_config(config, Box::new(default_store_factory()));
    register_fixture_commands(&mut builder.command_registry)?;   // hello, repeat, greet
    let envref = builder.build()?;
    write_fixtures(&envref).await?;
    assert_eq!(evaluate(envref.clone(), "-R/data/triple.txt", None).await?.try_into_string()?,
               "hellohellohello");
    assert_eq!(evaluate(envref, "-R/data/greet.csv", None).await?.try_into_string()?, "hello");
}
```

Today the second assertion fails for the first two documents, and the third does not parse. `recipes: [recipes_yaml]` keeps the
first assertion and fails the second with `AssetNotFound`. `recipes: []` fails both.

## Example 2: Placing an integration's provider

A test factory declares `fixed` (argument `name`, a provider answering one key) and is chained
with `with_recipe_provider_factory`:

```yaml
recipes:
  - recipes_yaml
  - { type: fixed, name: extra.txt }
  - manifest
```

`-R/data/extra.txt` evaluates through `fixed`. `{type: manifest}` builds the same chain as
`manifest`. Without the factory, the build fails naming `fixed`, position 1, and the known types.

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| `recipes: bogus` | parse error naming `default`, `trivial` | `unknown_keyword_is_a_parse_error` |
| `recipes: [trivial]` | parse error | `trivial_is_not_a_list_entry` |
| `[manifest, manifest]` | configuration error at build | `duplicate_entry_is_refused` |
| `manifest` without `records` | configuration error naming the feature | `manifest_without_records_names_the_feature` (`--no-default-features`) |
| a key in `recipes.yaml` and a manifest | configuration error naming both, in both orders | `a_key_two_members_define_is_refused_in_any_order` |
| a malformed manifest beside a valid `recipes.yaml` key | the manifest's error, in both orders | `a_failing_member_fails_the_lookup_in_any_order` |
| `with_config(..).with_records_recipe_provider()` | one manifest provider; chunks evaluate | `records_recipe_provider_is_not_added_twice` |
| round trip | `KindDefault` omits the key; `[]` stays `[]`; a bare name stays bare | `recipes_config_round_trips_canonically` |

## Test Plan

- Unit, `liquers-core/src/recipes.rs` `recipes_config_tests`:
  - `absent_and_default_mean_kind_default` (AC-4)
  - `trivial_aliases_mean_an_empty_chain` (AC-2, AC-4)
  - `unknown_keyword_is_a_parse_error` (AC-4)
  - `trivial_is_not_a_list_entry` (AC-4)
  - `bare_name_equals_type_map` (AC-12)
  - `recipes_config_round_trips_canonically` (AC-10)
- Unit, `liquers-core/src/recipes.rs` `recipe_provider_chain_tests`:
  - `single_owner_resolves_in_any_order` (AC-7)
  - `a_key_two_members_define_is_refused_in_any_order` (AC-11)
  - `a_failing_member_fails_the_lookup_in_any_order` (AC-11)
  - `named_chain_reports_member_names` (AC-1)
  - `appended_provider_is_consulted_after_the_configured_one` (existing, rewritten; AC-11): two
    members answering `Some` for `a` is now an error naming `#0` and `#1`
  - `first_some_wins` (existing) renamed `single_some_is_used` (AC-7)
  - `push_names_members_by_position` (AC-1)
- Integration, `liquers-core/tests/recipe_provider_chain_config.rs`:
  - `configured_list_is_the_chain_in_order` (AC-1): the built chain's `names()` equal the listed
    entries in order, and a two-owner error lists them in list order
  - `empty_list_resolves_nothing` (AC-2)
  - `absent_recipes_is_recipes_yaml_in_core` (AC-3)
  - `integration_entry_type_is_placed_in_the_list` (AC-5, AC-12): Example 2
  - `unknown_entry_type_names_entry_position_and_known_types` (AC-6)
  - `duplicate_entry_is_refused` (AC-6)
  - `setters_build_the_same_chain_as_documents` (AC-8): `with_recipe_provider_choice`, `with_recipes`, and on
    `GenericEnvironment` `with_default_recipe_provider` / `with_trivial_recipe_provider`
  - `kind_defaults_resolve` (AC-3, AC-8): `resolve_recipes` of each core kind's `default_recipes()` is `Ok`
  - `try_with_recipes_on_an_environment` (AC-8): `GenericEnvironment::try_with_recipes` with a list
    and with an unknown type
  - `unbuildable_kind_default_fails_lookups_not_construction` (AC-8): a test kind whose
    `default_recipes()` names an unknown type; `with_default_recipe_provider()` does not panic and a
    lookup returns the resolution error
  - `appended_entry_duplicate_of_a_listed_entry_is_skipped` (AC-6, AC-8)
  - `appended_provider_after_a_configured_list_is_consulted` (AC-8)
- `liquers-core/src/environment_config.rs`: `config_roundtrips_and_applies` (existing) extended so
  `KindDefault` omits the key and `[]` survives (AC-10); its `RecipeProviderChoice` comparisons
  become `RecipesConfig`
- Integration, `liquers-lib/tests/recipe_provider_chain_config.rs`, each test gated on its own
  (`#[cfg(feature = "records")]`, or as noted), no file-level gate:
  - `configured_lib_build_keeps_manifests` (AC-3, AC-9): Example 1, all three documents
  - `explicit_list_without_manifest_drops_chunks` (AC-1, AC-5)
  - `default_entry_splices_the_kind_list` (AC-1): `[default]` behaves as absent
  - `default_recipe_provider_setter_keeps_manifests` (AC-8, AC-9): `DefaultEnvironment::new()` +
    `with_default_recipe_provider()`, the `liquers-web` construction path, so the web fix is tested
    natively without a wasm build
  - `records_recipe_provider_is_not_added_twice` (AC-8, AC-11): also `[manifest]` +
    `with_records_recipe_provider()`
  - `raw_manifest_appended_after_the_default_is_a_two_owner_error` (AC-11): the documented
    migration hazard
  - `a_key_two_members_define_is_refused_in_any_order` (AC-11): `recipes.yaml` and manifest
  - `lib_kind_defaults_resolve` (AC-3)
  - `manifest_without_records_names_the_feature` (AC-6): `#[cfg(not(feature = "records"))]`
- `liquers-records/src/provider.rs`: `a_chunk_name_the_sibling_recipes_yaml_also_defines_is_refused`
  becomes `the_manifest_provider_answers_a_chunk_a_sibling_recipes_yaml_also_defines`. The provider
  alone answers `Some`, the `recipes.yaml` message assertion is dropped, and the refusal is now a
  chain test (AC-11).
- Python, `liquers-py/tests/test_ENVIRON_environment.py`, using a `tmp_path` filesystem store as in
  `test_ENVIRON02_…` and registering commands as `conftest.py` does:
  - `test_ENVIRON_recipes_trivial_and_empty_list_resolve_nothing` (AC-9)
  - `test_ENVIRON_recipes_default_keeps_recipes_and_manifests` (AC-9)
  - `test_ENVIRON_recipes_explicit_list` (AC-9)
  - `test_ENVIRON_recipes_unknown_value_is_refused` (AC-9)

  The Rust unit test `a_non_default_recipe_provider_is_refused` is removed.
- Commands:
  - `cargo test -p liquers-core --lib recipes`
  - `cargo test -p liquers-core --test recipe_provider_chain_config`
  - `cargo test -p liquers-lib --test recipe_provider_chain_config`
  - `cargo test -p liquers-lib --no-default-features --test recipe_provider_chain_config`
  - `cargo test -p liquers-lib --no-default-features --features records --test recipe_provider_chain_config`
  - `bash scripts/check-build-matrix.sh`
  - `cargo test -p liquers-records --all-features --lib provider`
  - `bash scripts/test-liquers-py.sh -k ENVIRON`

`liquers-agent`: no test of its own; Phase 5 records that a workspace build gives it
`[recipes_yaml, manifest]` (features unify), which `cargo test -p liquers-agent` does not see.

## Learning Log

- `RecipeProviderChain::contains` asks each member's `contains`, not `recipe_opt`, contrary to its
  doc table. Corrected in Phase 4.
- `ManifestRecipeProvider` enforced the `recipes.yaml` clash by reading the file itself, so the
  rule only ever held for that one pairing.
- `liquers-web`'s environment is a `LibKind` `DefaultEnvironment`, so a native `liquers-lib` test
  of its setter covers its manifest loss.
