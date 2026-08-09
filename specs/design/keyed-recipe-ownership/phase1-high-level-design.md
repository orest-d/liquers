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

## Decided (verified against the source)

- **Ownership means "the manager's key→asset map holds this asset under this key".** That is
  `lookup_key_asset` on both managers (`:4975` scc read, `:5668` `Mutex<HashMap>` read).
- **The standard path is always registered, and registered early enough.** Both managers insert
  inside `get_resource_asset` — queued via `entry_async().or_insert_with()` (`:4020`, atomic
  insert-if-absent, so also correct under concurrency), immediate via a double-checked `Mutex`
  insert (`:5503`) — before `get` reaches `run_inline`/`job_queue.submit`. The lookup therefore
  finds the caller itself and the id comparison is unchanged.
- **No registered owner ⇒ the asset evaluates its own recipe.** The result is not shared under the
  key; that is the accepted tradeoff for a non-standard path.
- **The ownership test must be volatility-aware, not a bare map lookup.** The manager never
  *resolves* a volatile key through the map (`:4041`, `:5466` mint a fresh asset), but the map can
  still hold an entry for that key — inserted unconditionally by the `set_state`/store-asset paths
  (`:3237`, `:4817`), or left from before the key's recipe became volatile. A bare lookup would
  find that entry and delegate, reproducing the cycle. Rule: *volatile ⇒ self owns; otherwise
  consult the map.* `resolve_volatility_before_evaluation()` already runs at the top of
  `evaluate_recipe`, so `lock.is_volatile` is available.

## Open Questions

1. Should a non-owner asset still persist under the key? `evaluate_and_store` persists
   unconditionally and `save_to_store` targets `recipe.key().or(recipe.store_to_key())` (`:2052`),
   so an ad-hoc keyed asset writes to the store under that key. It already does today (delegation
   installs the delegate's state, which is then persisted), so the change is not a new write but a
   possibly different value — computed from the caller's input state rather than taken from the
   shared asset. The only production route is `Context::apply(&pure_key_query, state)`
   (`context.rs:617`). Explicit decision wanted, not inheritance.
2. Where does the ownership test live — a private helper on `AssetRef`, or a trait method on
   `AssetManager` that names the question (`key_owner`)? Both managers would share one body either
   way; the trait option makes the contract visible to future managers.
3. Should a re-entrancy guard be added to `ImmediateAssetManager::get` *as well*, to catch cycles
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
