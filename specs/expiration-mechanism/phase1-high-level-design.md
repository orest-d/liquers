# Phase 1: High-Level Design - Asset Expiration Mechanism

## Feature Name

Asset Expiration Mechanism

## Purpose

Enable time-based lifecycle management for assets by allowing them to automatically transition from Ready to Expired status after a specified duration. This supports use cases like rate-limited API data, time-sensitive computations, and automatic cache invalidation, while complementing the existing volatility system (which marks assets as "use once" vs. "expires after duration").

## Core Interactions

### Query System
No direct query syntax changes. Expiration is metadata-driven and transparent to query execution. Assets retrieved from cache check expiration before use.

### Store System
No changes to AsyncStore trait. Store continues to persist assets with metadata including expiration fields. Expired assets remain accessible via AssetRef but are removed from AssetManager. Expired assets can be read and changed to Override status to preserve user-modified data.

### Command System
Command metadata extended with `expires` field (similar to existing `volatile` field). Register_command! macro gains `expires:` metadata parameter. Example: `expires: "in 5 min"` or `expires: "EOD"`.

### Asset System
- **AssetRef** gains `expired()` method to manually trigger expiration (sets status to Expired, sends notification, removes from AssetManager)
- **AssetManager** monitors Ready assets with finite expiration_time, triggers expiration at the appropriate time (500ms accuracy sufficient). When asset becomes Ready, expiration_time must be at least 500ms in future.
- **AssetNotificationMessage** extended with `Expired` variant for expiration notifications
- **Status enum** already has `Expired` variant (no changes needed)
- Expiration time is fixed but assets may expire early (e.g., if dependency is modified in future). Assets will never expire after expiration_time.

### Metadata System
- **MetadataRecord, Metadata, AssetInfo** extended with two fields:
  - `expires: Expires` - specification of when asset should expire (e.g., "in 5 min", "EOD", "never"), defaults to Never
  - `expiration_time: ExpirationTime` - computed timestamp when asset will expire (UTC), defaults to Never
- **Expires enum** in new `liquers-core/src/expiration.rs` module - parses/serializes human-readable strings like "immediately", "never", "in 1 hour", "at 12:00", "EOD", "2026-03-01 15:00". Can specify timezone (defaults to system timezone).
- **ExpirationTime** enum with Never variant and Timestamp(DateTime<Utc>) variant - always stored in UTC

### Plan System
Plan extended with expiration inference logic (similar to existing volatility inference). For each step, compute minimum expiration_time from command metadata and known dependencies. Add info step documenting which dependency caused the expiration_time.

### Value Types
No new ExtValue variants. Expiration is orthogonal to value types.

### Web/API
No API changes. Expiration handled internally by AssetManager. Clients can subscribe to expiration notifications via existing notification channels.

### UI
No direct UI changes in Phase 1. UI elements using AssetRef can listen for Expired notifications and request fresh assets from AppState if needed.

## Crate Placement

**liquers-core** - Primary implementation
- New module: `src/expiration.rs` (Expires enum, ExpirationTime struct, parsing logic)
- Extend: `src/metadata.rs` (add expires/expiration_time fields)
- Extend: `src/command_metadata.rs` (add expires field)
- Extend: `src/assets.rs` (AssetRef.expired(), AssetManager monitoring logic)
- Extend: `src/plan.rs` (expiration inference similar to volatility)

**liquers-macro** - Macro extension
- Extend: register_command! macro to support `expires:` metadata parameter

No changes to liquers-store, liquers-lib, liquers-axum, or liquers-py in Phase 1.

## Design Decisions & Rationale

### 1. Monitoring Implementation: Interval vs. Priority Queue

**Option A: tokio::time::interval (polling)**
```rust
// Pros: Simple implementation, predictable overhead
// Cons: Checks all assets every interval, O(n) per tick
tokio::spawn(async move {
    let mut interval = time::interval(Duration::from_millis(500));
    loop {
        interval.tick().await;
        for asset in assets.iter() {
            if asset.is_expired() { asset.expire(); }
        }
    }
});
```

**Option B: Priority queue (BinaryHeap)**
```rust
// Pros: O(log n) insert/remove, only wakes when needed
// Cons: More complex, requires heap reorganization on changes
let mut heap: BinaryHeap<(Instant, AssetId)> = BinaryHeap::new();
loop {
    if let Some((expire_at, id)) = heap.peek() {
        sleep_until(*expire_at).await;
        let (_, id) = heap.pop().unwrap();
        expire_asset(id);
    }
}
```

**Decision:** Phase 2 will benchmark both and choose based on typical asset counts (<1000 assets: interval fine; >10000 assets: priority queue)

### 2. Soft Expiration Model

**Expired assets remain readable** - AssetRef holders can still access data after expiration. This supports:
- Graceful degradation (stale data better than no data)
- **Override capability** - expired assets can be read and changed to Override status to preserve user-modified data
- Debugging (inspect expired assets to understand expiration cause)

### 3. Fixed Expiration Time

Expiration time is **immutable but can expire early**:
- Asset may expire before expiration_time if dependency is modified (future enhancement)
- Asset will NOT expire after expiration_time (guaranteed upper bound)
- When asset transitions to Ready, expiration_time must be at least 500ms in future (prevents immediate expiration race conditions)

### 4. Timezone Handling

- **Expires specification**: Can include timezone (e.g., "at 12:00 PST", "EOD EST"), defaults to system timezone
- **ExpirationTime storage**: Always UTC internally (avoids DST ambiguity, simplifies comparison)
- Conversion happens during Expires → ExpirationTime computation in plan

### 5. Automatic Dependency Propagation

**Out of scope for Phase 1.** When a dependency expires, dependents are NOT automatically expired. Future phases may add:
- Dependency tracking (record which assets depend on which)
- Propagation policy (immediate vs. lazy expiration of dependents)
- Notification chains (notify UI of entire dependency tree expiration)

## References

- Related: `liquers-core/src/metadata.rs` (Status::Expired, Status::Volatile, is_volatile field)
- Related: `liquers-core/src/interpreter.rs` (IsVolatile trait for volatility inference)
- Related: Command metadata volatility pattern (CommandMetadata.volatile field)
- Inspiration: Redis EXPIRE command, HTTP Cache-Control max-age directive
