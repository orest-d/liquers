# Phase 2: Solution & Architecture - keyed-recipe-ownership

## Overview

Three changes in `liquers-core/src/assets.rs`, no new types outside it and no API change in any
other crate:

1. **A non-evaluating ownership query.** `AssetManager::owned_key_asset(&key) -> Option<AssetRef<E>>`
   answers "which asset is registered as this key's owner", reading the key→asset map and never
   starting an evaluation. `AssetRef::evaluate_recipe` uses it instead of `AssetManager::get`.
2. **Volatile assets are never served from the key map.** `owned_key_asset` drops a volatile entry
   and reports no owner; both managers' `get` treat `Status::Volatile` as a stale-terminal state,
   alongside `Expired | Error | Cancelled`.
3. **A re-entrancy guard on the inline path.** `ImmediateAssetManager` records the asset ids whose
   inline run is in progress and refuses to start a second one, returning a `DependencyCycle` error
   naming the key instead of recursing.

(1) is the fix. (2) is the invariant (1) depends on, and closes a reuse hole on its own. (3) is the
diagnostic backstop: if (1) is ever bypassed, the failure is a typed error rather than a wasm stack
overflow.

## Data Structures

### New: `InlineRunGuard<'a>` (private to `assets.rs`)

```rust
/// RAII proof that this manager has started an inline run for one asset id.
///
/// Releases on drop, so an early return, an error or a dropped future all clear the id.
/// Holds a borrow of the set, never a `MutexGuard`, so it is safe to hold across an `.await`.
struct InlineRunGuard<'a> {
    running: &'a std::sync::Mutex<std::collections::HashSet<u64>>,
    asset_id: u64,
}

impl Drop for InlineRunGuard<'_> {
    fn drop(&mut self) { /* brief sync lock, remove asset_id */ }
}
```

**Ownership rationale**

- `running` is a shared borrow, not an `Arc`: the guard never outlives the `&self` of the `get`
  call that made it, so the lifetime is free and no refcount traffic is added to the hot path.
- The guard deliberately does **not** hold the `MutexGuard`. `ImmediateAssetManager`'s existing
  field comment states the discipline — *"locked only for brief SYNC get/insert/remove, NEVER
  across an `.await` (the guard is `!Send`, which statically enforces this on native)"* — and a
  guard that held the lock would be `!Send` and could not cross the `run_inline().await` it exists
  to span. `&Mutex<HashSet<u64>>` is `Send + Sync`, so the enclosing future stays `Send` on native.

**Serialization:** none — runtime-only, never persisted.

### Changed: `ImmediateAssetManager<E>`

```rust
pub struct ImmediateAssetManager<E: Environment> {
    id: std::sync::atomic::AtomicU64,
    envref: std::sync::OnceLock<EnvRef<E>>,
    assets: std::sync::Mutex<std::collections::HashMap<Key, AssetRef<E>>>,
    query_assets: std::sync::Mutex<std::collections::HashMap<Query, AssetRef<E>>>,
    dependency_manager: crate::dependencies::DependencyManager<E>,
    started: tokio::sync::OnceCell<()>,

    /// Asset ids whose inline run is in progress on this manager.
    ///
    /// Same locking discipline as `assets`: brief sync insert/remove, never across an `.await`.
    running_inline: std::sync::Mutex<std::collections::HashSet<u64>>,
}
```

Initialized empty in `new()`. `Default` already delegates to `new()`, so it is unchanged.

`DefaultAssetManager` gains no field: it does not run assets inline, and its execute-once guarantee
is already carried by `RunClaim` / `try_claim_for_run` (`:5099`).

### New enums

None.

### `ExtValue` extensions

None.

## Trait Implementations

### `AssetManager<E>` — two additive methods, both with default bodies

Additive with defaults, per the refactoring rule: neither manager is forced to override, and an
out-of-tree implementor keeps compiling. Only two implementors exist today, both in
`liquers-core/src/assets.rs` (`:4131`, `:5525`).

