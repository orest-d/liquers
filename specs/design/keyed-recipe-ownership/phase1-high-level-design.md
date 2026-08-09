# Phase 1: High-Level Design - keyed-recipe-ownership

## Feature Name

keyed-recipe-ownership — a non-evaluating ownership test in `AssetRef::evaluate_recipe`

## Purpose

`evaluate_recipe` decides whether it owns a keyed recipe by calling `AssetManager::get(&key)` and
comparing asset ids (`liquers-core/src/assets.rs:1833`). `get` is an *evaluating* call, so under
`ImmediateAssetManager` it re-enters `run_inline` on the asset that is already running and recurses
until the wasm stack dies (`CORE-IMMEDIATE-MANAGER-KEYED-RECURSION`, P1). Replace the ownership
test with a lookup that never evaluates, so the question "am I the asset registered for this key?"
is answered from the key→asset map instead of by starting an evaluation.

## Core Interactions

### Query System
None. No change to parsing, planning or `Key` encoding.

### Store System
None directly. Unblocks `-R/` in the browser, so the four `liquers-web` stores become reachable
through evaluation rather than only through `env.store()`.

### Command System
No new commands, no namespace change.

### Asset System
The whole change. `AssetRef::evaluate_recipe` switches from `AssetManager::get` to a non-evaluating
ownership query on the manager; both `DefaultAssetManager` (queued) and `ImmediateAssetManager`
must agree on what "registered owner" means, including the volatile case where the manager
deliberately registers nothing.

### Value Types
None.

### Web/API
No API change. Five `test.fixme` cases in `liquers-web/tests/e2e/store.spec.ts` become live, plus a
new wasm-side keyed-evaluation test — the regression guard the issue asks for.

### UI
None.

## Crate Placement

`liquers-core/src/assets.rs` only, plus tests in `liquers-core/tests/`, `liquers-web/tests/` and
`liquers-web/tests/e2e/`. Nothing else changes; the fix is below every other crate.

## Scope

In scope: the ownership test and its two manager implementations; the volatile-key case
(`VOLATILE-KEYED-RECIPE-SELF-DELEGATION`, P1, fails on the same line for the same reason — an
id comparison against an asset the manager mints fresh each call); regression tests on both targets.

Out of scope: `LIB-RECIPE-PROVIDER-PANIC` (same code path, different defect, separately filed);
the queued manager's redundant self-submission through `job_queue` (deduplicated today, worth
recording as an issue, not worth fixing here).

## Open Questions

1. What does *no registered owner* mean — the volatile case, and any asset holding a key recipe
   that the manager never put in the map? Self-evaluate (fixes the volatile issue, but the result
   is not shared under the key) or register-then-evaluate? Phase 2 decides.
2. Is `lookup_key_asset` sufficient, or does the ownership test deserve its own trait method with a
   name that states the question (`is_key_owner` / `key_owner`)? `lookup_key_asset` is a sync
   `Mutex`/`scc` read on both managers, so it is cheap either way.
3. Does the queued manager's insertion ordering guarantee the asset is in the map before `run`
   observes it? `get_nonvolatile_resource_asset` inserts via `entry_async().or_insert_with()`
   before returning, so it should — Phase 2 verifies against the concurrent `scc` path.
4. Should a re-entrancy guard in `ImmediateAssetManager::get` be added *as well*, to catch cycles
   this fix does not (option 2 in the issue)? Defence in depth versus a second mechanism to reason
   about.

## References

- `specs/issues/CORE-IMMEDIATE-MANAGER-KEYED-RECURSION.md` (P1, the defect)
- `specs/issues/VOLATILE-KEYED-RECIPE-SELF-DELEGATION.md` (P1, same line, same root cause)
- `specs/issues/LIB-RECIPE-PROVIDER-PANIC.md` (P2, same path, out of scope)
- `specs/reference/ASSETS.md` — asset lifecycle and manager contract
- `specs/design/liquers-web-store/` — M6, where this was found
- `liquers-core/src/assets.rs:1824-1880` (`evaluate_recipe`), `:4456` (queued `get`), `:5616`
  (immediate `get`), `:4975` / `:5668` (`lookup_key_asset`)
