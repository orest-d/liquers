# Phase 4: Implementation Plan - `AsyncRecipeProvider::contains` Answers Addressability

0. **Decision gate.** Confirm the trait shape (Phase 1, question 1). Steps 1-5 implement the
   recommended default; for the required-method alternative, replace step 2 with the table in
   Phase 2 §Alternative and add `trivial_provider_contains_nothing`.
1. **Failing test.** Add `default_contains_answers_for_an_unlisted_producible_key` to
   `liquers-core/src/recipes.rs` `mod tests`; confirm it fails at HEAD.
   Proof: `cargo test -p liquers-core --lib default_contains_answers`.
2. **Default body.** Replace the default `contains` with the `recipe_opt` delegation and the
   Phase 2 doc comment; add the listing/addressability paragraph to the trait doc.
   Proof: step-1 test passes.
3. **Parity tests.** Add `default_provider_contains_matches_its_listing` and
   `contains_propagates_recipe_errors`. Proof: `cargo test -p liquers-core --lib recipes::`,
   `cargo test -p liquers-core --lib plan::` (counting provider),
   `cargo test -p liquers-records --all-features --lib --tests`.
4. **Docs and records.** `specs/reference/api/DOC_08_RECIPES_PLANS.md`: the contract paragraph
   (History row + `reviewed:`); review `reference/ASSETS.md`. Close
   `specs/issues/RECIPE-CONTAINS-DEFAULT-ASSUMES-ENUMERABILITY.md` with a resolution recording
   the chosen trait shape. Regenerate and check the docs index.
5. **Checks.** `cargo fmt`, `cargo test -p liquers-core --lib --tests`,
   `cargo test -p liquers-lib --lib --tests`, `cargo check -p liquers-core --target
   wasm32-unknown-unknown` (trait attributes). Diff review: no change to `assets_with_recipes`
   or `has_recipes`, overrides untouched, no `println!`.

## Final Review

Executable once the trait-shape decision is confirmed; the recommended path is a one-body change
with three tests. Rollback restores the old default body.