```rust
/// The asset currently registered as this key's owner, if any.
///
/// **Non-evaluating.** Reads the key→asset map; never starts, submits, fast-tracks or
/// resolves an evaluation. This is what makes it usable from inside an evaluation.
///
/// A volatile asset is never an owner: it cannot be shared and cannot be reused, so a
/// volatile entry is removed and `None` is returned. `None` therefore means "no asset is
/// registered for this key", and the caller owns the key's recipe itself.
async fn owned_key_asset(&self, key: &Key) -> Option<AssetRef<E>> {
    let asset = self.lookup_key_asset(key)?;
    if asset.is_volatile().await {
        self.remove_key_asset_if(key, asset.id()).await;
        return None;
    }
    Some(asset)
}

/// Remove this key's entry only if it is still the asset with `asset_id`.
///
/// Returns whether an entry was removed. The id check keeps a slow caller from evicting a
/// replacement inserted after its own lookup.
///
/// The default is lookup-compare-remove, which is correct but not atomic; both in-tree
/// managers override it to do the whole thing under their own map lock.
async fn remove_key_asset_if(&self, key: &Key, asset_id: u64) -> bool {
    match self.lookup_key_asset(key) {
        Some(existing) if existing.id() == asset_id => {
            self.remove_key_asset(key).await;
            true
        }
        Some(_) | None => false,
    }
}
```

**Overrides**

| Implementor | `owned_key_asset` | `remove_key_asset_if` |
|---|---|---|
| `DefaultAssetManager` | default | override: `scc` `get_async` + id compare + `remove_async`, mirroring the block already inlined in `get` (`:4470-4477`) |
| `ImmediateAssetManager` | default | override: one `assets` `Mutex` acquisition covering compare and remove |

**Bounds:** none beyond the trait's existing `E: Environment`. Both methods are `async` and take
`&self`, so the trait stays object-safe and keeps working behind `Arc<dyn …>`.

**`async_trait`:** both inherit the trait's existing
`#[cfg_attr(not(target_arch = "wasm32"), async_trait)]` /
`#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]` pair. No new gating.

**Why `Option`, not `Result`:** the question has no failure mode. A missing entry is an answer, not
an error, and returning `Result` would push a `?` into `evaluate_recipe` for a case that cannot
occur. This is the concrete improvement over the status quo, where the ownership test could fail —
or, worse, evaluate.

### `AssetRef<E>` — one new accessor

```rust
/// Whether this asset is volatile: flagged before evaluation, or volatile as a final status.
///
/// Deliberately does **not** consult `Metadata::is_volatile()`. That returns true for a
/// value whose stored metadata carries `is_volatile` while its status is `Override` — the
/// user-supplied override, which is exactly the case that must stay reusable.
pub async fn is_volatile(&self) -> bool {
    let lock = self.data.read().await;
    lock.is_volatile || lock.status == Status::Volatile
}
```

Sits beside `is_expired()` (`:2392`) and follows its shape. No name collision:
`AssetManager::is_volatile(&self, key)` is a different trait on a different type.

### `AssetRef<E>::evaluate_recipe` — the ownership test

Current (`:1830-1836`):

```rust
if let Ok(Some(key)) = recipe.key() {
    let envref = self.get_envref().await;
    let manager = envref.get_asset_manager();
    let asset = manager.get(&key).await?;          // ← evaluates; recurses under Inline
    if asset.id() == self.id() { /* own the recipe */ } else { /* delegate */ }
}
```

Replacement:

```rust
if let Ok(Some(key)) = recipe.key() {
    let envref = self.get_envref().await;
    let manager = envref.get_asset_manager();
    match manager.owned_key_asset(&key).await {
        // Someone else is the registered owner: delegate, unchanged.
        Some(owner) if owner.id() != self.id() => { /* existing delegation body */ }
        // Registered owner is this asset, or nothing is registered (volatile, or an
        // untracked asset built from a key recipe): resolve and evaluate the recipe here.
        Some(_) | None => { /* existing own-recipe body */ }
    }
}
```

`Some(_) | None` rather than `_ =>`: `Option` is not a Liquers-owned enum, but writing the arms out
keeps the three cases visible, which is the point of the rule.

**Both bodies are moved verbatim.** No behaviour inside either arm changes; only the predicate
that selects between them, and its cost — a map read instead of an evaluation.

### `ImmediateAssetManager<E>` — inherent methods

```rust
impl<E: Environment> ImmediateAssetManager<E> {
    /// Claim the right to run `asset_id` inline here, or `None` if a run is already in
    /// progress for it on this manager.
    fn try_enter_inline(&self, asset_id: u64) -> Option<InlineRunGuard<'_>> { … }
}
```

Applied at the three sites that can hand back a *shared* asset and then run it inline:

| Site | Line | Call guarded |
|---|---|---|
| `get` | `:5647` | `asset_ref.run_inline()` |
| `get_asset` (query branch) | `:5567` | `assetref.run_inline()` |

