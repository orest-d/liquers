# Phase 2: Solution & Architecture - keyed-delegation-hand-off

## Overview

One rule, added in one place: **an asset never records a dependency edge onto an asset that shares
its own dependency-graph node.** When that holds, `record_dependency_on_asset` returns `Ok(())`
without touching metadata or the `DependencyManager`, and the caller proceeds to
`wait_for_dependency`.

Node identity is `AssetRef::bound_key_candidate()` — the key each asset was *constructed* with —
falling back to the recipe-derived `DependencyKey`. `AssetData::recipe` is mutable (provider
resolution replaces it mid-evaluation), so it cannot be the primary identity; this matches
`Context::schedule_dependency_asset`, which classifies a keyed dependent by `owner_key()` for the
same reason. *(Revised after PR 32 review — the first implementation compared recipe keys only.)*

This is option (1) of the issue's *Expected behaviour*. Option (2) — delete the branch and always
self-evaluate — is rejected: it loses value sharing in the stale-owner case, and it makes two
assets compute the same key concurrently, which the branch exists to prevent.

## Known-Issue Preflight

| Issue | Status / priority | Relevance | Blocking? |
|---|---|---|---|
| `ASSET-KEYED-DELEGATION-ALWAYS-CYCLES` | accepted, P0 | The issue being fixed. | — |
| `CONTEXT-APPLY-BARE-KEY-ILL-DEFINED` | **rejected**, P0 | Names the spurious cycle as one outcome of a bare-key `apply`. Its rejection ("`apply` is not required to consume or transform its input state") makes the ad-hoc-asset route a *supported* path, not a misuse — so the P0 rating on this issue stands. Its text needs a small correction after the fix. | No |
| `VOLATILE-KEYED-RECIPE-SELF-DELEGATION` | closed, P1 | Reported the same mechanism from the volatile angle; closed by `keyed-recipe-ownership`, which made the branch unreachable for volatile keys. Its closing note points here. | No |
| `INLINE-PATH-LACKS-EXECUTE-ONCE` | accepted, P2 | Under `ImmediateAssetManager` the hand-off runs through the *trait-default* `wait_for_dependency`, i.e. `dependency.get()`, which has no `RunClaim`. Two inline callers handing off to the same owner rely on `is_finished` rather than an atomic claim. Pre-existing and unchanged by this fix; the fix reaches that path rather than creating it. | No |
| `QUEUED-MANAGER-EVICTION-RACE` | accepted, P2 | The stale-owner scenario in the issue's *Impact* is one way an eviction race produces a non-owner holding a keyed recipe. Independent; this fix makes that state recover instead of failing. | No |

No blocker. No priority change recommended.

## Function Signatures

### Change 1 — same-node guard in `record_dependency_on_asset`

`liquers-core/src/assets.rs:1107`. Signature unchanged:

```rust
pub(crate) async fn record_dependency_on_asset(
    &self,
    dependency: &AssetRef<E>,
) -> Result<(), Error>
```

Current structure, in order: derive `dep_key`; read the dependency's version; upsert a
`DependencyRecord` into **this asset's metadata**; then — only if `self.recipe.key()` is `Some` —
derive `current_dep_key`, run `would_create_cycle`, and `add_dependency`.

New structure: derive `dep_key`, then derive `current_dep_key` **before** the metadata write, and
return early when the two are equal. Concretely:

```rust
// Immutable identity first: this is the authoritative same-node test.
let bound_node = self.bound_key_candidate().await;
if bound_node.is_some() && bound_node == dependency.bound_key_candidate().await {
    return Ok(());          // same graph node — hand-off, not a dependency
}
let dep_key = /* unchanged */;
let current_dep_key = {
    let lock = self.data.read().await;
    lock.recipe.key().ok().flatten().map(|k| DependencyKey::from(&k))
};
// Recipe-derived fallback, for assets with no bound key candidate.
if current_dep_key.as_ref() == Some(&dep_key) {
    return Ok(());
}
/* version read, metadata upsert, cycle check, add_dependency — unchanged,
   reusing `current_dep_key` instead of re-reading the lock */
```

