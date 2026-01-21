# Asset Set Operation Specification

## Overview

This specification describes the extension of AssetManager to support setting data and metadata directly, similar to how stores work. This enables external systems to inject data into the asset management system, either as source data or as overrides to computed results.

## Motivation

Currently, assets can only be created through query evaluation or recipe execution. There are scenarios where external data should be injected directly:
- Loading data from external sources as "source" assets
- Testing by providing mock data
- Manual overrides of computed results
- Caching pre-computed values from external systems
- Modifications of generated data/setup.

In these cases, it should be clear that this data is not generated, but entered or modified by the user. This is indicated by state as Source or Override.

## Requirements

### Core Functionality

1. **Set Operation**: Add methods to `AssetManager` trait:
   - `async fn set(&self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), Error>`
   - `async fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), Error>`

2. **Key-Only Constraint**: Only key assets (assets identified by a Key or with a Query that is equivalent to a key via `query.key()`) are settable. Non-key queries should return an error.

3. **Asset Not in Memory**: If the asset is not currently tracked by AssetManager:
   - Store the serialized data and metadata directly to the store
   - Set status to `Source` (no recipe) or `Override` (if recipe exists in recipe provider)
   - The asset will be loaded from store on next access

4. **Asset Submitted/In-Progress**: If the asset exists in AssetManager with status `Submitted`, `Dependencies`, or `Processing`:
   - Do this by calling a new `cancel()` method on AssetRef
   - Wait for cancellation to complete (this is what `cancel().await?` should do)
   - Replace data and metadata with new values
   - Set appropriate status (`Override`)

1. **Asset Finished**: If the asset exists with any finished status (`Ready`, `Error`, `Expired`, `Cancelled`, `Source`):
   - Replace data and metadata with new values
   - Determine new status:
     - `Source`: if recipe does NOT exist for this key
     - `Override`: if recipe DOES exist for this key
   - Invalidate binary representation

2. **Metadata-Only Update**: `set_metadata` should:
   - Update only metadata fields
   - Preserve existing data unchanged
   - Fail with error if asset has no data yet (status is `None`, `Recipe`, `Submitted`, etc.)

All updates should send notification through the notification channel.


### New Status: Override

Add `Override` status to the `Status` enum in `metadata.rs`:

```rust
/// Asset has data that overrides the recipe calculation.
/// The recipe exists but was not used to calculate this data.
/// Override can be cleared to recalculate using the recipe.
Override,
```

Status properties:
- `has_data()`: true
- `is_finished()`: true
- `is_processing()`: false
- `can_have_tracked_dependencies()`: false

### Clear Override Operation (remove)

Add a method to remove data from AssetManager. As a side-effect this would clear Override status and trigger recalculation:

```rust
async fn remove(&self, key: &Key) -> Result<(), Error>
```

```rust
async fn remove_asset(&self, query: &Query) -> Result<(), Error>
```

Behavior:
- Send notification `Removed` to the `AssetData`
- lock `AssetData` on `AssetRef`, remove data and binary in the `AssetData`, then the `AssetData` is removed from the `AssetManager`, remove data from store and finally - This does NOT trigger recalculation.

## Cancellation Mechanism

Use the existing `Cancel` message should be send to the service channel.
The processing task should listen and gracefully shut down.
Processing task should send `Canceling` immediately after the `Cancel` message is received from the service channel.
After gracefully finishing, `Canceled` message should be sent to notofocation channel
and finally `JobFinished`.

This can be done via cancel method:
```rust
impl AssetRef {
    pub async fn cancel(&self) -> Result<(), Error>
}
```

This method:
- Sets an `Cancel` message to the service channel.
- Waits (with timeout) for status to change to `Cancelled` or `JobFinished` on notification channel.
- Returns Ok even if timeout occurs (best-effort)

## Data Validation

**No validation is performed** when setting data. The data is stored as-is. Deserialization errors will occur when the asset is read if the data is incompatible with the expected type.

Rationale: Validation would require type-specific knowledge and dependencies, adding complexity. The type system at read time will catch mismatches.

## Implementation Details

### Files to Modify

1. **`liquers-core/src/metadata.rs`**
   - Add `Override` status to `Status` enum
   - Update `has_data()`, `is_finished()`, etc. to handle `Override`
   - Update status serialization/deserialization

