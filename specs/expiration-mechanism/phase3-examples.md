# Phase 3: Examples & Use-cases - Asset Expiration Mechanism

## Example Type

**User choice:** Conceptual code (snippets demonstrating intended usage, not runnable prototypes)

## Overview Table

| # | Type | Name | Purpose |
|---|------|------|---------|
| 1 | Example | Command with Time-Based Expiration | Primary use case: register command with `expires:`, evaluate, observe expiration lifecycle |
| 2 | Example | Plan Expiration Inference | Advanced: multi-step query with minimum expiration propagation across dependencies |
| 3 | Example | Edge Cases (3 scenarios) | Unmanaged asset expiration, Immediately/volatile interaction, expired read + Override |
| 4 | Unit Tests | Expiration module tests | Parsing, serialization, ordering, conversion, helper methods |
| 5 | Integration Tests | End-to-end expiration tests | Command metadata, plan inference, monitoring, metadata accessors |

## Example 1: Command with Time-Based Expiration

**Scenario:** A command fetches rate-limited API data that should automatically expire after 5 minutes.

**Context:** API provider allows limited requests per minute; stale data is acceptable up to 5 minutes. The command declares `expires: "in 5 min"` in metadata, and the asset lifecycle manages everything automatically.

### 1a. Register the Command

```rust
use liquers_macro::register_command;
use liquers_core::{state::State, error::Error};

// Define the command function
async fn fetch_stock_price(
    state: State<Value>,
    symbol: String,
) -> Result<Value, Error> {
    let price = call_external_api(&symbol).await?;
    Ok(Value::from(format!("${:.2}", price)))
}

// Register with expiration metadata
let cr = env.get_mut_command_registry();
register_command!(cr,
    async fn fetch_stock_price(state, symbol: String) -> result
    namespace: "api"
    label: "Fetch Stock Price"
    doc: "Fetch current stock price (expires in 5 min)"
    expires: "in 5 min"
)?;
```

### 1b. Build Plan with Expiration Inference

```rust
let query = parse_query("api/fetch_stock_price-AAPL")?;
let plan = make_plan(&envref, &query).await?;

// Plan infers expiration from command metadata
assert_eq!(plan.expires, Expires::InDuration(std::time::Duration::from_secs(300)));
assert!(!plan.is_volatile); // 5 min expiration != volatile
```

### 1c. Evaluate and Observe Expiration

```rust
let asset_ref = envref.evaluate(&query).await?;

// Asset is Ready with computed expiration_time (~5 min from now)
let metadata = asset_ref.get_metadata().await?;
let expiration_time = metadata.expiration_time();
assert!(matches!(expiration_time, ExpirationTime::At(..)));

// Data accessible
let state = asset_ref.get().await?;
assert!(state.to_string().contains("$"));

// After 5 minutes: monitor task expires the asset
// Status changes to Expired, AssetNotificationMessage::Expired sent
// Asset removed from AssetManager cache

// Data STILL accessible (soft expiration)
let stale_state = asset_ref.get().await?;
assert!(stale_state.to_string().contains("$"));
assert_eq!(asset_ref.status().await, Status::Expired);

// Request fresh data: same query creates new asset (old one evicted from cache)
let fresh_asset = envref.evaluate(&query).await?;
assert_ne!(fresh_asset.id(), asset_ref.id());
```

**Expected behavior:**
1. `expires: "in 5 min"` parsed into `Expires::InDuration(300s)` in CommandMetadata
2. Plan inherits `expires` from command metadata
3. Asset transitions to Ready, `expiration_time` = `now + 5 min` (UTC)
4. Monitor task's priority queue fires at `expiration_time`
5. Status changes Ready -> Expired, notification sent, asset removed from cache
6. Stale data remains readable via existing AssetRef holders
7. Next query for same resource triggers fresh evaluation

## Example 2: Plan Expiration Inference from Dependencies

**Scenario:** A multi-step query where commands have different expiration policies. The plan adopts the most restrictive (earliest) expiration.

**Context:** Data pipeline: fetch API data (10 min) -> aggregate (30 min) -> format report (never). The final asset should expire in 10 min because it depends on the shortest-lived step.

### 2a. Register Commands with Different Expirations

