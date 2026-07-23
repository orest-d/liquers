# Phase 2: Solution & Architecture - expiration-safety

## Overview

No new structs/enums, no new crate dependencies, no command/API surface. Three targeted changes
in `liquers-core/src/assets.rs`: (1) the expiration monitor's heap entry holds a `WeakAssetRef<E>`
instead of a strong ref, (2) `poll_state()` stops serving data for `Status::Expired`, paired with
a new `poll_state_any_status`/`get_any_status` read family that deliberately bypasses that guard,
(3) two new **shared default** methods on the `AssetManager<E>` trait — `get_any_status` and
`to_override` — give keyed assets an explicit, non-evaluating recovery and promote-to-`Override`
path. `rust-best-practices` skill is now installed in this environment (it was not, when this
document was first drafted); its idiom checks (explicit matches, minimal bounds, no new
`unwrap`/`expect`) are reflected below and should be invoked directly for any further revision.

**`async-wasm-refactor` sync note (added after this document's initial approval, before
execution):** `async-wasm-refactor` — an independent, parallel effort making `liquers-core` run in
the browser via wasm — merged into `main` while this branch was open (`liquers-core/src/assets.rs`
alone: +1153/-279 lines). Re-reading the current code after merging main in changed one part of
this document's strategy for the better (Trait Implementations, below) and shifted every line
number cited throughout (all refreshed below against the post-merge file). The underlying
algorithm — what `get_any_status`/`to_override` actually do — is unchanged.

**Naming (revised from the initial `_also_expired` draft, per user feedback):**
- The read method is named `get_any_status`/`poll_state_any_status`, following the existing
  `_unchecked` convention already used in this file (`State::data_unchecked()`, and the wider Rust
  convention `Vec::get_unchecked`) for "bypasses a normal guard" — here, the guard is "not
  expired." `_any_status` was chosen over `_unchecked` because it is self-describing without the
  unsafe-adjacent connotation `_unchecked` carries in Rust.
- The promote method is named `to_override`, reusing the verb already established by
  `AssetRef::to_override()` (`assets.rs:1932`). This also corrects a scoping error in the initial
  draft: promoting to `Override` is **not** Expired-specific — a `Ready` asset can be pinned to
  `Override` the same way, exactly as `AssetRef::to_override()` already does for every data-bearing
  status. The manager-level method must therefore not be named after `Expired` at all.

## Data Structures

### Modified: `TimedAsset<E>` (`assets.rs:2969`)

```rust
struct TimedAsset<E: Environment> {
    expiration: chrono::DateTime<chrono::Utc>,
    asset_id: u64,
    asset_ref: WeakAssetRef<E>,   // was: AssetRef<E>
}
```

**Ownership rationale:** `WeakAssetRef<E>` already exists (`assets.rs:746`, `.downgrade()` at
`:988`) and is used elsewhere for non-retaining tracking (`DependencyManager`'s expired-asset
list). The monitor must hold no strong references (WP-3 item 1) so a dropped/evicted asset's
memory isn't kept alive purely by a pending timer. `Ord`/`Eq`/`PartialOrd`/`PartialEq` impls are
unaffected — they compare only `expiration`/`asset_id`. This struct, `ExpirationMonitorMessage`,
and `DefaultAssetManager` itself are now `#[cfg(not(target_arch = "wasm32"))]`-gated (a
consequence of `async-wasm-refactor`, unrelated to this WP) — the monitor is native-only, which
was already implicitly true (it uses `tokio::spawn`/timers unavailable on wasm) and needs no
change here.

**No serialization impact:** `TimedAsset` is monitor-internal, never (de)serialized.

### New Enums / New Structs

None required.

### ExtValue Extensions

Not applicable.

## Trait Implementations

### Trait: `AssetManager<E>` (`assets.rs:2383`)

Two new **shared default** methods (revised from an initial "required, no default" draft — see
"Why shared default, not per-manager" below):

