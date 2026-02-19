# Phase 3: Primary Use Case Examples - Volatility System

This document demonstrates practical, real-world scenarios where volatility matters in the Liquers framework. Each example shows how volatile commands, the 'v' instruction, and AssetManager behavior work together.

---

## Use Case 1: Real-Time Dashboard with Current Timestamp

### Scenario

A data dashboard displays financial market data alongside a "last updated" timestamp. The timestamp command always produces different output, so it must NEVER be cached. Every query must produce a fresh timestamp.

```
Dashboard User Query: /financial/data/v/timestamp
  ├── Fetch market data (non-volatile, can cache)
  └── Get current time (volatile, never cache)
```

### Command Definition

```rust
// File: liquers-lib/src/commands.rs

use liquers_macro::register_command;
use liquers_core::state::State;
use liquers_core::error::Error;
use chrono::Local;

// Define the command separately
fn current_timestamp(state: &State<Value>) -> Result<Value, Error> {
    let now = Local::now().to_rfc3339();
    Ok(Value::from(format!("Last updated: {}", now)))
}

// In environment setup (liquers-lib/src/lib.rs):
let cr = env.get_mut_command_registry();
register_command!(cr,
    fn current_timestamp(state) -> result
    label: "Current Timestamp"
    doc: "Get current timestamp (always fresh, never cached)"
    volatile: true  // KEY: Mark as volatile
)?;
```

### Query Execution

```rust
// File: liquers-lib/examples/volatile_timestamp.rs

use liquers_core::query::Query;
use liquers_core::interpreter::make_plan;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let env = SimpleEnvironment::new().await?;

    // User query includes 'v' to get volatile result
    let query1 = Query::parse("financial/data/v/timestamp")?;

    // Phase 1: Build plan (checks commands and 'v' instruction)
    let mut plan1 = make_plan(env.clone(), &query1).await?;

    // After Phase 1:
    // - plan1.is_volatile = false (no volatile commands yet)

    // Phase 2: Check asset dependencies
    // - 'v' instruction is PROCESSED HERE
    // - plan1.is_volatile = true (due to 'v' instruction)
    // - Step::Info added: "Volatile due to instruction 'v' at position X"

    // Get asset from manager
    let asset_ref1 = env.asset_manager().get_asset_from_query(&query1).await?;

    // AssetManager behavior:
    // - Checks: plan.is_volatile = true
    // - Result: CREATES NEW AssetRef (no caching!)

    // Evaluate and read value
    let value1 = asset_ref1.get_value().await?;
    println!("Timestamp 1: {}", value1); // "Last updated: 2026-02-17T10:23:45+00:00"

    // Small delay
    std::thread::sleep(Duration::from_secs(1));

    // Request same query again
    let asset_ref2 = env.asset_manager().get_asset_from_query(&query1).await?;

    // AssetManager behavior AGAIN:
    // - Checks: plan.is_volatile = true
    // - Result: CREATES NEW AssetRef (not cached from asset_ref1!)

    let value2 = asset_ref2.get_value().await?;
    println!("Timestamp 2: {}", value2); // "Last updated: 2026-02-17T10:23:46+00:00" (1 second later!)

    // Values are different because:
    // 1. query had 'v' instruction → plan.is_volatile = true
    // 2. AssetManager NEVER cached volatile results
    // 3. Each request created new AssetRef → fresh evaluation

    Ok(())
}
```

### Expected Output

```
Timestamp 1: Last updated: 2026-02-17T10:23:45+00:00
Timestamp 2: Last updated: 2026-02-17T10:23:46+00:00
```

### Why Volatility Matters

Without the 'v' instruction or volatile command marking:
- AssetManager would cache the first result
- Second request would return cached timestamp from 1 second ago
- Dashboard would show stale "last updated" time
- Users would see incorrect information

With volatility system:
- Each request forces fresh evaluation
- Dashboard always shows current timestamp
- Data freshness guaranteed at all times

### Metadata Representation

After evaluation completes:

```rust
// asset_ref1.get_metadata() returns MetadataRecord with:
MetadataRecord {
    status: Status::Volatile,          // Asset has volatile value
    is_volatile: true,                 // Was volatile during evaluation
    query: Query::parse("financial/data/v/timestamp")?,
    message: "Volatile due to instruction 'v' at position 2",
    // ... other fields
}

// asset_ref2.get_metadata() returns DIFFERENT MetadataRecord:
// - Different timestamp value
// - Separate asset (not reused from asset_ref1)
// - Confirms no caching occurred
```

---

## Use Case 2: Volatile Command - Random Sampling

### Scenario

A data analysis pipeline includes a "random sample" command that selects N random rows from a dataset. This command is inherently volatile—different outputs on each execution. Without marking it as volatile, downstream queries might incorrectly cache results and provide stale samples.

```
Query: /data/customers/random_sample
  ├── Load customer data (non-volatile)
  └── Select random rows (volatile command)
Result: Each execution produces different random subset
```

### Command Definition

```rust
// File: liquers-lib/src/commands.rs

use liquers_macro::register_command;
use liquers_core::state::State;
use liquers_core::error::Error;
use rand::seq::SliceRandom;
use polars::prelude::*;

// Define the command function
fn random_sample(state: &State<Value>, sample_size: usize) -> Result<Value, Error> {
    // Assume state contains a Polars DataFrame
    let df = state.try_into_dataframe()?;

    let n_rows = df.height();
    if sample_size > n_rows {
        return Err(Error::general_error(
            format!("Sample size {} exceeds dataset size {}", sample_size, n_rows)
        ));
    }

    // Randomly select indices
    let mut indices: Vec<usize> = (0..n_rows).collect();
    let mut rng = rand::thread_rng();
    indices.shuffle(&mut rng);

    let sampled_df = df.slice(0, sample_size);
    Ok(Value::from(sampled_df))
}

// In environment setup (liquers-lib/src/lib.rs):
let cr = env.get_mut_command_registry();
register_command!(cr,
    fn random_sample(state, sample_size: usize = 100) -> result
    label: "Random Sample"
    doc: "Randomly select N rows from input (volatile - different each time)"
    namespace: "data"
    volatile: true  // KEY: Mark as volatile
)?;
```

### Query Execution Flow

```rust
// File: liquers-lib/examples/volatile_random_sampling.rs

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let env = SimpleEnvironment::new().await?;

    // Query: load data then random sample
    let query = Query::parse("data/load_customers/data/random_sample/50")?;

    // PHASE 1: Build plan (check commands)
    let mut plan = make_plan(env.clone(), &query).await?;

    println!("After Phase 1:");
    println!("  plan.is_volatile = {}", plan.is_volatile); // false (no 'v' instruction yet)
    println!("  steps: [load_customers, random_sample]");

    // PHASE 2: Check asset dependencies & command volatility
    let cmr = env.get_command_metadata_registry();

    for step in &plan.steps {
        if let Step::Action { name, .. } = step {
            if let Some(cmd_meta) = cmr.get(name) {
                if cmd_meta.volatile {
                    println!("  Found volatile command: {}", name);
                    plan.is_volatile = true;
                    plan.steps.push(Step::Info(
                        format!("Volatile due to command '{}'", name)
                    ));
                }
            }
        }
    }

    println!("After Phase 2:");
    println!("  plan.is_volatile = {}", plan.is_volatile); // true! (random_sample is volatile)

    // First execution
    println!("\n--- Execution 1 ---");
    let asset_ref1 = env.asset_manager().get_asset_from_query(&query).await?;

    // AssetManager checks: plan.is_volatile == true
    // → Creates NEW AssetRef (asset_id = 12345)
    // → Does NOT cache it

    let value1 = asset_ref1.get_value().await?;
    println!("Sample 1:\n{}", value1); // 50 random customers

    // Second execution (same query)
    println!("\n--- Execution 2 ---");
    let asset_ref2 = env.asset_manager().get_asset_from_query(&query).await?;

    // AssetManager checks: plan.is_volatile == true
    // → Creates ANOTHER NEW AssetRef (asset_id = 12346, different ID!)
    // → Does NOT reuse cache from asset_ref1

    let value2 = asset_ref2.get_value().await?;
    println!("Sample 2:\n{}", value2); // Different 50 random customers

    // Verify results are different
    let df1 = value1.try_into_dataframe()?;
    let df2 = value2.try_into_dataframe()?;

    let mut equal = true;
    for (row1, row2) in df1.iter().zip(df2.iter()) {
        if row1 != row2 {
            equal = false;
            break;
        }
    }

    println!("Results identical? {} (expected: false)", equal);

    Ok(())
}
```

