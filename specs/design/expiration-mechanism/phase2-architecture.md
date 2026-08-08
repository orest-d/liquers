# Phase 2: Solution & Architecture - Asset Expiration Mechanism

## Overview

The expiration mechanism is implemented as a new `liquers-core/src/expiration.rs` module defining `Expires` (human-readable specification) and `ExpirationTime` (UTC timestamp or Never). These types are integrated into metadata structures (MetadataRecord, AssetInfo, Metadata), command metadata (CommandMetadata), the plan builder (expiration inference from dependencies), and the asset manager (background monitoring with expiration trigger). The register_command! macro gains `expires:` keyword support. The implementation mirrors the existing volatility pattern throughout the codebase.

## Data Structures

### New Module: `liquers-core/src/expiration.rs`

#### Enum: Expires

```rust
/// Specification of when an asset should expire, relative to current time.
/// Serializes/deserializes as a human-readable string.
#[derive(Debug, Clone, PartialEq)]
pub enum Expires {
    /// Asset never expires (default)
    Never,
    /// Asset expires immediately (implies volatile)
    Immediately,
    /// Asset expires after a duration from when it becomes Ready
    InDuration(std::time::Duration),
    /// Asset expires at a specific time of day (UTC or with timezone)
    AtTimeOfDay {
        hour: u32,
        minute: u32,
        second: u32,
        /// Timezone offset in seconds from UTC. None = system timezone.
        tz_offset: Option<i32>,
    },
    /// Asset expires on a specific day of week at 00:00
    OnDayOfWeek {
        /// 0=Monday, 6=Sunday (chrono::Weekday convention)
        day: u32,
        /// Timezone offset in seconds from UTC. None = system timezone.
        tz_offset: Option<i32>,
    },
    /// Asset expires at the end of the current day (next 00:00)
    EndOfDay {
        /// Timezone offset in seconds from UTC. None = system timezone.
        tz_offset: Option<i32>,
    },
    /// Asset expires at end of week (next Monday 00:00)
    EndOfWeek {
        /// Timezone offset in seconds from UTC. None = system timezone.
        tz_offset: Option<i32>,
    },
    /// Asset expires at end of month (1st of next month 00:00)
    EndOfMonth {
        /// Timezone offset in seconds from UTC. None = system timezone.
        tz_offset: Option<i32>,
    },
    /// Asset expires at a specific date and time (always UTC after parsing)
    AtDateTime(chrono::DateTime<chrono::Utc>),
}
```

**Variant semantics:**
- `Never`: Default. Asset does not expire. `is_volatile()` on this evaluates to false.
- `Immediately`: Asset expires right away. Implies volatile semantics. `to_expiration_time()` returns `ExpirationTime::Immediately`.
- `InDuration`: Relative expiration (e.g., "5 min", "in 1 hour"). Duration computed from when asset becomes Ready.
- `AtTimeOfDay`: Next occurrence of given time (e.g., "at 12:00"). If time has passed today, it means tomorrow.
- `OnDayOfWeek`: Next occurrence of given weekday at 00:00 (e.g., "on Tuesday").
- `EndOfDay`: Alias for "at 00:00 tomorrow". "EOD" and "end of day" both parse to this.
- `EndOfWeek`: Next Monday at 00:00.
- `EndOfMonth`: 1st of next month at 00:00.
- `AtDateTime`: Absolute UTC timestamp (e.g., "2026-03-01 15:00").

**No default match arm:** All match statements on this enum must be explicit.

**Default:**
```rust
impl Default for Expires {
    fn default() -> Self {
        Expires::Never
    }
}
```

#### Enum: ExpirationTime

```rust
/// Computed absolute expiration timestamp, always in UTC.
/// This is the resolved form of Expires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpirationTime {
    /// Asset never expires
    Never,
    /// Asset expires immediately (volatile)
    Immediately,
    /// Asset expires at a specific UTC timestamp
    At(chrono::DateTime<chrono::Utc>),
}
```

**Variant semantics:**
- `Never`: No expiration. Sorts as maximum (greater than any `At` timestamp).
- `Immediately`: Immediate expiration. Sorts as minimum (less than any `At` timestamp).
- `At(DateTime<Utc>)`: Specific UTC timestamp.

**Ordering:** `Immediately < At(any_time) < Never`. This supports `min()` for dependency-based inference.

