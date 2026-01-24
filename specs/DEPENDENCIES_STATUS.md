# Dependencies Status Specification

## Overview

The `Dependencies` status indicates that an asset is waiting for its dependencies to be evaluated before it can proceed with its own computation. This status should be set by the interpreter/plan executor when an asset's recipe requires other assets that are not yet ready.

## Current State

- `Status::Dependencies` exists in the enum (`liquers-core/src/metadata.rs`)
- Status properties are defined:
  - `has_data()`: false
  - `is_finished()`: false
  - `is_processing()`: false
  - `can_have_tracked_dependencies()`: false
- **Not currently used**: No code sets this status

## Intended Behavior

### When to Set Dependencies Status

The interpreter should set `Status::Dependencies` when:
1. An asset's recipe is being evaluated
2. The recipe contains a query/key reference to another asset
3. That referenced asset is not yet ready (`!status.is_finished()` or `!status.has_data()`)

### State Transitions

```
┌───────────────┐
│   Submitted   │
└───────┬───────┘
        │
        │ JobStarted (interpreter begins)
        ▼
┌───────────────┐
│  Processing   │
└───────┬───────┘
        │
        │ (interpreter finds unready dependency)
        ▼
┌───────────────┐
│ Dependencies  │ ◄─────┐
└───────┬───────┘       │
        │               │
        │ (dependency   │ (another dependency
        │  ready)       │  not ready)
        │               │
        ▼               │
┌───────────────┐       │
│  Processing   │ ──────┘
└───────┬───────┘
        │
        │ (all dependencies ready, computation complete)
        ▼
┌───────────────┐
│     Ready     │
└───────────────┘
```

### Notifications

When transitioning to Dependencies:
- Service: (none needed, internal transition)
- Notification: `StatusChanged(Dependencies)`

When transitioning out of Dependencies:
- Service: (none, driven by dependency completion)
- Notification: `StatusChanged(Processing)`

## Implementation

### 1. Interpreter Changes

In the plan/query interpreter (`liquers-core/src/interpreter.rs` or similar):

```rust
async fn evaluate_step(&mut self, step: &PlanStep) -> Result<(), Error> {
    // Check if step requires a dependency
    if let Some(dep_query) = step.get_dependency() {
        let dep_asset = self.env.get_asset_manager().get_asset(&dep_query).await?;

        if !dep_asset.status().await.has_data() {
            // Set Dependencies status
            self.current_asset.set_status(Status::Dependencies).await?;

            // Wait for dependency
            dep_asset.wait_for_ready().await?;

            // Back to Processing
            self.current_asset.set_status(Status::Processing).await?;
        }

        // Use dependency value...
    }
    // ... rest of step evaluation
}
```

### 2. AssetRef Helper Method

Add a method to wait for an asset to become ready:

```rust
impl<E: Environment> AssetRef<E> {
    /// Wait for the asset to reach a finished state with data
    pub async fn wait_for_ready(&self) -> Result<(), Error> {
        let mut rx = self.subscribe_notifications();

        loop {
            let status = self.status().await;
            if status.has_data() {
                return Ok(());
            }
            if status.is_finished() && !status.has_data() {
                // Finished but no data (Error, Cancelled)
                return Err(Error::dependency_not_ready(&self.get_query()));
            }

            // Wait for next notification
            rx.changed().await.map_err(|_| {
                Error::general_error("Notification channel closed".to_string())
            })?;
        }
    }
}
```

### 3. Context Integration

The `Context` passed to commands should provide dependency resolution:

```rust
impl<E: Environment> Context<E> {
    /// Get a dependency asset, setting Dependencies status if needed
    pub async fn get_dependency(&self, query: &Query) -> Result<State<E::Value>, Error> {
        let asset = self.env.get_asset_manager().get_asset(query).await?;

        if !asset.status().await.has_data() {
            // Update our asset's status
            self.set_status(Status::Dependencies).await?;

            // Wait for dependency
            asset.wait_for_ready().await?;

            // Back to processing
            self.set_status(Status::Processing).await?;
        }

        asset.get_state().await
    }
}
```

## Dependency Tracking

### Recording Dependencies

When an asset uses another asset as a dependency, this should be recorded for:
1. Cache invalidation (when dependency changes, dependent should be invalidated)
2. Debugging/visualization of dependency graph

```rust
pub struct DependencyRecord {
    /// The asset that has the dependency
    pub dependent: Query,
    /// The asset being depended upon
    pub dependency: Query,
    /// When the dependency was recorded
    pub timestamp: String,
}
```

### Metadata Extension

Add to `MetadataRecord`:

```rust
pub struct MetadataRecord {
    // ... existing fields ...

    /// Assets this asset depends on
    #[serde(default)]
    pub dependencies: Vec<Query>,

    /// Assets that depend on this asset (reverse lookup, optional)
    #[serde(default)]
    pub dependents: Vec<Query>,
}
```

## Cancellation Behavior

When an asset in `Dependencies` status receives a `Cancel` message:
1. Stop waiting for dependencies
2. Transition to `Cancelled`
3. Do NOT cancel the dependencies (they may be used by others)

```rust
AssetServiceMessage::Cancel => {
    // Works same as Processing - just set Cancelled
    self.set_status(Status::Cancelled).await?;
    // ... send notifications
}
```

## Files to Modify

1. **`liquers-core/src/assets.rs`**
   - Add `wait_for_ready()` to AssetRef
   - Add `subscribe_notifications()` if not present
   - Handle Dependencies in `process_service_messages()` (no special handling needed)

2. **`liquers-core/src/context.rs`**
   - Add `get_dependency()` method
   - Add `set_status()` delegation to asset

3. **`liquers-core/src/interpreter.rs`** (or plan executor)
   - Set Dependencies status when waiting for dependencies
   - Use Context methods for dependency resolution

4. **`liquers-core/src/metadata.rs`**
   - Add `dependencies` field to MetadataRecord
   - Add `dependents` field (optional)

5. **`liquers-core/src/error.rs`**
   - Add `DependencyNotReady` error type

## Tests

```rust
#[tokio::test]
async fn test_dependencies_status_set_when_waiting() {
    // Create asset A that depends on asset B
    // Asset B is not ready
    // Verify A transitions to Dependencies status
}

#[tokio::test]
async fn test_dependencies_to_processing_when_ready() {
    // Asset A waiting on B
    // B becomes Ready
    // Verify A transitions back to Processing
}

#[tokio::test]
async fn test_cancel_during_dependencies() {
    // Asset in Dependencies status
    // Send Cancel
    // Verify transitions to Cancelled
}

#[tokio::test]
async fn test_dependency_error_propagation() {
    // Asset A depends on B
    // B fails with Error
    // Verify A receives appropriate error
}
```

## Open Questions

1. **Circular Dependencies**: How to detect and handle circular dependencies? (A depends on B, B depends on A)

2. **Timeout**: Should there be a timeout for waiting on dependencies?

3. **Partial Dependencies**: If an asset has multiple dependencies and one fails, should it:
   - Fail immediately?
   - Wait for all to complete/fail?
   - Continue with available dependencies?

4. **Dependency Caching**: Should resolved dependency values be cached in the dependent's context to avoid re-fetching?