### Expected Output

```
After Phase 1:
  plan.is_volatile = false
  steps: [load_customers, random_sample]

After Phase 2:
  plan.is_volatile = true
  Found volatile command: data/random_sample

--- Execution 1 ---
Sample 1:
shape: (50, 3)
┌─────┬──────────┬──────────────┐
│ id  ┆ name     ┆ country      │
│ --- ┆ ---      ┆ ---          │
│ i64 ┆ str      ┆ str          │
╞═════╪══════════╪══════════════╡
│ 142 ┆ Alice J. ┆ United States│
│ 73  ┆ Bob K.   ┆ Canada       │
│ 289 ┆ Carol M. ┆ UK           │
...

--- Execution 2 ---
Sample 2:
shape: (50, 3)
┌─────┬──────────┬──────────────┐
│ id  ┆ name     ┆ country      │
│ --- ┆ ---      ┆ ---          │
│ i64 ┆ str      ┆ str          │
╞═════╪══════════╪══════════════╡
│ 56  ┆ David P. ┆ Australia    │
│ 201 ┆ Emma H.  ┆ France       │
│ 189 ┆ Frank L. ┆ Germany      │
...

Results identical? false (expected: true)
```

### Why Volatility Matters

**Without volatility marking:**
- First `random_sample` execution caches 50 random customers
- Second execution returns SAME 50 customers (cached)
- Appears deterministic when it should be random
- Breaks data analysis assumptions

**With volatility marking:**
- Each execution creates new AssetRef
- `random_sample` produces fresh random subset each time
- Results truly non-deterministic as expected
- Downstream analysis gets different samples, testing robustness

### Metadata Representation

```rust
// asset_ref1.get_metadata() returns:
MetadataRecord {
    status: Status::Volatile,          // Asset has volatile value
    is_volatile: true,                 // Originated from volatile command
    query: Query::parse("data/load_customers/data/random_sample/50")?,
    message: "Volatile due to command 'data/random_sample'",
    // ... data contains first 50 random rows
}

// asset_ref2.get_metadata() returns:
MetadataRecord {
    status: Status::Volatile,          // Same status
    is_volatile: true,                 // Same reason
    query: Query::parse("data/load_customers/data/random_sample/50")?,
    message: "Volatile due to command 'data/random_sample'",
    // ... data contains DIFFERENT 50 random rows
}
```

---

## Use Case 3: 'v' Instruction - Forcing Volatile on Non-Volatile Query

### Scenario

A user has a deterministic query (pure data transformations) but wants to force fresh evaluation every time by adding the 'v' instruction. This is useful for cache-busting or debugging stale data issues.

```
Normal query:   /sales/by_region/group_by_q/sum
Forced volatile: /sales/by_region/group_by_q/sum/v

Expected: Same data transformation, but never cached
```

### Query Parsing & Plan Building

```rust
// File: liquers-core/src/query.rs

// The 'v' instruction is already part of the Query DSL
// It's parsed like other instructions ('q', 'ns', etc.)

impl Query {
    pub fn parse(input: &str) -> Result<Query, Error> {
        // Input: "/sales/by_region/group_by_q/sum/v"
        // Parsed as:
        // - path: [sales, by_region]
        // - actions: [group_by, sum]
        // - instructions: [v]  <- NEW INSTRUCTION

        // Returns Query with is_volatile = true (set during parsing)
    }
}
```

### Plan Building with 'v' Instruction