**Manual Ord/PartialOrd impl required** (not derived, since variant order doesn't match desired semantics):
```rust
impl Ord for ExpirationTime {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (ExpirationTime::Immediately, ExpirationTime::Immediately) => std::cmp::Ordering::Equal,
            (ExpirationTime::Immediately, _) => std::cmp::Ordering::Less,
            (_, ExpirationTime::Immediately) => std::cmp::Ordering::Greater,
            (ExpirationTime::Never, ExpirationTime::Never) => std::cmp::Ordering::Equal,
            (ExpirationTime::Never, _) => std::cmp::Ordering::Greater,
            (_, ExpirationTime::Never) => std::cmp::Ordering::Less,
            (ExpirationTime::At(a), ExpirationTime::At(b)) => a.cmp(b),
        }
    }
}
impl PartialOrd for ExpirationTime {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
```

**Default:**
```rust
impl Default for ExpirationTime {
    fn default() -> Self {
        ExpirationTime::Never
    }
}
```

**Key methods:**
```rust
impl ExpirationTime {
    /// Returns true if the asset has expired at the given time
    pub fn is_expired_at(&self, now: chrono::DateTime<chrono::Utc>) -> bool;

    /// Returns true if the asset has expired now
    pub fn is_expired(&self) -> bool;

    /// Returns true if this is Never
    pub fn is_never(&self) -> bool;

    /// Returns true if this is Immediately
    pub fn is_immediately(&self) -> bool;

    /// Returns the minimum of two expiration times
    /// Used for dependency-based inference: min(self, dependency) = earliest expiration
    pub fn min(self, other: ExpirationTime) -> ExpirationTime;

    /// If expiration is in the past or within min_future duration, adjust to now + min_future.
    /// Used when asset transitions to Ready to ensure at least 500ms before expiration.
    pub fn ensure_future(&self, min_future: std::time::Duration) -> ExpirationTime;
}
```

### Parsing & Serialization: Expires

**String format examples:**
```
"never"
"immediately"
"5 min"
"in 5 min"
"in 1 hour"
"in 30 seconds"
"in 500 ms"
"in 2 days"
"in 1 week"
"in 3 months"
"at 12:00"
"at 15:30 UTC"
"at 12:00 EST"
"on Tuesday"
"on monday"
"EOD"
"end of day"
"end of week"
"end of month"
"2026-03-01"
"2026-03-01 15:00"
"2026-03-01T15:00:00Z"
```

**Parsing rules:**
1. Case-insensitive for keywords ("Never", "NEVER", "never" all work)
2. "in" prefix is optional for durations ("5 min" = "in 5 min")
3. Duration units: `ms`, `milliseconds`, `s`, `sec`, `seconds`, `min`, `minutes`, `h`, `hr`, `hours`, `d`, `days`, `w`, `weeks`, `mo`, `months`
4. "EOD" and "end of day" are aliases for EndOfDay
5. Day names: full ("Monday") and abbreviated ("Mon") accepted, case-insensitive
6. Timezone abbreviations: UTC, EST, CST, MST, PST, CET, EET (common abbreviations mapped to offsets)
7. Date formats: ISO 8601 (`2026-03-01`, `2026-03-01T15:00:00Z`), and `YYYY-MM-DD HH:MM`

**Implementation approach:** Use `nom` parser combinators (already a dependency) for structured parsing. Fall back to chrono's DateTime parsing for absolute timestamps.

```rust
impl std::str::FromStr for Expires {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Error>;
}

impl std::fmt::Display for Expires {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result;
}
```

**Serde:** String-based serialization for human-readable configs.
```rust
impl Serialize for Expires {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Expires {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}
```

### Parsing & Serialization: ExpirationTime

**Serde:** Serializes as `"never"`, `"immediately"`, or RFC 3339 UTC string.
```rust
impl Serialize for ExpirationTime {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error>;
}

impl<'de> Deserialize<'de> for ExpirationTime {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error>;
}

impl std::fmt::Display for ExpirationTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result;
}
```

### Conversion: Expires → ExpirationTime

```rust
impl Expires {
    /// Compute ExpirationTime from Expires specification at the given reference time.
    /// Reference time is typically when the asset becomes Ready.
    /// tz_offset_default is the system timezone offset in seconds if not specified in the Expires spec.
    pub fn to_expiration_time(
        &self,
        reference_time: chrono::DateTime<chrono::Utc>,
        tz_offset_default: i32,
    ) -> ExpirationTime;

    /// Returns true if this Expires implies volatile semantics
    pub fn is_volatile(&self) -> bool;
}
```

## Metadata Structure Extensions

### MetadataRecord (liquers-core/src/metadata.rs)

Add two fields:

```rust
pub struct MetadataRecord {
    // ... existing fields ...

    /// Specification of when the asset should expire.
    /// Defaults to Never (no expiration).
    #[serde(default)]
    pub expires: Expires,

    /// Computed absolute expiration time (UTC).
    /// Defaults to Never (no expiration).
    #[serde(default)]
    pub expiration_time: ExpirationTime,
}
```

**Rationale:**
- Not optional — defaults to `Never` via `Default` impl (per user requirement)
- `#[serde(default)]` for backwards compatibility with existing serialized metadata that doesn't include these fields

**MetadataRecord::new() update:**
```rust
// Both fields will automatically be Never via Default derive (no explicit setting needed)
```

**New helper methods on MetadataRecord:**
```rust
impl MetadataRecord {
    /// Returns true if the asset has a finite expiration time (not Never)
    pub fn has_expiration(&self) -> bool;

    /// Convenience: check if asset is currently expired
    pub fn is_expired(&self) -> bool;
}
```

### AssetInfo (liquers-core/src/metadata.rs)

Add two fields:

```rust
pub struct AssetInfo {
    // ... existing fields ...

    /// Specification of when the asset should expire.
    #[serde(default)]
    pub expires: Expires,

    /// Computed absolute expiration time (UTC).
    #[serde(default)]
    pub expiration_time: ExpirationTime,
}
```

**Update From<MetadataRecord> for AssetInfo and From<AssetInfo> for MetadataRecord:**
```rust
// Copy expires and expiration_time between AssetInfo and MetadataRecord
```

### Metadata enum (liquers-core/src/metadata.rs)

Add accessor methods following the established delegation patterns:

**Getters** (follow `is_volatile()`, `status()` pattern):
```rust
impl Metadata {
    /// Get expiration specification.
    /// For LegacyMetadata: tries to parse "expires" field from JSON object,
    /// returns Expires::Never if not present or unparseable.
    pub fn expires(&self) -> Expires {
        match self {
            Metadata::MetadataRecord(mr) => mr.expires.clone(),
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                if let Some(Value::String(s)) = o.get("expires") {
                    s.parse().unwrap_or(Expires::Never)
                } else {
                    Expires::Never
                }
            }
            Metadata::LegacyMetadata(_) => Expires::Never,
        }
    }

    /// Get computed expiration time (UTC).
    /// For LegacyMetadata: tries to parse "expiration_time" field from JSON object,
    /// returns ExpirationTime::Never if not present or unparseable.
    pub fn expiration_time(&self) -> ExpirationTime {
        match self {
            Metadata::MetadataRecord(mr) => mr.expiration_time.clone(),
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                if let Some(Value::String(s)) = o.get("expiration_time") {
                    // Deserialize from "never", "immediately", or RFC 3339 string
                    serde_json::from_value(Value::String(s.clone()))
                        .unwrap_or(ExpirationTime::Never)
                } else {
                    ExpirationTime::Never
                }
            }
            Metadata::LegacyMetadata(_) => ExpirationTime::Never,
        }
    }

    /// Returns true if asset has finite expiration (not Never).
    pub fn has_expiration(&self) -> bool {
        !self.expiration_time().is_never()
    }

    /// Returns true if the asset is currently expired based on expiration_time,
    /// or if the status is already Expired.
    pub fn is_expired(&self) -> bool {
        self.status() == Status::Expired || self.expiration_time().is_expired()
    }
}
```

**Setters** (follow `set_status()`, `with_key()` pattern):
```rust
impl Metadata {
    /// Set expiration specification.
    /// For LegacyMetadata(Object): inserts serialized string into JSON.
    /// For LegacyMetadata(Null): upgrades to MetadataRecord.
    /// For other LegacyMetadata: returns error.
    pub fn set_expires(&mut self, expires: Expires) -> Result<&mut Self, Error> {
        match self {
            Metadata::MetadataRecord(mr) => {
                mr.expires = expires;
                Ok(self)
            }
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                o.insert("expires".to_string(), Value::String(expires.to_string()));
                Ok(self)
            }
            Metadata::LegacyMetadata(serde_json::Value::Null) => {
                let mut m = MetadataRecord::new();
                m.expires = expires;
                *self = Metadata::MetadataRecord(m);
                Ok(self)
            }
            _ => Err(Error::general_error(
                "Cannot set expires on unsupported legacy metadata".to_string(),
            )),
        }
    }

    /// Set computed expiration time.
    /// Same LegacyMetadata handling pattern as set_expires.
    pub fn set_expiration_time(&mut self, expiration_time: ExpirationTime) -> Result<&mut Self, Error> {
        match self {
            Metadata::MetadataRecord(mr) => {
                mr.expiration_time = expiration_time;
                Ok(self)
            }
            Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
                o.insert(
                    "expiration_time".to_string(),
                    Value::String(expiration_time.to_string()),
                );
                Ok(self)
            }
            Metadata::LegacyMetadata(serde_json::Value::Null) => {
                let mut m = MetadataRecord::new();
                m.expiration_time = expiration_time;
                *self = Metadata::MetadataRecord(m);
                Ok(self)
            }
            _ => Err(Error::general_error(
                "Cannot set expiration_time on unsupported legacy metadata".to_string(),
            )),
        }
    }
}
```

**Pattern notes:**
- Getters return `Expires::Never` / `ExpirationTime::Never` for legacy metadata without the field — consistent with how `is_volatile()` returns `false` for legacy metadata without `is_volatile` field.
- Setters follow the `set_status()` 4-branch pattern: `MetadataRecord` (direct set), `LegacyMetadata(Object)` (JSON insert), `LegacyMetadata(Null)` (upgrade to MetadataRecord), `_` (error).
- Setters return `Result<&mut Self, Error>` for chaining (like `with_key()` and `set_status()`).
- Legacy JSON uses Display/FromStr (for `expires`) and serde string deserialization (for `expiration_time`) for storage.

## Command Metadata Extension

### CommandMetadata (liquers-core/src/command_metadata.rs)

Add field:

```rust
pub struct CommandMetadata {
    // ... existing fields ...

    /// Expiration specification for this command's output.
    /// If set (not Never), the result of this command will expire after the specified time.
    /// Like volatile, if an expiring command appears in a plan, all downstream steps
    /// inherit the minimum expiration time.
    #[serde(default)]
    pub expires: Expires,
}
```

**CommandMetadata::new() update:**
```rust
// expires: Expires::Never (via Default)
```

**Relationship to volatile:**
- `volatile: true` implies `Expires::Immediately`
- `expires: "immediately"` implies `volatile: true`
- Both can be set independently; the minimum is used (Immediately < any Duration)

## register_command! Macro Extension

### liquers-macro/src/lib.rs

**New DSL keyword:** `expires:`

```rust
// Usage in register_command!
register_command!(cr,
    fn my_command(state, param: String) -> result
    namespace: "my_ns"
    expires: "in 5 min"
)?;
```

**Parsing:** Add `Expires(String)` variant to `CommandSignatureStatement` enum.

```rust
pub(crate) enum CommandSignatureStatement {
    // ... existing variants ...
    Expires(String),  // raw string literal, parsed at runtime
}
```

**Keyword parsing:**
```rust
"expires" => {
    let _colon: syn::Token![:] = input.parse()?;
    let lit: syn::LitStr = input.parse()?;
    Ok(CommandSignatureStatement::Expires(lit.value()))
}
```

**Code generation:**
```rust
// In CommandSpec code generation
let expires_code = if !self.expires.is_empty() {
    let expires_str = &self.expires;
    quote! {
        cm.expires = #expires_str.parse().map_err(|e: liquers_core::error::Error| e)?;
    }
} else {
    quote!()
};
```

**CommandSpec extension:**
```rust
pub(crate) struct CommandSpec {
    // ... existing fields ...
    pub expires: String,  // empty string = not set = Never
}
```

## Plan Builder Extension

### PlanBuilder (liquers-core/src/plan.rs)

Add expiration tracking fields:

```rust
pub(crate) struct PlanBuilder<'c> {
    // ... existing fields ...

    /// Track minimum expiration from command metadata during plan building
    expires: Expires,
}
```

**New methods on PlanBuilder:**
```rust
impl<'c> PlanBuilder<'c> {
    /// Update plan expiration based on command metadata
    /// Takes the minimum of current and new expiration
    fn update_expiration(&mut self, command_expires: &Expires);

    /// Check if action command has expiration via CommandMetadata
    fn get_action_expiration(&self, command_key: &CommandKey) -> Expires;
}
```

**Integration into build():**
```rust
pub fn build(&mut self) -> Result<Plan, Error> {
    // ... existing logic ...
    self.plan.expires = self.expires.clone();
    // ... set is_volatile field ...
}
```

### Plan struct extension:

```rust
pub struct Plan {
    // ... existing fields ...

    /// Expiration specification inferred from commands in this plan.
    /// Computed as minimum expiration across all commands in the plan.
    #[serde(default)]
    pub expires: Expires,
}
```

### has_expirable_dependencies (new function, parallel to has_volatile_dependencies):

```rust
/// Check if plan has dependencies with expiration (Phase 2 check)
/// Finds the minimum expiration time across all dependency recipes.
/// Adds info step documenting which dependency caused the expiration time.
pub(crate) async fn has_expirable_dependencies<E: Environment>(
    envref: EnvRef<E>,
    plan: &mut Plan,
) -> Result<(), Error>;
```

**Integration into make_plan (interpreter.rs):**
```rust
pub(crate) async fn make_plan<E: Environment>(...) -> Result<Plan, Error> {
    // Phase 1: Build plan (existing)
    let plan = plan_builder.build()?;
    // Phase 2: Check volatile dependencies (existing)
    has_volatile_dependencies(envref.clone(), &mut plan).await?;
    // Phase 3: Check expirable dependencies (NEW)
    has_expirable_dependencies(envref.clone(), &mut plan).await?;
    Ok(plan)
}
```

### Recipe extension:

```rust
pub struct Recipe {
    // ... existing fields ...

    /// Expiration specification from command metadata or plan inference
    #[serde(default)]
    pub expires: Expires,
}
```

## Asset System Extension

### AssetData (liquers-core/src/assets.rs)

Add field:

```rust
pub struct AssetData<E: Environment> {
    // ... existing fields ...

    /// Computed expiration time for this asset (UTC, or Never)
    expiration_time: ExpirationTime,
}
```

**Initialization:** `expiration_time: ExpirationTime::Never` in constructors.

**Status finalization (in try_to_set_ready):**
```rust
// After existing volatile check:
if lock.is_volatile {
    // ... existing volatile logic ...
} else {
    lock.status = Status::Ready;
    // Compute expiration_time from recipe's expires specification
    let expires = &lock.recipe.expires; // Or from plan
    let now = chrono::Utc::now();
    let system_tz_offset = chrono::Local::now().offset().local_minus_utc();
    let expiration_time = expires.to_expiration_time(now, system_tz_offset);
    let expiration_time = expiration_time.ensure_future(
        std::time::Duration::from_millis(500)
    );
    lock.expiration_time = expiration_time.clone();
    if let Metadata::MetadataRecord(ref mut mr) = lock.metadata {
        mr.status = Status::Ready;
        mr.expires = expires.clone();
        mr.expiration_time = expiration_time;
    }
}
```

### AssetNotificationMessage extension:

```rust
pub enum AssetNotificationMessage {
    // ... existing variants ...
    Expired,
}
```

### AssetRef extension:

```rust
impl<E: Environment> AssetRef<E> {
    /// Manually trigger expiration of this asset.
    /// Sets status to Expired, sends Expired notification.
    /// Returns Ok(()) if successfully expired, Err if asset was already in a terminal non-Ready state.
    pub async fn expire(&self) -> Result<(), Error>;

    /// Get the current expiration time
    pub async fn expiration_time(&self) -> ExpirationTime;

    /// Check if this asset is currently expired
    pub async fn is_expired(&self) -> bool;

    /// Schedule automatic expiration via a spawned tokio task with Weak reference.
    /// Used for unmanaged assets (from apply_immediately) that are not tracked
    /// by the AssetManager's monitor. If all holders drop before expiration,
    /// the Weak ref becomes invalid and the task exits as no-op.
    pub fn schedule_expiration(&self, expiration_time: &ExpirationTime);
}
```

### DefaultAssetManager monitoring:

Two expiration mechanisms handle the two kinds of assets:

1. **Managed assets** (stored in `assets`/`query_assets` scc::HashMap) — tracked by the monitor task via priority queue + scheduled sleep
2. **Unmanaged assets** (from `apply_immediately`, not in the manager's maps) — each schedules its own expiration via `AssetRef::schedule_expiration()` with a Weak reference

#### Managed asset monitoring (priority queue)

```rust
pub struct DefaultAssetManager<E: Environment> {
    // ... existing fields ...

    /// Sender to signal the monitor task about new expiring assets.
    /// Non-generic message type for safe async task crossing.
    monitor_tx: mpsc::UnboundedSender<ExpirationMonitorMessage>,
}
```

**ExpirationMonitorMessage (non-generic to cross async task boundaries):**
```rust
/// Non-generic message type for the expiration monitor task.
/// Does not carry AssetRef directly — the monitor refetches from
/// the scc::HashMap when it needs to expire an asset.
enum ExpirationMonitorMessage {
    /// Track a new expiring asset by key and expiration time
    Track { key: Key, expiration_time: ExpirationTime },
    /// Stop tracking an asset (removed or expired)
    Untrack { key: Key },
    /// Shutdown the monitor
    Shutdown,
}
```

**Monitor task (priority queue + sleep_until):**
```rust
// The monitor holds references to the assets and query_assets maps
// and receives tracking messages via mpsc channel.
// Uses BinaryHeap as priority queue, sleeping until next expiration.
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};

tokio::spawn(async move {
    // Priority queue: earliest expiration at top
    let mut heap: BinaryHeap<Reverse<(chrono::DateTime<chrono::Utc>, Key)>> = BinaryHeap::new();
    // Keys that have been untracked but may still be in the heap
    let mut cancelled: HashSet<Key> = HashSet::new();

    loop {
        // Compute sleep duration until next expiration (or long sleep if empty)
        let sleep_duration = if let Some(Reverse((earliest, _))) = heap.peek() {
            let now = chrono::Utc::now();
            if *earliest <= now {
                std::time::Duration::ZERO
            } else {
                (*earliest - now).to_std().unwrap_or(std::time::Duration::from_millis(100))
            }
        } else {
            std::time::Duration::from_secs(3600) // Nothing to track — sleep long
        };

        tokio::select! {
            _ = tokio::time::sleep(sleep_duration) => {
                let now = chrono::Utc::now();
                // Expire all assets whose time has passed
                while let Some(Reverse((et, _))) = heap.peek() {
                    if *et <= now {
                        let Reverse((_, key)) = heap.pop().unwrap();
                        if cancelled.remove(&key) {
                            continue; // Was untracked, skip
                        }
                        // Look up AssetRef from the shared assets map
                        // Call asset_ref.expire() if found
                        // Remove from assets map after expiration
                    } else {
                        break; // Remaining entries are in the future
                    }
                }
            }
            msg = monitor_rx.recv() => {
                match msg {
                    Some(ExpirationMonitorMessage::Track { key, expiration_time }) => {
                        if let ExpirationTime::At(dt) = expiration_time {
                            cancelled.remove(&key); // In case re-tracked
                            heap.push(Reverse((dt, key)));
                        }
                        // ExpirationTime::Never and Immediately are not tracked here
                    }
                    Some(ExpirationMonitorMessage::Untrack { key }) => {
                        // BinaryHeap doesn't support efficient removal,
                        // so mark as cancelled — skipped when popped
                        cancelled.insert(key);
                    }
                    Some(ExpirationMonitorMessage::Shutdown) | None => break,
                }
            }
        }
    }
});
```

**Rationale for priority queue approach:**
- **No polling overhead** — the monitor sleeps until the next expiration time, waking only when needed
- When a new asset is tracked with an earlier expiration time, `tokio::select!` interrupts the current sleep and recomputes the sleep duration from the new heap top
- `cancelled` HashSet handles Untrack without heap reorganization (lazy deletion)
- Multiple cleanup events naturally batch when several assets expire at similar times

#### Unmanaged asset expiration (Weak-reference scheduled tasks)

Assets created by `apply_immediately` are not stored in the manager's `assets`/`query_assets` maps. They need a self-contained expiration mechanism.

**New method on AssetRef:**
```rust
impl<E: Environment> AssetRef<E> {
    /// Schedule automatic expiration of this asset after the given expiration time.
    /// Spawns a tokio task that holds a Weak reference to the asset data.
    /// If all AssetRef holders drop before expiration, the Weak reference
    /// becomes invalid and the task exits cleanly (no-op).
    /// If the asset is still alive at expiration time, expire() is called.
    pub fn schedule_expiration(&self, expiration_time: &ExpirationTime) {
        if let ExpirationTime::At(dt) = expiration_time {
            let weak_data = Arc::downgrade(&self.data);
            let id = self.id;
            let dt = *dt;
            tokio::spawn(async move {
                let now = chrono::Utc::now();
                if dt > now {
                    let duration = (dt - now).to_std().unwrap_or_default();
                    tokio::time::sleep(duration).await;
                }
                // Try to upgrade Weak → Arc
                if let Some(data) = weak_data.upgrade() {
                    let asset_ref = AssetRef { id, data };
                    let _ = asset_ref.expire().await;
                }
                // If Weak upgrade fails, all holders dropped — nothing to do
            });
        }
        // Never and Immediately: no task needed
    }
}
```

**Integration with apply_immediately:**
```rust
async fn apply_immediately(&self, recipe: Recipe, to: E::Value, payload: Option<E::Payload>)
    -> Result<AssetRef<E>, Error>
{
    // ... existing logic: create asset_ref, run_immediately ...
    asset_ref.run_immediately(payload).await?;

    // NEW: Schedule expiration for unmanaged assets
    let expiration_time = asset_ref.expiration_time().await;
    if !expiration_time.is_never() {
        asset_ref.schedule_expiration(&expiration_time);
    }

    Ok(asset_ref)
}
```

**Why Weak references:**
- Prevents the expiration task from keeping the asset alive after all callers have dropped it
- Clean garbage collection: if no one cares about the asset, the spawned task exits silently
- No need for the AssetManager to track unmanaged assets in its maps

### AssetManager trait extension:

```rust
pub trait AssetManager<E: Environment>: Send + Sync {
    // ... existing methods ...

    /// Register a managed asset for expiration monitoring.
    /// Called internally after asset transitions to Ready with non-Never expiration.
    /// For managed assets (stored in the manager's maps).
    async fn track_expiration(&self, key: &Key, expiration_time: &ExpirationTime) -> Result<(), Error>;

    /// Untrack a managed asset from expiration monitoring.
    /// Called when asset is removed from manager or manually expired.
    async fn untrack_expiration(&self, key: &Key) -> Result<(), Error>;
}
```

**DefaultAssetManager implementation:**
- `track_expiration`: Sends `ExpirationMonitorMessage::Track { key, expiration_time }` to the monitor task.
- `untrack_expiration`: Sends `ExpirationMonitorMessage::Untrack { key }` to the monitor task.

## Sync vs Async Decisions

| Function | Async? | Rationale |
|----------|--------|-----------|
| `Expires::from_str` | No | CPU-bound parsing, no I/O |
| `Expires::to_expiration_time` | No | Pure computation (time math) |
| `ExpirationTime::is_expired` | No | Pure comparison |
| `ExpirationTime::min` | No | Pure comparison |
| `AssetRef::expire` | Yes | Acquires write lock, sends notification |
| `AssetRef::expiration_time` | Yes | Acquires read lock |
| `AssetRef::is_expired` | Yes | Acquires read lock |
| `AssetManager::track_expiration` | Yes | Sends message to monitor task |
| `AssetManager::untrack_expiration` | Yes | Sends message to monitor task |
| `AssetRef::schedule_expiration` | No (spawns) | Spawns async task, but method itself is sync |
| Monitor task | Yes | Priority queue + sleep_until loop |
| `has_expirable_dependencies` | Yes | Queries recipe provider |

## Function Signatures Summary

### Module: `liquers_core::expiration` (NEW)

```rust
// Core types
pub enum Expires { Never, Immediately, InDuration(..), ... }
pub enum ExpirationTime { Never, Immediately, At(DateTime<Utc>) }

// Parsing
impl FromStr for Expires { type Err = Error; fn from_str(s: &str) -> Result<Self, Error>; }
impl Display for Expires { fn fmt(&self, f: &mut Formatter) -> fmt::Result; }
impl Display for ExpirationTime { fn fmt(&self, f: &mut Formatter) -> fmt::Result; }
// Note: ExpirationTime does NOT implement FromStr. It is always computed via
// Expires::to_expiration_time() or deserialized via custom serde (which handles "never"/"immediately"/RFC3339).

// Conversion
impl Expires {
    pub fn to_expiration_time(&self, reference_time: DateTime<Utc>, tz_offset_default: i32) -> ExpirationTime;
    pub fn is_volatile(&self) -> bool;
    pub fn is_never(&self) -> bool;
}

impl ExpirationTime {
    pub fn is_expired_at(&self, now: DateTime<Utc>) -> bool;
    pub fn is_expired(&self) -> bool;
    pub fn is_never(&self) -> bool;
    pub fn is_immediately(&self) -> bool;
    pub fn min(self, other: ExpirationTime) -> ExpirationTime;
    pub fn ensure_future(&self, min_future: std::time::Duration) -> ExpirationTime;
}

// AssetRef expiration methods
impl<E: Environment> AssetRef<E> {
    pub async fn expire(&self) -> Result<(), Error>;
    pub async fn expiration_time(&self) -> ExpirationTime;
    pub async fn is_expired(&self) -> bool;
    pub fn schedule_expiration(&self, expiration_time: &ExpirationTime); // spawns tokio task with Weak ref
}
```

## Integration Points

### Crate: liquers-core

**New file:** `liquers-core/src/expiration.rs`
- Exports: `Expires`, `ExpirationTime`
- Dependencies: `chrono`, `nom` (both already in Cargo.toml), `serde`

**Modify:** `liquers-core/src/lib.rs`
- Add `pub mod expiration;`

**Modify:** `liquers-core/src/metadata.rs`
- Add `use crate::expiration::{Expires, ExpirationTime};`
- Add `expires` and `expiration_time` fields to `MetadataRecord` and `AssetInfo`
- Add accessor methods to `Metadata` enum
- Update `From<MetadataRecord> for AssetInfo` and vice versa

**Modify:** `liquers-core/src/command_metadata.rs`
- Add `use crate::expiration::Expires;`
- Add `expires` field to `CommandMetadata`
- Update `CommandMetadata::new()`

**Modify:** `liquers-core/src/plan.rs`
- Add `use crate::expiration::Expires;`
- Add `expires` field to `PlanBuilder` and `Plan`
- Add `update_expiration()`, `get_action_expiration()` to PlanBuilder
- Add `has_expirable_dependencies()` function

**Modify:** `liquers-core/src/interpreter.rs`
- Add Phase 3 call to `has_expirable_dependencies()` in `make_plan()`

**Modify:** `liquers-core/src/assets.rs`
- Add `use crate::expiration::ExpirationTime;`
- Add `expiration_time` field to `AssetData`
- Add `Expired` variant to `AssetNotificationMessage`
- Add `expire()`, `expiration_time()`, `is_expired()` to `AssetRef`
- Add `track_expiration()` to `AssetManager` trait
- Add `ExpirationMonitorMessage`, monitoring task to `DefaultAssetManager`
- Update status finalization to compute expiration_time

**Modify:** `liquers-core/src/recipes.rs`
- Add `use crate::expiration::Expires;`
- Add `expires` field to `Recipe`

### Crate: liquers-macro

**Modify:** `liquers-macro/src/lib.rs`
- Add `Expires(String)` variant to `CommandSignatureStatement`
- Add parsing for `expires:` keyword
- Add `expires: String` field to `CommandSpec`
- Add code generation for setting `cm.expires`

### Dependencies

**No new dependencies required.**
- `chrono = "0.4.31"` — already in `liquers-core/Cargo.toml`
- `nom` — already in `liquers-core/Cargo.toml`
- `serde` — already in `liquers-core/Cargo.toml`

## Relevant Commands

### New Commands

No new commands are introduced by this feature. Expiration is a metadata-level mechanism controlled via:
- `expires:` in register_command! macro DSL
- Recipe metadata
- Plan inference

### Relevant Existing Namespaces

| Namespace | Relevance | Notes |
|-----------|-----------|-------|
| (all) | Any command can declare `expires:` in metadata | Global mechanism, not namespace-specific |

## Web Endpoints

No new or modified HTTP endpoints. Expiration is transparent to the API layer. Existing notification subscription mechanisms can observe `Expired` messages.

## Error Handling

### Error Constructors

```rust
// Parsing errors
Error::general_error(format!("Invalid expiration specification: '{}'", input))
Error::general_error(format!("Invalid duration unit: '{}'", unit))
Error::general_error(format!("Invalid day of week: '{}'", day))

// Expiration errors
Error::general_error(format!("Cannot expire asset in state {:?}", status))
```

### Error Scenarios

| Scenario | ErrorType | Constructor |
|----------|-----------|-------------|
| Invalid Expires string | General | `Error::general_error(format!(...))` |
| Unknown duration unit | General | `Error::general_error(format!(...))` |
| Unknown timezone abbreviation | General | `Error::general_error(format!(...))` |
| Expire non-Ready asset | General | `Error::general_error(format!(...))` |
| Invalid date/time format | General | `Error::general_error(format!(...))` |

### Error Propagation

All fallible functions return `Result<T, Error>`. External chrono parsing errors wrapped via `Error::general_error()`.

## Serialization Strategy

### Expires
- Custom `Serialize`/`Deserialize` via `Display`/`FromStr` (human-readable strings)
- Round-trip: `"in 5 min"` → `Expires::InDuration(300s)` → `"in 5 min"`
- Canonical forms: always serialize to normalized form (e.g., "in 5 min" not "in 300 seconds")

### ExpirationTime
- Custom `Serialize`/`Deserialize`
- `Never` → `"never"`, `Immediately` → `"immediately"`, `At(dt)` → RFC 3339 string
- Round-trip: `"2026-03-01T15:00:00Z"` → `ExpirationTime::At(...)` → `"2026-03-01T15:00:00.000Z"`

### MetadataRecord / AssetInfo
- `#[serde(default)]` on both `expires` and `expiration_time` fields
- Backwards compatible: old metadata without these fields deserializes with `Never` defaults

## Concurrency Considerations

### Thread Safety

**ExpirationMonitorMessage** channel:
- `mpsc::UnboundedSender<ExpirationMonitorMessage>` is `Send + Sync` — non-generic message type, can be cloned and shared
- Monitor task runs in its own tokio task — no lock contention with main logic
- Monitor task holds `Arc` reference to shared `scc::HashMap` for AssetRef lookup during expiration
- Priority queue sleeps until next expiration — zero overhead between expirations

**AssetRef::expire():**
- Acquires write lock on `AssetData` — exclusive access during status transition
- Sends notification via `watch::Sender` — non-blocking

**AssetRef::schedule_expiration() (Weak references for unmanaged assets):**
- Spawned task holds `Weak<RwLock<AssetData>>` — does NOT prevent garbage collection
- If all AssetRef holders drop, Weak upgrade fails and task exits silently
- Uses `tokio::time::sleep` for precise scheduling — no polling

**scc::HashMap for assets:**
- Lock-free concurrent map — same pattern as existing `assets` and `query_assets` maps
- No additional synchronization needed

### Race Conditions

**Asset expires while being read:** Safe because AssetRef holders retain Arc-counted reference to data. Expiration only changes status and removes from AssetManager — data remains accessible.

**Multiple expirations of same asset:** `AssetRef::expire()` checks current status — only Ready/Source/Override assets can be expired. Second call returns error (idempotent-safe for callers that handle the error).

**Managed + unmanaged dual-expiration:** For managed assets, only the monitor task triggers expiration (via the priority queue). For unmanaged assets (apply_immediately), the spawned Weak-reference task triggers expiration. No overlap between the two paths.

## Compilation Validation

**Expected to compile:** Yes, after implementing all signatures.

**Key validation points:**
- `Expires` and `ExpirationTime` are `Clone + Debug + Default + PartialEq` — compatible with metadata struct derives
- `ExpirationTime` has `Ord` impl — required for `min()` and priority comparison
- `serde(default)` on new fields — backwards compatible deserialization
- No new external dependencies — uses existing chrono, nom, serde
- `AssetNotificationMessage::Expired` variant — all existing match statements will need updating (compile error = intended)

## References to liquers-patterns.md

- [x] Crate dependencies: all changes in liquers-core + liquers-macro (correct flow)
- [x] No new ExtValue variants (expiration is metadata-level)
- [x] Commands registered via register_command! macro (extends macro DSL)
- [x] Error handling uses typed constructors (Error::general_error)
- [x] Async is default for AssetRef methods, sync for parsing
- [x] No unwrap/expect in library code
- [x] No default match arms on new enums
- [x] Serialization uses serde with proper annotations