```rust
/// Recovery-only read for a KEYED asset: returns its state regardless of status — including
/// `Status::Expired` — without submitting evaluation, without touching the dependency manager,
/// and without registering the entry back into the manager's normal in-memory cache.
/// `Ok(None)` if the key has no data-bearing state (in memory or in the store).
async fn get_any_status(&self, key: &Key) -> Result<Option<State<E::Value>>, Error> {
    /* default body — see algorithm below */
}

/// Pin a KEYED asset's current value (whatever status it is in — `Ready`, `Expired`, etc.) as
/// `Status::Override`, preserving the value without recomputation. Errors if there is no
/// data-bearing state for `key` to promote (in memory or in the store).
async fn to_override(&self, key: &Key) -> Result<(), Error> {
    /* default body — see algorithm below */
}
```

**Bounds:** None beyond the trait's existing `crate::maybe_send::MaybeSend + MaybeSync` (the
`async-wasm-refactor`-introduced relaxation of `Send + Sync` for wasm compatibility — both new
methods are built entirely from other already-compatible trait primitives, so they inherit this
bound automatically without any extra work).

**Why shared default, not per-manager (revised from the Phase 1 answer, after `async-wasm-refactor`
landed):** the original reasoning — "these are required because no generic primitive exists to
inspect a manager's internal storage, so a default would force double-serialization" — no longer
holds. `async-wasm-refactor` added exactly such primitives to this trait as part of hoisting most
manager logic (`remove`, `set_state`, `set_binary`, `contains`, `keys`, `listdir*`, ...) into
shared default methods: `fn lookup_key_asset(&self, key: &Key) -> Option<AssetRef<E>>` (sync,
brief), `async fn insert_key_asset(&self, key: &Key, asset: AssetRef<E>)`, and
`fn get_envref(&self) -> EnvRef<E>` (all required *primitives*, implemented once per manager;
`assets.rs:2865-2918` for the full primitive list). `get_any_status`/`to_override` are expressible
purely in terms of these primitives plus already-existing `AssetRef` methods
(`get_any_status`/`to_override`/`persistence_status`) — exactly the same pattern this refactor
already used for every other manager-facing capability. Writing them as **one shared default
method each** (not two per-manager copies) means: less code, `ImmediateAssetManager` gets the
capability automatically with zero extra work, and no future third manager needs to reimplement
the `PersistenceStatus` branching logic either. This does **not** reintroduce the
double-serialization risk the original reasoning worried about: the default body below calls
`store.set_metadata`/`store.set` directly (via `get_envref().get_async_store()`), the same way
`DefaultAssetManager`'s own overridden `set_state`/`set_binary` do — it is not built by piping
`get_any_status` into the generic `set_state` default (which *would* force re-serialization); it
implements the `PersistenceStatus` branch explicitly, same as originally designed.

**Neither existing manager needs to override the default.** `DefaultAssetManager`'s
`lookup_key_asset` delegates to `self.assets.read_sync(...)` (`scc::HashMap`, already efficient)
and its `insert_key_asset` to `self.assets.insert_async(...)` — exactly what a hand-written
`DefaultAssetManager`-specific version would do anyway, so there is no performance reason to
override. `ImmediateAssetManager` uses the same primitives against its own `std::sync::Mutex`-based
maps. (`DefaultAssetManager` *does* override several other trait defaults — `get`, `remove`,
`set_binary`, `set_state` — with its own `scc`-direct versions for reasons unrelated to this WP;
`get_any_status`/`to_override` are not in that list and don't need to be.)

### Shared default-method implementation (algorithm, bodies in Phase 4)

Works identically for `DefaultAssetManager` and `ImmediateAssetManager` (and any future manager)
without either overriding it.

**`get_any_status(key)`:**
1. If `self.lookup_key_asset(key)` finds an in-memory entry, return
   `Ok(asset_ref.get_any_status().await)` (new `AssetRef` method below) — covers `Expired` and
   every other data-bearing status.
2. Else, read directly from the store (`self.get_envref().get_async_store()`) bypassing
   `try_fast_track`'s status allow-list; if the stored `Metadata::status().has_data()` (existing
   helper, `metadata.rs:332`, already `true` for `Expired`), deserialize using the same
   binary/type-identifier logic `try_fast_track` uses (`assets.rs:509-527`) and return
   `Some(state)`. Deliberately skip dependency-manager version registration and re-insertion via
   `insert_key_asset` — this path must have no side effects on normal evaluation (WP-3 item 4).
3. Otherwise `Ok(None)`.

