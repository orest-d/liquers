# DOC-03: Assets and Execution Lifecycle

Status: Complete
Last reviewed: 2026-07-26

## Outcome

DOC-03 establishes an API-reference-level description of assets, evaluation
scheduling, reads, notifications, persistence, expiration, recovery, and keyed
mutation.

The primary reference is the module rustdoc in
[`liquers_core::assets`](../../liquers-core/src/assets.rs). It is organized around
public contracts rather than an application tutorial:

- What an asset and `AssetRef` represent
- Which entry points schedule and which evaluate before returning
- How `DefaultAssetManager` and `ImmediateAssetManager` differ
- When assets are reused, fast-tracked, or recreated
- What each read method waits for and exposes
- What watch notifications do and do not guarantee
- When persistence occurs and how its failure is represented
- How expiration, recovery, overrides, cancellation, and removal behave
- Which APIs are consumer-facing and which are framework infrastructure

## Authority and sources

Claims were verified in this order:

1. [`liquers-core/src/assets.rs`](../../liquers-core/src/assets.rs)
2. [`liquers-core/src/context.rs`](../../liquers-core/src/context.rs)
3. [`liquers-core/src/metadata.rs`](../../liquers-core/src/metadata.rs)
4. [`liquers-core/src/interpreter.rs`](../../liquers-core/src/interpreter.rs)
5. Asset, expiration, and failure tests under `liquers-core`
6. [`specs/ASSETS.md`](../ASSETS.md) and
   [`specs/ASSET_LIFECYCLE.md`](../ASSET_LIFECYCLE.md) as supplementary design and
   historical material

The two existing specifications are not authoritative API references. They contain
valuable design context, but also stale source locations, private implementation
entry points, and behavior that is not implemented.

## Concept inventory

| Concept | Primary API | Reference responsibility |
|---|---|---|
| Runtime asset record | `AssetData<E>` | Recipe, state representations, metadata, status, channels |
| Strong handle | `AssetRef<E>` | Waiting, polling, binary reads, notifications, cancellation |
| Weak handle | `WeakAssetRef<E>` | Non-owning identity and upgrade |
| Manager service | `AssetManager<E>` | Evaluation, keyed mutation, recovery, directories |
| Queued manager | `DefaultAssetManager<E>` | Native background queue and expiration monitor |
| Inline manager | `ImmediateAssetManager<E>` | Spawn-free evaluation and lazy expiration |
| Evaluation mode | `EvalMode` | Per-manager queued versus inline contract |
| Job queue | `JobQueue<E>` | Native bounded-concurrency scheduling |
| Lifecycle status | `Status` | Data/finished/processing classification |
| Notifications | `AssetNotificationMessage` | Best-effort watch-channel wake-ups |
| Service messages | `AssetServiceMessage` | Internal reliable lifecycle/log/progress control |
| Persistence outcome | `PersistenceStatus` | Persisted, non-serializable, or failed |
| Expiration | `ExpirationTime`, `AssetRef::expire` | Deadline and invalidation behavior |
| Recovery | `get_any_status`, `to_override` | Explicit keyed stale-value recovery |

## Public entry-point contract

| Entry point | Input state | Payload | Queued manager | Inline manager |
|---|---|---|---|---|
| `EnvRef::evaluate` | Empty | No | Returns a fast-tracked or scheduled handle | Returns after inline evaluation |
| `AssetManager::get_asset` | Empty | No | Fast-track or schedule | Fast-track or evaluate inline |
| `AssetManager::get` | Empty | No | Fast-track or schedule keyed asset | Fast-track or evaluate keyed asset inline |
| `AssetManager::apply` | Supplied | No | Schedule ad-hoc asset | Evaluate ad-hoc asset inline |
| `EnvRef::evaluate_immediately` | Empty | Required by signature | Evaluate before return | Evaluate inline before return |
| `AssetManager::apply_immediately` | Supplied | Optional | Bypass queue and evaluate before return | Evaluate inline before return |

The manager-mode distinction is part of the public contract. “Evaluate” does not
universally mean that the returned future completes only after the value is ready:
with `DefaultAssetManager`, ordinary `evaluate`, `get_asset`, `get`, and `apply`
return a handle after scheduling. Consumers then call `AssetRef::get`, poll, or
subscribe. With `ImmediateAssetManager`, those operations run evaluation in the
caller’s task.