These are the only two inline runs that can be handed a *shared* asset. `apply` (`:5575`) and
`apply_immediately` (`:5612`) mint a fresh id that cannot already be in the set, so a guard would
be dead code. `get_dependency_asset_with_payload` (`:5600`) is likewise excluded: it is reached
only for a query whose plan requires a payload, and `Required` forces volatility (`plan.rs:995`),
so `get_resource_asset` returns a fresh unshared asset — as its own comment states.

On a refused claim the site returns

```rust
Err(Error::dependency_cycle(&DependencyKey::from(&key)))
```

`ErrorType::DependencyCycle` already exists (`error.rs:34`) and `dependency_cycle` renders the key
(`error.rs:355`). For the query sites, `DependencyKey::from(&query)` gives the same shape.

**Why an id set and not `Status`.** A status-based guard would be the smaller change but does not
work: nothing sends `AssetServiceMessage::JobStarted` (the only producer of `Status::Processing`
outside `try_claim_for_run`), so an asset running inline never reaches `Processing`. Even if it
did, `run_with_future_inline` drives `process_service_messages` concurrently through
`futures::join!` (`:1792`), so status transitions are ordered relative to the evaluation only by
executor scheduling — on wasm's single thread, only at await points. The id set is authoritative at
the instant it is read.

**Why not reuse `RunClaim`.** `RunClaim` / `try_claim_for_run` are `#[cfg(not(target_arch =
"wasm32"))]` and take `&Arc<JobQueue<E>>`, which the inline manager does not have; its `Drop`
repair re-parks through that queue and uses `tokio::spawn`. Extending it to a queue-less inline
form is the right long-term shape — it would close the double-run window that `RunClaim`'s own doc
comment attributes to `run_with_future`'s `is_finished()`-only guard, on the inline path too — but
that is execute-once work, not this fix. Filed rather than folded in (see Phase 1, *Noted, not
fixed here*).

## Volatile entries in the key map

The invariant: **the key map never serves a volatile asset.**

`owned_key_asset` enforces it on the ownership path. The cache-serving loops enforce it by adding
one variant to the stale-terminal set they already match on:

```rust
if matches!(status, Status::Expired | Status::Error | Status::Cancelled | Status::Volatile) {
    // drop the entry if it is still this asset, then retry
}
```

Five sites carry that set, and all five take the variant — the same defect exists on the query map,
where `get_query_asset` likewise bypasses the map for a volatile query:

| Manager | Function | Line | Map |
|---|---|---|---|
| `DefaultAssetManager` | `get` | `:4467` | key |
| `DefaultAssetManager` | `get_asset` (query branch) | `:4154` | query |
| `DefaultAssetManager` | `get_dependency_asset` | `:4203` | both |
| `ImmediateAssetManager` | `get` | `:5621` | key |
| `ImmediateAssetManager` | `get_asset` (query branch) | `:5534` | query |

This cannot spin. The removal happens *before* the asset is run, and a freshly built volatile asset
is never inserted (`get_volatile_resource_asset` `:4041`, `get_volatile_query_asset` `:4091`,
`make_volatile` `:5466` all bypass the map), so the next iteration finds an empty slot and resolves
to a pre-execution asset — the same argument the existing comment at `:4199` makes for the other
three variants.

**A freshly computed volatile value is still returned.** The eviction is on *entry* to `get`: the
status is checked before the run, and after `run_inline` / `submit` the asset is returned without a
re-check. So a caller that asks for a key whose result turns out volatile gets that result; only the
*next* request evicts and recomputes. That is the intent of "used, never reused", and it is why the
variant belongs in the entry check rather than anywhere later.

**Scope note — this widens Phase 1 slightly.** Phase 1 framed the invariant around the key map;
three of the five sites above are the query map. The principle is a property of assets, not of
keys, and `get_query_asset` bypasses the map for volatile queries exactly as the keyed path does,
so the same reuse hole exists there. Fixing one and not the other would leave the invariant true
only by coincidence. Recorded as a deliberate widening, not an accident.

**Introspection is deliberately not covered.** The other `lookup_key_asset` callers — `contains`,
`get_asset_info`, the cancel/remove paths (`:3005`, `:3082`, `:3188`, `:3279`, `:3478`, `:3516`,
`:3545`, `:3571`) — keep the raw lookup. Reporting that a volatile asset exists is not reusing its
value, and `set_state` (`:3188`) cancels and removes the entry anyway.