**`to_override(key)`:** not Expired-specific — pins whatever current value exists (`Ready`,
`Expired`, `Partial`, etc.) as `Override`, mirroring `AssetRef::to_override()`'s own status
coverage:
1. If `self.lookup_key_asset(key)` finds an in-memory entry:
   - call the existing `AssetRef::to_override()` (`assets.rs:1932`, already handles every
     data-bearing status — including plain `Ready`, not just `Expired` — while preserving the
     value) to flip in-memory state;
   - branch on the existing `AssetRef::persistence_status()` (`PersistenceStatus`,
     `assets.rs:142`) recorded from the *original* evaluation:
     - `Persisted` — the store already holds the correct bytes; call
       `self.get_envref().get_async_store().set_metadata(key, &metadata)` only (status field
       updated to `Override`), no re-serialization.
     - `NonSerializable` — nothing was ever stored for this value; no store write at all.
     - `NotPersisted` | `None` — the original save failed or was never attempted; retry a full
       persist (serialize + `store.set(key, &binary, &metadata)`), updating
       `persistence_status` via the existing `record_persistence_result` path.
   - the entry is already in the manager's map via `lookup_key_asset` (or re-`insert_key_asset` if
     a race with the monitor/lazy-expiry check already evicted it) so a subsequent normal
     `get(key)` sees it immediately.
2. Else (no in-memory entry — e.g. already evicted after expiring): load the persisted state from
   the store exactly as `get_any_status`'s store-fallback does (only `Ready`/`Source`/`Override`/
   `Expired` are ever persisted, so this is the only status set reachable here); since
   deserialization succeeded, the bytes are provably already persisted, so rewrite **only** the
   metadata `status` field to `Override` via `store.set_metadata`. No in-memory `AssetRef` is
   created — the existing fast-track allow-list already includes `Override` (`assets.rs:498`), so
   the next normal `get(key)` loads it back into memory on its own.
3. If neither exists, `Err(Error::key_not_found(key))`.

This directly implements the user's Q2 answer: consistent end state, no double-serialization,
`PersistenceStatus` drives the retry-vs-skip-vs-metadata-only branch — generalized beyond
`Expired` to any current value, and now shared by every `AssetManager` implementor for free.

## New `AssetData`/`AssetRef` methods

```rust
// AssetData (sync, assets.rs, next to poll_state at :619)
pub fn poll_state_any_status(&self) -> Option<State<E::Value>> {
    match self.status {
        Status::Expired => { /* same construction as today's Expired arm of poll_state */ }
        _ => self.poll_state(),
    }
}

// AssetRef (async wrapper, assets.rs, next to poll_state at :2174)
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

**Modified `AssetData::poll_state()` (`assets.rs:619`):** move `Status::Expired` out of the
`Ready | Expired | Source | Override | Volatile` value-returning arm into its own arm returning
`None`. `Ready | Source | Override | Volatile` keep returning data unchanged. This is the minimal
fix for WP-3 item 2's `AssetRef` gap: it does **not** touch the `Error | Cancelled` arm (that
none-valued-`Some` behavior is WP-2's concern, out of scope here — touching it would be scope
creep against the approved Phase 1 boundary).

**Consequence, no code change required:** once `poll_state()` returns `None` for `Expired`,
`AssetRef::get()` (`assets.rs:2084`) falls through to its existing notification-wait loop, whose
`AssetNotificationMessage::Expired` arm (`:2129`) already returns
`Err("Asset expired while waiting for data")`. Since `mark_expired_status()` always sends that
exact notification in the same lock scope that sets `Status::Expired` (`:2021`), the `watch`
channel's last value is already `Expired` by the time a caller subscribes, so `get()` returns the
documented error immediately — it does not need to wait for a new notification. No behavioral
change to `get()` itself; this satisfies the acceptance for `test_assetref_get_does_not_serve_expired_state`
("returns a documented stale/expired error for detached refs"). This reasoning is unaffected by
`ImmediateAssetManager`'s lazy on-access expiry check (it still transitions status via the same
`AssetRef::expire()`/`expire_without_cascade()` path, so the same notification is sent).

**Not gated to keyed assets:** `poll_state_any_status`/`get_any_status` on `AssetRef` are harmless
for non-keyed refs too (pure in-memory peek, no store access, no promotion). The keyed-only
restriction lives entirely in the *manager*-level API (`&Key`-typed by construction) and in
`to_override`'s in-store recovery — there is no query-based counterpart.

## Expiration Monitor Fire Logic (`assets.rs:3100-3200`, both `select!` arms)

Replace the direct move (`let asset_ref = timed.asset_ref;`, `:3142`) with:

```rust
let Some(asset_ref) = timed.asset_ref.upgrade() else {
    // Asset already dropped elsewhere (no strong refs remain) — nothing to expire or evict.
    continue;
};
```

`Track` message construction (`:3112`, `:3217`) downgrades at heap-insertion time:
`TimedAsset { expiration: dt, asset_id, asset_ref: asset_ref.downgrade() }`. The
`ExpirationMonitorMessage::Track` payload itself keeps carrying a strong `AssetRef<E>` (the
sender already owns one; downgrading only at insertion keeps the message type unchanged and
avoids touching `schedule_expiration`'s call sites). `active_deadline_by_id` is untouched (already
id-keyed, holds no refs). This monitor is exclusive to `DefaultAssetManager`
(`#[cfg(not(target_arch = "wasm32"))]`); `ImmediateAssetManager` has no equivalent monitor at all —
it checks `AssetRef::is_expired()` lazily on access instead (`assets.rs:4838` onward), so this fix
is not needed there.

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

