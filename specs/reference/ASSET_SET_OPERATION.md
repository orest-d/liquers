---
title: Asset Set Operation Specification
kind: reference
audience: internal
area: [core/assets]
reviewed: 2026-10-09
---
# Asset Set Operation Specification

## Overview

This specification describes the extension of AssetManager to support setting data and metadata directly, similar to how stores work. This enables external systems to inject data into the asset management system, either as source data or as overrides to computed results.

## Motivation

Currently, assets can only be created through query evaluation or recipe execution. There are scenarios where external data should be injected directly:
- Manual overrides of computed results
- Loading user-defined data or data from external sources as "source" assets
- Modifications of generated data/setup
- Testing by providing mock data
- Caching pre-computed values from external systems
- Non-serializable data that can only exist in memory (GPU tensors, live connections, etc.)

In these cases, it should be clear that this data is not generated, but entered or modified by the user. This is indicated by status as `Source` or `Override`.

## Core Operations

### Two Set Operations

The AssetManager provides two complementary set operations:

#### 1. `set_binary()` - Binary Data Setting

```rust
async fn set_binary(&self, key: &Key, binary: &[u8], metadata: MetadataRecord) -> Result<(), Error>
```

- Sets binary (serialized) representation and metadata
- Clears any existing deserialized `data` field in AssetData
- Data can be reconstructed later via deserialization
- **Store only**: Does NOT create AssetRef in memory; writes directly to store
- Asset will be loaded from store on next access

#### 2. `set_state()` - State Setting

```rust
async fn set_state(&self, key: &Key, state: State<V>) -> Result<(), Error>
```

- Sets deserialized data and metadata from State
- Clears any existing `binary` field in AssetData
- **Memory + Store**: Creates new AssetRef with State AND serializes to store
- Supports non-serializable data (see Non-Serializable Data section)
- Data immediately available in memory for fast access

### `stored: false` in the supplied metadata

Both operations honour the **supplied** metadata's `stored` flag (`MetadataRecord::stored`, an
`Option<bool>`; absent means stored), not the recipe's. With `stored: Some(false)`, neither
operation writes anything to the store, which lets an explicit set bypass a `stored: false` recipe
by supplying `stored: true` in its own metadata. `set_state()` still creates the in-memory
`AssetRef`; `set_binary()` has nothing else to do. See [`ASSETS.md`](ASSETS.md) for the flag's
meaning.

### Key-Only Constraint

Only `Key` type is accepted (strict). Queries must be converted to Key by the caller first. This is enforced by the method signatures.

### Metadata Requirements

Both operations require `MetadataRecord` (not the `Metadata` enum which includes `LegacyMetadata`).

**Mandatory fields, now enforced** (they were asserted here before anything checked them):
- `type_identifier` — required, and must be **registered in this build**. Refused with
  `Error::general_error` naming the identifier.
- `type_name` — required, non-empty.
- `data_format` — the *effective* format, resolved from the declaration or the value's own default,
  must be one the type can be written in. Refused with `ErrorType::SerializationError` naming the
  type, the format and the supported set. This is the check that closed
  `CORE-METADATA-FORMAT-TYPE-CONSISTENCY`.
- `media_type`, when explicitly declared, must be well formed — no CR/LF, `type/subtype` — because
  it reaches an HTTP response header.

**Two exemptions from the format check**, both because the pairing is meaningless rather than
because the rule is inconvenient: an **error state** (which keeps the intended output's filename
while its value is gone and its type has become the none type, so the declared format describes
something that is no longer there), and a type that **declares no formats** at all (a UI element or
foreign handle, persisted as metadata only). The identifier check applies in both cases, and both
pass it — the none type is registered like any other.

There is no `error` type identifier: a failure is recorded in `is_error`/`Status::Error`, and the
type axis reports what is *available*. See `specs/reference/VALUE_TYPE_SYSTEM.md`, "How a failure is
typed".

