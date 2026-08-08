# Phase 2: Solution & Architecture - Expired-Safe Binary Reads

## Overview

A single new classifier, `Status::read_exposure()`, becomes the one place that decides what any
read may expose. Both the state family and the binary family derive their behaviour from it, so
they cannot drift apart again — which is the structural form of Phase 1's symmetry rule. Five
`*_binary` methods are added, four existing ones are brought under the classifier, and
`AssetRef::get`/`get_binary` gain a pre-wait expiry check.

No new dependencies, no new value types, no query or command changes. All work lands in
`liquers-core` (`metadata.rs`, `assets.rs`) plus a consumer fix in `liquers-axum`.

## Data Structures

### New Enum: `ReadExposure`

Lives in `liquers-core/src/metadata.rs`, beside `Status` — it classifies `Status`, so it belongs
with it rather than in `assets.rs`.

```rust
/// What a read of an asset in a given [`Status`] is permitted to expose.
///
/// This is the single decision point shared by the state-read family
/// (`poll_state`, `get`, …) and the binary-read family (`poll_binary`,
/// `get_binary`, …). Each family renders the same classification in its own
/// terms; neither re-derives it from `Status` directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadExposure {
    /// A real value is available: `Ready`, `Source`, `Override`, `Volatile`.
    Value,
    /// No value, but metadata is meaningful: `Directory`, `Error`, `Cancelled`.
    /// There is no binary counterpart of a metadata-only state.
    MetadataOnly,
    /// Data is retained but stale. Hidden from normal reads; returned by the
    /// `*_any_status` recovery pair: `Expired`.
    Expired,
    /// Nothing to expose yet. A waiting read blocks; a polling read returns
    /// `None`: `None`, `Recipe`, `Submitted`, `Dependencies`, `Processing`,
    /// `Partial`, `Storing`.
    Pending,
}
```

**Variant semantics and why these four:** they are exactly the distinct outcomes the existing
`poll_state` match already produces (value state / metadata-only state / `None`-because-expired /
`None`-because-not-yet), named instead of implied. Extracting them changes no state-read behaviour;
it makes the binary side expressible.

**No default match arm.** `Status::read_exposure` matches all fifteen `Status` variants
explicitly, and every consumer matches all four `ReadExposure` variants explicitly. A new `Status`
then fails to compile in exactly one place — the classifier — rather than falling silently into a
wrong bucket in eight.

**Derives:** `Debug, Clone, Copy, PartialEq, Eq`. `Copy` because it is a fieldless four-variant
tag. **No `Serialize`/`Deserialize`** — it is a derived classification, never persisted; the
persisted fact is `Status`.

### Not reused: `Status::has_data()`

`has_data()` (`metadata.rs:334`) looks like the needed gate and is not: it returns `true` for
`Expired` **and** for `Partial`, both of which normal reads must hide. It answers "is there a value
in there", which is the right question for `AssetManager::get_any_status`'s store fallback (where
it is already used) and the wrong one for a read gate. `read_exposure` is added alongside it; it
does not replace it, and neither is defined in terms of the other.

### Classification table

| `Status` | `ReadExposure` | `poll_state` today | `poll_binary` today | `poll_binary` after |
|---|---|---|---|---|
| `Ready`, `Source`, `Override`, `Volatile` | `Value` | value state | cached bytes | cached bytes (unchanged) |
| `Directory` | `MetadataOnly` | metadata-only state | **cached bytes** | `None` |
| `Error`, `Cancelled` | `MetadataOnly` | metadata-only state | **cached bytes** | `None` |
| `Expired` | `Expired` | `None` | **cached bytes** ← the bug | `None` |
| `None`, `Recipe`, `Submitted`, `Dependencies`, `Processing`, `Partial`, `Storing` | `Pending` | `None` | **cached bytes** | `None` |

The bug is wider than `Expired` alone: `poll_binary` ignores status entirely, so every row below
the first leaks whatever bytes happen to be cached. In practice `Error` and `fail_asset` clear
`binary`, and `Pending` statuses usually predate it — but "usually" is the whole complaint.

## Function Signatures

### `AssetData` (`liquers-core/src/assets.rs`)