2. **`liquers-core/src/assets.rs`**
   - Add `set()` method to `AssetManager` trait
   - Add `set_metadata()` method to `AssetManager` trait
   - Add `clear_override()` method to `AssetManager` trait
   - Add `cancel()` method to `AssetRef` impl (if notification channel insufficient)
   - Implement these methods in `DefaultAssetManager`

3. **`liquers-core/src/error.rs`**
   - Add error types:
     - `SetNonKeyAsset`: trying to set data for non-key query
     - `MetadataOnlyWithoutData`: trying to set metadata on asset without data
     - `ClearOverrideNotOverride`: trying to clear override on non-override asset
     - `ClearOverrideNoRecipe`: trying to clear override when no recipe exists

4. **`liquers-py/src/assets.rs`** (Python bindings)
   - Add Python bindings for new methods
   - Expose `set`, `set_metadata`, `clear_override` on Python AssetManager wrapper

5. **`liquers-axum/src/main.rs`** or handlers (Web API)
   - Add REST API endpoints:
     - `PUT /api/asset/{key}` - set data and metadata
     - `PATCH /api/asset/{key}/metadata` - set metadata only
     - `DELETE /api/asset/{key}/override` - clear override
   - Add authentication/authorization checks (if applicable)

6. **Tests**
   - `liquers-core/src/assets.rs` - Unit tests for set operations
   - `liquers-core/tests/` - Integration tests for set/clear override workflow
   - Test scenarios:
     - Set on non-existent asset
     - Set on in-progress asset (cancellation)
     - Set on finished asset
     - Override status transitions
     - Clear override and recalculation
     - Metadata-only updates
     - Error cases (non-key query, no data, etc.)

### Implementation Steps

1. Add `Override` status to `Status` enum and update helper methods
2. Add error types for new failure cases
3. Add `cancel()` method to `AssetRef` (internal implementation)
4. Implement `set()` in `DefaultAssetManager`:
   - Validate key-only constraint
   - Check if asset exists in memory
   - Handle different status cases
   - Determine Source vs Override status
   - Store to store if not in memory
5. Implement `set_metadata()` in `DefaultAssetManager`
6. Implement `clear_override()` in `DefaultAssetManager`
7. Add Python bindings
8. Add REST API endpoints
9. Write comprehensive tests

## Edge Cases and Considerations

### Volatile Assets
Volatile assets are recreated on every access. Setting data on a volatile asset:
- would work the same way as on non-volatile asset
- Asset may effectively become non-volatile, having Override status
  Justification: Data set by the user are always non-volatile.

### Concurrent Access
Multiple requests to set the same asset concurrently:
- Last write wins
- No extra locking or transaction semantics at AssetManager level should be needed. Unique key has only associated (at most) one `AssetData` object with multiple `AssetRef`, which is effectively `Arc<RwLock<AssetData>>`. 
- Store implementations may provide atomicity of stored data.

### Recipe Changes
If recipe changes after Override is set:
- Override status remains until explicitly removed
- Recipe change does not automatically invalidate override

### Dependencies
When an asset with `Override` status has dependents:
- Dependents should be invalidated/recalculated
- Note that store changes can't triggering invalidation. Current design breaks dependency tracking in case if changes are done in the store, therefore assets access is preferred.
- **TODO**: Define invalidation mechanism (future work, not in scope for this spec)

## Open Questions

1. Should we track who/what set the override (provenance)?
   - This can be done by `Metadata` - the future Session mechanism should set the user automatically in the metadata.

2. Should there be bulk set operations for efficiency?
   - No bulk, but there should be binary and State setting:
     - There should be `set_bin(&Key, Vec<u8>, MetadataRecord)>)`
     - and set(Key, State)

3. Should setting data on a volatile asset be an error or warning?
   - Data on volatile may be set with `Expired` state.
   - This should be done if metadata passed to set have volatile flag.
   - The same should actually be done in store too...

4. How to handle store failures during set?
   - Set should set both data and metadata and set the data in store. The whole thing should be an atomic operation. In case of a failure, everything should be removed before returning a failure.

## Future Enhancements

1. **Provenance Tracking**: Record who/what/when data was set
2. **Invalidation Cascade**: Automatically invalidate dependent assets when set
3. **Audit Logging**: (low priority) Track all set operations for debugging and compliance
4. **Bulk Operations**: (low priority) Efficient batch setting of multiple assets

## References

- Store interface: `liquers-core/src/store.rs`
- Asset lifecycle: `liquers-core/src/assets.rs`
- Status enum: `liquers-core/src/metadata.rs`
- Error types: `liquers-core/src/error.rs`
