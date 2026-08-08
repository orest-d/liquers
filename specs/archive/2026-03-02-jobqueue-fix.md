# JobQueue Fix Specification

**Status: IMPLEMENTED** (see `liquers-core/src/assets.rs`)

## Problem

In `liquers-core/src/assets.rs` at line ~1858, the `JobQueue::submit()` method has buggy logic:

```rust
pub async fn submit(&self, asset: AssetRef<E>) -> Result<(), Error> {
    let pending_count = self.pending_jobs_count().await;
    if pending_count < self.capacity {
        // Run immediately...
        let asset_clone = asset.clone();
        if let Err(e) = asset_clone.set_status(Status::Processing).await {
            eprintln!("Failed to set status for asset {}: {}", asset.id(), e);
        }
        tokio::spawn(async move {
            let _ = asset_clone.run().await;
        });
    } else {
        asset.submitted().await?;
    }
    let mut jobs = self.jobs.lock().await;
    let asset_id = asset.id();
    jobs.push(asset);
    jobs.retain(|a| a.id() != asset_id);  // BUG: Removes the just-pushed asset!
    Ok(())
}
```

The `jobs.retain()` call immediately removes the asset that was just pushed.

## Intended Behavior

1. **Duplicate Check**: If the same asset (by id) is already in the queue, keep the existing one (don't add duplicate)
2. **Capacity Check**: If fewer jobs are running than capacity, run immediately with `Processing` status
3. **Queue if Full**: If at capacity, set status to `Submitted` and add to queue
4. **Concurrency Limit**: Never run more jobs simultaneously than the specified capacity

## Corrected Logic

```rust
pub async fn submit(&self, asset: AssetRef<E>) -> Result<(), Error> {
    let asset_id = asset.id();

    // Check for duplicates first
    {
        let jobs = self.jobs.lock().await;
        if jobs.iter().any(|a| a.id() == asset_id) {
            // Asset already in queue, don't add duplicate
            return Ok(());
        }
    }

    let pending_count = self.pending_jobs_count().await;

    if pending_count < self.capacity {
        // Capacity available - run immediately
        let asset_clone = asset.clone();
        if let Err(e) = asset_clone.set_status(Status::Processing).await {
            eprintln!("Failed to set status for asset {}: {}", asset.id(), e);
        }

        // Add to jobs list for tracking
        {
            let mut jobs = self.jobs.lock().await;
            jobs.push(asset);
        }

        tokio::spawn(async move {
            let _ = asset_clone.run().await;
        });
    } else {
        // At capacity - queue the job
        asset.submitted().await?;

        let mut jobs = self.jobs.lock().await;
        jobs.push(asset);
    }

    Ok(())
}
```

## Additional Considerations

### 1. Cleanup of Finished Jobs
The `cleanup_completed()` method exists but may not be called regularly. Consider:
- Calling cleanup in the `run()` loop periodically
- Or cleaning up finished jobs in `submit()` before checking capacity

### 2. Race Condition in Duplicate Check
The current fix has a potential race between checking for duplicates and adding to queue. For robustness:

```rust
pub async fn submit(&self, asset: AssetRef<E>) -> Result<(), Error> {
    let asset_id = asset.id();
    let mut jobs = self.jobs.lock().await;

    // Check for duplicates while holding lock
    if jobs.iter().any(|a| a.id() == asset_id) {
        return Ok(());
    }

    // Count pending while holding lock
    let pending_count = jobs.iter()
        .filter(|a| {
            // Note: Can't await here, need sync status check
            // This is a limitation - may need redesign
        })
        .count();

    // ... rest of logic
}
```

**Problem**: `pending_jobs_count()` is async (calls `asset.status().await`), but we can't await while holding the mutex lock.

### 3. Proposed Redesign

Consider adding a sync status check or tracking running count separately:

```rust
pub struct JobQueue<E: Environment> {
    jobs: Arc<Mutex<Vec<AssetRef<E>>>>,
    running_count: Arc<AtomicUsize>,  // Track running jobs separately
    capacity: usize,
}
```

Then increment `running_count` when starting a job and decrement when finished.

## Files to Modify

1. **`liquers-core/src/assets.rs`**
   - Fix `JobQueue::submit()` method
   - Consider adding `running_count` atomic counter
   - Update `run()` loop to call `cleanup_completed()` periodically
   - Add sync method to check if asset is in queue

## Tests to Add

```rust
#[tokio::test]
async fn test_submit_no_duplicates() {
    // Submit same asset twice, verify only one in queue
}

#[tokio::test]
async fn test_submit_respects_capacity() {
    // Submit more jobs than capacity, verify correct number running
}

#[tokio::test]
async fn test_submit_immediate_when_capacity() {
    // Submit when under capacity, verify immediate execution
}

#[tokio::test]
async fn test_cleanup_removes_finished() {
    // Verify cleanup removes Ready/Error/Cancelled jobs
}
```

## Implementation Priority

1. Fix the immediate bug (retain logic)
2. Add duplicate checking
3. Consider atomic running counter for better concurrency
4. Add periodic cleanup to run loop