**Soft checks** add a `LogEntry::warning` and do not fail the write: a filename extension differing
from the base of the effective format, and a declared `media_type` differing from the derived one —
which is expected whenever an override is active.

See `specs/reference/VALUE_TYPE_SYSTEM.md`.

**Auto-updated fields:**
- `updated` timestamp - Set automatically to current time
- Log entry added - Records "Data set externally" or similar

## Status Determination

### Status Preservation Rules

When setting data, the status is determined as follows:

1. **Input status is `Expired`**: Preserved as `Expired` (respected, not changed)
2. **Input status is `Error`**: Preserved as `Error` with special handling:
   - Value/data is set to None
   - Binary data is ignored (not stored; existing binary deleted from store)
   - Only metadata is stored (with error information)
3. **All other input statuses**: Determined by recipe existence:
   - `Source` - if NO recipe exists for this key
   - `Override` - if recipe DOES exist for this key

### Status Fixed at Set-Time

The status is determined once at set time based on current recipe existence. Later recipe changes do NOT automatically update the status.

### New Status: Override

Add `Override` status to the `Status` enum in `metadata.rs`:

```rust
/// Asset has data that overrides the recipe calculation.
/// The recipe exists but was not used to calculate this data.
Override,
```

Status properties:
- `has_data()`: true
- `is_finished()`: true
- `is_processing()`: false
- `can_have_tracked_dependencies()`: false

## Concurrency and Locking

### Lock During Set

When `set_binary()` or `set_state()` is called:
- Acquire lock on the key
- Second caller waits until first completes
- No "last write wins" race conditions

### In-Flight Asset Handling

If the asset exists in AssetManager with an in-flight status (`None`, `Recipe`, `Submitted`,
`Dependencies`, `Processing`, `Partial`):

1. `AssetRef::cancel_for_replacement()`: close the replaced asset's metadata saver (nothing more of
   it is written to the store, including log lines its still-running command sends later), request
   its cancellation, and end it `Cancelled` at once (cause: `Asset <key> was replaced`) — in memory
   only
2. **Immediately** remove AssetRef from AssetManager
3. Proceed with set operation
4. The orphaned run, if still running, is discarded: a suspended async command is dropped; a sync
   command that returns later finds the asset finished, and its result changes neither the asset nor
   the store

### Discarding a late result

