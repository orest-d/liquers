Executed. Every command listed under "Validation" was run and passed.

# Phase 4 — Implementation plan and execution

## Plan

| # | File and symbols | Change | Depends on | Proof |
|---|---|---|---|---|
| 1 | `liquers-core/src/recipes.rs` — imports | Add `use std::sync::Arc;`. `fmt` was already imported for `Display`; `Serialize`/`Deserialize` come from the crate-level `#[macro_use] extern crate serde_derive`. | — | Compiles. |
| 2 | `liquers-core/src/recipes.rs` — `RecipeProviderChoice` | Add the enum after `RecipeList`, before `mod test`: two variants, `#[serde(rename_all = "lowercase")]`, `#[default]` on `Default`, `#[serde(alias = "none", alias = "no_recipes")]` on `Trivial`. | 1 | Round-trip and alias tests. |
| 3 | same — `provider`, `boxed_provider`, `as_str` | Three inherent methods, each an exhaustive match with no `_` arm. `provider`/`boxed_provider` are generic in `E: Environment`, so the type itself stays free of a type parameter. | 2 | Behaviour test. |
| 4 | same — `FromStr`, `Display` | `Err = liquers_core::error::Error` via `Error::general_error`; `Display` delegates to `as_str`. | 3 | Parse/display test. |
| 5 | same — `mod test` | The six tests of Phase 3. | 2–4 | They pass. |
| 6 | `specs/reference/api/DOC_08_RECIPES_PLANS.md` | New "Selecting a provider by name" subsection under the provider contract, plus `reviewed:` bump and a History row. | 5 | `docs_index.py --check`. |
| 7 | Issue, design and `specs/index.csv` | Close `RECIPE-PROVIDER-BY-NAME`, complete this design, regenerate the index. | 6 | `docs_index.py --check`. |

Rollback for steps 1–5 is a single-file revert: nothing in the workspace refers to the new type.

## What was implemented

`liquers-core/src/recipes.rs`, +255 lines, no deletions and no existing line changed. The public
surface is exactly the plan:

```rust
pub enum RecipeProviderChoice { Default, Trivial }   // Debug + Clone + Copy + PartialEq + Eq
                                                     // + Default + Serialize + Deserialize
impl RecipeProviderChoice {
    pub fn provider<E: Environment>(self) -> Arc<dyn AsyncRecipeProvider<E>>;
    pub fn boxed_provider<E: Environment>(self) -> Box<dyn AsyncRecipeProvider<E>>;
    pub fn as_str(self) -> &'static str;
}
impl std::str::FromStr for RecipeProviderChoice { type Err = Error; }
impl std::fmt::Display for RecipeProviderChoice;
```

Four matches, all exhaustive, no `_` arm.

## Deviations from Phase 2

1. **Aliases.** `Trivial` also deserializes from `none` and `no_recipes`, and `FromStr` accepts
   them. This is the maintainer's instruction at the gate, not a discovery during implementation.
   Serialization is unaffected: `Trivial` always emits `trivial`, so the aliases are an input
   convenience and do not widen the format commitment.
2. **Q1 settled as recommended** — `#[default]` on `Default`, with the rustdoc stating that this is
   the *document* default and deliberately differs from an environment constructor's unconfigured
   default. No change is made on the builder side; `environment-builder` remains free to choose.
3. **Q2 settled as recommended** — both `provider` and `boxed_provider` ship.
4. **One extra test** (six, not five) for the aliases, and a rustdoc doctest.

Nothing else in Phase 2's risk assessment changed: the reach is still one module in one crate, and
zero existing tests needed adjustment.

## Validation

| Command | Result |
|---|---|
| `cargo test -p liquers-core --lib` | 669 passed, 0 failed. |
| `cargo test -p liquers-core --lib recipes::` | 19 passed, including the six new. |
| `cargo test -p liquers-core --doc recipes` | 1 passed (the type's doctest). |
| `cargo check -p liquers-core --target wasm32-unknown-unknown` | Clean — `Arc<dyn …>` and `Box<dyn …>` both satisfy the `MaybeSend + MaybeSync` supertraits on wasm. |
| `bash scripts/check-build-matrix.sh` | All 14 configurations OK. |
| `cargo fmt -p liquers-core -- --check` | No deviation in the added code. Two pre-existing deviations elsewhere in `recipes.rs` (lines 922 and 959) were deliberately left as they were, to keep the diff scoped. |
| `python scripts/docs_index.py --check` | Clean. |

No test was weakened or skipped, and no unrelated failure was observed.
