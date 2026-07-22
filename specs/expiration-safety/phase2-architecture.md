# Phase 2: Solution & Architecture - expiration-safety

## Overview

No new structs/enums, no new crate dependencies, no command/API surface. Three targeted changes
in `liquers-core/src/assets.rs`: (1) the expiration monitor's heap entry holds a `WeakAssetRef<E>`
instead of a strong ref, (2) `poll_state()` stops serving data for `Status::Expired`, paired with
a new `poll_state_any_status`/`get_any_status` read family that deliberately bypasses that guard,
(3) two new required methods on the `AssetManager<E>` trait — `get_any_status` and `to_override` —
give keyed assets an explicit, non-evaluating recovery and promote-to-`Override` path.
`rust-best-practices` skill is not installed in this environment; Rust-idiom checks below
(explicit matches, minimal bounds, no new `unwrap`/`expect`) were applied manually against the
existing code style in `assets.rs`.

**Naming (revised from the initial `_also_expired` draft, per user feedback):**
- The read method is named `get_any_status`/`poll_state_any_status`, following the existing
  `_unchecked` convention already used in this file (`State::data_unchecked()`, and the wider Rust
  convention `Vec::get_unchecked`) for "bypasses a normal guard" — here, the guard is "not
  expired." `_any_status` was chosen over `_unchecked` because it is self-describing without the
  unsafe-adjacent connotation `_unchecked` carries in Rust.
- The promote method is named `to_override`, reusing the verb already established by
  `AssetRef::to_override()` (`assets.rs:1833`). This also corrects a scoping error in the initial
  draft: promoting to `Override` is **not** Expired-specific — a `Ready` asset can be pinned to
  `Override` the same way, exactly as `AssetRef::to_override()` already does for every data-bearing
  status. The manager-level method must therefore not be named after `Expired` at all.

## Data Structures

### Modified: `TimedAsset<E>` (`assets.rs:2399`)

```rust
struct TimedAsset<E: Environment> {
    expiration: chrono::DateTime<chrono::Utc>,
    asset_id: u64,
    asset_ref: WeakAssetRef<E>,   // was: AssetRef<E>
}
```

**Ownership rationale:** `WeakAssetRef<E>` already exists (`assets.rs:723`, `.downgrade()` at
`:961`) and is used elsewhere for non-retaining tracking (`DependencyManager`'s expired-asset
list). The monitor must hold no strong references (WP-3 item 1) so a dropped/evicted asset's
memory isn't kept alive purely by a pending timer. `Ord`/`Eq`/`PartialOrd`/`PartialEq` impls are
unaffected — they compare only `expiration`/`asset_id`.

**No serialization impact:** `TimedAsset` is monitor-internal, never (de)serialized.

### New Enums / New Structs

None required.

### ExtValue Extensions

Not applicable.

## Trait Implementations

### Trait: `AssetManager<E>` (`assets.rs:2248`)

Two new **required** methods (no default body — see rationale below):

```rust
/// Recovery-only read for a KEYED asset: returns its state regardless of status — including
/// `Status::Expired` — without submitting evaluation, without touching the dependency manager,
/// and without registering the entry back into the manager's normal in-memory cache.
/// `Ok(None)` if the key has no data-bearing state (in memory or in the store).
async fn get_any_status(&self, key: &Key) -> Result<Option<State<E::Value>>, Error>;

/// Pin a KEYED asset's current value (whatever status it is in — `Ready`, `Expired`, etc.) as
/// `Status::Override`, preserving the value without recomputation. Errors if there is no
/// data-bearing state for `key` to promote (in memory or in the store).
async fn to_override(&self, key: &Key) -> Result<(), Error>;
```

**Bounds:** None beyond the trait's existing `Send + Sync`.