`apply` and `apply_immediately` create ad-hoc assets and do not insert them into the
manager’s key or query maps. Only the immediate path accepts a payload. The
`apply_immediately` computation installs its value but does not persist it.

## Typical lifecycle

The current `State` and its metadata normally move through a short sequence:

```text
None or Recipe -> Submitted -> Processing -> Ready
                                 |
                                 +-> Dependencies -> Processing
                                 +-> Error or Cancelled
```

Queued evaluation uses `Submitted`; inline evaluation may skip it.
`Dependencies` indicates that evaluation is waiting for another asset. Successful
volatile evaluation ends in `Volatile` rather than `Ready`. A `Ready` or
`Override` state may later become `Expired`.

`Source`, `Override`, and `Directory` are terminal states established by store,
override, or directory operations rather than the typical evaluation sequence.
`Partial` is reserved for future intermediate-result functionality and is not
completely implemented.

## Volatility

Volatility resembles an extremely short expiration: a volatile asset is not kept
for reuse, and a later manager request evaluates it again. It differs from
expiration at the point where the value is created. A fresh volatile state is
valid and can be consumed by the dependent asset that requested it; its result
status is `Volatile`, not `Expired`.

Volatility is contagious. It can originate in a query, command, recipe,
immediate-expiration policy, or volatile dependency. An evaluation that depends
on volatile input also becomes volatile and produces a `Volatile` result instead
of a stable cached result.

## Identity, caching, and fast track

Asset identity has three relevant forms:

- A pure key query uses the keyed asset map.
- A non-key query uses the query asset map.
- An apply operation creates an untracked ad-hoc runtime asset.

Non-volatile key and query entries can be reused. Volatile requests create fresh
assets. Manager access treats cached `Expired`, `Error`, and `Cancelled` entries as
misses and creates a fresh asset.

Fast-track loading applies only when the recipe has a key and the asset has no
initial input value. It accepts stored `Ready`, `Source`, and `Override` metadata,
then:

1. Deserializes the stored value
2. Checks known dependency versions
3. Loads dependency records into the dependency manager
4. Installs data, binary, metadata, and stored status
5. Sends a `JobFinished` notification

Other stored statuses, deserialization errors, or stale known dependency versions
reject the fast track and continue with evaluation.

## Status and read contract

`Status` is a classifier, not an enforced transition type.
`AssetData::set_status` updates the status and metadata but does not validate a
transition graph.

### State exposure

| Status | `Status::has_data` | `AssetData::poll_state` |
|---|---:|---|
| `Directory` | No | A no-value state with directory metadata |
| `Partial` | Yes | `None` |
| `Error`, `Cancelled` | No | A no-value state with diagnostic metadata |
| `Ready`, `Source`, `Override`, `Volatile` | Yes | Stored value and metadata, when data exists |
| `Expired` | Yes | `None` |
| Other nonterminal statuses | No | `None` |

`poll_state_any_status` differs only for `Expired`, for which it returns retained
data when present. `try_poll_state` additionally returns `None` when the read lock
cannot be obtained immediately.

`AssetRef::get` does not start evaluation. It first polls, then waits on the watch
channel. Calling it on an unscheduled asset can wait indefinitely. Because
`poll_state` exposes error and cancellation as no-value states, a finalized
evaluation failure may be returned as `Ok(State)` with diagnostic metadata rather
than `Err`.

Binary polling is independent of the normal state-status filter:
`poll_binary` returns a cached binary whenever present. `get_binary` checks that
cache before calling `get`.

## Notification contract

Notifications use `tokio::sync::watch`, not a queue. The channel retains one latest
`AssetNotificationMessage`; a later status, log, or progress update can overwrite
an earlier message before a receiver observes it.

Therefore:

- Notifications are wake-up hints.
- They cannot be counted or replayed as lifecycle events.
- Clients must read status, metadata, or state after waking.
- Correct wait loops subscribe before the decisive state check and recheck before
  awaiting `changed()`.
- `JobFinished` is not the sole authority for success or failure.

