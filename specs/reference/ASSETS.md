---
title: Assets Specification
kind: reference
audience: internal
area: [core/assets]
reviewed: 2026-10-09
---
# Assets Specification

## Overview

Assets represent the third (outermost) layer of value encapsulation in Liquers:
- **Layer 1: Value** - The actual data and its type (enum of supported types)
- **Layer 2: State** - A value with its metadata (status, type, logs, etc.)
- **Layer 3: Asset** - A state that may be ready, queued, being produced, or producible on demand

Assets provide:
- Access to data, metadata, and binary representation
- Progress updates during computation
- Lifecycle management (creation, evaluation, caching, invalidation)
- **Concurrent access control** via `RwLock` - multiple readers or single writer

### Concurrency Model

Assets provide high-level concurrency control on top of low-level store access:

| Layer | Mechanism | Responsibility |
|-------|-----------|----------------|
| **Asset** | `RwLock<AssetData>` | Concurrent access to in-memory state; multiple readers OR single writer |
| **Store** | File locking / atomicity | Atomic data+metadata writes; no high-level coordination |

- **AssetRef** (`Arc<RwLock<AssetData>>`) ensures safe concurrent access to asset state
- **Store** only guarantees atomicity of individual `set(key, data, metadata)` operations
- Store should rely on file locking for its atomicity guarantees
- High-level coordination (e.g., preventing concurrent computations) is handled at Asset layer, not Store layer

## Core Structures

### AssetData
Internal structure holding the actual asset state:
- `id`: Unique identifier
- `recipe`: How to compute the asset (Recipe)
- `data`: Optional computed value (`Arc<Value>`)
- `binary`: Optional serialized representation (`Arc<Vec<u8>>`)
- `metadata`: Metadata record
- `status`: Current Status
- `initial_state`: Starting state for computation
- Service channel (mpsc): For internal control messages
- Notification channel (watch): For client notifications

### AssetRef
A clonable reference to AssetData: `Arc<RwLock<AssetData>>`
- Multiple references can exist to the same asset
- Provides async API for asset operations

### AssetManager
Manages asset lifecycle:
- `assets`: Map of Key -> AssetRef (non-volatile key assets)
- `query_assets`: Map of Query -> AssetRef (non-volatile query assets)
- `job_queue`: Queue for asset evaluation