```rust
impl<E: Environment> AssetData<E> {
    /// Unchanged signature; gains an explicit `ReadExposure` match.
    pub fn poll_state(&self) -> Option<State<E::Value>>;

    /// Unchanged.
    pub fn poll_state_any_status(&self) -> Option<State<E::Value>>;

    /// Gated: returns `None` unless exposure is `Value`.
    pub fn poll_binary(&self) -> Option<(Arc<Vec<u8>>, Arc<Metadata>)>;

    /// NEW. Binary twin of `poll_state_any_status`: as `poll_binary`, but also
    /// returns retained bytes when exposure is `Expired`.
    pub fn poll_binary_any_status(&self) -> Option<(Arc<Vec<u8>>, Arc<Metadata>)>;

    /// NEW, `pub(crate)`. Status-blind access to the cached bytes for the
    /// persistence path. Not a read of the asset's exposed value.
    pub(crate) fn binary_unchecked(&self) -> Option<(Arc<Vec<u8>>, Arc<Metadata>)>;
}
```

`binary_unchecked` follows the naming precedent of `State::data_unchecked()` — "you are bypassing a
guard, say so at the call site". Its sole caller is `AssetRef::save_to_store`.

**Why it is needed.** The main evaluation path is safe by construction: `try_to_set_ready()` runs
before `persist_with_status_tracking` (`assets.rs:1853`, with a comment saying persistence depends
on it), so status is `Value`-exposure there. **But that is not true of every path.**
`AssetRef::set_state` (`:2506`) persists at `:2548` with whatever status the supplied state
carries — its own `else` branch logs `"set_state called with non-ready state"` (`:2542`), so a
non-`Value` status is an anticipated case, and it is reachable in production through
`Context::set_state` (`context.rs:789`), not only from tests.

So the read gate and the persistence path must be decoupled on correctness grounds, not merely
tidiness: writing bytes to the store is not a read of the asset's exposed value. Coupling them
would let a future change to the gate silently change what gets persisted.

### `AssetRef` (`liquers-core/src/assets.rs`)

```rust
impl<E: Environment> AssetRef<E> {
    /// CHANGED: pre-wait expiry check. Returns `Err` for `ReadExposure::Expired`
    /// instead of blocking on a notification that will not arrive.
    pub async fn get(&self) -> Result<State<E::Value>, Error>;

    /// CHANGED: consults status before short-circuiting on cached bytes.
    /// `Err` for `Expired` and `MetadataOnly`; waits for `Pending`.
    pub async fn get_binary(&self) -> Result<(Arc<Vec<u8>>, Arc<Metadata>), Error>;

    /// Unchanged signature; inherits the gate from `AssetData`.
    pub async fn poll_binary(&self) -> Option<(Arc<Vec<u8>>, Arc<Metadata>)>;
    pub fn try_poll_binary(&self) -> Option<(Arc<Vec<u8>>, Arc<Metadata>)>;

    /// NEW. Twin of `poll_state_any_status`.
    pub async fn poll_binary_any_status(&self) -> Option<(Arc<Vec<u8>>, Arc<Metadata>)>;

    /// NEW. Twin of `get_any_status`; alias for `poll_binary_any_status`,
    /// matching how `get_any_status` aliases `poll_state_any_status`.
    pub async fn get_binary_any_status(&self) -> Option<(Arc<Vec<u8>>, Arc<Metadata>)>;
}
```

**No `try_poll_binary_any_status`.** The state side has no `try_poll_state_any_status`, and
symmetry is the rule being applied — adding one would break it in the other direction.

## Trait Implementations

No new traits are defined and no new trait is implemented for any type. `ReadExposure` derives its
traits (§Data Structures) and implements nothing by hand. The only trait *change* is one added
method with a default body on the existing `AssetManager` trait.

### Trait: `AssetManager<E>` (`liquers-core/src/assets.rs`)

```rust
#[async_trait]
pub trait AssetManager<E: Environment> {
    /// Existing.
    async fn get_any_status(&self, key: &Key) -> Result<Option<State<E::Value>>, Error>;

    /// NEW, with a default implementation. Recovery-only binary read for a KEYED
    /// asset: returns retained bytes regardless of status, without submitting
    /// evaluation, touching the dependency manager, or re-registering the entry.
    /// `Ok(None)` if the key has no data-bearing bytes in memory or in the store.
    async fn get_binary_any_status(
        &self,
        key: &Key,
    ) -> Result<Option<(Arc<Vec<u8>>, Arc<Metadata>)>, Error>;
}
```

