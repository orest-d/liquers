# Phase 4: Implementation Plan - Asset Expiration Mechanism

## Overview

**Feature:** Asset Expiration Mechanism

**Architecture:** New `liquers-core/src/expiration.rs` module defining `Expires` (human-readable specification) and `ExpirationTime` (UTC timestamp). These types are integrated into metadata (MetadataRecord, AssetInfo, Metadata), command metadata (CommandMetadata), plan builder (expiration inference), asset system (monitoring + expiration trigger), and the register_command! macro (`expires:` DSL keyword).

**Estimated complexity:** High

**Prerequisites:**
- Phase 1, 2, 3 approved
- All open questions resolved
- No new dependencies required (chrono, nom, serde already in Cargo.toml)

## Design Clarifications (from final review)

1. **`Expires` derives `PartialEq` AND `Eq`** — `std::time::Duration` and `chrono::DateTime<Utc>` both implement `Eq`, so this is safe and enables future use as HashMap key.

2. **`AssetRef::expire()` status contract:** `Ready` and `Override` statuses can transition to `Expired`. `Source` cannot be expired (no recipe to recover; expiring would be a soft delete with no recovery path). `Expired` is idempotent (returns Ok). All other statuses return error. Override expiration makes sense because the asset can be recalculated via a fresh query. Expired assets can be purged since they can be re-evaluated.

3. **Plan expiration inference — two-pass computation:**

   **Sources (both use known times only):**
   - **Commands in the current plan:** Their `CommandMetadata.expires` is known at plan-build time. PlanBuilder converts each to `ExpirationTime` using `Utc::now()` as approximate reference, takes the minimum, and stores the most restrictive `Expires` in the Plan.
   - **Dependencies (other assets/recipes via `has_expirable_dependencies`):** Only contribute if they are **already evaluated** with a known `expiration_time` (i.e., the asset exists in AssetManager with status Ready and a non-Never `expiration_time`). Unevaluated dependencies are treated as `Never`.
   - The minimum is taken across both sources.

   **Two-pass computation:**
   - **Pass 1 — At plan-build time (estimate):** Best-effort computation using command metadata + currently known dependency expiration times. This gives an early estimate that can be used for provisional tracking.
   - **Pass 2 — At finalization (asset becomes Ready):** Recomputed with all dependencies now evaluated and their expiration times known. This is the **authoritative** `expiration_time`. The result may differ from the estimate in either direction:
     - **Shorter:** A dependency that was unknown at plan-build time turns out to have a short expiration.
     - **Longer:** A short-lived dependency was re-evaluated during plan execution and now has a fresh, later expiration time.
   - The Pass 2 result is stored in metadata and used for monitoring. Pass 1 is informational only (for Info steps in the plan).

4. **Canonical serialization for `InDuration`:** Use largest unit that divides evenly. `300s → "in 5 min"`, `90s → "in 90 seconds"`, `7200s → "in 2 hours"`, `86400s → "in 1 day"`.

5. **DST limitation:** `to_expiration_time()` captures timezone offset at call time via `chrono::Local::now().offset()`. If DST transitions occur between plan-build and Ready-time, "end of day" could be off by up to 1 hour. This is documented in code comments as a known limitation.

6. **No `unwrap()` in monitor task:** The `heap.pop()` after `while let Some(...)` is logically safe but must be written as `if let Some(Reverse((_, key))) = heap.pop()` to comply with CLAUDE.md.

7. **LegacyMetadata parse failures:** Log `warn!` when an `expires` field exists in legacy JSON but fails to parse, rather than silently defaulting to `Never`.

## Implementation Phases

The implementation is organized into 7 phases (A-G) with 18 steps total:

| Phase | Steps | Description |
|-------|-------|-------------|
| A | 1-2 | Core expiration module (types, parsing, serialization) |
| B | 3-5 | Metadata integration (MetadataRecord, AssetInfo, Metadata enum) |
| C | 6-7 | Command metadata + macro extension |
| D | 8-10 | Plan builder + dependency inference |
| E | 11-14 | Asset system (AssetData, AssetRef, monitoring, apply_immediately) |
| F | 15 | Recipe struct extension |
| G | 16-18 | Unit tests, integration tests, final validation |

## Implementation Steps

### Step 1: Create expiration module — Expires and ExpirationTime types

**File:** `liquers-core/src/expiration.rs` (NEW)

**Action:**
- Create new module with `Expires` enum (9 variants: `#[derive(Debug, Clone, PartialEq, Eq)]`), `ExpirationTime` enum (3 variants: `#[derive(Debug, Clone, PartialEq, Eq)]`)
- Implement `Default` for both enums
- Implement manual `Ord`/`PartialOrd` for `ExpirationTime` (Immediately < At < Never)
- Implement `ExpirationTime` methods: `is_expired_at()`, `is_expired()`, `is_never()`, `is_immediately()`, `min()`, `ensure_future()`
- Implement `Expires` methods: `to_expiration_time()`, `is_volatile()`, `is_never()`

