---
id: DIRECTORY-LISTING-DEPENDENCY-IS-NEVER-REGISTERED-OR-CHECKED
kind: issue
title: Directory listing dependency is never registered or checked
status: draft
priority: P2
complexity: M
area: [core/assets]
design: 
created: 2026-09-17
github:
---
## Problem

The planner records a dependency on a directory listing. Nothing else in the system ever acts on
it.

`Step::GetAssetDirectory` inserts `PlanDependency::new(DependencyKey::from_dir_key(&resolved_key),
DependencyRelation::StateArgument)` (`liquers-core/src/plan.rs:2647`), producing a dependency key
of the form `-R-dir/{encoded_key}`. That form is a first-class part of the `DependencyKey`
vocabulary — it is documented at `liquers-core/src/metadata.rs:126`, it has `from_dir_key`,
`is_dir_key` and `dir_key()` accessors, and `dependency_key_classifies_and_extracts_dir_key`
tests all three.

The edge is then dropped, on two independent paths:

1. **It is never registered.** `AssetManager::register_plan_dependencies`
   (`liquers-core/src/assets.rs:4344`) adds an edge only when
   `dependency_manager().get_version(&plan_dep.key)` returns `Some`. Every call site of
   `DependencyManager::register_version` builds its key with `DependencyKey::from(&key)` — the
   `-R/` pure-key form (`assets.rs:1195, 4209, 5589, 5721, 6757, 6818`). No call site anywhere
   registers a version under an `-R-dir/` key, so `get_version` is always `None` for one and the
   `add_dependency` call is skipped without a diagnostic.
2. **It could not be resolved afterwards either.** The gap-filling path at `assets.rs:4209` reads
   `dep_key.key()`, which returns `None` for the `-R-dir/` form, and `continue`s. The fast-track
   staleness check `dependency_blocks_fast_track` (`assets.rs:1064`) calls
   `Key::try_from(dep_key)`, which rejects the same form (`metadata.rs:256`) and returns `false` —
   "inconclusive", i.e. does not block.

## Impact

An asset derived from a directory listing is not invalidated when the directory's contents change.
Adding, removing or renaming an entry changes no key the dependent depends on, because the
dependent depends on nothing: the one edge that would have carried that fact was silently
discarded when its plan was registered.

P2 rather than higher because directory-derived assets are uncommon today — the capability exists
ahead of a consumer. It stops being P2 the moment anything derives a value from a listing, and the
symptom then is a wrong cached answer rather than an error, which is the expensive kind.

The absence is also load-bearing for future work. An index over a corpus is the obvious use of a
directory dependency — it is exactly how "a new document was added" would reach a derived index —
and `design/store-and-asset-search/options-analysis.md` §E2 has to treat that route as unverified
because of this.

## Expected behaviour

Either the directory dependency is made real, or it is removed rather than left as a recorded
no-op.

Made real means: a directory listing has a version (a hash over its entries would do, but what it
must be derived from is a design question — entry names alone, or names plus each entry's
version); something registers that version through the dependency manager when a listing is
produced or when the store's contents under that key change; `register_plan_dependencies` then
finds it, and the cascade already in place does the rest. The `Key::try_from` rejections at
`assets.rs:1064` and `assets.rs:4209` need a deliberate answer for the dir form rather than
falling into their "not store-addressable" branches.

Questions the fix has to answer:

- What a directory listing's version is computed over, and whether it is recursive.
- Which component observes a directory change — the store, the asset manager, or an explicit audit.
  A store is not currently obliged to notice its own writes at the dependency level.
- Whether a dependency on a listing also implies a dependency on the entries, or strictly on the
  membership set.
- Whether `register_plan_dependencies` should warn on a plan dependency whose key resolves to no
  version, instead of skipping it silently. That silence is what made this invisible.

## Discovery

Found while writing the options analysis for `store-and-asset-search`, 2026-09-17, checking whether
an index could be maintained as a derived asset. Verified at HEAD by following every
`register_version` call site and both `Key::try_from(&DependencyKey)` uses; no test exercises a
`-R-dir/` dependency beyond `DependencyKey` classification.
