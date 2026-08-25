# Phase 3: Examples & Use-cases - Keyed Asset Registration Semantics

## High-Level Introduction

These conceptual examples make the approved boundary visible. A low-level keyed registration claims empty map reachability and nothing else; it never silently replaces an asset or decides its lifecycle. Public keyed workflows retain their existing responsibility: set_state replaces through its explicit cancellation and dependency path, while to_override promotes the current asset and persists it. The manager-level asynchronous mutation lock orders those workflows, cache installation, and keyed eviction with durable writes.

Example 1 fixes the original queued versus immediate disagreement. Example 2 shows why atomic map insertion alone is insufficient: a delayed to_override persistence must not overwrite a newer set_state result in the store. No query, command, or user-facing API is introduced.

## Example Type

**User choice:** Conceptual code.

The operation is crate-private, not a user workflow. Inline unit tests and the existing expiration integration suite are the canonical executable evidence; a public example would misleadingly advertise an unavailable API.

## Overview Table

| # | Type | Name | Purpose | Drafted By |
|---|---|---|---|---|
| 1 | Example | First keyed claim wins | Defines uniform insert-if-absent semantics and its lifecycle boundary | Primary and unit-test drafters |
| 2 | Example | Promotion cannot persist over replacement | Proves the mutation gate protects the final map and store state | Race and integration-test drafters |
| 3 | Test suite | Built-in manager parity | Exercises the crate-private primitive on both concrete managers | Unit-test drafter |
| 4 | Test suite | Durable mutation serialization | Uses a deterministic gated store with public manager workflows | Integration-test drafter |

## Example 1: First Keyed Claim Wins

### Connection to the High-Level Design

This directly resolves the Phase 1 map-contract mismatch. Duplicate registration has one meaning in the queued and immediate managers, without making a duplicate a replacement event. Its boolean result stays internal because the operation is not part of the AssetManager extension contract.

### Scenario

Two runtime paths construct different AssetRef instances for the same key. The first owns the keyed slot. The second may remain useful as an untracked asset, but it must not displace, expire, cancel, notify, persist, or cause dependent expiration of the first asset. The caller observes the failed claim and keeps the keyed owner unchanged.

### Sequence of Steps

1. An internal manager path creates first for report.txt and calls the concrete manager's crate-private try_insert_key_asset method.
2. The method atomically claims the empty map slot and returns true.
3. A second path creates second for the same key and makes the same call.
4. The atomic map operation returns false; lookup still returns first.
5. Neither primitive call applies a lifecycle transition or store write. A cache-miss or durable-mutation caller holds the mutation gate around its larger workflow.

### Core Example Code

~~~rust
// Inline assets.rs test code, visible to the crate-private inherent method.
let key = parse_key("report.txt")?;
let first = manager.create_asset(key.clone().into());
let second = manager.create_asset(key.clone().into());

assert!(manager.try_insert_key_asset(&key, first.clone()).await);
assert!(!manager.try_insert_key_asset(&key, second.clone()).await);
assert_eq!(manager.lookup_key_asset(&key).map(|asset| asset.id()), Some(first.id()));
~~~

The executable test also asserts that the rejected claim does not change either ref's status or produce a primitive lifecycle side effect. It does not expect a cascade.

### Guide and Executable Example

No guide or standalone example is required. The executable evidence is an inline assets.rs test because try_insert_key_asset is pub(crate) and must not become an external usage pattern.

**Expected result:**

~~~text
first claim succeeds; duplicate claim fails; lookup still returns the first asset
~~~

## Example 2: Promotion Cannot Persist Over a Replacement

### Connection to the High-Level Design

This validates Phase 2's reason for a global asynchronous mutation gate. An atomic map insert alone cannot stop an older operation from awaiting a store write and later making durable state stale. Serialization means the final in-memory owner and persisted bytes represent the later explicit set_state replacement.

### Scenario

race.txt has a recipe-backed asset that has been persisted. It becomes expired and a caller promotes it with to_override. That branch uses set_metadata, not a full value write. A test store pauses exactly that metadata update. While paused, another task starts set_state with replacement value new. The replacement must remain blocked on the mutation gate. Once promotion is released, set_state completes and writes its value and metadata last.

### Sequence of Steps

