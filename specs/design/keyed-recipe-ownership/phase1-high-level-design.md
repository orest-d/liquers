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
id comparison against an asset the manager mints fresh each call); enforcing *a volatile asset is
never taken from the key map* wherever the map is read, which the ownership test depends on and
which fixes the indefinite reuse of a `set_state`-inserted volatile asset; a re-entrancy guard in
`ImmediateAssetManager::get`; regression tests on both targets.

Out of scope: `LIB-RECIPE-PROVIDER-PANIC` (same code path, different defect, separately filed);
`Context::apply` with a bare key; persistence rules, which are already coherent.

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
- **A volatile asset is never owned by the manager and is never reused.** The manager never
  *resolves* a volatile key through the map (`:4041`, `:5466` mint a fresh asset), but the map can
  still hold an entry for that key — inserted unconditionally by the `set_state`/store-asset paths
  (`:3237`, `:4817`), or left from before the key's recipe became volatile. The rule is therefore
  general: whenever an asset is taken from the key map and found volatile, it is removed and the
  request proceeds as if nothing were cached. Both the ownership test and `get_resource_asset` obey
  it, through one shared non-evaluating helper.

  *This is a live defect, not only a hazard for the fix.* `Status::Volatile.is_finished()` is
  `true` (`metadata.rs:443`) and both managers' `get` return on `is_finished()`, with the expiry
  re-check gated on `status == Status::Ready`. A volatile value written through `set_state` under a
  key with no recipe (so `is_volatile(key)` consults the recipe, finds none, returns `false`) is
  cached and reused indefinitely.
- **Persistence rules are unchanged.** Storing a volatile asset is deliberate: it gives the user the
  chance to override. The value written is by definition not valid and must not be read back, and
  today it is not — `save_to_store` writes metadata with `Status::Volatile`, and `try_fast_track`
  accepts only a stored `Ready | Source | Override` (`:670-681`). A user who deliberately writes
  `Override` under that key is honoured. The mechanism is already coherent; this design does not
  touch it.

- **A re-entrancy guard is added to `ImmediateAssetManager::get` as well**, to report rather than
  crash if the ownership test is ever bypassed. It cannot be shown redundant: the dependency graph
  does cycle-check before the manager is reached (`register_scheduled_dependency` precedes
  `get_dependency_asset`, `context.rs:465-473`), but that check is skipped for an ad-hoc dependent
  (`dependent_opt == None` — no key, no query) and deliberately bypassed for payload dependencies,
  which detect cycles through `active_payload_queries` instead. Either can still reach `get` on a
  key that is mid-evaluation. The guard turns an undebuggable wasm stack overflow into a typed
  error naming the key.

## Open Questions

1. Where does the shared non-evaluating helper live — a method on `AssetManager` that names the
   question (`owned_key_asset`), or a free function over `lookup_key_asset` + `remove_key_asset`?
   The trait option makes the contract visible to future managers; Phase 2 picks.
2. What should the re-entrancy guard *do* — return `Error::dependency_cycle` naming the key, or
   return the asset unrun and let the caller wait? The latter risks a deadlock on wasm's single
   thread. Leaning to the error; Phase 2 confirms against `wait_for_dependency`.

## Noted, not fixed here

- `Context::apply(&pure_key_query, state)` (`context.rs:617`) is ill-defined: it builds an untracked
  asset whose recipe is a bare key, so the supplied input state is discarded by the key recipe, and
  the result is persisted under that key with status `Ready` — which `try_fast_track` will later
  accept. This is pre-existing (today the ad-hoc asset delegates, then persists the delegate's
  state); after this change the value written when nothing is registered would derive from the
  caller's input state instead. To be filed as its own issue rather than special-cased here.
- The queued manager's `evaluate_recipe` re-submits its own asset to `job_queue`, harmless only
  because `try_to_start_immediately` dedups by asset id (`:5207`). Disappears with this fix; worth
  filing if any similar self-submission remains.

## References

- `specs/issues/CORE-IMMEDIATE-MANAGER-KEYED-RECURSION.md` (P1, the defect)
- `specs/issues/VOLATILE-KEYED-RECIPE-SELF-DELEGATION.md` (P1, same line, same root cause)
- `specs/issues/LIB-RECIPE-PROVIDER-PANIC.md` (P2, same path, out of scope)
- `specs/reference/ASSETS.md` — asset lifecycle and manager contract
- `specs/design/liquers-web-store/` — M6, where this was found
- `liquers-core/src/assets.rs:1824-1880` (`evaluate_recipe`), `:4456` (queued `get`), `:5616`
  (immediate `get`), `:4975` / `:5668` (`lookup_key_asset`)