**File:** `liquers-core/src/assets.rs` — all changes above (monitor struct/logic, `poll_state`
split, new `*_any_status` methods, two new shared-default trait methods).

**No changes** to `liquers-store`, `liquers-lib`, `liquers-axum`, `liquers-py`: confirmed by
scanning for existing callers of `poll_state`, `get()`, and the `AssetManager` trait outside
`liquers-core` — none match on `Status::Expired` today, so no caller-audit fixes are needed for
this WP (unlike WP-2's broader audit). `cargo check -p liquers-py` still gate-checked in Phase 4
since the trait gains new methods (public core trait changed, per CLAUDE.md rule) — but because
they are **shared defaults**, not required methods, `AssetManager<E>`'s two existing implementors
(`DefaultAssetManager`, `ImmediateAssetManager`, confirmed to be the only two in the codebase as of
the `async-wasm-refactor` merge) compile unchanged; neither needs to be touched.

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

No new shared state. `get_any_status`/`to_override` read/write through whatever key→asset map and
`AsyncStore` each manager already exposes via `lookup_key_asset`/`insert_key_asset`/`get_envref`
(`scc::HashMap` for `DefaultAssetManager`, `std::sync::Mutex<HashMap>` for `ImmediateAssetManager`),
under the same per-asset `RwLock<AssetData<E>>` (`AssetRef::data`) locking discipline as every
other method in this file. The monitor's weak-ref change strictly *reduces* what it can observe
concurrently (an upgraded ref behaves exactly as before; a failed upgrade is a no-op), so it
introduces no new race window.

## Compilation Validation

- [x] All type signatures specified above
- [x] No new `unwrap()`/`expect()` — `to_override`'s "nothing to promote" case returns `Err`, not
  a panic
- [x] `AssetManager` trait gains two new methods with **default bodies** — both existing
  implementors (`DefaultAssetManager`, `ImmediateAssetManager`) compile unchanged, no per-manager
  edits required; `cargo check -p liquers-py` re-verified in Phase 4 per CLAUDE.md rule (public
  core trait changed) as a formality, not because it's expected to break
- [x] Explicit match arms only — `poll_state`'s new `Status::Expired => None` arm keeps every
  other status enumerated explicitly (no `_ =>`)
- [x] New default methods respect the trait's `MaybeSend + MaybeSync` bound (built purely from
  other already-compatible primitives — `lookup_key_asset`, `get_envref`, `insert_key_asset`,
  `AssetRef` methods — so this is automatic, not something requiring extra cfg-gating)

## References to liquers-patterns.md

- [x] Crate dependencies: `liquers-core` only, one-way flow preserved
- [x] Error handling uses typed constructors (`Error::key_not_found`), not `Error::new`
- [x] Async is default for I/O (store reads/writes); no new sync wrappers needed
- [x] No default match arms introduced
- [x] `AsyncStore` pattern unchanged — no new trait, reuses `get`/`set_metadata`/`set`