1. Evaluate a recipe-backed keyed asset for race.txt and confirm PersistenceStatus is Persisted.
2. Expire it, then arm a cloneable ToOverrideGateStore whose set_metadata signals entry and waits for test-controlled release.
3. Spawn manager.to_override for the key and await the store-entry signal.
4. Create a pinned `manager.set_state` future with a serializable `new_state` and poll it once. It must return `Pending`: the registered recipe makes this state an `Override`, and the older promotion owns the metadata gate, so the setter is pending on the manager mutation lock rather than merely not yet scheduled. Its value-write probe must not fire.
5. Release the metadata gate and await both tasks.
6. Fetch the key and assert the current ref is new, has Override status and value new, and the underlying store has new bytes and metadata. The former ref is detached Override, not implicitly expired or cascaded.

### Core Example Code

~~~rust
let old = manager.get(&key).await?;
assert_eq!(old.persistence_status().await, PersistenceStatus::Persisted);
old.expire().await?;

store.arm_metadata_gate();
let promote = tokio::spawn({
    let manager = manager.clone();
    let key = key.clone();
    async move { manager.to_override(&key).await }
});
store.wait_until_metadata_write_started().await;

let mut replace = Box::pin(manager.set_state(&key, new_state));
let is_pending = poll_fn(|cx| {
    Poll::Ready(matches!(replace.as_mut().poll(cx), Poll::Pending))
}).await;
assert!(is_pending, "replacement must wait at the mutation gate");
assert!(!store.new_value_write_started());

store.release_metadata_gate();
promote.await??;
replace.await?;

let current = manager.get(&key).await?;
assert_ne!(current.id(), old.id());
assert_eq!(current.status().await, Status::Override);
assert_eq!(current.get().await?.try_into_string()?, "new");
assert_eq!(store.get(&key).await?.0, b"new");
~~~

The real test uses the controlled store gate and an explicit first poll, not scheduling delays, to establish the interleaving. It runs for DefaultAssetManager and ImmediateAssetManager. The immediate manager needs it too: wasm is single-threaded, but await permits re-entrancy around store persistence.

### Guide and Executable Example

No public guide is needed. Canonical executable evidence belongs beside the existing to_override persistence cases in liquers-core/tests/expiration_integration.rs.

**Expected result:**

~~~text
set_state waits for promotion; after release, the newer state wins in both map and store
~~~

## Corner Cases

### Memory and ownership

- A rejected duplicate claim leaves both AssetRef objects valid. Only the successful ref is reachable by key; there is no implicit drop, cancellation, timer change, or notification.
- `set_state` always runs its existing cancellation/untracking path for the observed old ref before it replaces the map owner. `cancel()` leaves an already terminal `Ready` or `Override` ref unchanged, so that independently held ref is detached rather than implicitly expired; map registration itself must not initiate an expensive dependent-expiration cascade or notification.
- Volatile ownership cleanup, lazy expiry, and monitor expiry re-check current identity while holding the mutation gate, so they cannot remove a newer replacement.

### Concurrency and async execution

- The one manager lock orders external keyed set_binary, set_state, remove, and to_override, plus keyed cache installation and eviction. It intentionally spans store work; ordinary cache hits and query assets do not acquire it.
- ImmediateAssetManager retains its short-lived std mutex map guard and drops it before await. The Tokio mutation lock is the only lock deliberately held across await.
- The duplicate primitive does not take the global gate itself. Its callers take it when a claim is part of cache installation or durable mutation; direct same-crate tests validate only its atomic map contract.

### Errors and persistence

- If set_state cannot claim after its identity-safe removal, it returns the existing keyed general error and performs no durable write. Under normal gated flows this is defensive, not an expected race.
- Failed persistence retains the current in-memory asset contract. The gate prevents an old write from racing a newer mutation; it does not add transactions or change PersistenceStatus semantics.
- Existing persisted, retry, and non-serializable to_override behavior remains unchanged. The gate test targets set_metadata because it is the persisted promotion branch that can become durably stale.

### Serialization and integration boundaries

- No serialized fields, wire format, store trait, query syntax, command, web API, or wasm-specific API is added.
- A successful set_state continues to register one dependency version and hence triggers the existing applicable cascade once. try_insert_key_asset triggers none.
- Store-only recovery remains relevant: a promotion after cache eviction operates on the current reloaded ref, never a detached older ref.

## Documentation and Learning Log

### Guide Candidate Workflows and Examples

There is no end-user or contributor guide workflow. The planned references should link the executable test areas rather than duplicate crate-private code: duplicate-claim tests in assets.rs and the gated durable-ordering integration test.