```rust
fn fetch_api_data(state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(r#"{"price": 42.50}"#))
}

fn aggregate_data(state: &State<Value>) -> Result<Value, Error> {
    let input = state.try_into_string()?;
    Ok(Value::from(format!(r#"{{"aggregated": {}}}"#, input)))
}

fn format_report(state: &State<Value>) -> Result<Value, Error> {
    let input = state.try_into_string()?;
    Ok(Value::from(format!("Report: {}", input)))
}

let cr = env.get_mut_command_registry();
register_command!(cr,
    fn fetch_api_data(state) -> result
    namespace: "data"
    expires: "in 10 min"
)?;
register_command!(cr,
    fn aggregate_data(state) -> result
    namespace: "data"
    expires: "in 30 min"
)?;
register_command!(cr,
    fn format_report(state) -> result
    namespace: "reports"
)?;
```

### 2b. Plan Building with Minimum Expiration

```rust
// Query: text-Hello/data/fetch_api_data/data/aggregate_data/reports/format_report
let plan = make_plan(&envref, &query).await?;

// PlanBuilder scans each action step:
//   fetch_api_data: expires = InDuration(600s) -> plan.expires = InDuration(600s)
//   aggregate_data: expires = InDuration(1800s) -> min(600, 1800) = 600s (no change)
//   format_report:  expires = Never -> min(600, Never) = 600s (no change)

assert_eq!(plan.expires, Expires::InDuration(std::time::Duration::from_secs(600)));
```

### 2c. Dependency Inference via has_expirable_dependencies

```rust
// Phase 3 of make_plan(): check dependencies
// If any dependency recipe has expiration, take minimum
// Info step added: "Expiration inferred: in 10 min (from data/fetch_api_data)"

let info_steps: Vec<_> = plan.steps.iter()
    .filter(|s| matches!(s, Step::Info(..)))
    .collect();

// Info step documents which dependency caused the expiration constraint
assert!(info_steps.iter().any(|s| {
    if let Step::Info(msg) = s {
        msg.contains("expir") && msg.contains("10 min")
    } else {
        false
    }
}));
```

### 2d. Asset Finalization

```rust
// When asset becomes Ready, expiration_time computed from plan.expires:
//   reference_time = Utc::now()
//   system_tz_offset = chrono::Local::now().offset().local_minus_utc()
//   expiration_time = expires.to_expiration_time(reference_time, system_tz_offset)
//   expiration_time = expiration_time.ensure_future(Duration::from_millis(500))

// Metadata updated:
//   metadata.expires = Expires::InDuration(600s)
//   metadata.expiration_time = ExpirationTime::At(now + 600s)

// Monitor receives Track message via mpsc channel
```

**Expected behavior:**
- Plan expiration = `min(10 min, 30 min, Never)` = `10 min`
- Info step documents the source of the constraint
- Asset inherits the plan's expiration when finalized to Ready

## Example 3: Edge Cases

### 3A: Unmanaged Asset Expiration (apply_immediately)

Assets created via `apply_immediately` are not stored in AssetManager's maps. They use `AssetRef::schedule_expiration()` with Weak references.

```rust
// apply_immediately creates unmanaged asset
let asset_ref = envref.apply_immediately(recipe, initial_value, None).await?;

// After run_immediately completes, schedule_expiration is called internally:
// asset_ref.schedule_expiration(&expiration_time);

// Implementation:
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
            // Try to upgrade Weak -> Arc
            if let Some(data) = weak_data.upgrade() {
                let asset_ref = AssetRef { id, data };
                let _ = asset_ref.expire().await;
            }
            // If upgrade fails: all holders dropped, task exits silently
        });
    }
    // Never/Immediately: no task spawned
}
```

**Key behaviors:**
- If all AssetRef holders drop before expiration: Weak upgrade fails, task exits (no leak)
- If holders remain: expire() is called, status -> Expired, notification sent
- ExpirationTime::Never or Immediately: no task spawned

### 3B: Expires::Immediately and Volatile Interaction

```rust
// Immediately implies volatile
assert!(Expires::Immediately.is_volatile());  // true
assert!(!Expires::InDuration(Duration::from_secs(300)).is_volatile());  // false

// In status finalization (try_to_set_ready):
if expires.is_volatile() {
    // Expires::Immediately → Status::Volatile (same as volatile: true)
    lock.is_volatile = true;
    lock.status = Status::Volatile;
    lock.expiration_time = ExpirationTime::Immediately;
} else if !expires.is_never() {
    // Finite expiration → Status::Ready with expiration_time
    lock.status = Status::Ready;
    lock.expiration_time = expires.to_expiration_time(now, tz_offset);
}

// When both volatile: true and expires: "in 1 min" are set:
// volatile takes precedence (checked first in finalization logic)
```