**Code changes:**
```rust
// NEW FILE: liquers-core/src/expiration.rs

use crate::error::Error;
use chrono::{DateTime, Utc};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub enum Expires {
    Never,
    Immediately,
    InDuration(Duration),
    AtTimeOfDay { hour: u32, minute: u32, second: u32, tz_offset: Option<i32> },
    OnDayOfWeek { day: u32, tz_offset: Option<i32> },
    EndOfDay { tz_offset: Option<i32> },
    EndOfWeek { tz_offset: Option<i32> },
    EndOfMonth { tz_offset: Option<i32> },
    AtDateTime(DateTime<Utc>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpirationTime {
    Never,
    Immediately,
    At(DateTime<Utc>),
}

// Default, Ord/PartialOrd, methods as per Phase 2 signatures
```

**Validation:**
```bash
cargo check -p liquers-core
# Expected: Compiles (module not yet exported)
```

**Rollback:**
```bash
rm liquers-core/src/expiration.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 architecture (data structures section), `liquers-core/src/error.rs` for Error constructors
- **Rationale:** Complex enum design with manual Ord, conversion logic, and time math requires architectural judgment

---

### Step 2: Add parsing (FromStr) and serialization (Display, Serde) for Expires and ExpirationTime

**File:** `liquers-core/src/expiration.rs`

**Action:**
- Implement `FromStr` for `Expires` using nom parser combinators
- Implement `Display` for `Expires` (canonical normalized forms)
- Implement `Display` for `ExpirationTime` ("never", "immediately", or RFC 3339)
- Implement custom `Serialize`/`Deserialize` for both types (string-based)
- Export module from `liquers-core/src/lib.rs`

**Parsing rules (from Phase 2):**
1. Case-insensitive keywords
2. "in" prefix optional for durations
3. Duration units: ms, seconds, min, minutes, h, hours, d, days, w, weeks, mo, months
4. "EOD" / "end of day" aliases
5. Day names: full and abbreviated, case-insensitive
6. Timezone abbreviations: UTC, EST, CST, MST, PST, CET, EET
7. Date formats: ISO 8601, YYYY-MM-DD HH:MM

**Code changes:**
```rust
// ADD to liquers-core/src/expiration.rs:

impl std::str::FromStr for Expires {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Error> { /* nom parsing */ }
}

impl std::fmt::Display for Expires {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { /* canonical forms */ }
}

impl std::fmt::Display for ExpirationTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { /* "never"/"immediately"/RFC3339 */ }
}

// Custom Serde via Display/FromStr for Expires
// Custom Serde for ExpirationTime ("never"/"immediately"/RFC3339)
```

```rust
// MODIFY: liquers-core/src/lib.rs
// Add after existing module declarations:
pub mod expiration;
```

**Validation:**
```bash
cargo check -p liquers-core
# Expected: Compiles with expiration module exported
```

**Rollback:**
```bash
git checkout liquers-core/src/lib.rs
# Revert parsing code in expiration.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 (parsing section), existing nom usage in `liquers-core/src/parse.rs`, chrono DateTime parsing
- **Rationale:** Complex nom parser combinators with multiple formats, timezone handling, and canonical serialization require careful implementation

---

### Step 3: Extend MetadataRecord and AssetInfo with expires/expiration_time fields

**File:** `liquers-core/src/metadata.rs`

**Action:**
- Add `use crate::expiration::{Expires, ExpirationTime};`
- Add `expires: Expires` and `expiration_time: ExpirationTime` fields to `MetadataRecord` (with `#[serde(default)]`)
- Add same fields to `AssetInfo` (with `#[serde(default)]`)
- Add `has_expiration()` and `is_expired()` helper methods to `MetadataRecord`
- Update `From<MetadataRecord> for AssetInfo` and `From<AssetInfo> for MetadataRecord` to copy new fields

**Code changes:**
```rust
// MODIFY: MetadataRecord struct (after is_volatile field ~line 540)
#[serde(default)]
pub expires: Expires,
#[serde(default)]
pub expiration_time: ExpirationTime,

// MODIFY: AssetInfo struct (after is_volatile field ~line 385)
#[serde(default)]
pub expires: Expires,
#[serde(default)]
pub expiration_time: ExpirationTime,

// NEW: Methods on MetadataRecord
impl MetadataRecord {
    pub fn has_expiration(&self) -> bool { !self.expiration_time.is_never() }
    pub fn is_expired(&self) -> bool { self.expiration_time.is_expired() }
}

// MODIFY: From<MetadataRecord> for AssetInfo — copy expires, expiration_time
// MODIFY: From<AssetInfo> for MetadataRecord — copy expires, expiration_time
```