```rust
// File: liquers-core/src/plan.rs (during make_plan execution)

pub async fn make_plan<E: Environment>(
    envref: EnvRef<E>,
    query: &Query,
) -> Result<Plan, Error> {
    let mut pb = PlanBuilder::new(query, cmr)?;

    // PHASE 1: Build plan from query actions
    // Loop through actions: [group_by, sum]
    // Check each action's command metadata for volatility
    // group_by: metadata.volatile = false
    // sum: metadata.volatile = false
    // Result: plan.is_volatile = false after Phase 1

    // PHASE 1.5: Check for 'v' instruction
    if query.instructions.contains(&Instruction::V) {
        // FOUND 'v' INSTRUCTION!
        pb.mark_volatile("Volatile due to instruction 'v'");
        // Result: plan.is_volatile = true
        // Added Step::Info explaining why
    }

    let mut plan = pb.build()?;

    // PHASE 2: Check asset dependencies (skip if already volatile)
    if !plan.is_volatile {
        has_volatile_dependencies(envref, &mut plan).await?;
    }

    Ok(plan)
}
```

### Execution Comparison

```rust
// File: liquers-lib/examples/volatile_v_instruction.rs

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let env = SimpleEnvironment::new().await?;

    // Setup: Load sales data into store (cached, non-volatile)
    let data_key = Key::new("sales_data");
    env.store().set(&data_key, Value::from(sales_dataframe)).await?;

    println!("=== WITHOUT 'v' instruction ===\n");

    // Query 1: Normal (non-volatile)
    let query_normal = Query::parse("sales_data/group_by_q/sum")?;

    // Plan building
    let plan1 = make_plan(env.clone(), &query_normal).await?;
    println!("Query: {}", query_normal);
    println!("plan.is_volatile: {}", plan1.is_volatile); // false
    println!("Steps: {:?}", plan1.steps);
    println!("  - No 'v' instruction found");
    println!("  - No volatile commands detected");

    // First execution
    let asset1 = env.asset_manager().get_asset_from_query(&query_normal).await?;
    let value1 = asset1.get_value().await?;
    println!("Result 1: {:?}", value1);
    println!("Asset ID: 1000");

    // Second execution (same query)
    let asset2 = env.asset_manager().get_asset_from_query(&query_normal).await?;

    // AssetManager behavior:
    // - Checks: plan.is_volatile = false
    // - Lookup cache by query key
    // - FOUND in cache! (same query as before)
    // - Result: RETURNS EXISTING AssetRef (asset_id = 1000)

    let value2 = asset2.get_value().await?;
    println!("Result 2: {:?}", value2);
    println!("Asset ID: 1000 (SAME! Cached)");

    println!("\n=== WITH 'v' instruction ===\n");

    // Query 2: Same transformation, but with 'v'
    let query_volatile = Query::parse("sales_data/group_by_q/sum/v")?;

    // Plan building
    let plan2 = make_plan(env.clone(), &query_volatile).await?;
    println!("Query: {}", query_volatile);
    println!("plan.is_volatile: {}", plan2.is_volatile); // true!
    println!("Steps: {:?}", plan2.steps);
    println!("  - 'v' instruction found at position X");
    println!("  - Added Step::Info: 'Volatile due to instruction 'v''");

    // First execution
    let asset3 = env.asset_manager().get_asset_from_query(&query_volatile).await?;
    let value3 = asset3.get_value().await?;
    println!("Result 3: {:?}", value3);
    println!("Asset ID: 2000");

    // Second execution (same query)
    let asset4 = env.asset_manager().get_asset_from_query(&query_volatile).await?;

    // AssetManager behavior:
    // - Checks: plan.is_volatile = true
    // - Result: CREATES NEW AssetRef (asset_id = 2001)
    // - Does NOT cache it

    let value4 = asset4.get_value().await?;
    println!("Result 4: {:?}", value4);
    println!("Asset ID: 2001 (NEW! Not cached)");

    // Compare
    println!("\n=== Caching Behavior ===\n");
    println!("Without 'v': Assets 1 and 2 are SAME object (cached)");
    println!("  - Both point to asset_id = 1000");
    println!("  - Second request used cache");
    println!();
    println!("With 'v': Assets 3 and 4 are DIFFERENT objects (not cached)");
    println!("  - Asset 3 → asset_id = 2000");
    println!("  - Asset 4 → asset_id = 2001 (new)");
    println!("  - Each request forced fresh evaluation");

    Ok(())
}
```