### 3C: Expired Asset Read and Override

```rust
// After expiration: data still accessible (soft expiration)
assert_eq!(asset_ref.status().await, Status::Expired);
let value = asset_ref.get().await?;  // Still works!

// User can change status to Override to preserve modified data
// AssetRef provides a public method for this transition
asset_ref.to_override().await?;

// Override is terminal: asset won't be re-executed or expire again
assert_eq!(asset_ref.status().await, Status::Override);

// Note: Phase 2 specifies that AssetRef::expire() will be added as a public method.
// This conceptual example shows the intended API usage.
// Expected behavior: calling expire() on an Override asset should return an error
// since Override is a terminal status that cannot transition to Expired.
```

## Corner Cases

### 1. Memory

**Priority queue growth with many expiring assets:**
- BinaryHeap grows linearly with tracked assets. cancelled HashSet handles untracked entries via lazy deletion.
- When an asset is untracked, its key is added to `cancelled` set. When popped from heap, it's skipped.
- Mitigation: Periodic heap compaction when `cancelled.len() > heap.len() / 2`.

**Weak reference cleanup for unmanaged assets:**
- Spawned tasks hold `Weak<RwLock<AssetData>>`. If all holders drop, Weak::upgrade() returns None and task exits.
- No resource leak: the tokio task itself is lightweight (sleep + one upgrade check).

### 2. Concurrency

**Asset expires while being read:**
- Safe: AssetRef holders retain Arc-counted reference. Expiration only changes status and removes from AssetManager. Data remains accessible via existing AssetRef.

**Multiple expirations of same asset:**
- `AssetRef::expire()` checks current status. Only Ready/Source assets can be expired. Second call returns Err (idempotent-safe).

**Monitor task vs manual expire() race:**
- Both paths acquire write lock on AssetData. Second writer sees status already changed and skips/returns error.

**Monitor shutdown:**
- DefaultAssetManager sends `ExpirationMonitorMessage::Shutdown` on drop. Monitor task breaks out of loop and exits.

### 3. Errors

**Parse invalid Expires strings:**
- `"invalid-format".parse::<Expires>()` returns `Err(Error::general_error("Invalid expiration specification: 'invalid-format'"))`
- Unknown duration units, timezones, day names all return descriptive errors.

**Expire non-Ready asset:**
- `asset_ref.expire().await` returns `Err` if status is not Ready/Source/Override. Error message includes current status.

**schedule_expiration with ExpirationTime::Never:**
- No-op: `if let ExpirationTime::At(dt)` doesn't match, no task spawned.

### 4. Serialization

**MetadataRecord with expires round-trip:**
- `#[serde(default)]` on both fields ensures backwards compatibility.
- Old metadata without `expires`/`expiration_time` deserializes with `Never` defaults.

**LegacyMetadata with expires string:**
- Getter: `if let Some(Value::String(s)) = o.get("expires") { s.parse().unwrap_or(Expires::Never) }`
- Setter: `o.insert("expires", Value::String(expires.to_string()))`

**Expires serialization canonical forms:**
- `Expires::InDuration(300s)` serializes as `"in 5 min"` (normalized)
- `ExpirationTime::At(dt)` serializes as RFC 3339 UTC string
- Round-trip: parse -> display -> parse produces same result

### 5. Time-Related

**ensure_future() with past expiration:**
- `ExpirationTime::At(past_time).ensure_future(500ms)` returns `ExpirationTime::At(now + 500ms)`
- Prevents race condition where asset becomes Ready with already-passed expiration time.

**Timezone handling:**
- `Expires::EndOfDay { tz_offset: None }` uses system timezone by default
- `Expires::AtTimeOfDay { hour: 12, minute: 0, second: 0, tz_offset: Some(-18000) }` = "at 12:00 EST"
- `ExpirationTime` is always UTC after conversion

**ExpirationTime ordering edge cases:**
- `ExpirationTime::Immediately < ExpirationTime::At(any_past_time) < ExpirationTime::Never`
- `min(Never, At(t)) = At(t)`, `min(Immediately, anything) = Immediately`