**Validation:**
```bash
cargo check -p liquers-core
# Expected: May have warnings from unused fields; existing tests still compile
```

**Rollback:**
```bash
git checkout liquers-core/src/metadata.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-core/src/metadata.rs` (MetadataRecord ~line 487, AssetInfo ~line 338), Phase 2 metadata section
- **Rationale:** Field additions following established is_volatile pattern

---

### Step 4: Add Metadata enum accessor methods (getters + setters)

**File:** `liquers-core/src/metadata.rs`

**Action:**
- Add `expires()` getter: 3-branch pattern (MetadataRecord, LegacyMetadata(Object), LegacyMetadata(_))
- Add `expiration_time()` getter: same 3-branch pattern
- Add `has_expiration()` and `is_expired()` convenience methods
- Add `set_expires()` setter: 4-branch pattern (MetadataRecord, Object, Null upgrade, error)
- Add `set_expiration_time()` setter: same 4-branch pattern

**Code changes:**
```rust
// ADD to impl Metadata { ... } (near existing is_volatile(), set_status() methods)

pub fn expires(&self) -> Expires {
    match self {
        Metadata::MetadataRecord(mr) => mr.expires.clone(),
        Metadata::LegacyMetadata(serde_json::Value::Object(o)) => {
            if let Some(serde_json::Value::String(s)) = o.get("expires") {
                s.parse().unwrap_or(Expires::Never)
            } else { Expires::Never }
        }
        Metadata::LegacyMetadata(_) => Expires::Never,
    }
}

pub fn expiration_time(&self) -> ExpirationTime { /* similar 3-branch */ }
pub fn has_expiration(&self) -> bool { !self.expiration_time().is_never() }
pub fn is_expired(&self) -> bool {
    self.status() == Status::Expired || self.expiration_time().is_expired()
}

pub fn set_expires(&mut self, expires: Expires) -> Result<&mut Self, Error> {
    /* 4-branch: MetadataRecord, Object, Null->upgrade, error */
}
pub fn set_expiration_time(&mut self, et: ExpirationTime) -> Result<&mut Self, Error> {
    /* 4-branch: same pattern */
}
```

**Validation:**
```bash
cargo check -p liquers-core
# Expected: Compiles, existing metadata tests still pass
cargo test -p liquers-core metadata
```

**Rollback:**
```bash
git checkout liquers-core/src/metadata.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-core/src/metadata.rs` (Metadata enum, existing getter/setter patterns: is_volatile ~line 943, set_status ~line 841), Phase 2 metadata accessor section
- **Rationale:** 4-branch setter pattern with LegacyMetadata upgrade requires careful match handling

---

### Step 5: Update Status enum match statements for Expired variant

**File:** `liquers-core/src/metadata.rs`

**Action:**
- Verify all existing match statements on Status include the `Expired` variant (it already exists at line 38)
- Check `has_data()`, `can_have_tracked_dependencies()`, `is_finished()`, `is_processing()` methods
- Expired semantics: `has_data() = true`, `is_finished() = true`, `is_processing() = false`, `can_have_tracked_dependencies() = false`
- This step is validation only — the Expired variant already exists; ensure no match arms are missing

**Validation:**
```bash
cargo check -p liquers-core
cargo test -p liquers-core
# Expected: All existing tests pass
```

**Rollback:** N/A (validation step)

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-core/src/metadata.rs` (Status enum and match statements)
- **Rationale:** Simple verification of existing match arms

---

### Step 6: Extend CommandMetadata with expires field

**File:** `liquers-core/src/command_metadata.rs`

**Action:**
- Add `use crate::expiration::Expires;`
- Add `#[serde(default)] pub expires: Expires` field to `CommandMetadata`
- Initialize to `Expires::Never` in `CommandMetadata::new()` and `from_key()`

**Code changes:**
```rust
// MODIFY: CommandMetadata struct (after volatile field ~line 773)
#[serde(default)]
pub expires: Expires,

// MODIFY: CommandMetadata::new() (~line 800)
expires: Expires::Never,

// MODIFY: CommandMetadata::from_key() (~line 828)
expires: Expires::Never,
```

**Validation:**
```bash
cargo check -p liquers-core
# Expected: Compiles, existing tests pass
```

**Rollback:**
```bash
git checkout liquers-core/src/command_metadata.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-core/src/command_metadata.rs` (CommandMetadata struct ~line 702, new() ~line 800, from_key() ~line 828)
- **Rationale:** Direct field addition following volatile pattern

---

### Step 7: Extend register_command! macro with expires: keyword

**File:** `liquers-macro/src/lib.rs`

