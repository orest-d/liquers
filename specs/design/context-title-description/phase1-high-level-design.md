# Phase 1: High-Level Design - `Context::set_title` and `Context::set_description`

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** **Open design question - precedence over a recipe-declared title:** whether a
  command's `set_title` / `set_description` replaces the title and description a `recipes.yaml`
  entry declared for the key.
- **Explanation:** The mechanics are fully determined by existing code — the recipe's metadata is
  applied *before* the command runs, the live metadata record is merged (never replaced) when the
  value is installed, and `save_to_store` persists it — so a working design exists under the
  recommended answer; the precedence rule is a user-facing contract and stays visible.
- **Open questions:**
  1. **Open design question - precedence:** recommended answer: *the command wins* for both
     fields (last write wins, and the command runs last). Consequence: after evaluation, a
     listing shows the command's title instead of the recipe's; before evaluation, the recipe's.
     Alternative: a recipe-declared non-empty title is final and `set_title` is ignored (or
     errors) — keeps listings stable but makes the command's knowledge unusable exactly where a
     recipe exists. A split (recipe owns title, command owns description) is the coherent middle
     ground the issue mentions.
  2. **Proposed resolution - persistence and version:** the set fields persist with the keyed
     asset's stored metadata and do not affect `version` (content hash of the bytes). No
     alternative is defensible — `Version::from_content` never sees metadata.

## Problem and Evidence

`Context` (`liquers-core/src/context.rs`) lets a command write `filename` (`set_filename` ≈877),
`expires` (≈1061), `payload_required` (≈1081) and the error, plus log/progress entries, but not
`title` or `description`. The fields are writable in principle: `MetadataRecord::with_title` /
`with_description` (`metadata.rs` ≈1454), the recipe adoption in
`AssetRef::evaluate_recipe_outcome` (`assets.rs` ≈3044) and
`AssetRef::set_description_fields` (≈3530, `pub(crate)`, used by
`AssetManager::set_description` for `Source` assets). A command, the component that knows most
about its result, cannot describe it ("1 284 rows, 7 columns").

## Expected Behaviour and Acceptance Criteria

1. `Context::set_title(&self, title: &str) -> Result<(), Error>` and
   `Context::set_description(&self, description: &str) -> Result<(), Error>`, async, shaped like
   `set_filename`.
2. After evaluation, the asset's `get_metadata()` / `get_asset_info()` show the set values.
3. For a keyed, stored asset, the stored metadata carries them; reloading (fast track) shows them.
4. Under the recommended precedence, a command-set title replaces the recipe's title for that
   evaluation; a command that does not call the setter leaves the recipe's title untouched.
5. `version` is identical with and without the call for the same bytes.
6. On `LegacyMetadata` the call returns `ErrorType::NotSupported` (as `set_description_fields`
   already does) rather than silently dropping.

## Affected Users and Systems

Command authors (Rust; Python/JS bindings later), listing UIs and `AssetInfo` readers, the
agent-memory corpus (`AGENT-MEMORY-SERVICE`). Systems: `Context`, `AssetRef` metadata.

## Scope and Non-Goals

In scope: the two `Context` methods, tests, docs. Non-goals: exposing them in `liquers-py`
(`Context` binding only has `info`) or `liquers-web` (`JS-COMMAND-CANNOT-ACCESS-CONTEXT`);
changing `AssetManager::set_description`'s `Source`-only rule; titles for unevaluated recipe keys.

## Compatibility

Additive public methods. Under the recommendation, assets whose commands start calling the
setters change their post-evaluation titles; nothing changes for existing commands.

## Documentation Assessment

`reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` (lists the metadata-writing context
methods ≈423, ≈469) — add both and the precedence rule. `guides/COMMAND_REGISTRATION_GUIDE.md`
— one example near context usage. Close the issue.

## Design Dependencies

- `overlaps` `axum-assets-endpoints`: its describe endpoint (`AssetManager::set_description`)
  is the user-side counterpart for `Source` assets; no change to it.
- `overlaps` `record-streams` none; `dependency-audit-and-expiry-provenance` none.

## Consolidated Findings

- Ordering already guarantees "command wins" with no extra code: the recipe's title is written
  into the live record in `evaluate_recipe_outcome` before the plan runs, and `evaluate` merges
  type identifier/dependencies into the live record rather than replacing it. Choosing the
  alternative precedence would need *new* code (remember the recipe's title and re-apply or
  refuse). This asymmetry is why "command wins" is recommended.
- Reuse `AssetRef::set_description_fields` (already `pub(crate)`, same crate as `Context`) so
  the legacy-metadata refusal and field semantics are shared with `AssetManager::set_description`.
- Contexts derived for nested `apply` steps share the parent's `assetref` (`context.rs` ≈850), so
  a later step's call wins — the same rule as `set_filename`; nested `evaluate` creates a
  separate asset and does not touch the parent's title.
- The setter must not take `key_mutation_lock` or check status: the asset is mid-evaluation and
  owned by this run.
- Validation: an integration test with a registered command using `context`, keyed and unkeyed,
  plus a recipe-precedence test.

## Review

The design is feasible and small; the remaining question is a product decision with a
recommended answer that requires no extra mechanism.