### Usage and Meaning

For maintainers, map reachability is not lifecycle. try_insert_key_asset means first claimant wins; it does not replace. A future replacement operation must return the displaced ref and explicitly define cancellation, dependency, persistence, and lifecycle policy.

### Repeatable Development Guidance

When adding a keyed map installation or removal, identify whether it can await before store work. If it can, acquire the manager mutation gate and re-check the map condition inside it. Do not add implicit lifecycle behavior to a low-level map primitive.

### Corrections and Unexpected Learning

The concern first appeared to be a HashMap insert versus scc insert difference. Review found the more serious stale-store race between to_override and set_state. Switching the immediate map to scc aligns only one map operation and does not solve durable ordering, so it is intentionally rejected.

## Test Plan

### Unit Tests

**File:** liquers-core/src/assets.rs, inline test module.

- Add try_insert_key_asset_is_insert_if_absent_default and try_insert_key_asset_is_insert_if_absent_immediate. Each creates distinct refs for one key, asserts true then false, and verifies lookup retains the first ref.
- Assert rejected registration has no primitive lifecycle effects: its status and value remain unchanged and it creates no store/dependency effect. Do not rely on watch-notification absence, which is best-effort/coalescing. No cascade is expected.
- Update remove_key_asset_if_respects_id to install the first ref with the new crate-private helper, then use explicit removal and reinsertion for the replacement setup. Remove its issue-specific ambiguity workaround while retaining the identity-safe removal assertion.
- Update test_set_state_replacement_untracks_old_timer to use the new helper instead of direct queued-map insertion, preserving the old-timer suppression assertion.
- Before deleting the trait method, migrate every remaining direct `insert_key_asset` use: the old trait-default `set_state`/`to_override` call sites being replaced by concrete manager workflows, the assets.rs helper and duplicate/removal tests, and the concrete `ImmediateAssetManager` tests in context.rs and interpreter.rs. Those latter tests call the `pub(crate)` inherent `try_insert_key_asset` helper and assert its successful claim.
- Cover the defensive set_state failed-claim branch only with a controllable internal setup that proves error plus no store write. Do not create a timing-based contention test for a state normal locking prevents.

### Integration Tests

**File:** liquers-core/tests/expiration_integration.rs.

- Add test_to_override_and_set_state_are_serialized_default_manager and test_to_override_and_set_state_are_serialized_immediate_manager. Use an AsyncMemoryStore wrapper with an armed, one-shot set_metadata gate and a separate value-write probe. While the promotion owns the metadata gate, explicitly poll a pinned `set_state` future and require `Poll::Pending`; then assert it cannot reach the probe. This proves mutation-lock blocking without relying on task scheduling. Verify final map identity, value, Override metadata, and stored bytes all belong to the new state.
- Place the wrapper with WP3CountingStore and reuse existing recipe-backed, persisted, retry, non-serializable, and store-only recovery fixtures where possible. Gate persisted metadata, not set, because that is the vulnerable to_override write.
- Import and instantiate `ImmediateEnvironment` for the immediate case; the recipe fixture must be registered and the replacement state serializable so the test exercises the `Override` plus durable `set` branch.
- Retain existing integration coverage for one normal replacement dependency cascade and promotion's persisted, retry, and non-serializable branches.

**File:** liquers-core/tests/manager_parametric.rs.

- Keep the public-trait parity suite as the external-contract guard. Add a keyed public set_state then to_override scenario only when its shared fixture can represent both managers without exposing the crate-private helper. It checks observable parity, not internal duplicate registration.

### Manual Validation

~~~powershell
cargo test -p liquers-core --lib assets::tests
cargo test -p liquers-core --test expiration_integration
cargo test -p liquers-core --test manager_parametric
cargo check -p liquers-core --target wasm32-unknown-unknown
~~~

Success requires both managers to preserve the first duplicate claimant, deterministic gated ordering to leave new in memory and store, no regression in expiry or ownership tests, and a successful wasm-target check.

## Auto-Invoke: liquers-unittest Skill Output

Liquers unit-test guidance selects inline tests for crate-private helpers and integration tests for store-visible async ordering. The gate wrapper uses signals and controlled release instead of sleeps, proving the interleaving without relying on thread scheduling. Existing AsyncMemoryStore wrappers are reused instead of adding a public test hook or broad new fixture framework.