**The hole this closes.** `Status::Volatile.is_finished()` is `true` (`metadata.rs:443`), both
`get` loops return on `is_finished()`, and the expiry re-check is gated on `status ==
Status::Ready` — so a map-registered asset that ends `Status::Volatile` is served from cache
forever. It is reachable: registration decides volatility from `Recipe::is_volatile(env)`, which
resolves the plan (`interpreter.rs:453`), but `try_to_set_ready` (`:1293`) also marks the result
volatile when `metadata.expires()` is volatile — an expiry a command sets *during* evaluation,
which no registration-time check can see.

**Correction to Phase 1.** Phase 1 attributed this hole to `set_state`. That was wrong:
`set_state` maps an incoming `Status::Volatile` to `Override` or `Source` (`:3210`, `:4649`), so it
never inserts a volatile-status asset. The runtime-expiry route above is the real one. `is_volatile`
on `AssetRef` is defined to match — flag or status, not metadata — precisely so a `set_state`
`Override` entry stays reusable, which is the override mechanism Phase 1 decided to preserve.

## Function Signatures

Every signature the implementation adds or changes, in one place.

```rust
// --- liquers-core/src/assets.rs ---

// trait AssetManager<E: Environment>  — additive, both defaulted
async fn owned_key_asset(&self, key: &Key) -> Option<AssetRef<E>>;
async fn remove_key_asset_if(&self, key: &Key, asset_id: u64) -> bool;

// impl AssetManager<E> for DefaultAssetManager<E>
async fn remove_key_asset_if(&self, key: &Key, asset_id: u64) -> bool;   // override, atomic

// impl AssetManager<E> for ImmediateAssetManager<E>
async fn remove_key_asset_if(&self, key: &Key, asset_id: u64) -> bool;   // override, atomic

// impl<E: Environment> AssetRef<E>
pub async fn is_volatile(&self) -> bool;                                  // new

// impl<E: Environment> ImmediateAssetManager<E>
fn try_enter_inline(&self, asset_id: u64) -> Option<InlineRunGuard<'_>>;  // new, sync

// private to assets.rs
struct InlineRunGuard<'a>;
impl Drop for InlineRunGuard<'_>;
```

**Unchanged signatures.** `AssetRef::evaluate_recipe`, `AssetManager::get`,
`AssetManager::get_asset`, `lookup_key_asset`, `insert_key_asset` and `remove_key_asset` keep their
current signatures exactly; only bodies change. Nothing in `liquers-py`'s surface moves.

## Sync vs Async

| Item | Choice | Rationale |
|---|---|---|
| `owned_key_asset` | async | must `.await` the asset's `RwLock` for the volatility check |
| `remove_key_asset_if` | async | matches the existing `remove_key_asset` / `insert_key_asset` primitives |
| `AssetRef::is_volatile` | async | reads `data: Arc<RwLock<AssetData>>`, like `is_expired` |
| `try_enter_inline` | **sync** | a brief `std::sync::Mutex` insert; making it async would invite holding the lock across an await, which the field's own comment forbids |

No blocking I/O is introduced. No lock is held across an `.await`.

## Integration Points

| Crate | Change |
|---|---|
| `liquers-core` | `src/assets.rs` only. `src/error.rs`, `src/metadata.rs`, `src/context.rs` unchanged. |
| `liquers-macro`, `liquers-store`, `liquers-lib`, `liquers-axum`, `liquers-py` | none — the trait grows two defaulted methods and no signature changes |
| `liquers-web` | no source change; five `test.fixme` markers removed in `tests/e2e/store.spec.ts` |

Dependency flow is respected: the change is at the bottom of the stack and points nowhere upward.

## Relevant Commands

**New commands:** none. This is a defect fix in the asset subsystem; it adds no query syntax, no
action and no namespace.

**Existing namespaces involved:** none functionally. Tests need a command that is `volatile: true`
to build a volatile keyed recipe; `liquers-core`'s own test environment registers such commands
locally (as `payload_inheritance.rs` already does), so no `liquers-lib` namespace is pulled in and
`specs/command_registry.yaml` does not change.

## Error Handling

- `Error::dependency_cycle(&DependencyKey)` for a refused inline claim — an existing typed
  constructor, no new `ErrorType`.