The service channel is an unbounded MPSC channel and is intended for internal
reliable control, logging, and progress updates. Once an asset is finished, late
progress and control messages are dropped; one late log message may still be
appended.

## Persistence contract

Queued and ordinary inline evaluation use `evaluate_and_store`:

1. Evaluate the recipe.
2. Install the value and metadata.
3. Set `Ready` or `Volatile`.
4. Publish `ValueProduced`.
5. Attempt serialization and store persistence when a key or `store_to` key exists.
6. Record `PersistenceStatus`.

The default asset data configuration requests background persistence. The queued
manager can therefore expose a ready in-memory value before the store write
finishes. Inline-manager persistence is forced synchronous to preserve its
spawn-free contract.

Persistence failures do not convert a successfully computed value into an
evaluation failure:

- `Persisted`: value and metadata were written.
- `NonSerializable`: serialization is unsupported for the value representation.
- `NotPersisted`: another persistence error occurred.
- `None`: persistence was not attempted or was skipped, including cancellation.

`set_binary` is a store-first keyed operation and does not leave a new in-memory
`AssetRef`. `set_state` creates an in-memory entry and writes data plus metadata, or
metadata only if serialization fails. Both cancel and evict an existing keyed
entry. Except for explicitly supplied `Expired` and `Error`, external values become
`Override` when a recipe exists and `Source` otherwise.

## Expiration, recovery, and cancellation

`AssetRef::expire` accepts `Ready`, `Override`, or already `Expired`:

- `Ready` and `Override` transition to `Expired`.
- `Expired` is idempotent.
- `Source` and other statuses return an error.
- Keyed expiration persists the expired metadata when a store entry exists.
- Public expiration cascades to dependents.

The queued manager monitors finite future deadlines. The immediate manager has no
timer and detects expiration lazily during manager access. Volatile assets finish
as `Volatile` and are not placed in the reusable maps.

Normal manager access does not serve expired cached state. Explicit keyed recovery
uses:

- `AssetManager::get_any_status`: read retained state without evaluation or cache
  registration.
- `AssetManager::to_override`: promote retained keyed state to `Override`.

`AssetRef::cancel` is best-effort and only acts on `Submitted`, `Dependencies`,
`Processing`, and `Partial`. Native cancellation waits up to five seconds for a
terminal notification and returns success on timeout. The cancellation flag guards
later store writes.

`AssetManager::remove` cancels an in-memory keyed asset, removes it from the manager
and dependency manager, and removes stored data. It does not delete the recipe, so
a recipe-backed asset can be requested and evaluated again.

## Public versus infrastructure APIs

Preferred application-facing APIs:

- `EnvRef::evaluate` and `EnvRef::evaluate_immediately`
- `AssetManager::get_asset`, `get`, `apply`, and keyed mutation/recovery methods
- `AssetRef::get`, polling, status, metadata, binary, notification, cancellation,
  expiration, and persistence-status methods

Framework-facing or low-level APIs:

- `AssetData`
- `AssetServiceMessage`
- `MetadataSaver`
- Direct access through public `AssetRef::data`
- `JobQueue`
- Manager lifecycle primitives such as `set_envref`, `dependency_manager`,
  insertion/removal, and expiration tracking

These groups are not enforced by Rust visibility consistently. The rustdoc now
labels the boundary, but a future API pass should narrow or separate it.

## Conflicts and unresolved gaps