**Action:**
- Add `Expires(String)` variant to `CommandSignatureStatement` enum
- Add parsing for `"expires"` keyword (string literal value)
- Add `expires: String` field to `CommandSpec` (empty = not set)
- Add code generation: `cm.expires = #expires_str.parse().map_err(|e: liquers_core::error::Error| e)?;`

**Code changes:**
```rust
// MODIFY: CommandSignatureStatement enum (~line 742)
// Add after Volatile(bool):
Expires(String),

// MODIFY: impl Parse for CommandSignatureStatement (~line 753)
// Add case:
"expires" => {
    let _colon: syn::Token![:] = input.parse()?;
    let lit: syn::LitStr = input.parse()?;
    Ok(CommandSignatureStatement::Expires(lit.value()))
}

// MODIFY: CommandSpec struct — add field:
pub expires: String,

// MODIFY: CommandSpec processing — handle Expires variant:
CommandSignatureStatement::Expires(s) => { self.expires = s; }

// MODIFY: code generation — generate expires setting code:
let expires_code = if !self.expires.is_empty() {
    let expires_str = &self.expires;
    quote! {
        cm.expires = #expires_str.parse().map_err(|e: liquers_core::error::Error| e)?;
    }
} else {
    quote!()
};
```

**Validation:**
```bash
cargo check -p liquers-macro
cargo check -p liquers-core
# Expected: Macro compiles; liquers-core compiles with new macro support
```

**Rollback:**
```bash
git checkout liquers-macro/src/lib.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-macro/src/lib.rs` (CommandSignatureStatement ~line 742, Parse impl ~line 753, CommandSpec struct, code generation), Phase 2 macro section, `specs/REGISTER_COMMAND_FSD.md`
- **Rationale:** Macro code generation requires understanding of syn/quote patterns and the existing DSL structure

---

### Step 8: Extend Plan and PlanBuilder with expires field

**File:** `liquers-core/src/plan.rs`

**Action:**
- Add `use crate::expiration::Expires;`
- Add `expires: Expires` field to `PlanBuilder` (default: Never)
- Add `expires: Expires` field to `Plan` struct (with `#[serde(default)]`)
- Add `update_expiration()` method to PlanBuilder (takes minimum)
- Add `get_action_expiration()` method to PlanBuilder (reads from CommandMetadata)
- Wire into `build()`: copy PlanBuilder.expires to Plan.expires

**Code changes:**
```rust
// MODIFY: PlanBuilder struct (~line 876, after is_volatile field)
expires: Expires,

// MODIFY: PlanBuilder::new() — initialize expires: Expires::Never

// NEW: PlanBuilder methods
fn update_expiration(&mut self, command_expires: &Expires) {
    // Convert both to ExpirationTime, take min, then update self.expires
    // Add Step::Info documenting the constraint if changed
}

fn get_action_expiration(&self, command_key: &CommandKey) -> Expires {
    // Look up command in registry, return command_metadata.expires
}

// MODIFY: Plan struct (~line 1319, after is_volatile field)
#[serde(default)]
pub expires: Expires,

// MODIFY: PlanBuilder::build() — set plan.expires = self.expires.clone()
```

**Validation:**
```bash
cargo check -p liquers-core
# Expected: Compiles with warnings (methods not yet called from action processing)
```

**Rollback:**
```bash
git checkout liquers-core/src/plan.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-core/src/plan.rs` (PlanBuilder ~line 876, Plan ~line 1319, is_volatile/mark_volatile pattern ~line 916-929, build() method), Phase 2 plan section
- **Rationale:** Minimum-expiration inference logic with ExpirationTime comparison requires architectural judgment

---

### Step 9: Wire expiration into PlanBuilder action processing

**File:** `liquers-core/src/plan.rs`

**Action:**
- In the action step processing (where `is_action_volatile()` is called), also call `get_action_expiration()` and `update_expiration()`
- Mirror the volatility propagation pattern: for each action step, check if command has expires, update plan's minimum
- Ensure Info step is added when expiration changes

**Code changes:**
```rust
// MODIFY: In the action step processing loop (where mark_volatile is called)
// After: if self.is_action_volatile(command_key) { self.mark_volatile(); }
// Add:
let action_expires = self.get_action_expiration(command_key);
if !action_expires.is_never() {
    self.update_expiration(&action_expires);
}
```

**Validation:**
```bash
cargo check -p liquers-core
# Expected: Compiles; plan now tracks expiration from commands
```

**Rollback:**
```bash
git checkout liquers-core/src/plan.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-core/src/plan.rs` (action processing loop, is_action_volatile calls)
- **Rationale:** Direct pattern following from volatility to expiration

---

### Step 10: Add has_expirable_dependencies function + integrate into make_plan

**Files:** `liquers-core/src/plan.rs`, `liquers-core/src/interpreter.rs`

