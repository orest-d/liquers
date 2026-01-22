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

#### 1. `set()` - Binary Data Setting

```rust
async fn set(&self, key: &Key, binary: &[u8], metadata: MetadataRecord) -> Result<(), Error>
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

### Key-Only Constraint

Only `Key` type is accepted (strict). Queries must be converted to Key by the caller first. This is enforced by the method signatures.

### Metadata Requirements

Both operations require `MetadataRecord` (not the `Metadata` enum which includes `LegacyMetadata`).

**Mandatory fields:**
- `data_format` - Required for deserialization (e.g., "json", "csv", "bin")
- `type_identifier` - Required to know what Value type to deserialize into

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

When `set()` or `set_state()` is called:
- Acquire lock on the key
- Second caller waits until first completes
- No "last write wins" race conditions

### In-Flight Asset Handling

If the asset exists in AssetManager with status `Submitted`, `Dependencies`, or `Processing`:

1. Set `cancelled = true` flag on AssetData (prevents orphan writes)
2. Send `Cancel` message to service channel
3. **Immediately** remove AssetRef from AssetManager
4. Proceed with set operation
5. Orphaned task (if still running) will check `cancelled` flag and silently drop results

### Cancelled Flag Safety Mechanism

The `cancelled: bool` flag on AssetData prevents race conditions with long-running, non-cooperative commands (e.g., ML training in Python):

```rust
pub struct AssetData<E: Environment> {
    // ... existing fields ...

    /// If true, this asset has been cancelled and should not write results.
    cancelled: bool,
}
```

**Write prevention points** (all must check `cancelled` flag):
- `ValueProduced` handler
- Store write operations
- Status updates

## Error Recovery

If set operation fails mid-way (e.g., store write fails):

1. Delete data from both store and AssetManager (best effort)
2. If deletion also fails, add that error to the existing error
3. Return the error to caller

This ensures no partial/inconsistent state remains.

## Dependency Invalidation (future enhancement)
NOTE: Dependency tracking is not implemented yet, this is a design of a future behaviour.

When `set()` or `set_state()` modifies an existing asset:

1. Find all dependents (assets that depend on this key)
2. Set their status to `Expired`
3. Add warning to their log: "Expired due to user changing dependency key"
4. **Full cascade**: If A→B→C and we set(C), both B and A become `Expired`
5. **Synchronous**: set() blocks until all dependents are invalidated

## Store Routing

When multiple stores exist in a StoreRouter:
- Use standard router logic: first prefix match
- The store whose prefix matches the key receives the write

## Notifications

Setting an asset triggers notifications:

1. `Cancelling` - sent when cancel is initiated (if asset was processing)
2. `Cancelled` - sent after AssetRef is removed
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
}
```

This method:
1. Check if asset is being evaluated (`Submitted`, `Dependencies`, or `Processing`) - otherwise return Ok
2. Set `cancelled = true` on AssetData
3. Send `Cancel` message to the service channel
4. Wait (with timeout) for status to change to `Cancelled` or `JobFinished` on notification channel
5. Return Ok even if timeout occurs (best-effort)

### Processing Task Behavior

The processing task should:
1. Listen for `Cancel` message on service channel
2. Send `Cancelling` notification immediately upon receiving Cancel
3. Gracefully shut down
4. Send `Cancelled` notification
5. Send `JobFinished` notification

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
   - Add `cancelled: bool` field to `AssetData`
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
2. Add `cancelled` flag to `AssetData`
3. Add `cancel()` method to `AssetRef`
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
6. **Metadata Consistency Validation**: Validate data_format/type_identifier/media_type consistency (Issue #2)

## Related Issues

- Issue #2: METADATA-CONSISTENCY - Validation of metadata fields
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
- Issues: `specs/ISSUES.md`
