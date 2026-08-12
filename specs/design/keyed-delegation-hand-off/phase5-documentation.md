# Phase 5: Documentation - keyed-delegation-hand-off

## Completion Preconditions

Implementation is finished and validated (see **Validation**), the design carries no outstanding
review or user comments, and every documentation change below was checked against the implemented
and tested behaviour rather than against the plan.

**Date:** 2026-08-12 · **Issue closed:** `ASSET-KEYED-DELEGATION-ALWAYS-CYCLES` (P0)

## Implementation Summary

One rule, in one place. `AssetRef::record_dependency_on_asset`
(`liquers-core/src/assets.rs`) now tests node identity before it writes anything and returns
`Ok(())` on a match:

> **Two assets that resolve to the same key are one node of the dependency graph, and a node has no
> edge to itself.** Waiting on such an asset is a hand-off, not a dependency.

Node identity is `AssetRef::bound_key_candidate()` — the key each asset was *constructed* with —
falling back to the recipe-derived `DependencyKey`. `AssetData::recipe` is mutable, so it cannot be
the primary identity; `Context::schedule_dependency_asset` already classifies keyed dependents by
`owner_key()` for the same reason.

That single early return unblocks the keyed-delegation branch of `AssetRef::evaluate_recipe`, which
could never succeed before: the branch is only entered when the delegate is registered under the
*caller's own* key, so `would_create_cycle` was always asked about a self-edge and always answered
`true`. Delegation now reaches `AssetManager::wait_for_dependency` and returns the owner's value.

Supporting changes: the delegation call site's comment was rewritten to state the hand-off
semantics (no code change — the `record_dependency_on_asset` call is deliberately kept, since it
stays correct if a delegate is ever registered under a key other than its own recipe key), and the
method's doc comment now carries the rule.

## Validation

| Test | Location | Result |
|---|---|---|
| `keyed_delegation_default`, `keyed_delegation_immediate` | `liquers-core/tests/manager_parametric.rs` | Inverted from asserting `Dependency cycle` to asserting the owner's value with the call counter still at `1`. Both pass. |
| `record_dependency_on_asset_skips_same_node_hand_off` | `liquers-core/src/assets.rs` | New. Nothing recorded in metadata, no self-edge in the graph. |
| `record_dependency_on_asset_records_distinct_key` | same | New. The guard is not over-broad. |
| `record_dependency_on_asset_hand_off_survives_owner_recipe_resolution` | same | New (PR 32 review). The owner's recipe resolved to a pure-key alias — still one node. Verified to fail without the `bound_key_candidate` identity. |
| `test_keyed_asset_evaluating_its_own_key_is_a_cycle` | `liquers-core/tests/dependency_scheduling.rs` | New. A genuine keyed self-dependency via `Context::evaluate` is still rejected, and does not hang. |
| Full core suite | `cargo test -p liquers-core --lib --tests` | 637 passed, 0 failed. |
| Standard loop | `cargo test -p liquers-lib --lib --tests` | 369 passed, 0 failed. |

Not run: the `liquers-web` wasm loops. They require a `cargo clean` between them and the native
loop under this environment's disk allowance, and the change adds no wasm-specific behaviour —
`liquers-web` uses `ImmediateAssetManager`, whose delegation path is covered natively by
`keyed_delegation_immediate`. Stated rather than claimed.

## Conformance and Remaining Work

Conforms to the request and to the approved design. The issue offered two fix directions and Phase
2 chose (1), "delegation stops recording a self-dependency", over (2), "remove the branch": option
(2) would lose value sharing in the stale-owner case and have two assets compute one key
concurrently, which is what the branch exists to prevent. Scope did not drift across the phases —
Phase 1's two open questions were both answered in Phase 2 (guard placement; re-persistence
excluded and filed).

**Added beyond the plan:** nothing. **Omitted:** the redundant re-persist, deliberately — see below.

**Review round (PR 32).** Codex found that the first implementation compared *recipe* keys, and
`AssetData::recipe` is mutable: when a provider resolves `K` to a pure-key alias `L`, owner
evaluation replaces the owner's recipe, so its `recipe.key()` becomes `L` while the delegate still
holds `K`. The same-node test then missed the hand-off and recorded `K -> L` carrying the owner's
metadata version — a version for `K` — which `DependencyManager::add_dependency` compares against
`L`'s registered version and can expire `K` for. Confirmed and fixed by taking identity from
`bound_key_candidate()` (the immutable construction-time key), with the recipe comparison retained
as a fallback for assets that have no bound key. The finding is sound and the fix matches an
existing convention: `Context::schedule_dependency_asset` already carries the comment "Provider
resolution can replace the mutable recipe, so deriving this identity from `AssetData::recipe` would
register edges under the wrong key."