**Default method, not required.** Per the project rule "extend, don't mutate, established traits":
a required method would break both implementors (`DefaultAssetManager`, `ImmediateAssetManager`)
and any downstream implementor, for no gain — the default is expressible from existing primitives
(`lookup_key_asset`, `get_envref`), exactly as `get_any_status` was in PR #11.

**It is strictly cheaper than its state twin.** `get_any_status`'s store fallback already reads
`(binary, metadata)` and then calls `deserialize_stored_value` (`assets.rs:3295`). The binary
counterpart returns after the `store.get` and skips deserialization entirely — no `E::Value`
round-trip, so it also works for a stored type this build cannot deserialize.

**Object safety** is preserved — no generic method parameters, no `Self` by value — though it is
not currently a binding constraint: `AssetManager` is reached through the associated type
`Arc<E::AssetManager>` (`context.rs:176`, `:250`), and there is no `dyn AssetManager` anywhere in
the workspace. The trait carries `#[async_trait]` (`assets.rs:2650`), which boxes the returned
futures and so keeps object safety *available*; the added method does nothing to forfeit it.

**Implementors:** `DefaultAssetManager` and `ImmediateAssetManager` both inherit the default and
need no code. Neither overrides `get_any_status` today, so neither has a reason to override its
binary twin.

## Behaviour Matrix

The contract, stated once. Every cell is derived from `ReadExposure`.

| Method | `Value` | `MetadataOnly` | `Expired` | `Pending` |
|---|---|---|---|---|
| `poll_state` | `Some(value state)` | `Some(metadata-only)` | `None` | `None` |
| `poll_state_any_status` | `Some(value state)` | `Some(metadata-only)` | `Some(value state)` | `None` |
| `try_poll_state` | as `poll_state`, `None` if lock busy | | | |
| `get` | `Ok(value state)` | `Ok(metadata-only)` | **`Err`** | waits |
| `poll_binary` | `Some(bytes)` if cached | `None` | `None` | `None` |
| `poll_binary_any_status` | `Some(bytes)` if cached | `None` | `Some(bytes)` if cached | `None` |
| `try_poll_binary` | as `poll_binary`, `None` if lock busy | | | |
| `get_binary` | `Ok(bytes)`, serializing if needed | **`Err`** | **`Err`** | waits, then as `Value` |

Bold cells are the behaviour changes. `AssetManager::get_binary_any_status` follows
`poll_binary_any_status`, wrapped as `Result<Option<_>, Error>` with the store fallback.

**`get_binary` for `Pending`** waits via `get()` and then re-derives — so a `Pending` asset that
finishes into `Error` yields `Err`, and one that finishes `Ready` yields bytes. This is what makes
"wait" and "the value is unavailable" distinguishable, which the current short-circuit conflates.

## Error Handling

All errors are `liquers_core::error::Error` via typed constructors; no `Error::new`.

| Situation | Constructor | Rationale |
|---|---|---|
| `get`/`get_binary` on `Expired` | `Error::general_error(format!("Asset {} is expired; use get_any_status/get_binary_any_status to read retained data", …))` | Mirrors the existing "Asset expired while waiting for data" wording at `assets.rs:2372`, and names the recovery route |
| `get_binary` on `Error` | the asset's own recorded error, from `State::value_error()` | Phase 1: reuse the recorded failure rather than inventing one; preserves the traceback |
| `get_binary` on `Cancelled` | `Error::general_error("Asset was cancelled; no binary representation".to_owned())` | Cancellation deliberately records no error (`ASSETS.md`, Terminal Outcome Contract), so one must be constructed |
| `get_binary` on `Directory` | `Error::general_error("Asset is a directory; no binary representation".to_owned())` | Same reason |

`Error` and `Cancelled` are **separate match arms**, not a combined one, despite sharing a
`ReadExposure` — they differ in whether an error exists to reuse. This is the one place the four
statuses in `MetadataOnly` must be distinguished, and it is inside `get_binary` only.

## Sync vs Async Decisions

Unchanged throughout — every new method mirrors its state twin's sync/async character:

- `AssetData::poll_binary_any_status` is **sync**: `AssetData` is the inner data, already under a
  lock held by the caller. Matches `poll_state_any_status`.