There is no flag to consult. Every terminal transition of a run happens once, from an in-flight
status (`finalize_status_with_version`, `fail_asset`, `finish_cancelled` all check it under the
asset's write lock), and the run installs, persists and registers a value only if it finalized. A
replaced asset is already `Cancelled`, so its run's result is dropped. `save_to_store` additionally
skips an asset in status `Cancelled`. External managers call `cancel_for_replacement()` the same way.

`AssetRef::to_override` discards an in-flight run the same way but keeps the asset (it becomes the
key's `Override`), so its metadata saver stays open.

## Error Recovery

If set operation fails mid-way (e.g., store write fails):

1. Delete data from both store and AssetManager (best effort)
2. If deletion also fails, add that error to the existing error
3. Return the error to caller

This ensures no partial/inconsistent state remains.

## Dependency Invalidation (future enhancement)
NOTE: Dependency tracking is not implemented yet, this is a design of a future behaviour.

When `set_binary()` or `set_state()` modifies an existing asset:

1. Find all dependents (assets that depend on this key)
2. Set their status to `Expired`
3. Add warning to their log: "Expired due to user changing dependency key"
4. **Full cascade**: If A→B→C and we set(C), both B and A become `Expired`
5. **Synchronous**: `set_binary()` blocks until all dependents are invalidated

## Store Routing

When multiple stores exist in a StoreRouter:
- Use standard router logic: first prefix match
- The store whose prefix matches the key receives the write

## Notifications

Setting an asset triggers notifications:

1. To the replaced asset, if it was in flight: `StatusChanged(Cancelled)` and `JobFinished`
2. To the replaced asset: `Removed`, its last message
3. Subscribers (including WebSocket) should request new AssetRef after receiving these

WebSocket service is responsible for:
- Getting new AssetRef after cancellation
- Subscribing to new notification channel
- Notifying WebSocket subscribers of the change with new asset ID and status

## Cancellation Mechanism

### Cancel Method on AssetRef

```rust
impl AssetRef {
    pub async fn cancel(&self) -> Result<(), Error>
    pub async fn cancel_for_replacement(&self) -> Result<(), Error>
}
```

`cancel()`:
1. `Submitted`: end `Cancelled` at once (under the write lock, so no runner claims it in between);
   the command is never run
2. `Dependencies`, `Processing`, `Partial`: set the cancellation request (shared with the run's
   `Context`, readable through `Context::is_cancelled`); any other status: return Ok
3. Wait (with a 5 s timeout, native only) for the run to finish
4. Return Ok whether or not the cancel took effect (best-effort)

The run decides the outcome: a compute still suspended is dropped and the asset ends `Cancelled`; a
command that already returned `Ok` ends ready and is stored, and the request is cleared. See
`specs/guides/COMMAND_DESIGN_GUIDE.md` §Cooperative cancellation.

`cancel_for_replacement()` is described under §In-Flight Asset Handling.

### Processing Task Behavior

The service loop's `Cancel` message only sets the request (kept for external senders); it never
sets a status. The run races its compute against the request and records one terminal status.

## Remove Operations

Remove asset data from AssetManager and store:

```rust
async fn remove(&self, key: &Key) -> Result<(), Error>
async fn remove_asset(&self, query: &Query) -> Result<(), Error>
```

Behavior:
1. Send `Removed` notification to AssetData
2. Lock AssetData on AssetRef
3. Remove data and binary in AssetData
4. Remove AssetData from AssetManager
5. Remove data from store
6. Does NOT trigger recalculation

## Non-Serializable Data

`set_state()` supports non-serializable values (GPU tensors, live connections, Python objects with native resources).

### Behavior

1. Create AssetRef with State in memory
2. Attempt serialization to store
3. If serialization fails: store metadata only (no binary), mark `binary_available: false`
4. Asset only retrievable while AssetRef exists in memory

### Eviction Handling

Memory uses LRU eviction (configurable). When non-serializable asset is evicted:

1. **With recipe**: Re-execute recipe to regenerate data
2. **Without recipe (Source)**: Data lost permanently, `get()` returns error

See Issue #4 (NON-SERIALIZABLE) and Issue #5 (SOURCE-EVICTION) for future improvements.

## Volatile Assets

Setting data on a volatile asset:
- Works the same as non-volatile
- Asset becomes non-volatile with `Source` or `Override` status
- Rationale: User-specified data is always non-volatile

Exception: If user explicitly sets with `Expired` status, that is respected.

## Data Validation

**No validation is performed** when setting data. The data is stored as-is. Deserialization errors will occur when the asset is read if the data is incompatible with the expected type.

Rationale: Validation would require potentially costly de-serialization, adding complexity.

## Implementation Details

### Files to Modify

1. **`liquers-core/src/metadata.rs`**
   - Add `Override` status to `Status` enum
   - Update `has_data()`, `is_finished()`, etc. to handle `Override`

2. **`liquers-core/src/assets.rs`**
   - (A `cancelled: bool` field was planned here; replaced by `cancel_for_replacement`, see
     §In-Flight Asset Handling)
   - Add `set()` method to `AssetManager` trait
   - Add `set_state()` method to `AssetManager` trait
   - Add `remove()` and `remove_asset()` methods
   - Add `cancel()` method to `AssetRef` impl
   - Implement in `DefaultAssetManager`

3. **`liquers-core/src/error.rs`**
   - No new error types should be necessary

4. **Python bindings** - Out of scope for now

5. **Web API** - Handled via WEB_API_SPECIFICATION.md, including:
   - `POST /api/assets/data/{key}` - set binary
   - `DELETE /api/assets/data/{key}` - remove
   - `GET /api/assets/remove/{key}` - remove
   - `POST /api/assets/cancel/{key}` - cancel

6. **Tests**
   - Set on non-existent asset without recipe (→ Source)
   - Set on non-existent asset with recipe (→ Override)
   - Set on in-progress asset (cancellation flow)
   - Set on finished asset
   - Set with Expired status (preserved)
   - Set with Error status (preserved, no data stored)
   - Dependency invalidation cascade
   - Concurrent set operations (locking)
   - Remove asset without recipe
   - Remove overridden asset with recipe

### Implementation Steps

1. Add `Override` status to `Status` enum and update helper methods
2. (Superseded) the `cancelled` flag on `AssetData` — see §Discarding a late result
3. Add `cancel()` and `cancel_for_replacement()` methods to `AssetRef`
4. Implement `set()` in `DefaultAssetManager`:
   - Acquire lock on key
   - Check if asset exists in memory; if processing, cancel
   - Determine status (Expired preserved, else Source/Override)
   - Write to store
   - Update timestamp and add log entry
   - Trigger dependency invalidation
5. Implement `set_state()` in `DefaultAssetManager`:
   - Acquire lock on key
   - Check if asset exists in memory; if processing, cancel
   - Create new AssetRef with State
   - Attempt serialization to store (handle non-serializable gracefully)
   - Update timestamp and add log entry
   - Trigger dependency invalidation
6. Implement `remove()` and `remove_asset()`
7. Write comprehensive tests

## Future Enhancements

1. **Key-Level ACL**: Access control for who can set which keys (Issue #7)
2. **Upload Size Limits**: Configurable max binary size (Issue #6)
3. **Provenance Tracking**: Record who/what/when data was set (via Session mechanism)
4. **Audit Logging**: Track all set operations for debugging and compliance
5. **Background Set**: Async version that returns immediately
6. ~~**Metadata Consistency Validation**~~ — done, 2026-08-18; see "Metadata Requirements" above.

## Related Issues

- `CORE-METADATA-FORMAT-TYPE-CONSISTENCY` — **closed** 2026-08-18 by `value-type-system`
- Issue #3: CANCEL-SAFETY - Cancelled flag implementation details
- Issue #4: NON-SERIALIZABLE - Non-serializable data support
- Issue #5: SOURCE-EVICTION - Handling evicted non-serializable Source assets
- Issue #6: UPLOAD-SIZE-LIMIT - Binary size limits
- Issue #7: KEY-LEVEL-ACL - Access control

## References

- Store interface: `liquers-core/src/store.rs`
- Asset lifecycle: `liquers-core/src/assets.rs`
- Status enum: `liquers-core/src/metadata.rs`
- Error types: `liquers-core/src/error.rs`
- Issues: `specs/issues/`

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-09 | §In-Flight Asset Handling and §Cancellation Mechanism rewritten: `cancel_for_replacement` discards a replaced run's late result (no `cancelled` flag); `cancel()` is a request the run decides; replacement notifications as sent. | phase-5 (`design/asset-cancellation-outcome/`) |
| 2026-09-27 | Reviewed against the code for record-streams: the binary operation is `set_binary()` (was written `set()`); both operations honour the supplied metadata's `stored: false`. Repaired this History table's header | phase-5 (`design/record-streams/`) |
| 2026-08-26 | Corrected the error-state exemption: an errored asset is typed by the value it holds, which is none. There is no `error` identifier. | `design/foreign-value-type-registration/` |
| 2026-08-18 | The mandatory-field rules this document asserted are now enforced, in two tiers; records which checks reject, which warn, and the two exemptions from the format check. | `design/value-type-system/` |
| 2026-08-08 | Last substantive edit, carried into `reference/` unchanged. Not reviewed against the implementation since. | migration |