**Action:**
- Add `has_expirable_dependencies()` async function to `plan.rs` (parallel to `has_volatile_dependencies()`)
- Checks **already-evaluated** dependency assets for their known `expiration_time` (not speculative future evaluations)
- Only assets with status Ready and non-Never `expiration_time` contribute
- Takes minimum across known dependency expiration times, adds Info step
- Call from `make_plan()` in `interpreter.rs` after `has_volatile_dependencies()`

**Code changes:**
```rust
// NEW in plan.rs:
pub(crate) async fn has_expirable_dependencies<E: Environment>(
    envref: EnvRef<E>,
    plan: &mut Plan,
) -> Result<(), Error> {
    // For each GetAsset/GetMetadata step:
    //   Look up existing asset in AssetManager
    //   If asset exists with Ready status and non-Never expiration_time:
    //     Take min(plan's current expiration_time, dependency's expiration_time)
    //     Add Info step: "Expiration constrained by dependency X (expires at T)"
    // Dependencies not yet evaluated are skipped (treated as Never)
}

// MODIFY in interpreter.rs, make_plan() function:
// After: has_volatile_dependencies(envref.clone(), &mut plan).await?;
// Add:
has_expirable_dependencies(envref.clone(), &mut plan).await?;
```

**Validation:**
```bash
cargo check -p liquers-core
# Expected: Compiles; make_plan now includes expiration inference
```

**Rollback:**
```bash
git checkout liquers-core/src/plan.rs
git checkout liquers-core/src/interpreter.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-core/src/plan.rs` (has_volatile_dependencies ~line 1659), `liquers-core/src/interpreter.rs` (make_plan function), Phase 2 plan section
- **Rationale:** Async dependency checking with recipe provider requires understanding of the full evaluation pipeline

---

### Step 11: Extend AssetData and AssetNotificationMessage

**Files:** `liquers-core/src/assets.rs`, `liquers-axum/src/assets/websocket.rs`, `liquers-lib/src/ui/element.rs`, `liquers-lib/src/ui/runner.rs`

**Action:**
- Add `use crate::expiration::{Expires, ExpirationTime};`
- Add `expiration_time: ExpirationTime` field to `AssetData` (default: Never)
- Add `Expired` variant to `AssetNotificationMessage` enum
- Update ALL match statements on `AssetNotificationMessage` across the workspace:
  - `liquers-core/src/assets.rs`: at least 2 match blocks — add `Expired` arm (likely same handling as `StatusChanged`)
  - `liquers-axum/src/assets/websocket.rs`: `convert_notification` function — map `Expired` to websocket message (e.g., reuse `StatusChanged(Expired)` serialization)
  - `liquers-lib/src/ui/element.rs`: `update()` method — handle `Expired` notification (trigger re-evaluation or display stale indicator)
  - `liquers-lib/src/ui/runner.rs`: any `matches!` or match blocks — add `Expired` arm

**Code changes:**
```rust
// MODIFY: AssetData struct (~line 240, after is_volatile)
expiration_time: ExpirationTime,

// MODIFY: AssetData constructors — initialize to ExpirationTime::Never

// MODIFY: AssetNotificationMessage enum
Expired,

// UPDATE: All match statements on AssetNotificationMessage in:
// - liquers-core/src/assets.rs (2+ match blocks)
// - liquers-axum/src/assets/websocket.rs (convert_notification)
// - liquers-lib/src/ui/element.rs (update method)
// - liquers-lib/src/ui/runner.rs (notification handling)
```

**Validation:**
```bash
cargo check --workspace
# Expected: All crates compile with Expired variant handled everywhere
```

**Rollback:**
```bash
git checkout liquers-core/src/assets.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-core/src/assets.rs` (AssetData ~line 240, AssetNotificationMessage enum, all match statements), any crates that match on AssetNotificationMessage
- **Rationale:** Adding enum variant triggers compile errors across codebase; need to find and fix all match sites

---

### Step 12: Add AssetRef expiration methods

**File:** `liquers-core/src/assets.rs`

**Action:**
- Add `expire()` async method: sets status to Expired, sends notification, returns Result
- Add `expiration_time()` async method: reads expiration_time field
- Add `is_expired()` async method: checks status
- Add `schedule_expiration()` sync method: spawns tokio task with Weak reference