**Genuine self-dependency is unaffected**, which was worth pinning explicitly rather than arguing.
`record_dependency_on_asset` has one production caller, the delegation branch. A command calling
`Context::evaluate` on its own asset's key travels `schedule_dependency_asset` →
`register_scheduled_dependency` → `would_create_cycle` and still fails fast.
`test_keyed_asset_evaluating_its_own_key_is_a_cycle` now covers that end to end; the existing
coverage was a `register_scheduled_dependency` unit test and an *expression* cycle, neither of
which would catch a future change that moved this exemption into the shared path.

One correction during implementation: T1 as sketched in Phase 3 used
`DependencyManager::expire(&key)` to prove no edge exists, which always fails — `expire` includes
the root key by construction, so it returns the key itself whether or not an edge is present. The
test uses `expire_dependents`, which excludes the root and therefore reports a self-edge as the key
expiring itself. The assertion is stronger than intended, not weaker: it also catches a fix that
skipped only the cycle check while still offering the edge.

## Issues Filed

- **Closed:** `ASSET-KEYED-DELEGATION-ALWAYS-CYCLES` — resolution and evidence recorded in the
  issue body.
- **Filed:** `DELEGATED-VALUE-REPERSISTED` (draft, P3/S, `core/assets`). A delegating asset installs
  the owner's state and `evaluate_and_store` then writes it to the store again under the same key.
  Idempotent but wasteful. Out of scope here: it is a property of `evaluate_and_store`, not of the
  dependency-cycle check. Unreachable before this fix, because delegation always errored first.
- **Corrected, not reopened:** `CONTEXT-APPLY-BARE-KEY-ILL-DEFINED` (rejected) and
  `VOLATILE-KEYED-RECIPE-SELF-DELEGATION` (closed) both described the spurious cycle as current
  behaviour; both now point at the fix.

## Important Learning

1. **The cycle check was never wrong.** `would_create_cycle` returning `true` for
   `dependent == dependency` is the correct answer to the question it is asked. The defect was that
   the question was asked at all. Fixing the checker instead would have weakened genuine cycle
   detection.
2. **The metadata record mattered as much as the graph edge.** It is easy to see the fix as "skip
   the cycle check". But `DependencyRecord`s are persisted and replayed through
   `DependencyManager::load_from_records` by `track_asset`, so a self-record would have reinstalled
   the self-edge on every reload. The guard therefore had to move *above* the metadata write, which
   is why `current_dep_key` is now derived early.
3. **Identity must come from an immutable field.** `AssetData::recipe` is replaced by provider
   resolution mid-evaluation, so any identity test built on it is only correct until the asset
   evaluates. The codebase already knew this in two places — `bound_key_candidate` exists for it,
   and `Context::schedule_dependency_asset` classifies keyed dependents by `owner_key()` for it —
   and the first version of this fix still reached for the recipe. Review caught it. When adding a
   comparison over asset identity, check which field the codebase already trusts.
4. **`bound_owner_key()` already encodes ownership correctly**, so `track_asset` needed no change: a
   delegating asset is not the registered owner, does not re-register a version for the key, and
   does not expire the owner's dependents. Persistence has no equivalent check — that asymmetry is
   `DELEGATED-VALUE-REPERSISTED`.
5. **The test that caught this was written to be inverted.** `scenario_keyed_delegation` asserted
   the broken outcome and panicked with instructions if it ever produced a value. That is what made
   this a fifteen-minute change rather than an investigation, and it is worth copying when a branch
   is known-broken but its *selection* still needs pinning. The call-counter assertion is the
   load-bearing one: a regression to self-evaluation would still produce the right value.

## Documentation Delivered

`specs/reference/DEPENDENCIES_STATUS.md` — new section **"Delegation is a hand-off, not a
dependency"**, plus corrections to the F-1 bullet list, Flow A step 3, and the
`record_dependency_on_asset` glossary entry. `reviewed:` bumped to 2026-08-12 with a `## History`
row. That reference is the entry point; this folder is history.

`affects_docs` is `[specs/reference/DEPENDENCIES_STATUS.md]`. The other `core/assets` candidates —
`ASSETS.md`, `ASSET_LIFECYCLE.md`, `PROJECT_OVERVIEW.md` — were reviewed and state nothing about
the delegation dependency rule, so they needed no edit. No guide was created: delegation is an
internal branch with no user-facing workflow and no "how do I …" question to answer.

One pre-existing inconsistency was found and *not* fixed: Flow A steps 5, 7 and 8 in
`DEPENDENCIES_STATUS.md` still describe the pre-2026-07-15 wait mechanics (`B.run()` inline,
`leave_dependencies_for_resubmit`), which the document's own "Non-blocking dependency scheduling"
section supersedes. It is recorded in the `## History` row rather than silently carried as
reviewed.