### 6. Integration

**Command metadata to plan to asset pipeline:**
- `CommandMetadata.expires` -> `PlanBuilder.update_expiration()` -> `Plan.expires` -> `AssetData.expiration_time`

**Volatile + expires interaction:**
- `volatile: true` takes precedence in finalization. If both set, asset becomes Volatile (not Ready with expiration).
- `Expires::Immediately` and `volatile: true` are semantically equivalent.

**Notification flow:**
- Expiration triggers `AssetNotificationMessage::Expired` via watch channel.
- All subscribers (UI elements, other assets) receive the notification.
- After expiration, asset removed from AssetManager's scc::HashMap.

## Test Plan

### Unit Tests

**File:** `liquers-core/src/expiration.rs` (inline `#[cfg(test)] mod tests`)

| Category | Tests | Coverage |
|----------|-------|----------|
| Expires parsing (FromStr) | 25+ tests | "never", "immediately", durations (ms/s/min/h/d/w/mo), "at HH:MM", "on Day", "EOD", "end of X", dates, case insensitivity, invalid input |
| Expires Display (round-trip) | 6 tests | Each variant display, parse -> display -> parse consistency |
| Expires Serde | 4 tests | JSON serialize/deserialize, round-trip for each variant |
| ExpirationTime Ordering | 9 tests | Immediately < At < Never, At comparisons, PartialOrd consistency |
| ExpirationTime Methods | 15 tests | is_expired_at, is_expired, is_never, is_immediately, min, ensure_future |
| Expires -> ExpirationTime | 7 tests | Never -> Never, Immediately -> Immediately, InDuration -> At, EndOfDay -> At, AtDateTime -> At |
| Expires Helpers | 6 tests | is_volatile, is_never for each variant |

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // --- Parsing ---
    #[test]
    fn test_parse_never() {
        let e: Expires = "never".parse().unwrap();
        assert_eq!(e, Expires::Never);
    }

    #[test]
    fn test_parse_immediately() {
        let e: Expires = "immediately".parse().unwrap();
        assert_eq!(e, Expires::Immediately);
    }

    #[test]
    fn test_parse_in_5_min() {
        let e: Expires = "in 5 min".parse().unwrap();
        assert_eq!(e, Expires::InDuration(std::time::Duration::from_secs(300)));
    }

    #[test]
    fn test_parse_5_min_without_in() {
        let e: Expires = "5 min".parse().unwrap();
        assert_eq!(e, Expires::InDuration(std::time::Duration::from_secs(300)));
    }

    #[test]
    fn test_parse_eod() {
        let e: Expires = "EOD".parse().unwrap();
        assert!(matches!(e, Expires::EndOfDay { .. }));
    }

    #[test]
    fn test_parse_case_insensitive() {
        let e1: Expires = "NEVER".parse().unwrap();
        let e2: Expires = "Never".parse().unwrap();
        assert_eq!(e1, Expires::Never);
        assert_eq!(e2, Expires::Never);
    }

    #[test]
    fn test_parse_invalid() {
        assert!("invalid-format".parse::<Expires>().is_err());
    }

    // --- Ordering ---
    #[test]
    fn test_ordering_immediately_less_than_at() {
        let imm = ExpirationTime::Immediately;
        let at = ExpirationTime::At(chrono::Utc::now());
        assert!(imm < at);
    }

    #[test]
    fn test_ordering_at_less_than_never() {
        let at = ExpirationTime::At(chrono::Utc::now());
        let never = ExpirationTime::Never;
        assert!(at < never);
    }

    #[test]
    fn test_min_returns_earliest() {
        let imm = ExpirationTime::Immediately;
        let never = ExpirationTime::Never;
        assert_eq!(imm.clone().min(never), ExpirationTime::Immediately);
    }

    // --- Conversion ---
    #[test]
    fn test_never_to_expiration_time() {
        let et = Expires::Never.to_expiration_time(chrono::Utc::now(), 0);
        assert_eq!(et, ExpirationTime::Never);
    }

    #[test]
    fn test_immediately_to_expiration_time() {
        let et = Expires::Immediately.to_expiration_time(chrono::Utc::now(), 0);
        assert_eq!(et, ExpirationTime::Immediately);
    }

    #[test]
    fn test_in_duration_to_expiration_time() {
        let now = chrono::Utc::now();
        let et = Expires::InDuration(std::time::Duration::from_secs(300))
            .to_expiration_time(now, 0);
        if let ExpirationTime::At(dt) = et {
            let diff = dt.signed_duration_since(now);
            assert!(diff.num_seconds() >= 299 && diff.num_seconds() <= 301);
        } else {
            panic!("Expected ExpirationTime::At");
        }
    }

    // --- Helpers ---
    #[test]
    fn test_is_volatile() {
        assert!(Expires::Immediately.is_volatile());
        assert!(!Expires::Never.is_volatile());
        assert!(!Expires::InDuration(std::time::Duration::from_secs(60)).is_volatile());
    }

    // --- ensure_future ---
    #[test]
    fn test_ensure_future_adjusts_past() {
        let past = chrono::Utc::now() - chrono::Duration::seconds(10);
        let et = ExpirationTime::At(past);
        let adjusted = et.ensure_future(std::time::Duration::from_millis(500));
        if let ExpirationTime::At(dt) = adjusted {
            assert!(dt > chrono::Utc::now());
        } else {
            panic!("Expected ExpirationTime::At");
        }
    }
}
```

### Integration Tests

**File:** `liquers-core/tests/expiration_integration.rs`

| Test | Description |
|------|-------------|
| `test_command_with_expires_metadata` | Register command with `expires: "in 1 sec"`, verify CommandMetadata.expires set, evaluate, wait, verify expired |
| `test_plan_expiration_inference` | Two commands with different expires, verify plan gets minimum |
| `test_asset_manager_monitoring` | Short-lived asset, verify monitor task expires it |
| `test_metadata_expires_round_trip` | Set/get expires on MetadataRecord and Metadata enum |
| `test_metadata_expiration_time_round_trip` | Set/get expiration_time, verify UTC preservation |
| `test_metadata_legacy_compatibility` | LegacyMetadata with/without expires field, verify defaults |
| `test_register_command_macro_expires` | Macro with `expires:` keyword, verify generated metadata |
| `test_expiration_time_ordering` | Verify Immediately < At < Never |
| `test_ensure_future_prevents_immediate` | Past expiration adjusted to now + 500ms |
| `test_expires_is_volatile` | Immediately is volatile, others are not |
| `test_multiple_expiring_commands` | Different durations, verify correct expiration order |
| `test_asset_ref_expire_method` | Manual expire() call, verify status and notification |
| `test_concurrent_expirations` | Multiple assets expiring at similar times |
| `test_expires_serialization_round_trip` | JSON serialize -> deserialize preserves value |

```rust
// Example integration test structure
#[tokio::test]
async fn test_command_with_expires_metadata() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    fn expiring_data(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("test"))
    }

    let cr = &mut env.command_registry;
    register_command!(cr,
        fn expiring_data(state) -> result
        expires: "in 1 sec"
    )?;

    // Verify CommandMetadata.expires is set
    let key = CommandKey::new_name("expiring_data");
    let metadata = cr.command_metadata_registry.get(key).unwrap();
    assert!(!metadata.expires.is_never());

    // Evaluate and verify expiration lifecycle
    let envref = env.to_ref();
    let result = evaluate(envref, "/-/expiring_data", None).await?;
    // ... verify expiration_time set, wait, verify expired ...

    Ok(())
}
```

### Manual Validation

**Commands to run (after implementation):**

```bash
# Run unit tests for expiration module
cargo test -p liquers-core expiration -- --nocapture

# Run integration tests
cargo test -p liquers-core --test expiration_integration -- --nocapture

# Run full test suite to check for regressions
cargo test
```

**Success criteria:**
- All expiration unit tests pass
- All integration tests pass
- No regressions in existing tests
- `cargo clippy -p liquers-core` clean

## Auto-Invoke: liquers-unittest Skill Output

The unit test templates above were generated following liquers-unittest patterns:
- `#[cfg(test)] mod tests` for inline unit tests
- `#[tokio::test]` for async integration tests
- `Result<(), Box<dyn std::error::Error>>` return type for tests using `?`
- `type CommandEnvironment` alias before `register_command!` calls
- Descriptive test names: `test_parse_never`, `test_ordering_immediately_less_than_at`
- One primary assertion per test where possible
- Test file placement: unit tests inline, integration tests in `tests/` directory