### Expected Output

```
=== WITHOUT 'v' instruction ===

Query: sales_data/group_by_q/sum
plan.is_volatile: false
Steps: [group_by, sum]
  - No 'v' instruction found
  - No volatile commands detected
Result 1: DataFrame { shape: (5, 2), ... }
Asset ID: 1000
Result 2: DataFrame { shape: (5, 2), ... }
Asset ID: 1000 (SAME! Cached)

=== WITH 'v' instruction ===

Query: sales_data/group_by_q/sum/v
plan.is_volatile: true
Steps: [group_by, sum, Info("Volatile due to instruction 'v' at position 2")]
  - 'v' instruction found at position X
  - Added Step::Info: 'Volatile due to instruction 'v''
Result 3: DataFrame { shape: (5, 2), ... }
Asset ID: 2000
Result 4: DataFrame { shape: (5, 2), ... }
Asset ID: 2001 (NEW! Not cached)

=== Caching Behavior ===

Without 'v': Assets 1 and 2 are SAME object (cached)
  - Both point to asset_id = 1000
  - Second request used cache

With 'v': Assets 3 and 4 are DIFFERENT objects (not cached)
  - Asset 3 → asset_id = 2000
  - Asset 4 → asset_id = 2001 (new)
  - Each request forced fresh evaluation
```

### Why Volatility Matters

**Without the 'v' instruction:**
- Results are deterministic and safely cached
- Second request returns cached result instantly
- Efficient for repeated queries

**With the 'v' instruction (user explicitly marks volatile):**
- User forces fresh evaluation each time
- Useful for cache-busting: detecting if store data changed
- Useful for debugging: "do I have stale data?"
- Slightly less efficient but guarantees fresh execution
- Control is in user's hands via query syntax

### Metadata Representation

```rust
// asset1.get_metadata() (WITHOUT 'v'):
MetadataRecord {
    status: Status::Ready,             // Normal ready state (not volatile)
    is_volatile: false,                // Not volatile
    query: Query::parse("sales_data/group_by_q/sum")?,
    message: "Completed successfully",
}

// asset2.get_metadata() (WITHOUT 'v'):
// SAME OBJECT as asset1 - no new metadata created

// asset3.get_metadata() (WITH 'v'):
MetadataRecord {
    status: Status::Volatile,          // Volatile status
    is_volatile: true,                 // Marked volatile by 'v' instruction
    query: Query::parse("sales_data/group_by_q/sum/v")?,
    message: "Volatile due to instruction 'v' at position 2",
}

// asset4.get_metadata() (WITH 'v'):
// DIFFERENT OBJECT from asset3 - new metadata with fresh timestamp
```

---

## Combining Multiple Volatility Sources

### Scenario: Real-Time Analytics with Random Sampling

A single query that combines multiple sources of volatility:

```
/financial/data/v/random_sample/100/format_report
  ├── Load financial data (non-volatile)
  ├── 'v' instruction (forces volatile)
  ├── random_sample (volatile command)
  ├── Take 100 rows
  └── format_report (non-volatile)
```

### Volatility Propagation

```rust
// Plan building:
// PHASE 1: Check commands
//   - load: non-volatile
//   - format_report: non-volatile
//   Result: plan.is_volatile = false

// PHASE 1.5: Check 'v' instruction
//   - Found 'v'!
//   Result: plan.is_volatile = true

// PHASE 1.75: Check for volatile commands in remaining steps
//   - random_sample: VOLATILE COMMAND DETECTED
//   Result: plan.is_volatile already true (stays true)

// PHASE 2: Check dependencies (skipped, already volatile)
//   Result: plan.is_volatile = true

// Final Plan:
Plan {
    steps: [
        Step::Action("load"),
        Step::Info("Volatile due to instruction 'v'"),
        Step::Action("random_sample"),
        Step::Action("format_report"),
        Step::Info("Volatile due to command 'random_sample'"),
    ],
    is_volatile: true,  // TRUE - from multiple sources
}
```

