# Phase 1: High-Level Design - `Context::set_title` and `Context::set_description`

## Design Readiness

- **Readiness:** ready
- **Automatic fixing:** not-eligible — adds `pub` methods `Context::set_title` / `set_description`
  (rule 4)
- **Leading issue:** None
- **Explanation:** The precedence question was decided by the maintainer on 2026-10-04: **the
  recipe wins**. A recipe states the user's intention for the asset, while a command is generic
  and usually lacks the information for a good title or description. Under that rule the setters
  fill only what a resolved recipe left empty, which needs one flag per field on the asset and no
  other new mechanism.
- **Open questions:** None

## Decision Record

| Question | Decision (2026-10-04) | Consequence |
|---|---|---|
| Precedence against a recipe-declared title/description | **Recipe wins** (maintainer) | A non-empty title or description from the key's resolved recipe is final; the command's call leaves it unchanged |
| Same rule for both fields? | Yes, applied **per field** (derived) | A recipe with a title but no description lets the command supply the description, since the user stated no intention for it |
| What the command sees when its value is not used | `Ok(())`, nothing changed (derived) | A generic command need not know whether a recipe exists, so a recipe's precedence is not an error |
| Persistence and version | Persisted with a keyed asset's metadata; never part of `version` | `Version::from_content` hashes bytes only |

"Recipe" here means a recipe **resolved by the recipe provider for the asset's key**
(`recipes.yaml`, a manifest). Ad-hoc recipes built for plain queries and plain `-R/` resource
keys (`Recipe::from(&Query)` → title `"Ad-hoc query"`, `Recipe::from(&Key)` →
`"Ad-hoc key-query"`) are placeholders that never reach the metadata, and they never block a
command.

## Problem and Evidence

`Context` (`liquers-core/src/context.rs`) lets a command write `filename` (`set_filename` ≈877),
`expires` (≈1061), `payload_required` (≈1081) and the error, plus log/progress entries, but not
`title` or `description`. The fields are writable in principle: `MetadataRecord::with_title` /
`with_description` (`metadata.rs` ≈1454), the recipe adoption in
`AssetRef::evaluate_recipe_outcome` (`assets.rs` ≈3036-3050, the only place a recipe's title
enters an asset's metadata), and `AssetRef::set_description_fields` (≈3530, `pub(crate)`, used
by `AssetManager::set_description` for `Source` assets). A command cannot describe its result
("1 284 rows, 7 columns") even where no recipe has.

## Expected Behaviour and Acceptance Criteria

1. `Context::set_title(&self, title: &str) -> Result<(), Error>` and
   `Context::set_description(&self, description: &str) -> Result<(), Error>`, async, shaped like
   `set_filename`.
2. For a query asset or an unrecipe'd resource key, the set values appear in `get_metadata()` /
   `get_asset_info()` after evaluation.
3. For a keyed asset whose resolved recipe declares a non-empty title (resp. description), the
   recipe's value survives the command's call, and the call returns `Ok(())`.
4. For a keyed asset whose recipe leaves a field empty, the command's value is used for that
   field and persisted in the stored metadata; reloading (fast track) shows it.
5. `version` is identical with and without the call for the same bytes.
6. On `LegacyMetadata` the call returns `ErrorType::NotSupported` (as `set_description_fields`
   already does), not silently dropping. This applies only when the call would write.

## Affected Users and Systems

Command authors (Rust; Python/JS bindings later), listing UIs and `AssetInfo` readers.
Systems: `Context`, `AssetData`/`AssetRef` metadata.

## Scope and Non-Goals

Non-goals: exposing the setters in `liquers-py` (its `Context` binding has only `info`) or
`liquers-web` (`JS-COMMAND-CANNOT-ACCESS-CONTEXT`); changing `AssetManager::set_description`'s
`Source`-only rule; titles for unevaluated recipe keys (those come from the recipe already).

## Compatibility

Additive public methods. No existing command calls them, so nothing changes until one does.
Recipes keep exactly the authority they have today.

## Documentation Assessment

`reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` (lists the metadata-writing context
methods ≈423, ≈469): add both and the precedence rule. `guides/COMMAND_REGISTRATION_GUIDE.md`:
one example near context usage. Close the issue, recording the decision.

## Design Dependencies

- `overlaps` `axum-assets-endpoints`: its describe endpoint (`AssetManager::set_description`)
  is the user-side counterpart for `Source` assets; no change to it.

## Consolidated Findings

- Ordering alone would make the command win: the recipe's title is written into the live record
  in `evaluate_recipe_outcome` *before* the plan runs. "Recipe wins" therefore needs the asset to
  remember which fields the recipe supplied. Two private `bool`s on `AssetData`
  (`recipe_sets_title`, `recipe_sets_description`), set in that adoption block when the recipe's
  value is non-empty, are enough.
- Comparing the current metadata title with `lock.recipe.title` instead of flags is rejected:
  `lock.recipe` holds an ad-hoc placeholder title for every unrecipe'd asset, and an equality
  test cannot tell "recipe supplied it" from "command happened to write the same string".
- Reuse `AssetRef::set_description_fields` for the write, so the legacy-metadata refusal is
  shared with `AssetManager::set_description`. The flag check happens under the same write lock
  as the write, so no race with a concurrent recipe adoption.
- `AssetData::reset` (≈1563) must clear both flags with the metadata.
- Contexts derived for nested `apply` steps share the parent's `assetref` (`context.rs` ≈850), so
  a later step's call wins among commands, the same as `set_filename`. Nested `evaluate` creates
  a separate asset.
- The setters take no `key_mutation_lock` and check no status: the asset is mid-evaluation and
  owned by this run.

## Review

Feasible and small; the decision is recorded and every acceptance criterion has a test in
Phase 3.