`bound_key_candidate()` is `query().and_then(|q| q.key())`, and `AssetData::query` is set once at
construction and never replaced. It is `Some` for both ends of every delegation, because both
assets are built from a bare-key recipe (which `is_pure_query()` accepts). The recipe-derived test
is kept as a fallback for assets built from a recipe carrying no pure query, which have no bound
key candidate at all.

Four consequences, all intended:

1. **No metadata record.** A `DependencyRecord` naming the asset's own key is not merely useless:
   `DependencyManager::track_asset` feeds persisted records back through `load_from_records`, which
   would install a self-edge in the graph on every reload.
2. **No graph edge**, so `would_create_cycle` is never consulted for the self case and
   `Error::dependency_cycle` is not returned. The cycle check itself is *correct* and is left alone
   — `would_create_cycle` returning `true` for `dependent == dependency` is the right answer to the
   question it was asked; the fix is to stop asking it.
3. **Identity survives provider resolution.** `evaluate_recipe` overwrites the owner's
   `AssetData::recipe` with the recipe the provider resolved. When that recipe is a pure-key
   *alias* `L`, the owner's `recipe.key()` becomes `L` while the delegate still holds `K`, so a
   recipe-only test misses the hand-off. It would then record `K -> L` carrying the owner's
   metadata version — a version for `K` — and `add_dependency` compares that against `L`'s
   registered version and expires `K` when they differ. Comparing construction-time keys avoids
   this entirely.
4. **The guard is general, not delegation-specific.** It is placed in
   `record_dependency_on_asset` rather than at the call site because the invariant belongs to the
   dependency graph, not to delegation. `record_dependency_on_asset` has exactly one production
   caller (the delegation branch) and one test caller, so the blast radius is the same either way;
   the general placement is the one that stays true if a second caller appears. It does **not**
   weaken genuine cycle detection: a runtime self-dependency (a command calling `Context::evaluate`
   on its own key) never reaches this function — it goes through
   `Context::schedule_dependency_asset` → `DependencyManager::register_scheduled_dependency` →
   `would_create_cycle`, which still rejects `K -> K`.

Ordering note: moving the `current_dep_key` read above the metadata upsert takes the `data` read
lock once more in the non-equal path and releases it before the write lock is taken. No lock is
held across the two, so no new deadlock is possible.

### Change 2 — call-site comment in `evaluate_recipe`

`liquers-core/src/assets.rs:1885`. No code change; the existing comment says "Record delegation as
a dependency wait", which is what the reference documents and what is now wrong. It is replaced
with the hand-off statement and a pointer to the guard. The `record_dependency_on_asset` call is
**kept**: the guard makes it a no-op in the same-key case, and it stays correct if a delegate is
ever registered under a key other than its own recipe key.

### What is deliberately not changed

- **`would_create_cycle`** — see above.
- **`AssetManager::wait_for_dependency`** (both the trait default and the `DefaultAssetManager`
  override) — already correct; this fix reaches it for the first time.
- **`DependencyManager::track_asset`** — already guards the delegating asset via
  `bound_owner_key()`, which returns `None` for a non-owner. The delegating asset therefore does
  *not* re-register a version for the key or expire the owner's dependents. Verified, not changed.
- **Re-persisting the owner's value.** After delegation returns a state, `evaluate_and_store`
  installs it and calls `persist_with_status_tracking`, writing the owner's bytes and metadata to
  the store a second time under the same key. Idempotent but wasteful. Out of scope — it is a
  property of `evaluate_and_store`, not of the cycle check — and filed as
  `DELEGATED-VALUE-REPERSISTED`.

## Data Structures

None added or changed. `DependencyKey`, `DependencyRecord`, `Version`, `Recipe`, `AssetRef`,
`AssetData` all keep their current shape and serialization.

## Sync vs Async

Unchanged: `record_dependency_on_asset` stays `async` (it takes `RwLock` guards and calls the
async `DependencyManager`). No blocking I/O introduced.

## Trait Implementations and Generic Parameters

Unchanged: `impl<E: Environment> AssetRef<E>`. The guard uses only `Recipe`/`DependencyKey`, which
are environment-independent.