| Priority | Gap | Evidence and impact | Recommended action |
|---:|---|---|---|
| P0 | Expired binary reads bypass the normal expired-state policy | `poll_binary` is status-independent and `get_binary` checks it before `get`; stale cached bytes may be returned by a normal binary read | Fix the bug tracked as `ASSET-EXPIRED-CACHED-BINARY-READ` in `specs/ISSUES.md` |
| P0 | Recovery “data-bearing” contract differs between memory and store | Manager `get_any_status` checks `has_data` for store fallback, but delegates to `AssetRef::get_any_status` in memory, where `Error` and `Cancelled` produce no-value states | Define whether recovery returns diagnostic no-value states or only retained values, then test both paths |
| P0 | Mid-execution expired-dependency stale-value branch is unreachable | `wait_for_dependency` handles `Expired` by calling normal `poll_state`, which always hides expired data; its documented `Some(state)` propagation branch cannot run | Use the explicit any-status poll if stale consumption is intended, or remove that policy and test the failure contract |
| P1 | Public trait exposes a private dependency-manager type | Rust warns that `AssetManager::dependency_manager` is public while `DependencyManager` is `pub(crate)`; external implementations cannot name the required return type | Move lifecycle primitives to a sealed/internal trait or make the type intentionally public |
| P1 | Existing asset specifications describe nonexistent partial/checkpoint APIs | `ASSETS.md` presents `Context::set_partial`, `get_partial`, `has_partial`, preview/checkpoint metadata, and transitions that are absent from source | Mark those sections proposed or move them to a design document |
| P1 | Existing lifecycle map uses stale public/private entry points and source lines | `ASSET_LIFECYCLE.md` presents internal `run`, `run_immediately`, and other methods as public entry points and predates the inline manager | Regenerate it from the verified reference or label it historical |
| P1 | `AssetRef::to_override` and manager `to_override` have different safety envelopes | The handle method can manufacture a none-valued override from several data-less states; the manager method is documented as promoting a data-bearing keyed state | Define one promotion invariant and reject states that do not satisfy it |
| P1 | Public `AssetRef::data` allows bypassing lifecycle bookkeeping | External mutation can avoid notifications, persistence status, dependency tracking, or status/metadata synchronization | Make the field private and add targeted extension methods |
| P1 | `AssetRef::get` represents terminal failure as `Ok(State)` | This is tested behavior but surprising to callers expecting `Result::Err` for evaluation failure | Preserve if intentional, but document error-state inspection prominently across evaluation APIs |
| P2 | `Partial` is reserved but incomplete | `Status::Partial` exists and is classified as data-bearing, but `AssetData::poll_state` hides it and no `Context::set_partial/get_partial/has_partial` implementation exists | Keep it documented as future functionality until production and retrieval semantics are implemented |
| P2 | `MetadataSaver::save_immediately` is not immediate on native | It coalesces and throttles writes | Rename to reflect scheduling/coalescing |
| P2 | Job-queue count names overlap or mislead | `running_count` and `pending_jobs_count_sync` both return the running counter; `queued_jobs_count` includes all tracked jobs | Define and rename queue metrics |
| P2 | `cleanup_completed` documentation lists only Ready/Error | Implementation removes every `Status::is_finished` entry | Correct the method reference |

## Coding-agent and human-developer impact

The reference should prevent several high-cost mistakes:

- Awaiting `EnvRef::evaluate` and assuming the value is ready under every manager
- Calling `AssetRef::get` on a manually constructed, unscheduled asset
- Treating watch notifications as a lossless lifecycle log
- Assuming `Ready` implies persistence has completed
- Reusing an expired handle and expecting automatic recomputation
- Using ordinary reads when explicit stale recovery is required
- Treating `set_binary` and `set_state` as equivalent in-memory operations
- Selecting `DefaultAssetManager` for a Wasm or runtime-free context

For humans, the same distinctions make the public surface understandable without
requiring the scheduler’s internal call graph. For coding agents, the entry-point
and read matrices provide exact behavior suitable for API selection and generated
control flow.

## Verification

The following existing tests were used as executable evidence:

- Fast-track status and corrupted-payload tests in `assets::tests`
- Queued and immediate apply tests in `assets::tests`
- Asset failure contract tests
- Expired-state, manager cache-miss, recovery, and override tests in
  `expiration_integration.rs`
- Volatile asset tests in `volatility_integration.rs`
- Job queue capacity, duplicate, claim, cancellation, and cleanup tests

Final verification:

- `cargo test -p liquers-core`: 401 executable tests passed; 2 doctests passed
  and 2 doctests were ignored
- `cargo doc -p liquers-core --no-deps`: passed without rustdoc warnings
- All local Markdown link targets in this analysis and the tracker exist
- `git diff --check` passed for the DOC-03 files

The test build still reports the existing `private_interfaces` warning for
`AssetManager::dependency_manager`; that verified warning is recorded above as an
API-surface gap.