**Why required, not default:** every other *data-bearing* trait method (`get`, `set_state`,
`remove`, `contains`) is required because it depends on the manager's internal storage shape;
only pure orchestration helpers (`get_dependency_asset`, `drain_dependencies`,
`wait_for_dependency`) get default bodies, because those are expressible purely in terms of other
required methods. `get_any_status`/`to_override` are the same kind of storage-shape-dependent
operation (which in-memory map exists, if any; how to read the store without going through the
fast-track/evaluation path). A generic default composed from existing trait methods
(`get_any_status` piped into `set_state`) would necessarily re-serialize the value on every
promotion — exactly the double-serialization the user asked to avoid — so no default is provided;
each manager (today `DefaultAssetManager`, later the `async-wasm-refactor` manager) implements
both against its own internals. This was confirmed with the user: `get_any_status` must be a
trait method precisely because a second manager implementation is coming and must expose the
same capability.

### `DefaultAssetManager<E>` implementation (algorithm, bodies in Phase 4)

**`get_any_status(key)`:**
1. If `self.assets` has an in-memory entry for `key`, return
   `asset_ref.get_any_status().await` (new `AssetRef` method below) — covers `Expired` and every
   other data-bearing status.
2. Else, read directly from the store (`store.get(key)`) bypassing `try_fast_track`'s status
   allow-list; if the stored `Metadata::status().has_data()` (existing helper,
   `metadata.rs:332`, already `true` for `Expired`), deserialize using the same binary/type-
   identifier logic `try_fast_track` uses (`assets.rs:486-504`) and return `Some(state)`.
   Deliberately skip dependency-manager version registration and re-insertion into `self.assets` —
   this path must have no side effects on normal evaluation (WP-3 item 4).
3. Otherwise `Ok(None)`.

**`to_override(key)`:** not Expired-specific — pins whatever current value exists (`Ready`,
`Expired`, `Partial`, etc.) as `Override`, mirroring `AssetRef::to_override()`'s own status
coverage:
1. If `self.assets` has an in-memory entry for `key`:
   - call the existing `AssetRef::to_override()` (`assets.rs:1833`, already handles every
     data-bearing status — including plain `Ready`, not just `Expired` — while preserving the
     value) to flip in-memory state;
   - branch on the existing `AssetRef::persistence_status()` (`PersistenceStatus`,
     `assets.rs:134`) recorded from the *original* evaluation:
     - `Persisted` — the store already holds the correct bytes; call
       `store.set_metadata(key, &metadata)` only (status field updated to `Override`), no
       re-serialization.
     - `NonSerializable` — nothing was ever stored for this value; no store write at all.
     - `NotPersisted` | `None` — the original save failed or was never attempted; retry a full
       persist (serialize + `store.set(key, &binary, &metadata)`), updating
       `persistence_status` via the existing `record_persistence_result` path.
   - leave the (now `Override`) `AssetRef` in `self.assets` (or reinsert if a race with the
     monitor already evicted it) so a subsequent normal `get(key)` sees it immediately.
2. Else (no in-memory entry — e.g. already evicted after expiring): load the persisted state from
   the store exactly as `get_any_status`'s store-fallback does (only `Ready`/`Source`/`Override`/
   `Expired` are ever persisted, so this is the only status set reachable here); since
   deserialization succeeded, the bytes are provably already persisted, so rewrite **only** the
   metadata `status` field to `Override` via `store.set_metadata`. No in-memory `AssetRef` is
   created — the existing fast-track allow-list already includes `Override` (`assets.rs:475`), so
   the next normal `get(key)` loads it back into memory on its own.
3. If neither exists, `Err(Error::key_not_found(key))`.

This directly implements the user's Q2 answer: consistent end state, no double-serialization,
`PersistenceStatus` drives the retry-vs-skip-vs-metadata-only branch — now correctly generalized
beyond `Expired` to any current value, per the user's correction above.

## New `AssetData`/`AssetRef` methods

