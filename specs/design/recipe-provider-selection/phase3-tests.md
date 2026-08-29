Written after Phase 2 was approved, before implementation.

# Phase 3 — Examples, reproduction and tests

## Reproduction

There is nothing to reproduce in the defect sense: the gap is an *absent* capability, not a wrong
result. The standing evidence is that no function in the workspace maps a name to a provider —
`grep -rn "RecipeProviderChoice\|recipe_provider_by_name"` over `HEAD` returns nothing, and every
selection is a literal `Box::new(DefaultRecipeProvider)`. The "reproduction" is therefore the first
acceptance test: a name in a YAML document producing the right provider.

All tests are runnable, colocated in `liquers-core/src/recipes.rs`'s existing `mod test`, and
assert externally meaningful behaviour rather than mirroring the match arms.

## Tests

| Test | Acceptance criterion | What it asserts |
|---|---|---|
| `recipe_provider_choice_yields_providers_that_differ_in_behaviour` | 2, 3 | The happy path. `Default.provider()` resolves a recipe from an `AsyncMemoryStore` holding `folder/recipes.yaml` (`has_recipes` true, `recipe` returns the stored title); `Trivial.boxed_provider()` reports no recipes for the same store, `recipe_opt` is `None` and `recipe` is an error. The two are distinguished by behaviour, not by type name, and both the `Arc` and the `Box` shape are exercised. Reuses the harness of `test_default_recipe_provider`, so it is gated `#[cfg(feature = "async_store")]` for the same reason. |
| `recipe_provider_choice_round_trips_through_yaml` | 1, 4 | Serialization emits the lowercase canonical name and deserialization returns the same variant, for both variants. |
| `recipe_provider_choice_round_trips_through_json` | 1, 4 | The same, in JSON, asserting the exact `"default"` / `"trivial"` string. |
| `recipe_provider_choice_accepts_trivial_aliases` | 4 | `trivial`, `none` and `no_recipes` all deserialize to `Trivial` in YAML, in JSON and through `FromStr`, and serialization normalizes back to `trivial`. This is the maintainer's addition to the issue's scope. |
| `recipe_provider_choice_parses_and_displays_names` | 1 | `FromStr` and `Display` agree with the wire form; an unknown name is an error naming the rejected input, in `FromStr` **and** in the deserializer — the error path, and the guarantee that a typo is not silently the default. |
| `recipe_provider_choice_defaults_to_default` | Q1 | `RecipeProviderChoice::default()` is `Default`. |
| Rustdoc example on the type | 4 | Runs as a doctest: `no_recipes` deserializes to `Trivial` and reports `as_str() == "trivial"`. |

## Edge and corner cases

- **Error path** — an unknown name, covered above in both the parser and the deserializer.
- **Exhaustiveness** (criterion 2) is not a runtime test: it is enforced by the compiler, because
  none of the four matches has a `_` arm. Adding a variant fails to build until every match is
  extended. Recorded here so the absence of a test is a decision rather than an omission.
- **Concurrency, persistence, memory** — not applicable. The type is field-free `Copy` data and the
  providers are stateless unit structs; Phase 2 marked these not applicable with that reason.
- **Bindings** — not applicable in this issue: nothing in `liquers-web` or `liquers-py` is wired up
  here, by the Phase 1 non-goals.
- **wasm32** — the type must compile there too, since `liquers-web` is its eventual consumer.
  Covered by `scripts/check-build-matrix.sh` rather than by a test.

## Review

*Against Phase 1:* each of the five acceptance criteria has a named test, except the "no existing
behaviour changes" criterion (5), which is covered by the unchanged existing suite rather than by a
new test — nothing existing is touched.

*Against Phase 2:* the signatures the tests call (`provider`, `boxed_provider`, `as_str`, `FromStr`,
`Display`) are exactly those Phase 2 specifies, and the risk table's "5 unit tests" became six plus a
doctest, the extra one being the alias behaviour the maintainer added at the gate.

*Against repository conventions:* colocated `#[cfg(test)] mod test`, `#[tokio::test]` for the async
case, `AsyncMemoryStore` + `SimpleEnvironment<Value>` for the harness, `eprintln!` never `println!`,
and `unwrap()` only inside tests.