- `owned_key_asset` and `is_volatile` cannot fail and return no `Result`.
- No `Error::new`, no `unwrap`, no `expect`. Existing `Mutex` poisoning handling
  (`unwrap_or_else(|e| e.into_inner())`) is reused for `running_inline`.

## Rust Best Practices Review

Applied to this document; blocking items are resolved above rather than left as notes.

**Resolved during design**

- *A guard holding a `MutexGuard` across `.await` would be `!Send`* — hence `InlineRunGuard` holds
  `&Mutex`, not the guard. Would otherwise have failed to compile on native under `async_trait`.
- *`Metadata::is_volatile()` is the wrong predicate* — it is true for an `Override` entry whose
  stored metadata carries the flag, which would have evicted exactly the override case Phase 1
  decided to keep. `AssetRef::is_volatile` uses flag-or-status.
- *A bare `remove_key_asset` after a lookup can evict a replacement* — hence the id-checked
  `remove_key_asset_if`, matching the compare-before-remove already inlined in both `get` loops.
- *Trait changes must be additive* — both new methods carry default bodies; no existing signature
  moves.
- *No default match arm* — the `matches!` sets and the `Option` match are written out.

**Advisory, accepted**

- `remove_key_asset_if` duplicates logic already inlined three times in the `get` loops. Replacing
  those with calls to it is a genuine simplification but touches working code beyond the fix;
  proposed as an optional final step in Phase 4, not a requirement.
- `owned_key_asset`'s default body performs two map operations in the volatile case. The path is
  cold (a volatile entry in the map is the exceptional case) and each is a brief sync lock.

## Review Findings

Two passes were run against this document — Phase 1 conformity, and alignment with the code at
every integration point. Everything found is resolved above except where noted.

**Phase 1 conformity** — no drift. Every Phase 1 decision is carried: self-evaluation when no owner
is registered; persistence untouched; `LIB-RECIPE-PROVIDER-PANIC` and `Context::apply`-with-a-key
left alone; both Phase 1 open questions answered. One deliberate widening (query map as well as key
map), recorded in place rather than smuggled in.

**Codebase alignment** — checked and clear:

- `lookup_key_asset(key) -> Option<AssetRef<E>>` and `remove_key_asset(key) -> ()` match the uses
  in the default bodies; `?` on `Option` inside an `async fn -> Option<_>` is fine under
  `async_trait`'s desugaring.
- The trait already carries defaulted `async fn`s (`is_volatile`, `get_dependency_asset`,
  `makedir`), so the pattern is established; both new methods take `&self` and no generics, so
  object safety behind `Arc<dyn AssetManager<E>>` is preserved.
- `DependencyKey: From<&Key>` (`metadata.rs:233`) and `From<&Query>` both exist, so
  `Error::dependency_cycle` can be built at each guard site.
- **Lock ordering is safe.** `owned_key_asset` calls `AssetRef::is_volatile`, which takes
  `data.read()` — possibly on the *calling* asset. At the ownership test no lock is held:
  `initial_state_and_recipe` acquires and releases inside its own block (`:1815`). This matters
  because `tokio::sync::RwLock` is write-preferring, so a re-entrant read behind a queued writer
  would deadlock. The call it replaces (`manager.get`) reaches the same lock, so no new hazard is
  introduced — but the property is now load-bearing and should not be broken by moving the test
  earlier.
- **`std::collections::HashSet` is not yet imported** in `assets.rs` (`HashMap` and `VecDeque`
  are). One import line; called out so it is not discovered at compile time.
- `remove_expired_from_maps` (`:5695`) removes by id from either map, so the
  `get_dependency_asset` site needs no new primitive.

## Open Questions

None blocking. Both Phase 1 questions are settled above: the helper is a defaulted `AssetManager`
method (`owned_key_asset`), and the re-entrancy guard returns `Error::dependency_cycle` rather than
handing back an unrun asset — which on wasm's single thread would deadlock any caller that then
awaited its value.

## References

- Phase 1: `./phase1-high-level-design.md`
- `liquers-core/src/assets.rs` — `evaluate_recipe` `:1826`, queued `get` `:4456`, inline `get`
  `:5616`, `lookup_key_asset` `:4975`/`:5668`, `try_to_set_ready` `:1284`, `RunClaim` `:5039`
- `liquers-core/src/error.rs:355` — `dependency_cycle`
- `liquers-core/src/metadata.rs:443` — `Status::Volatile.is_finished()`
- `specs/reference/ASSETS.md` — asset lifecycle and manager contract