```rust
// AssetData (sync, assets.rs, next to poll_state at :596)
pub fn poll_state_any_status(&self) -> Option<State<E::Value>> {
    match self.status {
        Status::Expired => { /* same construction as today's Expired arm of poll_state */ }
        _ => self.poll_state(),
    }
}

// AssetRef (async wrapper, assets.rs, next to poll_state at :2075)
pub async fn poll_state_any_status(&self) -> Option<State<E::Value>> {
    self.data.read().await.poll_state_any_status()
}

/// Peek-only, no waiting: Expired (and any other data-bearing status) returns its value;
/// non-terminal or data-less statuses return `None` immediately (unlike `get()`, this never
/// blocks — recovery reads a snapshot, it does not await completion).
pub async fn get_any_status(&self) -> Option<State<E::Value>> {
    self.poll_state_any_status().await
}
```

**Modified `AssetData::poll_state()` (`assets.rs:596`):** move `Status::Expired` out of the
`Ready | Expired | Source | Override | Volatile` value-returning arm into its own arm returning
`None`. `Ready | Source | Override | Volatile` keep returning data unchanged. This is the minimal
fix for WP-3 item 2's `AssetRef` gap: it does **not** touch the `Error | Cancelled` arm (that
none-valued-`Some` behavior is WP-2's concern, out of scope here — touching it would be scope
creep against the approved Phase 1 boundary).

**Consequence, no code change required:** once `poll_state()` returns `None` for `Expired`,
`AssetRef::get()` (`assets.rs:1985`) falls through to its existing notification-wait loop, whose
`AssetNotificationMessage::Expired` arm (`:2030-2034`) already returns
`Err("Asset expired while waiting for data")`. Since `mark_expired_status()` always sends that
exact notification in the same lock scope that sets `Status::Expired` (`:1922`), the `watch`
channel's last value is already `Expired` by the time a caller subscribes, so `get()` returns the
documented error immediately — it does not need to wait for a new notification. No behavioral
change to `get()` itself; this satisfies the acceptance for `test_assetref_get_does_not_serve_expired_state`
("returns a documented stale/expired error for detached refs").

**Not gated to keyed assets:** `poll_state_any_status`/`get_any_status` on `AssetRef` are harmless
for non-keyed refs too (pure in-memory peek, no store access, no promotion). The keyed-only
restriction lives entirely in the *manager*-level API (`&Key`-typed by construction) and in
`to_override`'s in-store recovery — there is no query-based counterpart.

## Expiration Monitor Fire Logic (`assets.rs:2548-2621`, both `select!` arms)

Replace the direct move (`let asset_ref = timed.asset_ref;`) with:

```rust
let Some(asset_ref) = timed.asset_ref.upgrade() else {
    // Asset already dropped elsewhere (no strong refs remain) — nothing to expire or evict.
    continue;
};
```

`Track` message construction (`:2534`, `:2639`) downgrades at heap-insertion time:
`TimedAsset { expiration: dt, asset_id, asset_ref: asset_ref.downgrade() }`. The
`ExpirationMonitorMessage::Track` payload itself keeps carrying a strong `AssetRef<E>` (the
sender already owns one; downgrading only at insertion keeps the message type unchanged and
avoids touching `schedule_expiration`'s call sites). `active_deadline_by_id` is untouched (already
id-keyed, holds no refs).

## Generic Parameters & Bounds

No new generics. All changes reuse `E: Environment` already in scope throughout `assets.rs`.

## Sync vs Async Decisions

| Function | Async? | Rationale |
|---|---|---|
| `AssetData::poll_state_any_status` | No | Pure in-memory read under an already-held lock, mirrors `poll_state` |
| `AssetRef::poll_state_any_status` / `get_any_status` | Yes | Acquires `RwLock::read()`, mirrors existing `poll_state`/`poll_binary` async wrappers |
| `AssetManager::get_any_status` / `to_override` | Yes | Store I/O (`AsyncStore`) is async; matches every other manager trait method |

## Function Signatures

See "New `AssetData`/`AssetRef` methods" and "Trait Implementations" above — all new function
signatures for this WP are listed there; there is no separate free-function module.

## Integration Points