### AssetManager Behavior

```rust
// AssetManager sees: plan.is_volatile = true
// Behavior: NEVER cache this asset
// Result: Fresh evaluation every time
// Metadata will show: "Volatile due to 'v' instruction AND command 'random_sample'"
```

### Key Insight: Contagious Volatility

Once ANY source marks a plan as volatile:
- That plan is volatile
- All results are volatile
- No caching occurs
- Context marked volatile for nested evaluations
- Any State derived from volatile Context becomes volatile

This ensures data freshness throughout the query evaluation tree.

---

## Summary: Three Core Volatility Mechanisms

| Mechanism | Example | When to Use | Effect |
|-----------|---------|------------|--------|
| **Volatile Command** | `current_time`, `random` | Built-in behavior—declare in `CommandMetadata` | Every execution creates new AssetRef, never cached |
| **'v' Instruction** | Query ending with `/v` | User forces cache-busting or debugging | Overrides normal caching for that query |
| **Volatile Dependency** | Recipe marked `volatile: true` | Asset dependencies require fresh evaluation | Plan marked volatile if any dependency is volatile |

All three mechanisms result in the same behavior at the AssetManager level: **no caching, fresh evaluation every time**.

---

## Testing Volatility Behavior

### Unit Test Example

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_volatile_command_never_cached() {
        let env = SimpleEnvironment::new().await.unwrap();

        // Register a volatile command
        let cr = env.get_mut_command_registry();
        register_command!(cr,
            fn test_random(state) -> result
            volatile: true
        ).unwrap();

        // Execute same query twice
        let query = Query::parse("test/test_random").unwrap();
        let asset1 = env.asset_manager().get_asset_from_query(&query).await.unwrap();
        let asset2 = env.asset_manager().get_asset_from_query(&query).await.unwrap();

        // Asset IDs should differ
        let id1 = asset1.read().await.id;
        let id2 = asset2.read().await.id;
        assert_ne!(id1, id2, "Volatile assets should not be cached");
    }

    #[tokio::test]
    async fn test_v_instruction_prevents_caching() {
        let env = SimpleEnvironment::new().await.unwrap();

        // Query with 'v' instruction
        let query_volatile = Query::parse("test/data/v").unwrap();
        let asset1 = env.asset_manager().get_asset_from_query(&query_volatile).await.unwrap();
        let asset2 = env.asset_manager().get_asset_from_query(&query_volatile).await.unwrap();

        // Different assets created
        let id1 = asset1.read().await.id;
        let id2 = asset2.read().await.id;
        assert_ne!(id1, id2, "'v' instruction should prevent caching");
    }

    #[tokio::test]
    async fn test_normal_query_is_cached() {
        let env = SimpleEnvironment::new().await.unwrap();

        // Query without volatility
        let query = Query::parse("test/data").unwrap();
        let asset1 = env.asset_manager().get_asset_from_query(&query).await.unwrap();
        let asset2 = env.asset_manager().get_asset_from_query(&query).await.unwrap();

        // Same asset returned from cache
        let id1 = asset1.read().await.id;
        let id2 = asset2.read().await.id;
        assert_eq!(id1, id2, "Normal queries should be cached");
    }
}
```

---

## References

- **Phase 1 Design**: See `specs/volatility-system/phase1-high-level-design.md`
- **Phase 2 Architecture**: See `specs/volatility-system/phase2-architecture.md`
- **Query DSL**: `liquers-core/src/query.rs` - instruction parsing
- **Plan Builder**: `liquers-core/src/plan.rs` - volatility detection
- **Asset Manager**: `liquers-core/src/assets.rs` - caching behavior
- **Command Metadata**: `liquers-core/src/command_metadata.rs` - volatile flag