- `AssetRef::poll_binary_any_status` / `get_binary_any_status` are **async**: they acquire
  `self.data.read().await`. Matches `poll_state_any_status` / `get_any_status`.
- `try_poll_binary` stays **sync**, using `try_read()`. Matches `try_poll_state`.
- `AssetManager::get_binary_any_status` is **async** (`#[async_trait]`): it may hit the store.
- `Status::read_exposure` is **sync and pure** — no I/O, no allocation, no lock.

**No lock held across `.await`** in any new method: each acquires the read lock, derives, and drops
it before returning, as the existing wrappers do.

## Generic Parameters & Bounds

No new generic parameters and no new bounds. Every new method sits in an existing
`impl<E: Environment>` block or on the existing `AssetManager<E>` trait. `ReadExposure` is
non-generic — it depends only on `Status`, so it adds no bound anywhere and `metadata.rs` gains no
dependency on `assets.rs`.

## Integration Points

### `liquers-core/src/metadata.rs`
Add `ReadExposure` and `Status::read_exposure()`. No existing item changes.

### `liquers-core/src/assets.rs`
Rewrite `AssetData::poll_state` and `poll_binary` over `read_exposure`; add
`poll_binary_any_status` and `binary_unchecked`; add the four `AssetRef` methods and the pre-wait
checks in `get`/`get_binary`; point `save_to_store` (`:1944`) at `binary_unchecked`; add the
`AssetManager::get_binary_any_status` default. Update the module-level read-contract table
(`:100-116`), which currently documents the bug as intended behaviour.

### `liquers-axum/src/query/handlers.rs`
Two polling loops (`:61`, `:175`). Each must:
1. Keep using `poll_binary` — now correctly gated.
2. Replace the `_ =>` catch-all status arm — `:109` in the GET loop, `:216` in the POST loop (the
   latter an empty `_ => {}`) — with explicit arms, per the project's no-default-match-arm rule.
   The catch-all currently swallows `Expired` as "still processing", which after the gate becomes
   a 30-second hang instead of stale bytes.
3. Return an error response for `Expired`, per Phase 1 — not a re-request from the manager.

This is the only consumer change in the workspace. `liquers-py`'s `get_binary` is the unrelated
`Cache` trait; `liquers-web` does not use these methods.

### Documentation
`specs/reference/ASSETS.md` §"Status and reads" gains the behaviour matrix; the read-method table
there and in the `assets.rs` module docs both currently state the buggy contract.

## Relevant Commands

**No new commands, and no existing command namespace is involved.** This is core asset-layer
plumbing beneath the command system: no `register_command!` invocation, no change to
`specs/command_registry.yaml`, and no query syntax touched. Nothing to regenerate or validate with
`liquers-validate`.

*User confirmation requested at the Phase 2 gate: is that the right read, or should a recovery
command (e.g. an `any_status` fetch usable from a query) be in scope? Phase 1 scoped this to the
Rust API only.*

## Backward Compatibility

Four behaviour changes are visible to existing callers, all deliberate:

1. `poll_binary`/`try_poll_binary` return `None` where they previously returned bytes, for every
   status except `Value`. **This is the bug fix.**
2. `get_binary` returns `Err` for `Expired`/`MetadataOnly` rather than stale bytes or a hang.
3. `get` returns `Err` for an already-`Expired` asset rather than blocking forever.
4. `AssetManager` gains a method — source-compatible for implementors via the default body.

No public type is removed or renamed; no signature changes. `liquers-py` compiles unaffected (it
implements neither `AssetManager` nor calls these methods).

## Open Questions

1. **Is `AssetRef::get`'s pre-wait expiry check in scope?** Carried from Phase 1 and assumed
   **yes** throughout this document (Behaviour Matrix, change 3 above). It is ~5 lines and removes
   a latent hang identical to the one being fixed on the binary side. If it is scoped out, the
   matrix's `get`/`Expired` cell reverts to "waits forever" and the divergence must be filed as its
   own issue.
2. Should `EXPIRATION-RECOVERY-WEB-API` grow to cover `get_binary_any_status`? Affects that issue's
   scope, not this design's code. (Carried from Phase 1.)
3. Do the axum handlers need the same treatment for `Status::Partial`? It is `Pending`, so the
   loop will keep spinning to timeout — correct today (intermediate reads unsupported), but the
   explicit match will make the choice visible for the first time.