### Crate: `liquers-core` only

**File:** `liquers-core/src/assets.rs` — all five changes above (monitor struct/logic, `poll_state`
split, new `*_any_status` methods, two new trait methods + `DefaultAssetManager` impl).

**No changes** to `liquers-store`, `liquers-lib`, `liquers-axum`, `liquers-py`: confirmed by
scanning for existing callers of `poll_state`, `get()`, and the `AssetManager` trait outside
`liquers-core` — none match on `Status::Expired` today, so no caller-audit fixes are needed for
this WP (unlike WP-2's broader audit). `cargo check -p liquers-py` still gate-checked in Phase 4
since the trait gains new required methods (any external `impl AssetManager` — none currently
exist outside `liquers-core` — would need updating; there are none today besides
`DefaultAssetManager`, so this is a non-issue until `async-wasm-refactor` lands its own impl).

**Dependencies:** none added.

## Relevant Commands

### New Commands

None.

### Relevant Existing Namespaces

None. This WP is entirely `AssetManager`/`AssetRef` internals; it introduces no
`register_command!` entries and no query-visible behavior beyond "an expired resource is
recomputed instead of served stale," which is already how `~query` evaluation behaves.

**Ask user:** confirm no command or namespace surface (e.g. an `override`/`recover` command
wrapping `to_override`) is wanted in this WP — the current plan treats that as a future, separate
follow-up (would need an axum route + a `liquers-lib` command wrapping the new trait methods).

## Web Endpoints

**None in this WP** (matches the Phase 1-approved crate placement: `liquers-core` only). Exposing
`get_any_status`/`to_override` over `liquers-axum` is a natural follow-up but out of scope here.

## Error Handling

No new error types. Reuses existing constructors:

| Scenario | Constructor |
|---|---|
| `to_override` called for a key with no data-bearing state | `Error::key_not_found(key)` |
| Store read/deserialize failure in `get_any_status`'s store-fallback | propagate via `?` (existing `Error` from `AsyncStore`/`deserialize_from_bytes`) |
| Retry-persist failure inside `to_override` | recorded via existing `record_persistence_result`, not raised as a hard error (matches current `persist_with_status_tracking` behavior — a failed background/foreground save does not fail the caller) |

## Serialization Strategy

Not applicable — no new serializable structs. `to_override`'s whole point is to avoid an extra
serialize/deserialize round trip when the store already holds correct bytes.

## Concurrency Considerations

No new shared state. `get_any_status`/`to_override` read/write through the same `scc::HashMap`
(`self.assets`) and `AsyncStore` the rest of `DefaultAssetManager` already uses, under the same
per-asset `RwLock<AssetData<E>>` (`AssetRef::data`) locking discipline as every other method in
this file. The monitor's weak-ref change strictly *reduces* what it can observe concurrently (an
upgraded ref behaves exactly as before; a failed upgrade is a no-op), so it introduces no new
race window.

## Compilation Validation

- [x] All type signatures specified above
- [x] No new `unwrap()`/`expect()` — `to_override`'s "nothing to promote" case returns `Err`, not
  a panic
- [x] `AssetManager` trait gains two required async methods — the only implementor today,
  `DefaultAssetManager`, is updated in the same PR, so the trait stays object-safe and fully
  implemented; `cargo check -p liquers-py` re-verified in Phase 4 per CLAUDE.md rule (public core
  trait changed)
- [x] Explicit match arms only — `poll_state`'s new `Status::Expired => None` arm keeps every
  other status enumerated explicitly (no `_ =>`)

## References to liquers-patterns.md

- [x] Crate dependencies: `liquers-core` only, one-way flow preserved
- [x] Error handling uses typed constructors (`Error::key_not_found`), not `Error::new`
- [x] Async is default for I/O (store reads/writes); no new sync wrappers needed
- [x] No default match arms introduced
- [x] `AsyncStore` pattern unchanged — no new trait, reuses `get`/`set_metadata`/`set`
