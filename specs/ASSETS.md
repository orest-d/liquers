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
    Cancel,                            // Request cancellation
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
    // New (from ASSET_SET_OPERATION):
    Removed,                           // Asset was removed
    Cancelling,                        // Cancel in progress
    MetadataChanged,                   // Metadata-only update occurred
}
```

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
- **Expired**: Was ready but invalidated (e.g., dependency changed)
- **Cancelled**: Processing was cancelled via Cancel message
- **Source**: Data provided externally via set(), no recipe exists
- **Override**: Data provided externally via set(), recipe exists but was not used

### Status Properties

| Status       | has_data | is_finished | is_processing | can_have_deps |
|--------------|----------|-------------|---------------|---------------|
| None         | false    | false       | false         | false         |
| Directory    | false    | true        | false         | false         |
| Recipe       | false    | false       | false         | false         |
| Submitted    | false    | false       | false         | false         |
| Dependencies | false    | false       | false         | false         |
| Processing   | false    | false       | true          | false         |
| Partial      | true     | false       | true          | true          |
| Error        | false    | true        | false         | false         |
| Storing      | false    | false       | false         | true          |
| Ready        | true     | true        | false         | true          |
| Expired      | true     | true        | false         | false         |
| Cancelled    | false    | true        | false         | false         |
| Source       | true     | true        | false         | false         |
| Override     | true     | true        | false         | false         |

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

Submitted/Dependencies/Processing
            │
            │ Service: Cancel
            ▼
      ┌───────────┐
      │Cancelling │ (transient, internal)
      │───────────│
      │•Cancelling│
      └─────┬─────┘
            │
            │ (graceful shutdown complete)
            ▼
      ┌───────────┐
      │ Cancelled │
      │───────────│
      │•Status-   │
      │ Changed   │
      │•Job-      │
      │ Finished  │
      └───────────┘
```

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
1. set(key, data, metadata) called
2. Check if asset exists in AssetManager
3a. If Processing/Submitted: cancel().await
3b. Wait for cancellation
4. Check if recipe exists for key
5. Status → Override (recipe exists) or Source (no recipe)
6. Store data to store
7. Notification: ValueProduced, StatusChanged
```

### Scenario 4: Cancellation
```
1. cancel() called on AssetRef
2. Service: Cancel sent
3. process_service_messages receives Cancel
4. Status → Cancelled
5. Notification: StatusChanged(Cancelled), JobFinished
6. Metadata saved to store
7. cancel() returns Ok(())
```

### Scenario 5: Remove and Recalculate
```
1. remove(key) called on AssetManager
2. Notification: Removed sent
3. Lock AssetData, clear data/binary
4. Remove AssetRef from AssetManager maps
5. Remove data from store
6. (Optional) If recipe exists, trigger get_asset(key) for recalculation
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

**Resolution**: This is a bug. See `specs/JOBQUEUE_FIX.md` for the fix specification.

### 2. Dependencies Status (RESOLVED - needs implementation)
**Problem**: Dependencies status exists but is never set.

**Resolution**: The interpreter should set this status when waiting for dependencies. See `specs/DEPENDENCIES_STATUS.md` for implementation spec.

### 3. Cancel → Set Path (RESOLVED)
**Problem**: What status path for set() on Processing asset?

**Resolution**: Direct transition: `Processing → Cancelled → Override/Source`. After cancel() completes, set() directly sets the final status.

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

**Resolution**: Same behavior for all statuses - always delete. Recipes cannot be deleted (at the moment). After remove(), if AssetManager is asked for the asset and a recipe exists, the asset automatically starts in `Recipe` status. If no recipe exists, the asset doesn't exist.

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

## Open Issues

### Issue 1: Error Recovery / Retry
**Problem**: After Error status, how does retry work?

**Current understanding**: Retry would need to:
1. Reset status to Recipe or None
2. Resubmit to job queue

**Question**: Should there be an explicit `retry()` method, or should `get_asset()` automatically retry failed assets?

### Issue 2: Circular Dependencies
**Problem**: When implementing Dependencies status, how to detect A→B→A cycles?

**Options**:
- Detect during dependency resolution
- Timeout-based detection
- Require explicit dependency declaration

### Issue 3: Dependency Invalidation Cascade
**Problem**: When an asset changes (via set() or recompute), should dependent assets be automatically invalidated?

**Current**: Not implemented, marked as future work.

## References

- Implementation: `liquers-core/src/assets.rs`
- Metadata/Status: `liquers-core/src/metadata.rs`
- Set Operation Spec: `specs/ASSET_SET_OPERATION.md`
- JobQueue Fix Spec: `specs/JOBQUEUE_FIX.md`
- Dependencies Spec: `specs/DEPENDENCIES_STATUS.md`
- Store interface: `liquers-core/src/store.rs`