**Code changes:**
```rust
// ADD to impl<E: Environment> AssetRef<E> { ... }

pub async fn expire(&self) -> Result<(), Error> {
    let mut lock = self.data.write().await;
    match lock.status {
        Status::Ready | Status::Override => {
            // Ready: standard expiration; Override: allows recalculation
            lock.status = Status::Expired;
            if let Metadata::MetadataRecord(ref mut mr) = lock.metadata {
                mr.status = Status::Expired;
            }
            // Send notification
            let _ = lock.notification_sender.send(AssetNotificationMessage::Expired);
            Ok(())
        }
        Status::Expired => Ok(()), // Already expired, idempotent
        // Source cannot be expired (no recipe to recover from)
        status => Err(Error::general_error(
            format!("Cannot expire asset in state {:?}", status)
        )),
    }
}

pub async fn expiration_time(&self) -> ExpirationTime {
    self.data.read().await.expiration_time.clone()
}

pub async fn is_expired(&self) -> bool {
    self.data.read().await.status == Status::Expired
}

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
            if let Some(data) = weak_data.upgrade() {
                let asset_ref = AssetRef { id, data };
                let _ = asset_ref.expire().await;
            }
            // If Weak upgrade fails: all holders dropped, exit silently
        });
    }
    // Never/Immediately: no task spawned
}
```

**Validation:**
```bash
cargo check -p liquers-core
# Expected: Compiles
```

**Rollback:**
```bash
git checkout liquers-core/src/assets.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-core/src/assets.rs` (AssetRef impl block, existing methods like to_override ~line 1535, notification_sender usage), Phase 2 AssetRef section
- **Rationale:** Status transition logic with lock management and notification requires careful error handling

---

### Step 13: Add expiration monitoring to DefaultAssetManager

**File:** `liquers-core/src/assets.rs`

**Action:**
- Define `ExpirationMonitorMessage` enum (Track, Untrack, Shutdown)
- Add `monitor_tx: mpsc::UnboundedSender<ExpirationMonitorMessage>` to `DefaultAssetManager`
- Spawn monitor task in `DefaultAssetManager::new()`: priority queue + tokio::select! loop
- Implement `track_expiration()` and `untrack_expiration()` methods
- Wire into status finalization (try_to_set_ready): when asset becomes Ready with non-Never expiration, send Track message
- Implement `Drop` for `DefaultAssetManager`: sends `ExpirationMonitorMessage::Shutdown` through `monitor_tx` (send is synchronous on unbounded channel, safe in Drop)
- Add code comment explaining why `ExpirationTime::Immediately` is not tracked by monitor (implies volatile, never reaches Ready)

**Code changes:**
```rust
// NEW: ExpirationMonitorMessage
enum ExpirationMonitorMessage {
    Track { key: Key, expiration_time: ExpirationTime },
    Untrack { key: Key },
    Shutdown,
}

// MODIFY: DefaultAssetManager — add monitor_tx field
// MODIFY: DefaultAssetManager::new() — spawn monitor task, store monitor_tx

// NEW: Monitor task implementation (BinaryHeap + cancelled HashSet + tokio::select!)
// As specified in Phase 2 architecture

// MODIFY: try_to_set_ready() — after setting status to Ready:
// If expiration_time is not Never, send Track message via monitor_tx

// NEW: AssetManager trait methods
async fn track_expiration(&self, key: &Key, expiration_time: &ExpirationTime) -> Result<(), Error>;
async fn untrack_expiration(&self, key: &Key) -> Result<(), Error>;
```

**Validation:**
```bash
cargo check -p liquers-core
# Expected: Compiles; monitor task spawned but may not be fully wired yet
```

**Rollback:**
```bash
git checkout liquers-core/src/assets.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-core/src/assets.rs` (DefaultAssetManager struct, new(), try_to_set_ready ~line 979, existing tokio::spawn patterns), Phase 2 monitoring section (priority queue design)
- **Rationale:** Most complex step: async monitoring task with priority queue, tokio::select!, and proper lifecycle management

---

### Step 14: Wire expiration into status finalization and apply_immediately

**File:** `liquers-core/src/assets.rs`

**Action:**
- In `try_to_set_ready()`: **Pass 2 recomputation** — recompute expiration_time from recipe/plan expires AND all now-known dependency expiration times, call `ensure_future(500ms)`, set metadata fields, send Track message for managed assets
- In `apply_immediately()`: after run_immediately, call `schedule_expiration()` for unmanaged assets
- Handle volatile/expires interaction: if `expires.is_volatile()`, treat as Volatile status