`AssetManager` is a public trait that can be implemented **outside `liquers-core`**. Its two
supertraits are public: `DependencyManagerAccess` (return the manager's one `DependencyManager`,
an opaque type built with `DependencyManager::new()`) and `KeyMutationAccess` (one
`tokio::sync::Mutex<()>` per manager, never taken while an asset's `data` lock is held). The
lifecycle work is in provided methods — among them `record_expiry`, `publish_version`,
`expire_dependencies_result`, `cascade_expire_dependents`, `apply_external_change` — and three
policy accessors (`dependency_audit_policy`, `version_verification`, `external_change_policy`)
default to `Explicit`, `OnRead` and `UserInput`; a manager that is configurable overrides them.
Among the provided methods are the dependency checks: `stored_dependency_state` (the
stored-records walk), `trigger_dependency_audit` (per key, over the upstream closure),
`trigger_dependency_audit_all_registered` and `trigger_dependency_audit_store` (every stored
value, typically on start). What each consistency policy guarantees is in
[`DEPENDENCIES_STATUS.md` §Consistency policies](DEPENDENCIES_STATUS.md#consistency-policies). How
to implement and test one: [`ASSET_MANAGER_IMPLEMENTATION_GUIDE.md`](../guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md).

#### Key ownership

An asset registered in `assets` under a key is that key's **owner**. Ownership decides what
`AssetRef::evaluate_recipe` does with a keyed recipe: the owner resolves and evaluates the recipe,
and any other asset holding the same key recipe delegates to the owner.

The question is asked through `AssetManager::owned_key_asset(&key) -> Option<AssetRef>`, which is
**non-evaluating** — it reads the map and never starts, submits, fast-tracks or resolves an
evaluation. That is not an optimization but a requirement: `evaluate_recipe` asks about the key it
is itself evaluating, so an evaluating answer re-enters the asset that is asking. Asking
`AssetManager::get` instead recursed until the wasm stack was exhausted
(`CORE-IMMEDIATE-MANAGER-KEYED-RECURSION`).

`None` means no asset is registered, and the caller therefore owns the recipe. Three ways to get
there: the key is volatile, its recipe declares `cached: false` (see below), or the asset was built
outside the manager's maps (`apply`, `create_asset`).

#### Volatile assets are never owned

A volatile asset cannot be shared and cannot be reused, so the maps never *serve* one:

- neither map is consulted when resolving a volatile key or query — `get_volatile_resource_asset`,
  `get_volatile_query_asset` and `make_volatile` all mint a fresh asset;
- `owned_key_asset` drops a volatile entry it finds and reports no owner;
- `Status::Volatile` is a stale-terminal state alongside `Expired`, `Error` and `Cancelled` in
  `get`, `get_asset` and `get_dependency_asset`, so a cached volatile asset is evicted and rebuilt.

The last point matters because volatility is not always known at registration time: registration
consults the recipe, but `try_to_set_ready` also marks a result volatile from a metadata expiry set
by a command *during* evaluation. Without the eviction, such an asset is served from cache forever
— `Status::Volatile.is_finished()` is `true` and the expiry re-check only fires for
`Status::Ready`.

Eviction happens on **entry**, so the caller whose request produced a volatile value still receives
it. Used once, never reused.

A volatile value is still persisted, with `Status::Volatile` in its metadata. It is written as an
opportunity for the user to override, not as a value to read back: `try_fast_track` accepts only a
stored `Ready`, `Source` or `Override`.

#### `stored` and `cached`: opting out of the write and of reuse

Two recipe flags, `stored` and `cached`, are `Option<bool>` on `Recipe`, `MetadataRecord` and
`AssetInfo`, absent unless set; the accessors `stored()` and `cached()` (also on `Metadata`, where
legacy metadata answers `true`) read an absent flag as `true`. `get_resource_asset` in both
`DefaultAssetManager` and `ImmediateAssetManager` copies them from the key's recipe into the new
asset's metadata **before** anything can persist it, and `evaluate` re-copies them from the
provider's recipe when it adopts it.

| Flag, when `false` | What it skips | What it does not change |
|---|---|---|
| `stored` | Every store write for the key: `save_to_store` (value and metadata), the `MetadataSaver`'s status and progress writes, both native and wasm — no metadata-only entry is left either | A stored copy that already exists is still read, and fast-tracked in preference to recomputation: it may be `Override` data |
| `cached` | Registration: the manager mints a fresh asset per request (`get_uncached_resource_asset`, or the equivalent branch in the immediate manager) and never inserts it in `assets`, so it is evaluated for the request and dropped | The asset is **still the key's node in the dependency graph** — see below |

**Neither flag makes an asset volatile**, alone or together. Volatility is contagious and says the
result is single-use; these flags are about disk and memory, not purity, so a dependent of a
`cached: false` key is not volatile and is cached normally. Volatility is decided before either
flag is consulted.

**An uncached keyed asset stays the key's graph node.** `AssetRef::bound_owner_key` answers the key
for an asset that is constructed for it, not volatile, whose recipe targets the key and declares
`cached: false`, **when no other asset is registered** for the key — so its dependencies are
recorded and its version registered like a registered owner's, and a change upstream expires its
dependents. A registered owner, when one exists, stays the only answer, which keeps a delegating
asset answering `None`. Because no registered asset holds such a key, `expire_dependencies_result`
expires the key **in the store**: the stored metadata of a `Ready` or `Override` copy is rewritten
as `Expired`, so a fresh process does not fast-track data the graph knows is stale. The
non-registered-owner warning a keyed write normally records is skipped for an uncached asset, as for
a volatile one. The race between that store expiry and an evaluation already in flight is
`UNCACHED-STORED-COPY-EXPIRY-RACES-AN-INFLIGHT-EVALUATION`.

An explicit install decides for itself: `set_state` and `set_binary` read the **supplied**
metadata's `stored` flag, not the recipe's, so a caller may write a `stored: false` key by supplying
metadata that says `stored: true` — and one supplying `stored: false` gets no store write, while
the rest of the operation (the in-memory entry `set_state` creates, the version registration and the
cascade to dependents) proceeds.

## Communication Channels

### Service Channel (mpsc, reliable)
Used for internal control flow. Messages must not be dropped.

```rust
pub enum AssetServiceMessage {
    JobSubmitted,                      // Asset queued for processing
    JobStarted,                        // Processing has begun
    LogMessage(LogEntry),              // Log entry from computation
    UpdatePrimaryProgress(ProgressEntry),
    UpdateSecondaryProgress(ProgressEntry),
    Cancel,                            // Request cancellation (sets the request; never a status)
    ErrorOccurred(Error),              // Error during processing
    JobFinishing,                      // About to finish (housekeeping)
    JobFinished,                       // Processing complete
}
```

### Notification Channel (watch, best-effort)
Used to notify clients. Missing notifications are acceptable since clients can query current state.

```rust
pub enum AssetNotificationMessage {
    Initial,                           // Initial state when created
    JobSubmitted,                      // Asset was queued
    JobStarted,                        // Processing started
    StatusChanged(Status),             // Status transition occurred
    ValueProduced,                     // New value is available
    ErrorOccurred(Error),              // Error occurred
    LogMessage,                        // New log entry
    PrimaryProgressUpdated(ProgressEntry),
    SecondaryProgressUpdated(ProgressEntry),
    JobFinished,                       // Processing complete
    Expired,                           // The asset expired
    Removed,                           // The manager removed or replaced the asset (terminal)
}
```

`Removed` is sent by `AssetManager::remove`, `set_binary` and `set_state` to the live asset they
unmap: after `cancel()`, after the unmap, and after the key's new state is written, so it is the
asset's last message and an observer re-reading the key sees what the removal or write left. It is
not sent by expiry (`remove_expired_from_maps`), which has already announced `Expired`. Waiters treat
it as terminal: `wait_to_finish` and the wait inside `cancel` return, and `AssetRef::get` re-polls
the state and errors only if there is none ("removed while waiting").

**Note on watch channel**: The notification channel uses `watch` which only keeps the latest value. Intermediate notifications may be lost (e.g., multiple LogMessages). This is acceptable - clients should poll for full state if needed, notifications are hints only.

## Status Enum

```rust
pub enum Status {
    None,           // Initial/unknown state
    Directory,      // Represents a directory
    Recipe,         // Has recipe but no data yet
    Submitted,      // Queued for processing
    Dependencies,   // Waiting for dependencies (set by interpreter)
    Processing,     // Currently being computed
    Partial,        // Processing with preview/partial results available
    Error,          // Finished with error
    Storing,        // Being written to store (transient)
    Ready,          // Successfully computed and available
    Expired,        // Was ready but no longer valid
    Cancelled,      // Processing was cancelled
    Source,         // Data provided externally (no recipe)
    Override,       // Data overrides recipe (NEW from ASSET_SET_OPERATION)
}
```

### Status Descriptions

- **None**: Initial state when AssetData is created, before any processing
- **Directory**: Special status for directory assets (containers)
- **Recipe**: Asset has a recipe but no computed data yet; ready for computation
- **Submitted**: Asset is queued in JobQueue, waiting for capacity
- **Dependencies**: Asset is waiting for its dependencies to be evaluated (set by interpreter)
- **Processing**: Asset computation is actively running
- **Partial**: Processing with intermediate results available; used for both:
  - **Preview mode**: Quick low-quality result while full computation continues
  - **Checkpointing**: Saving intermediate state for recovery in long computations
- **Error**: Computation finished with an error
- **Storing**: Transient state during store write; if loaded from store with this status, treat as corrupted/Error
- **Ready**: Successfully computed, data available
- **Expired**: The data is stale — see §The one meaning of `Expired` below
- **Cancelled**: The evaluation was interrupted before it produced a value: its own cancel was
  requested, its command returned an `ErrorType::Cancelled` error, a dependency it waited for was
  cancelled (cascade), or it was replaced while running. The cancellation error is recorded in
  `error_data` (its `query` names the asset whose cancel was requested); `is_error` stays false
- **Source**: Data provided externally via set(), no recipe exists
- **Override**: Data provided externally via set(), recipe exists but was not used

### Status Properties

| Status       | has_data | is_finished | is_processing | can_have_deps | read_exposure |
|--------------|----------|-------------|---------------|---------------|---------------|
| None         | false    | false       | false         | false         | Pending       |
| Directory    | false    | true        | false         | false         | MetadataOnly  |
| Recipe       | false    | false       | false         | false         | Pending       |
| Submitted    | false    | false       | false         | false         | Pending       |
| Dependencies | false    | false       | false         | false         | Pending       |
| Processing   | false    | false       | true          | false         | Pending       |
| Partial      | true     | false       | true          | true          | Pending       |
| Error        | false    | true        | false         | false         | MetadataOnly  |
| Storing      | false    | false       | false         | true          | Pending       |
| Ready        | true     | true        | false         | true          | Value         |
| Expired      | true     | true        | false         | false         | Expired       |
| Cancelled    | false    | true        | false         | false         | MetadataOnly  |
| Source       | true     | true        | false         | false         | Value         |
| Override     | true     | true        | false         | false         | Value         |

### The one meaning of `Expired`

**`Expired` means one thing: the data is stale.** It never means anything else, and no read, store
or client should branch on how an asset came to hold it.

Two provenances reach it, and they are provenance, not meaning:

| Provenance | How it arises |
|---|---|
| Data that *was* valid and now is not | a dependency changed, an expiration time passed, `expire()` was called |
| An execution that never produced valid data | the evaluation consumed a dependency that expired mid-run and used its retained value rather than recomputing it (§Status and reads, below) |

The second gets exceptional handling on the *evaluation* path — the run finishes on the stale value
instead of restarting, which is what prevents an unbounded recompute loop — but that exception is
about how the run ends, not about what the resulting status means. The asset is stale either way.

This is why the second case is *not* a distinct status. A `Stale` variant would split one meaning
across two values, oblige roughly sixty-five `match` sites to handle both identically, break the
language bindings, and break forward compatibility for every store holding a status string a
previous build wrote. Provenance that is worth recording belongs in metadata beside the status,
where reading it is opt-in and ignoring it is free — which is what `ExpiryReason` is.

### Why an asset is `Expired`: `ExpiryReason`

`MetadataRecord.expiry_reason` (also on `AssetInfo`; `liquers-core/src/metadata.rs`) records why.
It is **recorded, never consulted**: no read path branches on it. It is meaningful only while the
status is `Expired` — `Metadata::expiry_reason()` returns `None` otherwise, and setting any other
status clears it — and is omitted from serialized metadata when absent, so older records load.

```rust,ignore
enum ExpiryReason {               // serde: tag "scope", snake_case
    Direct   { cause: ExpiryCause },                                    // this asset is the root
    Cascaded { cause: ExpiryCause, root: DependencyKey, via: DependencyKey },
}
```

`root` is the key the cause happened to; `via` is this asset's own direct dependency through which
the cascade reached it (equal to `root` for a direct dependent). The seven causes (serde tag
`kind`), the route that sets each, and its log level:

| `ExpiryCause` | Route | Root gets | Dependents get | Level |
|---|---|---|---|---|
| `Deadline { expiration_time }` | queued manager's expiration monitor; immediate manager's lazy check on `get` / `get_asset` | `Direct` | `Cascaded` (both managers: once the lazy check finds the deadline passed, it cascades as the monitor does) | Info |
| `Explicit` | `AssetRef::expire`, `AssetManager::expire(key)` (live or stored-only) | `Direct` | `Cascaded` | Info |
| `Audit { found }` | `trigger_dependency_audit*` in `AuditMode::Expire`; `found` is the current version, 0 when none | not expired | `Cascaded` | Warning |
| `StaleDependency { dependency }` | an evaluation that waited (through `wait_for_dependency`) on a dependency that expired meanwhile, or that recorded its edge against a dependency version the map had already replaced; an audit (`trigger_dependency_audit*` in `AuditMode::Expire`) that found a gap stale upstream, `dependency` naming what broke; the store audit (`trigger_dependency_audit_store`) on each stale stored value | `Direct`, born `Expired` (a stale stored value: `Direct`, persisted) | `Cascaded`, root = the asset (for an audit gap: the gap), cause still naming the stale input | Warning |
| `UpdatedInStore { actual }` | stored bytes no longer match the recorded version (§Content changed outside Liquers) | not expired: becomes input, or is deleted | `Cascaded` | Warning |
| `Updated { version }` | new content through Liquers: a recomputation with a new version, `set_state`, `set_binary`, `publish_version`, a fast-track load registering a different version, a changed command version, a changed folder listing | not expired | `Cascaded` | Info |
| `Removed` | `AssetManager::remove` deleting a value (a `Source`/`Override`, or a key with no recipe) | removed | `Cascaded` | Info |

Info is the contract working as designed; Warning is a stored assumption found false, or a
departure from the normal contract.

**One writer.** Every route, for every expired asset — live, or a stored-only copy expired in the
store — writes the reason through `AssetManager::record_expiry(metadata, subject, reason)`, in the
same metadata write as the `Expired` status. The default sets the field and appends
`ExpiryReason::log_entry(subject)` to the asset's log; legacy metadata is left untouched. Override
it to change wording or levels, add fields, or forward the event. It runs under the asset's `data`
write lock, so it must not block or reach any asset or lock. Only a real transition records: an
asset already `Expired` keeps the reason it has.

**A value supplied already `Expired`** through `set_state` or `set_binary` (either manager) does
not go through `record_expiry`: its cause is unknown to Liquers, so the structured `expiry_reason`
stays as supplied, usually `None`. The write logs the expiry instead, at the moment Liquers learns
of it: a warning `Asset expired`, then an info entry `Expiry recorded after the fact: {key} was
written already expired ({set_state|set_binary}); its original cause is unknown` — or, when the
supplied metadata carries a reason, `…; supplied reason: {that reason's log line}`. The supplied
log entries are kept. Every manager decides the written status with the same rule: `Expired` and
`Error` are kept, any other status becomes `Override` when the key has a recipe and `Source`
otherwise.

**Log lines.** `subject` is the asset's key, else its query, never its runtime id; `root` and `via`
print as dependency keys (`-R/…`). No version number appears. A direct reason reads
`{subject} expired: {what happened to it}`; a cascaded one reads
`{subject} expired: {what happened to root} triggered a cascade expiration`, followed by
` via direct dependency {via}` only when `via != root`. The phrases, from `log_entry`:

| Cause | Direct: `{subject} expired: …` | Cascaded: `… triggered a cascade expiration` |
|---|---|---|
| `Deadline` | `its expiration time {expiration_time} passed` (RFC 3339) | `expiration deadline on {root}` |
| `Explicit` | `expiration was requested explicitly` | `explicit expiration of {root}` |
| `Audit` | `an audit found it at a different version than recorded` | `an audit that found {root} at a different version than recorded` |
| `StaleDependency` | `it was evaluated with the expired value of {dependency}` | `the evaluation of {root} with the expired value of {dependency}` |
| `UpdatedInStore` | `its stored content was changed outside Liquers` | `a change to {root} made in the store outside Liquers` |
| `Updated` | `it received new content` | `new content of {root}` |
| `Removed` | `it was removed` | `the removal of {root}` |

For the chain `data/a.txt → data/b.txt → data/report.txt`, from
`tests/expiry_provenance_integration.rs`:

```text
data/a.txt expired: expiration was requested explicitly
data/b.txt expired: explicit expiration of -R/data/a.txt triggered a cascade expiration
data/report.txt expired: explicit expiration of -R/data/a.txt triggered a cascade expiration via direct dependency -R/data/b.txt
data/report.txt expired: new content of -R/data/a.txt triggered a cascade expiration via direct dependency -R/data/b.txt
```

### Who decides status

**The asset manager is authoritative. The store follows.**

The manager holds the live asset and decides its status; the store is kept in agreement as far as
it can be, and is consulted only for what the manager does not hold. Two consequences are load-bearing
and are the reason this is stated as a rule rather than left implicit:

- **Every expiry of a keyed asset writes the new status through to the store**
  (`AssetRef::mark_expired_status`), so an entry the manager later evicts is not resurrected as
  `Ready` by the next process. The write is best-effort and skipped for a key the store does not
  already hold — `set_metadata` on a missing key would otherwise mint a phantom entry with empty
  bytes — and a failure is logged rather than propagated: the manager's decision stands either way.
- **Anything asking about status asks the manager first** and the store only as a fallback. This is
  what `try_fast_track`'s dependency check does; see §Reusing a stored asset in
  [`ASSET_LIFECYCLE.md`](ASSET_LIFECYCLE.md).

The corollary is that two environments over one live store is **not** a supported configuration.
Synchronization is the manager's job and the store is not equipped for it. A second *process*
does not share a live store object anyway — it reads persisted bytes, which is what makes
"persist the status, then reload" the whole of the cross-process contract.

### Status and reads

`Status::read_exposure()` is the single decision point for what any read may expose. Both the
state-read family and the binary-read family derive from it, so they cannot drift apart.

| `ReadExposure` | Statuses |
|---|---|
| `Value` | `Ready`, `Source`, `Override`, `Volatile` |
| `MetadataOnly` | `Directory`, `Error`, `Cancelled` |
| `Expired` | `Expired` |
| `Pending` | `None`, `Recipe`, `Submitted`, `Dependencies`, `Processing`, `Partial`, `Storing` |

Note this is **not** `has_data()`, which is `true` for `Expired` and `Partial` — it answers "is
there a value in there", the right question for a store fallback and the wrong one for a read gate.

| Method | `Value` | `MetadataOnly` | `Expired` | `Pending` |
|---|---|---|---|---|
| `poll_state` | value state | metadata-only state | `None` | `None` |
| `poll_state_any_status` / `get_any_status` | value state | metadata-only state | value state | `None` |
| `try_poll_state` | as `poll_state`; `None` if the lock is unavailable | | | |
| `get` | `Ok(value state)` | `Ok(metadata-only)` | `Err` | waits |
| `poll_binary` | cached bytes | `None` | `None` | `None` |
| `poll_binary_any_status` | cached bytes | `None` | cached bytes | `None` |
| `try_poll_binary` | as `poll_binary`; `None` if the lock is unavailable | | | |
| `get_binary` | bytes, serializing if needed | `Err` | `Err` | waits, then as `Value` |
| `get_binary_any_status` | `Ok(Some)`, serializing if needed | `Ok(None)` | `Ok(Some)`, serializing if needed | `Ok(None)` |

**`MetadataOnly` has no binary counterpart.** A metadata-only *value* exists; a metadata-only *byte
string* does not, and `Some(empty)` would be a lie. Binary reads therefore report absence in
whatever the signature allows: `None`, or `Err` from `get_binary`. That error reuses the asset's own
recorded failure for `Error`, and is constructed for `Cancelled` and `Directory`, which record none.

**`get_binary_any_status` is not an alias for `poll_binary_any_status`**, though `get_any_status` is
one for `poll_state_any_status`. A retained value needs no materialising; bytes do. `AssetData::binary`
is populated at two sites and cleared at roughly ten, so an expired asset commonly retains its value
and no bytes — precisely when recovery is wanted — so the `get_` form serializes on demand.

**The manager-level recovery reads defer a placeholder to the store.**
`AssetManager::get_any_status(key)` and `get_binary_any_status(key)` answer from the live asset
mapped under the key, except when its status is `None` or `Recipe`: such an asset has produced or
loaded nothing yet — typically a concurrent `get` has mapped it and not yet fast-tracked the stored
value — so the store decides, exactly as for `remove` (`live_status_defers_to_store`). Every other
live status, in-flight ones included, answers from memory as the table above says. A contained key
whose store entry has no data object (metadata only) answers `Ok(None)`.

**Expiry is uniform, and opting out of it is explicit.** `Expired` is a cache miss for every normal
read of either family, even when serialized bytes are still cached. This includes an asset that is
**born** `Expired` — decided so in `finalize_status_with_version`, before it is persisted — because
its evaluation consumed a stale dependency: that result is fresh but uncacheable, and whether it is
acceptable is the caller's judgement. It is not relabelled after the fact; deciding before the write
is what keeps the store in agreement with the manager, which is the authority on status. The caller
makes it explicitly, through the `*_any_status` reads or `to_override`.

`AssetRef::save_to_store` deliberately bypasses this gate via `AssetData::binary_unchecked`:
persisting is not a read of the exposed value, and `set_state` persists at statuses the gate hides.

## Content changed outside Liquers

A stored value can be edited by something other than Liquers — a person replacing a file, another
tool writing the store. Liquers detects it on read by re-hashing the bytes against the recorded
version.

**Content-hash versions are flagged.** `Version::from_content(bytes)` is 127 bits of blake3 with
**bit 127** (`Version::HASH_FLAG`) set; every other constructor (`from_time_now`,
`from_specific_time`, `new_unique`) clears it. `Version::kind()` reads the flag: `ContentHash`,
`Unknown` (0) or `Timestamp`. Only content hashes are flagged, so command versions do not change.
`Version::verify(bytes)` returns `Verified`, or `Mismatch { actual, recorded }`, where `actual` is
`from_content(bytes)` and `recorded` is the old version's kind.

**Legacy values verify.** A value written before the flag existed carries an unflagged
`Version::from_bytes` hash. `verify` accepts an unflagged recorded version equal to
`from_bytes(bytes)`, so legacy content is not mistaken for an edit. For such a value `kind()` is a
guess (about half read as `ContentHash`); it changes only the log wording, never the outcome.
Anything else — a timestamp, `new_unique`, 0 — never equals a hash and is a mismatch.

**What a mismatch means** (`external_change_action`, the policy is `external_change`:
`user_input` by default, or `corrupted`):

| Stored status | With recipe | `UserInput` (default) | `Corrupted` |
|---|---|---|---|
| `Source` | any | keep `Source`, adopt `actual` (`AcceptAsInput`) | same |
| `Override` | any | keep `Override`, adopt `actual` | same |
| `Ready`, `Expired` | yes | becomes `Override`, adopt `actual` (`ConvertToOverride`) | stored data and metadata deleted, so the recipe recomputes it (`Delete`) |
| `Ready`, `Expired` | no (recipe removed since) | keep, adopt `actual` | same |
| any other | — | not checked | not checked |

A file with **no metadata** is recognised as a stored `Source` or `None` with recorded version 0,
and is treated as `Source` without a recipe and `Ready` with one. Without a recipe it is adopted
**in memory only**: the version map learns `actual`, nothing is written, and the next process
computes the same hash again. Every other adoption writes the sidecar with the new version (and
status) and a warning in the asset's log:

```text
content of data/a.txt changed outside Liquers; accepted as user input
no content hash was recorded for data/a.txt; adopting its content as user input
```

The second wording is used when the recorded version was a timestamp or 0. `Delete` leaves no
metadata to log in, so it reports on stderr. The changed asset itself is never `Expired`; its
dependents are, with `UpdatedInStore { actual }`. The version map is updated through
`audit_version`, not `register_version`, because after a restart `actual` is the first version the
map sees for the key.

**When the check runs.** Under `verify_versions: on_read` (default), wherever the manager already
holds the bytes: the fast track (`try_fast_track`, before the dependency checks; a `Delete`
refuses the fast track and the key is recomputed), and the store branch of `get_any_status` /
`get_binary_any_status`. An edit nobody has read yet is found by
`AssetManager::verify_stored_versions(key, deep, mode)`, which re-hashes every stored value under
`key` and reports `verified`, `skipped` and `changed` (`VersionVerificationReport`);
`AuditMode::ReportOnly` writes and registers nothing. A key with metadata and no data object is
skipped, not changed — deleting large intermediates is supported — and so are empty bytes under a
timestamp version, which is how `AsyncMemoryStore` answered a metadata-only entry before
`design/memory-store-metadata-only-entry/` (every in-tree store now answers `KeyNotFound`). `verify_versions: off` hashes nothing,
and `verify_stored_versions` then returns an empty report. A recorded-version audit
(`trigger_dependency_audit`) cannot see an edit; this can.

**Read-only stores.** `apply_external_change` runs under `key_mutation_lock` and is a no-op when
the change was already applied, by a concurrent reader or earlier in this process. A store write
that fails is reported on stderr and the result is kept in memory: the version map and the cascade
still happen, and the next process detects the change again. (`AsyncFileStore` itself writes a
sidecar for a bare file on first read; `STORE-NO-READ-ONLY-ADAPTER`.)

## State Machine Diagram

```
                                    ┌─────────────────────────────────────────┐
                                    │                                         │
                                    ▼                                         │
┌──────────┐                   ┌─────────┐                                    │
│          │   Asset created   │         │                                    │
│  (none)  │ ─────────────────►│  None   │                                    │
│          │                   │         │                                    │
└──────────┘                   └────┬────┘                                    │
                                    │                                         │
                    ┌───────────────┼───────────────┐                         │
                    │               │               │                         │
                    ▼               ▼               ▼                         │
             ┌──────────┐    ┌──────────┐    ┌──────────┐                     │
             │          │    │          │    │          │                     │
             │  Recipe  │    │  Source  │    │ Override │ ◄───── set() ───────┤
             │          │    │          │    │          │                     │
             └────┬─────┘    └────┬─────┘    └────┬─────┘                     │
                  │               │               │                           │
                  │               │               │ remove()                  │
                  │               │               ▼                           │
                  │               │          ┌─────────┐                      │
                  │               │          │ Recipe  │ (if recipe exists)   │
                  │               │          └─────────┘                      │
                  │               │                                           │
                  ▼               │                                           │
    ┌─────────────────────────────┼───────────────────────────────────────┐   │
    │                             │                                       │   │
    │  ┌───────────────┐          │                                       │   │
    │  │               │ ◄────────┘                                       │   │
    │  │   Submitted   │                                                  │   │
    │  │               │ ◄──────────────────────────────┐                 │   │
    │  └───────┬───────┘                                │                 │   │
    │          │                                        │                 │   │
    │          │ JobStarted                             │                 │   │
    │          ▼                                        │                 │   │
    │  ┌───────────────┐      ┌───────────────┐         │                 │   │
    │  │               │      │               │         │                 │   │
    │  │ Dependencies  │─────►│  Processing   │◄────────┤                 │   │
    │  │               │      │               │         │                 │   │
    │  └───────────────┘      └───────┬───────┘         │                 │   │
    │                                 │                 │                 │   │
    │          ┌──────────────────────┼─────────────────┼─────────┐       │   │
    │          │                      │                 │         │       │   │
    │          ▼                      ▼                 │         ▼       │   │
    │  ┌───────────────┐      ┌───────────────┐         │ ┌───────────────┐   │
    │  │               │      │               │         │ │               │   │
    │  │    Partial    │      │    Storing    │         │ │   Cancelled   │───┘
    │  │               │      │               │         │ │               │
    │  └───────┬───────┘      └───────┬───────┘         │ └───────────────┘
    │          │                      │                 │
    │          │                      ▼                 │
    │          │              ┌───────────────┐         │
    │          │              │               │         │
    │          └─────────────►│     Ready     │─────────┘ (retry on error)
    │                         │               │
    │                         └───────┬───────┘
    │                                 │
    │  JOB PROCESSING BOUNDARY        │ expiration
    └─────────────────────────────────┼───────────────────────────────────────
                                      ▼
                              ┌───────────────┐
                              │               │
                              │    Expired    │
                              │               │
                              └───────────────┘

    ┌───────────────┐
    │               │  (can occur from Processing, Partial, Dependencies)
    │     Error     │
    │               │
    └───────────────┘
```

## State Transitions with Messages

### Node Format
```
┌─────────────────────────────────┐
│ STATUS                          │
│─────────────────────────────────│
│ Notifications sent:             │
│   • NotificationMessage         │
└─────────────────────────────────┘
```

### Detailed State Graph

```
┌─────────────────────────────────┐
│ None                            │
│─────────────────────────────────│
│ • Initial                       │
└──────────────┬──────────────────┘
               │
               │ (fast-track success from store)
               ├─────────────────────────────────────────────────────────┐
               │                                                         │
               │ (submitted to JobQueue)                                 │
               │ ═══════════════════════                                 │
               │ Service: JobSubmitted                                   │
               ▼                                                         │
┌─────────────────────────────────┐                                      │
│ Submitted                       │                                      │
│─────────────────────────────────│                                      │
│ • StatusChanged(Submitted)      │                                      │
│ • JobSubmitted                  │                                      │
└──────────────┬──────────────────┘                                      │
               │                                                         │
               │ (JobQueue picks up job)                                 │
               │ ═══════════════════════                                 │
               │ Service: JobStarted                                     │
               ▼                                                         │
┌─────────────────────────────────┐                                      │
│ Processing                      │                                      │
│─────────────────────────────────│                                      │
│ • StatusChanged(Processing)     │                                      │
│ • JobStarted                    │                                      │
│ • LogMessage (during)           │                                      │
│ • PrimaryProgressUpdated        │                                      │
│ • SecondaryProgressUpdated      │                                      │
└──────────────┬──────────────────┘                                      │
               │                                                         │
    ┌──────────┼──────────┬──────────────┬───────────────┐               │
    │          │          │              │               │               │
    │ Cancel   │ Error    │ Partial      │ Success       │               │
    │ ════════ │ ═══════  │ result       │ ═══════════   │               │
    │ Service: │ Service: │              │ Service:      │               │
    │ Cancel   │ Error-   │              │ JobFinishing  │               │
    │          │ Occurred │              │               │               │
    ▼          ▼          ▼              ▼               │               │
┌────────┐ ┌────────┐ ┌────────┐   ┌───────────┐        │               │
│Cancelled│ │ Error  │ │Partial │   │  Storing  │        │               │
│────────│ │────────│ │────────│   │───────────│        │               │
│•Status-│ │•Status-│ │•Value- │   │           │        │               │
│ Changed│ │ Changed│ │ Produced   │           │        │               │
│•Job-   │ │•Error- │ │•Status-│   └─────┬─────┘        │               │
│ Finished │ Occurred│ │ Changed│         │              │               │
└────────┘ │•Job-   │ └───┬────┘         │              │               │
           │ Finished│     │              │              │               │
           └────────┘     │              │              │               │
                          │              ▼              │               │
                          │        ┌───────────┐        │               │
                          │        │   Ready   │ ◄──────┘               │
                          └───────►│───────────│ ◄──────────────────────┘
                                   │•Value-    │   (fast-track load:
                                   │ Produced  │    StatusChanged +
                                   │•Status-   │    JobFinished)
                                   │ Changed   │
                                   │•Job-      │
                                   │ Finished  │
                                   └─────┬─────┘
                                         │
                                         │ (expiration/invalidation)
                                         ▼
                                   ┌───────────┐
                                   │  Expired  │
                                   │───────────│
                                   │•Status-   │
                                   │ Changed   │
                                   └───────────┘


══════════════════════════════════════════════════════════════════════════
EXTERNAL DATA PATH (set operations from ASSET_SET_OPERATION)
══════════════════════════════════════════════════════════════════════════

Any State ──────► set(key, data, metadata) ──────┐
                                                  │
                  ┌───────────────────────────────┘
                  │
                  │ (if recipe exists)
                  ▼
            ┌───────────┐
            │ Override  │
            │───────────│
            │•Value-    │
            │ Produced  │
            │•Status-   │
            │ Changed   │
            └─────┬─────┘
                  │
                  │ remove(key)
                  ▼
            ┌───────────┐
            │  Recipe   │ (triggers recalculation if desired)
            │───────────│
            │•Removed   │
            │•Status-   │
            │ Changed   │
            └───────────┘

Any State ──────► set(key, data, metadata) ──────┐
                                                  │
                  ┌───────────────────────────────┘
                  │
                  │ (if NO recipe exists)
                  ▼
            ┌───────────┐
            │  Source   │
            │───────────│
            │•Value-    │
            │ Produced  │
            │•Status-   │
            │ Changed   │
            └───────────┘


══════════════════════════════════════════════════════════════════════════
CANCELLATION PATH
══════════════════════════════════════════════════════════════════════════

Submitted ── cancel() ──────────────────────────────────────► Cancelled
                       (claimed under the write lock; the command never runs)

Dependencies/Processing/Partial ── cancel() ──► request set (no status change)
            │
            │ the run races compute against the request (compute polled first)
            ├── compute returned Ok ──► finalize Ready/Volatile/Expired, persist (D2)
            ├── compute suspended   ──► compute dropped ──► Cancelled (cause recorded)
            └── command returned Error::cancelled (check_cancelled, cascade) ──► Cancelled

set/set_state/remove/to_override on an in-flight asset ──► Cancelled at once;
            the run's late result is discarded (cancel_for_replacement)
```

One terminal transition per run, from an in-flight status, by its run (or by the two claims above,
which need no run); whoever makes it sends the single `JobFinished`.

## Asset Lifecycle Scenarios

### Scenario 1: Simple Resource Load (Fast Track)
```
1. get_asset(key) called
2. AssetRef created with Recipe from key, status=None
3. try_fast_track() checks store
4. Data found in store → load binary and metadata
5. Status → Ready (or Source)
6. Notification: StatusChanged(Ready), JobFinished
```

### Scenario 2: Query Evaluation (Job Queue)
```
1. get_asset(query) called
2. AssetRef created, status=None
3. try_fast_track() fails (not a simple resource)
4. job_queue.submit() called
5. Status → Submitted, Service: JobSubmitted, Notification: JobSubmitted
6. JobQueue picks job when capacity available
7. Status → Processing, Service: JobStarted, Notification: JobStarted
8. Commands execute, progress updates sent
9. Value produced → Status → Ready
10. Service: JobFinishing, Notification: ValueProduced, JobFinished
11. Save to store (background or sync)
```

### Scenario 3: Set External Data (Override)
```
1. set_binary(key, data, metadata) or set_state(key, state) called; takes key_mutation_lock
2. If a live asset holds the key: cancel_for_replacement() (an in-flight one ends Cancelled at once,
   its run's late result is discarded and it writes nothing more to the store), then unmap it
3. Check if recipe exists for key
4. Status → Override (recipe exists) or Source (no recipe)
5. Store data to store; register the new version, expire dependents
6. Notification to the replaced asset: Removed (its last message)
```

### Scenario 4: Cancellation
```
1. cancel() called on AssetRef
2a. Submitted: Status → Cancelled at once; Notification: StatusChanged(Cancelled), JobFinished
2b. Dependencies/Processing/Partial: the cancellation request is set (shared with the Context)
3. The run's compute is dropped at its next suspension point, or the command sees
   context.is_cancelled() and returns context.check_cancelled()'s error
4. Status → Cancelled, cause in error_data; Notification: StatusChanged(Cancelled), JobFinished
   (in memory only: no metadata-only entry is written for the key)
   — or, if the command had already returned Ok: Status → Ready, persisted, request cleared
5. cancel() returns Ok(()) (after at most 5 s on native, whatever the outcome)
6. Dependents waiting on this asset get its cancellation error unchanged and, unless they handle
   it, end Cancelled too, each logging one cascade warning
```

### Scenario 5: Remove and Recalculate
```
1. remove(key) called on AssetManager; takes key_mutation_lock
2. Decide by status (see "Remove Semantics" below): Directory → StatusConflict
3. If a live asset holds the key: cancel_for_replacement(), untrack expiration, unmap it
4a. Delete (user value, or a key without a recipe): expire dependents, then drop the key from
    the dependency graph, then remove it from the store
4b. Drop a computed value: keep the stored record as status Recipe with its version; dependents
    are not expired
5. Notification to the unmapped asset: Removed (its last message)
6. A later get(key) on a key with a recipe evaluates it again
```

### Scenario 6: Preview Mode with Partial Status
```
1. Command starts processing, status=Processing
2. Command produces quick preview result
3. Command calls context.set_partial(preview_value)
4. Status → Partial, Notification: ValueProduced, StatusChanged(Partial)
5. Preview data available to clients
6. Command continues full computation
7. Command produces final result
8. Status → Ready, Notification: ValueProduced, StatusChanged(Ready), JobFinished
```

### Scenario 7: Checkpointing with Partial Status
```
1. Long-running command starts, status=Processing
2. Command periodically saves checkpoint via context.set_partial(checkpoint_state)
3. Status → Partial (if first checkpoint), Notification: ValueProduced
4. If crash/restart occurs:
   a. Asset loaded from store with Partial status
   b. Checkpoint data available via context.get_partial()
   c. Command resumes from checkpoint
5. Command completes → Status → Ready
```

## Partial Status Protocol

The `Partial` status enables two related but distinct use cases:

### Preview Mode
Fast feedback to users while expensive computation continues.

**Use case**: Image processing command produces low-resolution preview quickly, then high-resolution final result.

**Command implementation**:
```rust
async fn process_image(context: &Context, image: Image) -> Result<Image, Error> {
    // Quick preview
    let preview = image.thumbnail(100, 100);
    context.set_partial(preview).await?;

    // Full processing (expensive)
    let result = expensive_processing(image).await?;
    Ok(result)
}
```

### Checkpointing
Recovery from failures in long-running computations.

**Use case**: ML training that runs for hours, saves checkpoints periodically.

**Command implementation**:
```rust
async fn train_model(context: &Context) -> Result<Model, Error> {
    // Check for existing checkpoint
    let mut model = if let Some(checkpoint) = context.get_partial::<Model>().await? {
        context.log_info("Resuming from checkpoint");
        checkpoint
    } else {
        Model::new()
    };

    for epoch in model.current_epoch..total_epochs {
        model.train_epoch()?;

        // Save checkpoint periodically
        if epoch % checkpoint_interval == 0 {
            context.set_partial(model.clone()).await?;
        }
    }

    Ok(model)
}
```

### Context Methods for Partial

```rust
impl<E: Environment> Context<E> {
    /// Set partial/checkpoint data
    /// Transitions status to Partial if currently Processing
    /// Sends ValueProduced notification
    pub async fn set_partial(&self, value: E::Value) -> Result<(), Error>;

    /// Get partial/checkpoint data if available
    /// Returns None if no partial data exists
    pub async fn get_partial<T>(&self) -> Result<Option<T>, Error>
    where
        T: TryFrom<E::Value>;

    /// Check if partial data is available (for checkpoint recovery)
    pub async fn has_partial(&self) -> bool;
}
```

### Command Metadata

Commands should declare support for these protocols:

```rust
register_command!(cr,
    fn train_model(context) -> result
    label: "Train Model"
    doc: "Train ML model with checkpointing support"
    supports_checkpointing: true  // NEW: indicates checkpointing support
)?;

register_command!(cr,
    fn process_image(context, image: Image) -> result
    label: "Process Image"
    supports_preview: true  // NEW: indicates preview support
)?;
```

### State Transitions for Partial

```
Processing ──► set_partial() ──► Partial
                                    │
                    ┌───────────────┤
                    │               │
                    ▼               ▼
            set_partial()     final result
            (update data)     ──► Ready
                    │
                    └──► Partial (stays in Partial)
```

### Recovery Behavior

When loading an asset with `Partial` status from store:
1. Data represents the last checkpoint/preview
2. Command can access it via `context.get_partial()`
3. If command supports checkpointing: resume from checkpoint
4. If command doesn't support checkpointing: start fresh (partial data ignored)

**Note**: Commands that don't support checkpointing/preview never produce Partial status, so this case is for crash recovery of checkpoint-supporting commands.

## Resolved Design Decisions

The following issues were identified and resolved through discussion:

### 1. JobQueue Bug (RESOLVED - needs fix)
**Problem**: Line 1858 in `assets.rs` has buggy logic that removes assets immediately after adding them.

**Resolution**: This is a bug. See `specs/archive/2026-03-02-jobqueue-fix.md` for the fix specification.

### 2. Dependencies Status (RESOLVED - needs implementation)
**Problem**: Dependencies status exists but is never set.

**Resolution**: The interpreter should set this status when waiting for dependencies. See `specs/reference/DEPENDENCIES_STATUS.md` for implementation spec.

### 3. Cancel → Set Path (RESOLVED)
**Problem**: What status path for set() on Processing asset?

**Resolution**: Direct transition: `Processing → Cancelled → Override/Source`. `cancel_for_replacement()` ends the replaced asset `Cancelled` at once and discards its run's late result; set() then writes the final status.

### 4. Fast Track Notifications (RESOLVED)
**Problem**: Inconsistent notifications between fast-track and JobQueue paths.

**Resolution**: Fast-track should also send `StatusChanged(Ready/Source)` before `JobFinished` for consistency.

### 5. Partial Status Purpose (RESOLVED)
**Problem**: Unclear use case for Partial status.

**Resolution**: Partial is used for **both preview and checkpointing**:

1. **Preview mode**: Quick low-quality result available while full computation continues
2. **Checkpointing**: Saving intermediate state for recovery in long computations

**Protocol**:
- Partial data is available via `Context`
- Commands must explicitly support this protocol (opt-in)
- Only some commands will implement checkpointing and/or preview
- Command metadata should indicate support for these features

### 6. Watch Channel Limitations (RESOLVED)
**Problem**: Intermediate notifications may be lost.

**Resolution**: Acceptable as-is. Notifications are hints; clients should poll for full state if needed.

### 7. set_metadata Notification (RESOLVED)
**Problem**: What notification for metadata-only updates?

**Resolution**: Add new `MetadataChanged` notification type.

### 8. Storing Recovery (RESOLVED)
**Problem**: How to handle Storing status on load after crash?

**Resolution**: Treat as corrupted/Error. If an asset is loaded from store with `Storing` status, it indicates incomplete write and should be treated as an error.

### 9. Volatile + Override (RESOLVED)
**Problem**: Should original volatility be tracked?

**Resolution**: No need to track. After `remove()`, only the recipe specifies asset behavior - if the recipe is volatile, the asset becomes a volatile recipe again. Volatility is determined by recipe, not by historical state.

### 10. Remove Semantics (RESOLVED)
**Problem**: Should remove() behave differently based on status?

**Resolution** (`design/axum-assets-endpoints/`, superseding "always delete"): yes. `remove` is a
default method of `AssetManager`, serialized by the manager's `key_mutation_lock`. The status is
the live asset's, unless it is `None`/`Recipe` (nothing produced yet) or there is no live asset;
then the stored status decides.

| Status | Recipe? | Effect | Dependents |
|---|---|---|---|
| `Directory` | any | `Err(StatusConflict)` — use `removedir` | — |
| any other | no | delete: live asset, dependency-graph entry, stored entry | expired, **before** the key leaves the graph |
| `Source`, `Override` | yes | delete the user value as above; the key falls back to its recipe | expired |
| `Ready`, `Expired`, `Error`, `Cancelled`, `Volatile`, `Partial`, in flight | yes | drop the computed value: unmap the live asset; the stored record is rewritten as `Recipe`, data-bearing fields cleared, **version kept** (legacy metadata is deleted instead) | **not** expired — a recomputation that changes the content hash cascades then |
| none, or stored `Recipe` | yes | nothing to do (idempotent) | — |
| nothing live, nothing stored | no | `Err(KeyNotFound)` | — |

Keeping the version lets `AssetManager::version` answer for a dropped intermediate, so
`trigger_dependency_audit` keeps its dependents valid; a stored `Recipe` has no data
(`has_data()` is false), so nothing reads the empty bytes as a value, and it does not block a
dependent's fast track (`dependency_blocks_fast_track` treats it like an absent dependency).
Recipes themselves cannot be deleted by `remove`.

**Related keyed operations**, all default methods of `AssetManager` and none of them evaluating:

- `contains(key)` is *stored, or listed by the recipe provider*; `can_make(key)` is *stored, or
  producible by the recipe provider* (a template chunk is producible but not listed). Describing and
  submitting an unevaluated key use `can_make`.
- `listdir_keys_deep(key)` returns, for every **store** directory `d` under `key` (and `key`
  itself), everything `listdir_keys(d)` reports — recipe-declared keys included — never `key`
  itself, sorted and duplicate-free; so `keys()` includes root-level recipes. Directories only a
  recipe provider declares are not descended. Both managers share this one implementation.
- `removedir(key)`: `remove` for every key under the directory (deepest first, directory keys
  skipped; the directory's own recipe keys are included, so their live assets are unmapped), then `store.removedir`, which also deletes the directory's `recipes.yaml` and the
  `Recipe` records `remove` kept; live `Directory` assets under it are unmapped. It takes no lock
  itself (each `remove` does) and is not atomic. It refreshes the parent's listing version, so
  what was built from the parent's listing expires. Absent → `KeyNotFound`; not a directory →
  `StatusConflict`.
- `expire(key)`: a live `Ready`/`Override` asset expires and cascades (`AssetRef::expire`); a
  stored-only `Ready`/`Override` entry is marked `Expired` and its dependents are expired;
  `Expired` is idempotent; anything else (a `Source`, a recipe key with no value, an in-flight
  asset) is `StatusConflict`; unknown → `KeyNotFound`.
- `set_description(key, title, description)`: only for a `Source` (live and/or stored); data and
  version are unchanged; both `None` → `ParameterError`; another status → `StatusConflict`.
  Its command-side counterpart is `Context::set_title` / `set_description`, which does *not*
  override a title or description the recipe declared (see `DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`).
- `to_override(key)`: a `Source` is left unchanged, whether live or only stored.
- `get_asset_info(key)`: reads the live asset as it is (a cached `Expired`/`Error`/`Cancelled`
  entry is reported, not re-evaluated), else the store, else the recipe provider; unknown →
  `KeyNotFound`.
- `lookup_query_asset(query)`: the live asset for a query, without creating or submitting one (a
  pure-key query looks up the key). The observe-only counterpart of `get_asset`.
- `makedir(key)`: creates the directory in the store, refreshes the parent's listing version, and
  returns an unmapped asset with status `Directory`; it does not call `get`, which fails on a
  directory.

### 11. Concurrent set() Calls (RESOLVED)
**Problem**: Could concurrent set() calls cause inconsistency?

**Resolution**: RwLock is sufficient, BUT the write lock must be held during the entire operation including store write. This prevents the scenario where a slow store write could overwrite a newer value.

**Potential race without holding lock during store write:**
```
Thread A: lock, update, unlock, store.set("A") starts
Thread B: lock, update, unlock, store.set("B") completes
Thread A: store.set("A") completes → overwrites B in store!
Result: Memory has "B", Store has "A" → INCONSISTENT
```

**Solution**: Hold the write lock until store.set() completes.

## Terminal Outcome Contract (WP-2)

A finished asset has exactly **one observable terminal `State`**, and that `State` (backed by
its `Metadata`) is the single source of truth for the outcome. There is no separate outcome enum.

**Value XOR error.** A terminal `State` either carries a value (`Ready`/`Source`/`Override`/
`Volatile`/`Directory`) or represents a failure (`Error`/`Cancelled`) with no value — never both.

**`Cancelled` and `Error` are statuses, not errors in themselves.** Holding a state in either
status is legitimate; only *requesting a value* from it is an error:
- `Status::Error` stores the computed `Error` in `MetadataRecord.error_data` (serializable, so it
  survives persistence). Value extraction returns that stored error.
- `Status::Cancelled` records its cancellation error in `error_data` but is not an error
  (`is_error == false`). Value extraction returns that recorded error (`ErrorType::Cancelled`), whose
  `query` names the asset whose cancel was requested — the root cause of a cascade; a record without
  one (older stores) gets a synthesized `Error::cancelled(...)`.

**Neither status changes the type axis into an error type — there is none.** A failed asset holds no
value, so its `type_identifier` becomes the *none* type: the type reports what is available, not what
the asset was going to produce. That is what makes a failed asset storable, as metadata with no
bytes. See `specs/reference/VALUE_TYPE_SYSTEM.md`, "How a failure is typed".

**Accessors.**
- `AssetRef::poll_state() -> Option<State>`: `None` iff not finished; otherwise the terminal
  `State` (value- or error/cancelled-bearing).
- `AssetRef::get() -> Result<State, Error>`: waits, then returns `Ok(state)` for **any** obtained
  terminal outcome (including an error/cancelled state). `Err` covers two cases: a **delivery**
  failure — the terminal state could not be produced/obtained (store I/O, closed channel,
  finished-but-no-state anomaly) — **or `Status::Expired`**, whose data requires explicit opt-in.
  It consults status (`poll_state`), not notification *content*, so an overwritten `ErrorOccurred`
  notification cannot lose the error.

  The `Expired` case is checked **before** waiting. `poll_state` hides expired data, so without
  that check an already-expired asset would subscribe and block on a notification that has already
  been sent and will not repeat. `get_binary` behaves identically, and both name the recovery
  route (`*_any_status`, `to_override`) in the error. See §Status and reads.
- `State::value_state(self) -> Result<State, Error>` and the guarded extractors
  (`value`, `try_into_string`, `as_bytes`) return the failure error via `State::value_error()`.
  The ergonomic value path is `asset.get().await?.value_state()?`. `State.data` is private; use
  `data_unchecked()` only to forward/inspect a state without extracting a value.

**Failure recording.** One routine, `AssetRef::fail_asset(e)`, records a computed failure by
mutating the existing metadata (`with_error`, preserving the log/query/type audit trail) — it does
**not** replace the record. It acts only on an in-flight asset, once. A cancellation error routed to
it is not a failure: the asset ends `Status::Cancelled` (`with_cancellation`), not `Error`. A run's
own failure or cancellation is recorded in memory only, never as a metadata-only store entry.

**Re-evaluation.** `Error`, `Cancelled` and `Expired` are a **cache miss at the manager request
boundary**: `get(key)`/`get_asset(query)` drop such a stale-terminal asset and rebuild a fresh one
(a failure may be transient — hardware/volatile). `AssetRef::get()` itself does not re-evaluate, so
awaiting a completed evaluation reports the same outcome repeatably. Dependencies: a stale
`Error`/`Cancelled` dependency re-evaluates; a fresh error propagates as a dependency failure; a
fresh or mid-flight cancellation cascade-cancels the parent: `wait_for_dependency` returns the
dependency's recorded cancellation unchanged, logs `Dependency <dep> was cancelled; root cause:
<root>` on the parent (unless the parent's own cancel was requested, which then wins as the cause),
and the parent ends `Cancelled` unless its command handles the error and returns `Ok`. The wait
never sets the parent's status itself; its run does.

**Post-finish messages.** Once finalized, display-mutating/control service messages
(`UpdatePrimaryProgress`, `UpdateSecondaryProgress`, `JobSubmitted`, `JobStarted`, `Cancel`,
`ErrorOccurred`) are dropped (debug-logged); a late `LogMessage` is tolerated (at most one extra
log entry). The progress a command sent during its own run is the exception: it is applied even
when the status flipped to finished first, because the progress is finalized only after the
service loop has drained (below).

### Progress after completion

Progress exists to draw a progress bar, and a bar that will never move again is confusing. So when
a run ends — any terminal status: `Ready`, `Error`, `Cancelled`, … — the harness
(`AssetRef::finalize_primary_progress`, called by `run` and `run_inline` after the service loop has
drained) leaves the primary progress as follows:

| At finish | Primary progress afterwards |
|---|---|
| The command reported progress, and the last entry is already done | that entry, unchanged (its final message is kept) |
| The command reported progress, and the last entry is not done (a tick, 3/10, …) | a done entry carrying the last entry's message |
| The command never reported progress | none (`ProgressEntry::off()`), so no bar is drawn |
| Cancelled | `done("Cancelled")`, written when the asset is cancelled |

Secondary progress is cleared. A finished asset's `primary_progress()` is therefore always
`is_done()` or `is_off()`, and the same evaluation always leaves the same progress, on both the
spawning and the inline harness. The finalized progress is written to the store with the rest of
the metadata. **The status, not the progress, is the authoritative "is it finished" signal**: a
done bar says only that started progress has ended.

## Open Issues

### Issue 1: Error Recovery / Retry — RESOLVED (WP-2)
**Resolution**: `get_asset()`/`get(key)` automatically re-evaluate a stale `Error`/`Cancelled`
(and `Expired`) asset by dropping it from the manager map and rebuilding a fresh asset from its
recipe (see Terminal Outcome Contract → Re-evaluation). No explicit `retry()` method is needed;
re-evaluation is a property of *requesting* the asset, not of awaiting an in-flight one.

### Issue 2: Circular Dependencies — RESOLVED
Cycles are rejected at schedule time with `Error::dependency_cycle`
(`DependencyManager::register_scheduled_dependency`, `would_create_cycle`); see
[`DEPENDENCIES_STATUS.md`](DEPENDENCIES_STATUS.md).

### Issue 3: Dependency Invalidation Cascade — RESOLVED
A change of a key's version expires the dependents that recorded a different one, transitively,
each with an `ExpiryReason` (§Why an asset is `Expired`). The rules are in
[`DEPENDENCIES_STATUS.md`](DEPENDENCIES_STATUS.md) §Current contract.

## References

- Implementation: `liquers-core/src/assets.rs`
- Metadata/Status: `liquers-core/src/metadata.rs`
- Set Operation Spec: `specs/reference/ASSET_SET_OPERATION.md`
- JobQueue Fix Spec: `specs/archive/2026-03-02-jobqueue-fix.md`
- Dependencies Spec: `specs/reference/DEPENDENCIES_STATUS.md`
- Store interface: `liquers-core/src/store.rs`

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-09 | Cancellation is a request decided by the run: `Cancelled` status description, the cancellation path diagram, Scenarios 3-5 (`cancel_for_replacement`), §Terminal outcome (`Cancelled` records its cause in `error_data`; `fail_asset` acts once on an in-flight asset; cascade cancellation through `wait_for_dependency`). | phase-5 (`design/asset-cancellation-outcome/`) |
| 2026-10-08 | §Content changed outside Liquers: the memory store no longer answers a metadata-only entry with empty bytes; the empty-bytes skip is kept for older stores. | phase-5 (`design/metadata-only-entry-reload/`) |
| 2026-10-08 | §AssetManager names the dependency checks (`stored_dependency_state`, the per-key audit over the upstream closure, `trigger_dependency_audit_store`) and links §Consistency policies. The `StaleDependency` row gains the audit routes. | phase-5 (`design/dependency-chain-analysis-cost/`) |
| 2026-10-07 | §Why an asset is `Expired`: a value supplied already `Expired` keeps its supplied reason and logs the warning `Asset expired` plus an after-the-fact info entry; one written-status rule for every manager. New §Progress after completion: started progress of a finished asset is done, unstarted progress stays absent, finalized after the service loop drains. The `Deadline` row: the immediate manager's lazy check cascades too. The manager-level recovery reads defer a `None`/`Recipe` placeholder to the store, and answer `Ok(None)` for a metadata-only entry. The `StaleDependency` row also covers an edge recorded against a superseded version. | phase-5 (`design/supplied-expired-status-reason/`, `design/immediate-set-state-status-match/`, `design/finished-asset-progress-contract/`, `design/immediate-lazy-expiry-cascade/`, `design/recovery-read-defers-placeholder/`, `design/memory-store-metadata-only-entry/`, `design/dependency-edge-superseded-version/`) |
| 2026-10-06 | §Remove Semantics: `set_description` points to `Context::set_title` / `set_description` and its recipe-wins rule. | phase-5 |
| 2026-10-06 | §Related keyed operations: `contains` vs `can_make`; `listdir_keys_deep` is complete (recipe keys at every store directory, `keys()` includes root recipes); `removedir` unmaps the directory's own recipe assets. `refresh_listing_version` also notifies the recipe provider. | phase-5 |
| 2026-10-04 | §Related keyed operations: `makedir` and `removedir` refresh the parent's listing version (review fix on orest-d/liquers#75). | phase-5 |
| 2026-10-02 | Reviewed against `design/dependency-audit-and-expiry-provenance/`. §AssetManager: the trait is implementable outside core (public `DependencyManagerAccess` / `KeyMutationAccess`, policy accessors), pointing to the new guide. New §Why an asset is `Expired`: `ExpiryReason` (`Direct` / `Cascaded` with root and via), the seven causes with route, scope and level, `record_expiry` as the single overridable writer, and the log wording with real lines. New §Content changed outside Liquers: `HASH_FLAG` (bit 127), `VersionKind`, legacy unflagged verification, the decision table, no-metadata `Source` kept in memory, when the check runs, read-only stores. Open issues 2 and 3 marked resolved. | phase-5 |
| 2026-09-28 | §Notification Channel: the enum as implemented, with `Expired` and `Removed` (and when `Removed` is sent); the never-implemented `Cancelling`/`MetadataChanged` removed. Scenarios 3 and 5 rewritten. §Remove Semantics: the status-aware decision table replaces "always delete", plus `removedir`, `expire`, `set_description`, `to_override` on a `Source`, the non-evaluating `get_asset_info`, `lookup_query_asset` and `makedir`. | `design/axum-assets-endpoints/` |
| 2026-09-27 | Reviewed against `design/record-streams/` Phase 5. Added §`stored` and `cached` to §AssetManager: what each flag skips in both managers, that an existing stored copy is still preferred, that neither makes an asset volatile, that an uncached keyed asset stays the key's dependency-graph node (`bound_owner_key`) and has its stored copy marked `Expired` on an upstream change, and that `set_state`/`set_binary` read the supplied metadata's flag. §Key ownership: `cached: false` is a third way to have no registered owner. | phase-5 |
| 2026-09-15 | §Expiry: a stale-dependency completion is *born* `Expired` in `finalize_status_with_version` rather than relabelled afterwards by `finish_run_with_result`, so the stored status agrees with the manager. | `stale-dependency-status-finalization` |
| 2026-09-15 | Added §The one meaning of `Expired` (one meaning, two provenances, and why a `Stale` variant is not the answer) and §Who decides status (the manager is authoritative, every keyed expiry writes through to the store, ask the manager before the store, and two environments over one live store is not a supported configuration). | `stale-dependency-status-finalization` |
| 2026-08-26 | Recorded that a failed asset is typed by the value it holds, which is none; there is no `error` type identifier. | `design/foreign-value-type-registration/` |
| 2026-08-09 | Added §Key ownership and §Volatile assets are never owned to §AssetManager: the non-evaluating `owned_key_asset` contract, and the rule that a volatile asset is never served from either map. | `design/keyed-recipe-ownership` |
| 2026-08-08 | Added §Status and reads with the `ReadExposure` classification and the read behaviour matrix; added a `read_exposure` column to §Status Properties; amended §Terminal Outcome Contract → Accessors for `get`'s pre-wait expiry check. | `design/expired-binary-read-safety` |
| 2026-07-17 | Last substantive edit, carried into `reference/` unchanged. Not reviewed against the implementation since. | migration |