## Error Handling

The guard returns `Ok(())`; it introduces no new error. `Error::dependency_cycle` remains the
constructor for genuine cycles. No `Error::new`, no `unwrap`/`expect`, no `println!`.

## Match Statements

The guard is an `if` over an `Option<DependencyKey>` comparison, not a `match` over an enum, so the
no-default-arm rule does not apply. No existing match arm is added or removed.

## Integration Points

| Crate / module | Effect |
|---|---|
| `liquers-core/src/assets.rs` | The change. |
| `liquers-core/src/dependencies.rs` | Read-only: `would_create_cycle`, `add_dependency`, `track_asset` are called as before, just not for a self-node. |
| `liquers-core/tests/manager_parametric.rs` | `scenario_keyed_delegation` inverted per its own instructions. |
| `liquers-web` | Uses `ImmediateAssetManager`; benefits with no wasm-specific change. |
| `liquers-py`, `liquers-axum`, `liquers-lib` | No public API touched (`record_dependency_on_asset` is `pub(crate)`), nothing to update. |

## Relevant Commands

**New commands:** none.

**Existing namespaces touched:** none. The fix is below the command layer; the tests use the
locally registered `counted` and `greet` test commands only, so `specs/command_registry.yaml` does
not change and `registry_export` is unaffected.

## Documentation Architecture

| Path | Kind | Audience | Change |
|---|---|---|---|
| `specs/reference/DEPENDENCIES_STATUS.md` | reference | internal | In "Issue F-1 and the implemented fix", replace the bullet claiming delegation records the child in metadata and `DependencyManager` with the hand-off rule. In "Function glossary", correct the `record_dependency_on_asset` entry. Add a `## History` row, bump `reviewed:`. |
| `specs/issues/ASSET-KEYED-DELEGATION-ALWAYS-CYCLES.md` | issue | internal | `status: closed` + resolution note (§4.3). |
| `specs/issues/CONTEXT-APPLY-BARE-KEY-ILL-DEFINED.md` | issue | internal | Correct the sentence listing the spurious cycle as a possible outcome. |
| `specs/issues/VOLATILE-KEYED-RECIPE-SELF-DELEGATION.md` | issue | internal | Its closing pointer now leads to a closed issue; add the follow-on note. |
| `specs/issues/DELEGATED-VALUE-REPERSISTED.md` | issue | internal | **New**, `status: draft`, P3/S/core-assets — the redundant second store write. |
| `specs/README.md` | map | internal | Add the design folder. |
| `specs/index.csv` | index | internal | New design + new issue rows; status change on the closed issue. |

**Proposed `affects_docs`:** `[specs/reference/DEPENDENCIES_STATUS.md]`. Candidates generated by
`area: core/assets` also include `specs/reference/ASSETS.md`, `ASSET_LIFECYCLE.md` and
`PROJECT_OVERVIEW.md`; none of them states the delegation dependency rule, so they are reviewed in
Phase 5 and expected to need no edit.

**Links:** `specs/README.md` gains the design-folder entry. No capability-map entry changes, since
no capability is added.

## Review Notes

*Phase 1 conformity.* Scope is unchanged from Phase 1: one guard, one comment, the inverted tests,
the documented updates. Nothing new crept in; the re-persist question raised as Phase 1 open
question 1 is answered by exclusion + an issue, and open question 2 is answered above.

*Codebase alignment.* Signatures checked against `assets.rs` at HEAD:
`record_dependency_on_asset(&self, &AssetRef<E>) -> Result<(), Error>`,
`DependencyKey::from(&Key)`, `Recipe::key() -> Result<Option<Key>, Error>`,
`AssetManager::wait_for_dependency(&self, &AssetRef<E>, &AssetRef<E>) -> Result<State<E::Value>, Error>`,
`AssetRef::bound_owner_key(&self) -> Result<Option<Key>, Error>`. No existing helper already
performs a same-node check — `would_create_cycle`'s `dependent == dependency` early return is the
closest, and it answers the opposite question (it *reports* the self case rather than exempting
it), so there is nothing to reuse.