**Code changes:**
```rust
// MODIFY: try_to_set_ready() (~line 979)
// After existing volatile check:
if lock.is_volatile || lock.recipe.expires.is_volatile() {
    // Volatile path (existing + Immediately support)
    lock.status = Status::Volatile;
    lock.expiration_time = ExpirationTime::Immediately;
    // ...
} else {
    lock.status = Status::Ready;

    // Pass 2: Authoritative expiration computation
    // Step A: Compute from recipe's own expires specification
    let expires = &lock.recipe.expires;
    let now = chrono::Utc::now();
    let tz_offset = chrono::Local::now().offset().local_minus_utc();
    let mut et = expires.to_expiration_time(now, tz_offset);

    // Step B: Take minimum with all now-known dependency expiration times
    // Dependencies are now fully evaluated, so their expiration_times are authoritative.
    // This may differ from the Pass 1 estimate (shorter or longer).
    // for each dependency asset_ref:
    //     et = et.min(dependency_expiration_time);

    // Step C: Ensure minimum future duration
    let et = et.ensure_future(std::time::Duration::from_millis(500));

    if !et.is_never() {
        lock.expiration_time = et.clone();
        if let Metadata::MetadataRecord(ref mut mr) = lock.metadata {
            mr.expires = expires.clone();
            mr.expiration_time = et;
        }
        // Track for managed assets (send message to monitor)
    }
}

// MODIFY: apply_immediately()
// After run_immediately():
let et = asset_ref.expiration_time().await;
if !et.is_never() {
    asset_ref.schedule_expiration(&et);
}
```

**Validation:**
```bash
cargo check -p liquers-core
cargo test -p liquers-core
# Expected: Compiles; existing tests still pass
```

**Rollback:**
```bash
git checkout liquers-core/src/assets.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-core/src/assets.rs` (try_to_set_ready, apply_immediately, volatile handling), Phase 2 asset finalization section
- **Rationale:** Critical integration point with volatile/expires interaction logic

---

### Step 15: Extend Recipe struct with expires field

**File:** `liquers-core/src/recipes.rs`

**Action:**
- Add `use crate::expiration::Expires;`
- Add `#[serde(default)] pub expires: Expires` field to `Recipe`
- Initialize to `Expires::Never` in constructors
- Wire recipe expires from plan.expires during recipe creation in interpreter

**Code changes:**
```rust
// MODIFY: Recipe struct (~line 17, after volatile field)
#[serde(default)]
pub expires: Expires,

// MODIFY: Recipe constructors — add expires: Expires::Never

// MODIFY: interpreter.rs — where Recipe is created from Plan:
// Set recipe.expires = plan.expires.clone()
```

**Validation:**
```bash
cargo check -p liquers-core
# Expected: Compiles
```

**Rollback:**
```bash
git checkout liquers-core/src/recipes.rs
git checkout liquers-core/src/interpreter.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-core/src/recipes.rs` (Recipe struct), `liquers-core/src/interpreter.rs` (recipe creation)
- **Rationale:** Field addition following existing volatile pattern

---

### Step 16: Write unit tests for expiration module

**File:** `liquers-core/src/expiration.rs` (inline `#[cfg(test)] mod tests`)

**Action:**
- Add comprehensive unit tests as specified in Phase 3 test plan:
  - Parsing tests (25+): "never", "immediately", durations, time of day, day of week, EOD, dates, case insensitivity, invalid input
  - Display round-trip tests (6)
  - Serde round-trip tests (4)
  - ExpirationTime ordering tests (9)
  - ExpirationTime method tests (15): is_expired_at, min, ensure_future
  - Expires->ExpirationTime conversion tests (7)
  - Helper method tests (6): is_volatile, is_never

**Validation:**
```bash
cargo test -p liquers-core expiration
# Expected: All 70+ unit tests pass
```

**Rollback:**
```bash
# Remove #[cfg(test)] module from expiration.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:** Phase 3 examples (test plan section), `liquers-core/src/expiration.rs` (implementation), test patterns from `liquers-core/src/metadata.rs`
- **Rationale:** Comprehensive test suite requires understanding of all edge cases and proper test organization

---

### Step 17: Write integration tests

**File:** `liquers-core/tests/expiration_integration.rs` (NEW)

**Action:**
- Create integration test file with 14 tests as specified in Phase 3:
  - Command with expires metadata registration
  - Plan expiration inference
  - Asset manager monitoring (short-lived asset)
  - Metadata round-trips (MetadataRecord, Metadata enum, LegacyMetadata)
  - register_command! macro with expires:
  - ExpirationTime ordering
  - ensure_future
  - is_volatile semantics
  - Multiple expiring commands
  - AssetRef::expire() manual call
  - Concurrent expirations
  - Serialization round-trip

**Validation:**
```bash
cargo test -p liquers-core --test expiration_integration
# Expected: All 14 integration tests pass
```

**Rollback:**
```bash
rm liquers-core/tests/expiration_integration.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:** Phase 3 examples (integration test section), `liquers-core/tests/async_hellow_world.rs` (integration test pattern), all Phase 2 signatures
- **Rationale:** End-to-end tests require full pipeline understanding

---

### Step 18: Final validation and cleanup

**Files:** All modified files

**Action:**
- Run full workspace build
- Run full test suite
- Run clippy
- Fix any AssetNotificationMessage::Expired match arms in downstream crates (liquers-lib, liquers-axum)
- Verify no regressions

