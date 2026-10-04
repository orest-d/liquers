# Phase 1: High-Level Design - `contains` (Listed) and `can_make` (Producible)

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** **Open design question - asset-manager and HTTP naming:** whether the
  asset manager mirrors the provider split (`AssetManager::contains` = stored or listed,
  new `AssetManager::can_make` = can be got), and which of the two the HTTP
  `GET …/key/contains/{key}` endpoint reports.
- **Explanation:** The provider-level shape was decided by the maintainer on 2026-10-04: two
  methods, `contains` for what a folder shows and `can_make` for what can be produced on demand,
  each overridable, with `can_make` defaulting to `recipe_opt`. Every current caller of the
  provider's `contains` is really asking "can this be made?", so the decision also moves those
  callers to `can_make`; the manager-level name is the remaining choice.
- **Open questions:**
  1. **Open design question - asset-manager and HTTP naming.** *Recommended:* mirror the split.
     `AssetManager::contains(key)` = `store.contains || provider.contains` (stored or listed),
     and a new `AssetManager::can_make(key)` = `store.contains || provider.can_make`. The
     internal guards that precede a `get` (axum `submit_key`, the websocket subscribe handlers)
     switch to `can_make`. The HTTP `key/contains` endpoint keeps reporting
     `AssetManager::contains`, and a `key/can_make` route is added next to it. Consequence:
     the same word means the same thing in all three layers. A manifest template chunk such
     as `daily_0042.csv` then answers `contains: false` over HTTP (it does today: `true`) and
     `can_make: true`.
     *Alternative:* keep `AssetManager::contains` meaning "can be got" (implemented with the
     provider's `can_make`), add nothing at the manager or HTTP layer. Consequence: no HTTP
     behaviour change, but `contains` means "listed" for providers and "producible" for the
     manager.

## Decision Record

| Question | Decision (2026-10-04, maintainer) |
|---|---|
| Trait shape | Two methods on `AsyncRecipeProvider`: `contains(key)` — the key is among what its folder **shows** (`assets_with_recipes`); `can_make(key)` — the key can be **produced**, including recipes for assets created on demand that are not listed |
| Overridable | Each provider may implement its own `contains` and `can_make` |
| Defaults | `can_make` defaults to `recipe_opt(key).is_some()`; `contains` keeps its current default (search of `assets_with_recipes(parent)`), which is exactly "what the folder shows" |

Derived from the decision: **`can_make` ⊇ `contains`.** A listed key can be produced. The
default `can_make` satisfies this for any provider whose `recipe_opt` answers for its own listed
keys, which is the case for every provider in the repository.

## Problem and Evidence

There are three `contains`, one per layer:

| Layer | Method | Answers today |
|---|---|---|
| Store | `AsyncStore::contains(key)` | is something **physically stored** at `key`? |
| Recipe provider | `AsyncRecipeProvider::contains(key)` (`liquers-core/src/recipes.rs` ≈550) | default: is `key` in the parent's listing? Overridden by `ManifestRecipeProvider` (`liquers-records/src/provider.rs` ≈378) and `RecipeProviderChain` (≈1048) to mean "can be produced" |
| Assets | `AssetManager::contains(key)` (`liquers-core/src/assets.rs` ≈5216, ≈7472) | store says yes, **or** the provider's `contains` says yes |

A provider has two questions to answer about a directory, and they can legitimately differ:
what to **show** (`assets_with_recipes`) and what it can **produce** (`recipe_opt`).
`ManifestRecipeProvider` can produce chunk `data_0042.csv` from a template but lists only a
manifest's explicit chunks, since a template's names are unbounded. The single provider method
`contains` is used for both meanings: its default answers "shown", two overrides answer
"producible", and every caller wants "producible". A new provider of the on-demand kind inherits
"shown" silently, so its producible keys look absent.

Callers of the provider's `contains` at HEAD, all of which precede producing or describing the
key:

| Caller | Purpose | Should ask |
|---|---|---|
| `AssetManager::contains` (trait default ≈5216, `DefaultAssetManager` ≈7472) | "does this key exist" | see open question |
| `AssetManager::get_asset_info` (≈5206) | describe an unevaluated recipe key | `can_make` |
| `liquers-axum/src/assets/common.rs` ≈253 (metadata of an unevaluated key) | describe | `can_make` |

Callers of `AssetManager::contains`, all guards before `manager.get(key)`:
`liquers-axum/src/assets/common.rs` ≈278 (`submit_key`), `websocket.rs` ≈261 and ≈276
(subscribe), and the HTTP `key/contains` endpoint (`key_handlers.rs` ≈147).

## Expected Behaviour and Acceptance Criteria

1. `AsyncRecipeProvider` has `contains` (default: listing search, unchanged) and `can_make`
   (default: `recipe_opt(key).is_some()`), both overridable.
2. For a provider that produces an unlisted key and overrides neither: `contains == false`,
   `can_make == true`.
3. `DefaultRecipeProvider` and `TrivialRecipeProvider`: `contains == can_make` for every key
   (their listing and their recipes coincide).
4. `ManifestRecipeProvider`: `contains` true only for explicit chunks; `can_make` true for
   explicit and template chunks (its current override moves from `contains` to `can_make`).
5. `RecipeProviderChain`: `contains` = any member's `contains`; `can_make` = any member's
   `can_make` (its current recipe-lookup body moves to `can_make`).
6. Every internal caller that guards producing or describing a key uses `can_make`, so a
   template chunk can be described and submitted.
7. Errors from `recipe_opt` propagate from `can_make`.
8. The recipes reference documents both methods and the `can_make ⊇ contains` rule.

## Affected Systems

`liquers-core` (trait, chain, managers), `liquers-records` (manifest provider), `liquers-axum`
(asset API helpers and, under the recommendation, one new route). No query, store or
serialization change.

## Scope and Non-Goals

Non-goals: changing `assets_with_recipes` or `has_recipes`; the store contract
(`STORE_SEMANTICS.md` §2 constrains stores, not providers).

## Compatibility

Source-compatible for out-of-tree providers: the new method has a default and `contains` keeps
its default. Behaviour: the manifest provider's `contains` narrows to explicit chunks, which only
matters to callers that keep using `contains`. Under the recommended answer, HTTP `key/contains`
narrows the same way, and clients wanting "can I get this" use `key/can_make`.

## Documentation Assessment

`reference/api/DOC_08_RECIPES_PLANS.md` (provider contract) — both methods and the subset rule.
`reference/ASSETS.md` — `AssetManager::contains`/`can_make`. Under the recommendation, the axum
assets API reference (the document `axum-assets-endpoints` produced) — the new route. History
rows and `reviewed:` bumps. Close the issue, recording the decision.

## Design Dependencies

- `overlaps` `record-streams` (complete): built the first on-demand provider; its `contains`
  override and test move to `can_make`.
- `overlaps` `manifest-folder-listing-invalidation`: same provider, independent methods.
- `overlaps` `axum-assets-endpoints`: owns the `key/contains` endpoint and the guards.

## Consolidated Findings

- The decision turns one overloaded method into two named questions; the old default is kept
  for the question it actually answers.
- Every existing caller of the provider's `contains` wants `can_make`. Switching them is part of
  the change, not optional: otherwise a template chunk stops being describable the moment the
  manifest provider's override moves to `can_make`.
- The chain's existing `contains` body (`provider_index_for`, a `recipe_opt` scan) already is
  `can_make`; it moves, and a new any-member `contains` replaces it.
- The manifest provider's `contains` can use the trait default (it lists explicit chunks via
  `assets_with_recipes`); no override needed.
- Tests that assert "addressable" through `contains` must move to `can_make`:
  `contains_answers_for_a_template_name_far_beyond_any_listing` (records) and possibly
  `contains_is_true_if_any_provider_has_the_recipe` (core chain), depending on whether its mock
  lists the keys.

## Review

The provider-level contract is decided and fully specified; the manager/HTTP naming is the one
remaining user-facing choice, with a recommended answer.