**Validation:**
```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

**Rollback:** N/A (final validation step)

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** All implementation files, downstream crates that may need Expired variant handling
- **Rationale:** Cross-crate validation and fix-up requires judgment

---

## Testing Plan

### Unit Tests

**When to run:** After Step 16

**File:** `liquers-core/src/expiration.rs` (inline `#[cfg(test)] mod tests`)

**Command:**
```bash
cargo test -p liquers-core expiration -- --nocapture
```

**Expected:**
- 70+ unit tests pass
- All parsing variants covered
- Ordering semantics verified
- Round-trip serialization verified

### Integration Tests

**When to run:** After Step 17

**File:** `liquers-core/tests/expiration_integration.rs`

**Command:**
```bash
cargo test -p liquers-core --test expiration_integration -- --nocapture
```

**Expected:**
- 14 integration tests pass
- Command metadata → plan → asset pipeline verified
- Monitoring task works for short-lived assets
- Metadata accessors work for all Metadata variants

### Manual Validation

**When to run:** After Step 18

**Commands:**
```bash
# 1. Run all tests
cargo test --workspace
# Expected: All tests pass, no regressions

# 2. Run clippy
cargo clippy --workspace -- -D warnings
# Expected: No warnings

# 3. Verify key type sizes (optional)
# Ensure Expires and ExpirationTime are reasonably sized
```

**Success criteria:**
- All workspace tests pass
- No clippy warnings
- No regressions in existing functionality

## Agent Assignment Summary

| Step | Model | Skills | Rationale |
|------|-------|--------|-----------|
| 1 | sonnet | rust-best-practices | Complex enum design with manual Ord, time math |
| 2 | sonnet | rust-best-practices | Nom parser, timezone handling, canonical serialization |
| 3 | haiku | rust-best-practices | Field additions following is_volatile pattern |
| 4 | sonnet | rust-best-practices | 4-branch setter with LegacyMetadata upgrade |
| 5 | haiku | rust-best-practices | Verification of existing match arms |
| 6 | haiku | rust-best-practices | Direct field addition |
| 7 | sonnet | rust-best-practices | Macro code generation (syn/quote) |
| 8 | sonnet | rust-best-practices | Minimum-expiration inference logic |
| 9 | haiku | rust-best-practices | Direct pattern following from volatility |
| 10 | sonnet | rust-best-practices | Async dependency checking |
| 11 | sonnet | rust-best-practices | Enum variant addition with cross-crate impact |
| 12 | sonnet | rust-best-practices | Status transition with lock + notification |
| 13 | sonnet | rust-best-practices | Priority queue monitor task (most complex step) |
| 14 | sonnet | rust-best-practices | Volatile/expires interaction, finalization |
| 15 | haiku | rust-best-practices | Field addition following pattern |
| 16 | sonnet | rust-best-practices, liquers-unittest | Comprehensive unit test suite |
| 17 | sonnet | rust-best-practices, liquers-unittest | End-to-end integration tests |
| 18 | sonnet | rust-best-practices | Cross-crate validation |

## Rollback Plan

### Per-Step Rollback

Each step includes a specific rollback command. All steps modify or create files tracked by git.

### Full Feature Rollback

```bash
git checkout main
git branch -D feature/expiration-mechanism
# New files to delete:
rm liquers-core/src/expiration.rs
rm liquers-core/tests/expiration_integration.rs
# Modified files to restore:
git checkout liquers-core/src/lib.rs
git checkout liquers-core/src/metadata.rs
git checkout liquers-core/src/command_metadata.rs
git checkout liquers-core/src/plan.rs
git checkout liquers-core/src/interpreter.rs
git checkout liquers-core/src/assets.rs
git checkout liquers-core/src/recipes.rs
git checkout liquers-macro/src/lib.rs
```

### Partial Completion

If partially complete but need to pause:
1. Create feature branch: `git checkout -b feature/expiration-mechanism`
2. Commit WIP: `git commit -m "WIP: expiration mechanism - completed steps 1-N"`
3. Document status in `specs/expiration-mechanism/DESIGN.md`

## Documentation Updates

### CLAUDE.md

**No updates needed** — expiration follows existing volatility patterns (no new architectural patterns introduced).

### PROJECT_OVERVIEW.md

**Update:** Add brief mention of expiration mechanism in the Asset System section:
```markdown
- Asset expiration: Time-based lifecycle management via `Expires`/`ExpirationTime` in metadata
```

### specs/ASSETS.md

**Update:** Document the expiration lifecycle (Ready → Expired transition, soft expiration model, monitoring).

## Execution Options

After approval:
1. **Execute now** — implement steps sequentially with validation
2. **Create task list** — generate tasks for deferred execution
3. **Revise plan** — incorporate feedback
4. **Exit** — user implements manually using this plan
