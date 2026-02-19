# Phase 4: Implementation Plan - Volatility System

**Document Status:** Detailed step-by-step implementation guide for volatility system feature

**Cross-References:**
- Phase 1: `phase1-high-level-design.md`
- Phase 2: `phase2-architecture.md`
- Phase 3: `phase3-examples.md`
- Development Guide: `../../CLAUDE.md`

**Date:** 2026-02-17

---

## 1. Overview

This implementation plan provides a detailed, step-by-step guide to implementing the comprehensive volatility tracking and propagation system across liquers-core. The implementation is broken into 30 focused steps, organized into 7 logical phases:

0. **Pre-Implementation Cleanup** (Steps 0-0.5): Remove default match arms to ensure compiler enforces Status::Volatile handling
1. **Core Data Structures** (Steps 1-5, plus 2.5): Add volatility fields to Status, MetadataRecord, Metadata, AssetInfo, Plan, Context, AssetData
2. **Plan Building Foundation** (Steps 6-10.5): Implement PlanBuilder volatility detection, helper functions, and link parameter checking
3. **Dependency Resolution** (Steps 11-14.5): Implement circular dependency detection, volatile propagation, PlanBuilder audit, recipe provider guarantees
4. **Asset Management** (Steps 15-17): Modify AssetManager caching and implement to_override()
5. **Interpreter Integration** (Steps 18-20): Make make_plan async, integrate all pieces, Context preparation for side-effects
6. **Testing & Validation** (Steps 21-24): Comprehensive unit and integration tests

Each step is atomic, has clear validation criteria, and includes rollback instructions.

**CRITICAL:** Steps 0 and 0.5 MUST be completed before Step 1. These remove existing default match arms (`_ =>`) so the compiler will enforce explicit handling of the new Status::Volatile variant, preventing silent runtime bugs.

**Estimated Complexity:** Medium-High
**Estimated Time:** 38 agent-hours (~9-10 calendar days)

---

## 2. Prerequisites

Before starting implementation, ensure:

### Codebase Familiarization
- [ ] Read `CLAUDE.md` (project conventions)
- [ ] Read Phase 1-3 volatility specs
- [ ] **VERIFY:** Understand Status enum usage (grep for `match.*Status`)
- [ ] **VERIFY:** Understand Plan/PlanBuilder flow (`liquers-core/src/plan.rs`)
- [ ] **VERIFY:** Understand AssetManager lifecycle (`liquers-core/src/assets.rs`)
- [ ] **VERIFY:** Check actual Context struct - confirm it does NOT implement Clone
- [ ] **VERIFY:** Check query parsing code - understand how 'q', 'ns' instructions are handled
- [ ] **VERIFY:** Find all IsVolatile trait implementations - understand their structure
- [ ] **VERIFY:** Audit existing Status match statements - identify any with `_ =>` default arms

### Development Environment
- [ ] Rust 1.70+ installed
- [ ] `cargo check -p liquers-core` passes
- [ ] `cargo test -p liquers-core --lib` passes (all existing tests green)
- [ ] Git working directory clean or committed

### Knowledge Requirements
- Rust async/await patterns
- Tokio runtime usage
- Serde serialization
- Graph traversal algorithms (for circular dependency detection)

---

## 3. Implementation Steps

### **PHASE 0: Pre-Implementation Cleanup**

---

### Step 0: Remove Default Match Arms in assets.rs

**File:** `liquers-core/src/assets.rs`

**Action:**
- Search for all `match` statements on `Status` enum in assets.rs
- Remove any default match arms (`_ =>`)
- Make all Status matches exhaustive (explicit arm for every variant)
- This ensures compiler enforces handling of Status::Volatile when added

**Code changes:**
```bash
# Search for Status matches with default arms
rg "match.*status" liquers-core/src/assets.rs -A 20 | rg "_ =>"

# For each match, replace with explicit arms
# Example pattern to fix:
# OLD:
# match status {
#     Status::Ready => { ... }
#     Status::Error => { ... }
#     _ => { /* default */ }
# }
#
# NEW:
# match status {
#     Status::Ready => { ... }
#     Status::Error => { ... }
#     Status::None => { /* explicit */ }
#     Status::Directory => { /* explicit */ }
#     Status::Recipe => { /* explicit */ }
#     Status::Submitted => { /* explicit */ }
#     Status::Dependencies => { /* explicit */ }
#     Status::Processing => { /* explicit */ }
#     Status::Partial => { /* explicit */ }
#     Status::Storing => { /* explicit */ }
#     Status::Expired => { /* explicit */ }
#     Status::Cancelled => { /* explicit */ }
#     Status::Source => { /* explicit */ }
#     Status::Override => { /* explicit */ }
# }
```

**Validation:**
```bash
# Check compilation - should still pass
cargo check -p liquers-core

# Verify no default arms remain
rg "match.*[Ss]tatus" liquers-core/src/assets.rs -A 20 | rg "_ =>"

# Run tests to ensure no behavioral changes
cargo test -p liquers-core --lib assets::tests
```

**Rollback:**
```bash
git checkout liquers-core/src/assets.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** Read full `liquers-core/src/assets.rs`, understand all Status match locations
- **Rationale:** Must identify ALL Status matches (not just obvious ones) and convert systematically. Sonnet for thorough search and consistent refactoring pattern.

**CRITICAL:** This step MUST be completed BEFORE Step 1 (adding Status::Volatile). Otherwise, the new variant will compile but silently fall through to default arms, causing runtime bugs.

---

### Step 0.5: Update Downstream Crate Status Matches

**Files:** `liquers-lib/src/**/*.rs`, `liquers-py/src/**/*.rs`

**Action:**
- Search for all `match` statements on `Status` enum in downstream crates
- Remove any default match arms (`_ =>`)
- Make all Status matches exhaustive
- This ensures downstream crates will get compile errors when Status::Volatile is added (forcing deliberate handling)

**Code changes:**
```bash
# Search in liquers-lib
rg "match.*[Ss]tatus" liquers-lib/src/ -A 20 | rg "_ =>"

# Search in liquers-py (if exists)
rg "match.*[Ss]tatus" liquers-py/src/ -A 20 | rg "_ =>"

# For each match, replace with explicit arms (same pattern as Step 0)
```

**Validation:**
```bash
# Check compilation - should still pass
cargo check -p liquers-lib
cargo check -p liquers-py

# Verify no default arms remain
rg "match.*[Ss]tatus" liquers-lib/src/ -A 20 | rg "_ =>"

# Run tests
cargo test -p liquers-lib --lib
```

**Rollback:**
```bash
git checkout liquers-lib/src/
git checkout liquers-py/src/
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** Read Status matches in liquers-lib and liquers-py
- **Rationale:** Multi-crate search and update. Sonnet for systematic coverage.

**CRITICAL:** This step prevents silent bugs in downstream crates when Status::Volatile is added.

---

### **PHASE 1: Core Data Structures**

---

### Step 1: Add Status::Volatile Enum Variant

**File:** `liquers-core/src/metadata.rs`

**Action:**
- Add `Volatile` variant to Status enum (after `Override`, line ~46)
- Update `has_data()` method to return `true` for `Volatile`
- Update `is_finished()` method to return `true` for `Volatile`
- Update `can_have_tracked_dependencies()` method to return `false` for `Volatile`

**Code changes:**
```rust
// Line ~46 - Add new variant to Status enum
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, Copy)]
pub enum Status {
    None,
    Directory,
    Recipe,
    Submitted,
    Dependencies,
    Processing,
    Partial,
    Error,
    Storing,
    Ready,
    Expired,
    Cancelled,
    Source,
    Override,
    // NEW: Add this variant
    Volatile,  // Asset has volatile value (use once, then expires)
}

// Line ~60 - Modify has_data() method
pub fn has_data(&self) -> bool {
    match self {
        Status::Ready => true,
        Status::None => false,
        Status::Submitted => false,
        Status::Processing => false,
        Status::Partial => true,
        Status::Error => false,
        Status::Recipe => false,
        Status::Expired => true,
        Status::Source => true,
        Status::Cancelled => false,
        Status::Storing => false,
        Status::Dependencies => false,
        Status::Directory => false,
        Status::Override => true,
        Status::Volatile => true,  // NEW
    }
}

// Line ~77 - Modify can_have_tracked_dependencies() method
pub fn can_have_tracked_dependencies(&self) -> bool {
    match self {
        Status::Ready => true,
        Status::None => false,
        Status::Submitted => false,
        Status::Processing => false,
        Status::Partial => true,
        Status::Error => false,
        Status::Recipe => false,
        Status::Expired => false,
        Status::Source => false,
        Status::Cancelled => false,
        Status::Storing => true,
        Status::Dependencies => false,
        Status::Directory => false,
        Status::Override => false,
        Status::Volatile => false,  // NEW: Like Expired, volatile is terminal
    }
}

// Line ~97 - Modify is_finished() method
pub fn is_finished(&self) -> bool {
    match self {
        Status::Ready => true,
        Status::None => false,
        Status::Submitted => false,
        Status::Processing => false,
        Status::Partial => false,
        Status::Error => true,
        Status::Recipe => false,
        Status::Expired => true,
        Status::Source => false,
        Status::Cancelled => true,
        Status::Storing => false,
        Status::Dependencies => false,
        Status::Directory => false,
        Status::Override => false,
        Status::Volatile => true,  // NEW: Volatile is finished state
    }
}
```

**Validation:**
```bash
# Check compilation
cargo check -p liquers-core

# Run metadata module tests
cargo test -p liquers-core --lib metadata::tests

# Check for exhaustive match warnings
cargo clippy -p liquers-core -- -D warnings
```

**Rollback:**
```bash
git checkout liquers-core/src/metadata.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** Read `liquers-core/src/metadata.rs` (lines 1-150)
- **Rationale:** Simple enum variant addition with straightforward match arm updates. Haiku is sufficient for this mechanical change.

---

### Step 2: Add MetadataRecord.is_volatile Field

**File:** `liquers-core/src/metadata.rs`

**Action:**
- Locate `MetadataRecord` struct (around line 470-515)
- Add `pub is_volatile: bool` field before closing brace
- Add `is_volatile()` helper method to `MetadataRecord` impl block
- Initialize field to `false` in default constructor

**Code changes:**
```rust
// Line ~470-515 - Add field to MetadataRecord struct
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MetadataRecord {
    pub log: Vec<LogEntry>,
    #[serde(with = "query_format")]
    pub query: Query,
    #[serde(with = "option_key_format")]
    pub key: Option<Key>,
    pub status: Status,
    pub type_identifier: String,
    pub data_format: Option<String>,
    pub message: String,
    pub title: String,
    pub description: String,
    pub is_error: bool,
    pub error_data: Option<Error>,
    pub media_type: String,
    pub filename: Option<String>,
    pub unicode_icon: String,
    pub file_size: Option<u64>,
    pub is_dir: bool,
    pub progress: Vec<ProgressEntry>,
    pub updated: String,
    #[serde(default)]
    pub children: Vec<AssetInfo>,

    // NEW: Add this field
    /// If true, this value is known to be volatile even if status is not yet Volatile.
    /// Useful for in-flight assets (Submitted, Dependencies, Processing) where final
    /// value will be volatile when ready.
    /// NOTE: No #[serde(default)] - always required in serialized format per Phase 2
    pub is_volatile: bool,
}

// Add helper method to MetadataRecord impl (find existing impl block)
impl MetadataRecord {
    // ... existing methods ...

    // NEW: Add this method
    /// Returns true if the value is or will be volatile
    pub fn is_volatile(&self) -> bool {
        self.is_volatile || self.status == Status::Volatile
    }
}
```

**Validation:**
```bash
# Check compilation
cargo check -p liquers-core

# Create simple test
cargo test -p liquers-core --lib metadata::tests
```

**Rollback:**
```bash
git checkout liquers-core/src/metadata.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** Read `liquers-core/src/metadata.rs` (lines 470-600)
- **Rationale:** Simple field addition with basic helper method. No complex logic.

---

### Step 2.5: Add Metadata.is_volatile() and AssetInfo.is_volatile

**File:** `liquers-core/src/metadata.rs`

**Action:**
- Add `is_volatile() -> bool` method to `Metadata` enum (handles legacy metadata)
- Add `pub is_volatile: bool` field to `AssetInfo` struct
- Metadata.is_volatile() defaults to false for legacy cases without the field or status

**Code changes:**
```rust
// Add method to Metadata enum impl block
impl Metadata {
    // ... existing methods ...

    // NEW: Add this method
    /// Returns true if the value is or will be volatile.
    /// For legacy metadata without is_volatile field or Status::Volatile,
    /// defaults to false (non-volatile). Such cases should be detected in
    /// the future and marked as expired or override by the user.
    pub fn is_volatile(&self) -> bool {
        match self {
            Metadata::MetadataRecord(mr) => {
                mr.is_volatile || mr.status == Status::Volatile
            }
            Metadata::Simple(_) | Metadata::None => false,  // Legacy: default non-volatile
        }
    }
}

// Find AssetInfo struct and add field
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct AssetInfo {
    // ... existing fields ...

    // NEW: Add this field
    /// If true, this asset is or will be volatile
    #[serde(default)]  // Legacy support: old AssetInfo without this field defaults to false
    pub is_volatile: bool,
}
```

**Validation:**
```bash
# Check compilation
cargo check -p liquers-core

# Test metadata helper method
cargo test -p liquers-core --lib metadata::tests::test_metadata_is_volatile_legacy
```

**Rollback:**
```bash
git checkout liquers-core/src/metadata.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** Read `liquers-core/src/metadata.rs` Metadata enum and AssetInfo struct
- **Rationale:** Simple method addition with legacy support logic.

---

### Step 3: Add Plan.is_volatile Field

**File:** `liquers-core/src/plan.rs`

**Action:**
- Locate `Plan` struct definition (line ~1083)
- Add `pub is_volatile: bool` field
- Add `is_volatile()` getter and `set_volatile()` setter methods to Plan impl
- Update `Plan::new()` to initialize field to `false`

**Code changes:**
```rust
// Line ~1083 - Modify Plan struct
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Plan {
    pub query: Query,
    pub steps: Vec<Step>,

    // NEW: Add this field
    /// If true, this plan produces volatile results.
    /// Computed during plan building via two-phase volatility detection.
    /// NOTE: No #[serde(default)] - always required in serialized format per Phase 2
    pub is_volatile: bool,
}

// Line ~1095 - Modify Plan::new() constructor
impl Plan {
    pub fn new() -> Self {
        Plan {
            query: Query::new(),
            steps: Vec::new(),
            is_volatile: false,  // NEW: Initialize to false
        }
    }

    // ... existing methods ...

    // NEW: Add these methods
    /// Set the volatile flag (called during plan building)
    pub fn set_volatile(&mut self, is_volatile: bool) {
        self.is_volatile = is_volatile;
    }

    /// Get the volatile flag
    pub fn is_volatile(&self) -> bool {
        self.is_volatile
    }
}
```

**Validation:**
```bash
# Check compilation
cargo check -p liquers-core

# Test serialization
cargo test -p liquers-core --lib plan::tests
```

**Rollback:**
```bash
git checkout liquers-core/src/plan.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** Read `liquers-core/src/plan.rs` (lines 1083-1130)
- **Rationale:** Simple field addition with getter/setter. No complex logic.

---

### Step 4: Add Context.is_volatile Field

**File:** `liquers-core/src/context.rs`

**Action:**
- Locate `Context` struct (line ~160)
- Add `is_volatile: bool` field
- Modify `Context::new()` to accept `is_volatile` parameter
- Add `is_volatile()` and `with_volatile()` methods

**Code changes:**
```rust
// Line ~160 - Modify Context struct
pub struct Context<E: Environment> {
    assetref: AssetRef<E>,
    envref: EnvRef<E>,
    cwd_key: Arc<Mutex<Option<Key>>>,
    service_tx: tokio::sync::mpsc::UnboundedSender<AssetServiceMessage>,
    pub payload: Option<E::Payload>,

    // NEW: Add this field
    /// If true, this context is evaluating a volatile asset.
    /// Propagates to nested evaluations via context.evaluate()
    is_volatile: bool,
}

// Line ~169 - Modify Context::new() method
impl<E: Environment> Context<E> {
    pub async fn new(assetref: AssetRef<E>, is_volatile: bool) -> Self {
        let service_tx = assetref.service_sender().await;
        let envref = assetref.get_envref().await;
        Context {
            assetref,
            envref,
            cwd_key: Arc::new(Mutex::new(None)),
            service_tx,
            payload: None,
            is_volatile,  // NEW: Initialize from parameter
        }
    }

    // ... existing methods ...

    // NEW: Add these methods
    /// Returns true if this context is evaluating a volatile asset
    pub fn is_volatile(&self) -> bool {
        self.is_volatile
    }

    /// Create child context for nested evaluation, inheriting volatility
    ///
    /// NOTE: Context does NOT implement Clone trait (AssetRef prevents it).
    /// This method manually constructs a new Context with cloned Arc references.
    pub fn with_volatile(&self, volatile: bool) -> Self {
        Context {
            assetref: self.assetref.clone(),
            envref: self.envref.clone(),
            cwd_key: self.cwd_key.clone(),
            service_tx: self.service_tx.clone(),
            payload: self.payload.clone(),
            is_volatile: volatile || self.is_volatile,  // Propagate if parent is volatile
        }
    }
}
```

**IMPORTANT:** Phase 2 documentation may suggest Context::clone() exists, but it does NOT. The `with_volatile()` method shown above is the correct approach (manual construction with cloned Arc references). Do NOT attempt to derive Clone for Context.

**Validation:**
```bash
# Check compilation
cargo check -p liquers-core

# Check for Context::new() call sites that need updating
rg "Context::new\(" liquers-core/src/
```

**Rollback:**
```bash
git checkout liquers-core/src/context.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** Read `liquers-core/src/context.rs` (full file), search for all `Context::new` call sites
- **Rationale:** Signature change affects multiple call sites. Sonnet better for tracking propagation logic and updating callers.

---

### Step 5: Add AssetData.is_volatile Field

**File:** `liquers-core/src/assets.rs`

**Action:**
- Locate `AssetData` struct (line ~187)
- Add `is_volatile: bool` field
- Initialize to `false` in AssetData constructor
- Will be set properly in Step 15 when AssetManager is modified

**Code changes:**
```rust
// Line ~187 - Modify AssetData struct
pub struct AssetData<E: Environment> {
    id: u64,
    pub recipe: Recipe,
    envref: EnvRef<E>,
    service_tx: mpsc::UnboundedSender<AssetServiceMessage>,
    service_rx: Arc<Mutex<mpsc::UnboundedReceiver<AssetServiceMessage>>>,
    notification_tx: watch::Sender<AssetNotificationMessage>,
    _notification_rx: watch::Receiver<AssetNotificationMessage>,
    initial_state: State<E::Value>,
    query: Arc<Option<Query>>,
    data: Option<Arc<E::Value>>,
    binary: Option<Arc<Vec<u8>>>,
    pub(crate) metadata: Metadata,
    status: Status,
    pub(crate) save_in_background: bool,
    cancelled: bool,

    // NEW: Add this field
    /// If true, this asset is volatile (computed from recipe/plan before execution)
    is_volatile: bool,

    _marker: std::marker::PhantomData<E>,
}

// Find AssetData initialization and set is_volatile to false initially
// (Will be set properly in Step 15)
```

**Validation:**
```bash
# Check compilation
cargo check -p liquers-core

# Ensure AssetData still constructs
cargo test -p liquers-core --lib assets::tests
```

**Rollback:**
```bash
git checkout liquers-core/src/assets.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** Read `liquers-core/src/assets.rs` (lines 187-250)
- **Rationale:** Simple field addition. Initialization logic comes later in Step 15.

---

### **PHASE 2: Plan Building Foundation**

---

### Step 6: Add PlanBuilder.is_volatile Field

**File:** `liquers-core/src/plan.rs`

**Action:**
- Locate `PlanBuilder` struct (line ~724)
- Add `is_volatile: bool` field
- Initialize to `false` in `PlanBuilder::new()`

**Code changes:**
```rust
// Line ~724 - Modify PlanBuilder struct
pub struct PlanBuilder<'c> {
    query: Query,
    command_registry: &'c CommandMetadataRegistry,
    plan: Plan,
    allow_placeholders: bool,
    expand_predecessors: bool,

    // NEW: Add this field
    /// Track volatility during plan building
    is_volatile: bool,
}

// Line ~736 - Modify PlanBuilder::new()
impl<'c> PlanBuilder<'c> {
    pub fn new(query: Query, command_registry: &'c CommandMetadataRegistry) -> Self {
        PlanBuilder {
            query,
            command_registry,
            plan: Plan::new(),
            allow_placeholders: false,
            expand_predecessors: true,
            is_volatile: false,  // NEW: Initialize to false
        }
    }

    // ... rest of methods
}
```

**Validation:**
```bash
# Check compilation
cargo check -p liquers-core

# Verify PlanBuilder still constructs
cargo test -p liquers-core --lib plan::tests
```

**Rollback:**
```bash
git checkout liquers-core/src/plan.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** Read `liquers-core/src/plan.rs` (lines 724-760)
- **Rationale:** Simple field addition with initialization.

---

### Step 7: Implement PlanBuilder.mark_volatile() Helper

**File:** `liquers-core/src/plan.rs`

**Action:**
- Add `mark_volatile(reason: &str)` method to PlanBuilder impl
- Sets `is_volatile = true` and adds `Step::Info` with reason

**Code changes:**
```rust
// Add to PlanBuilder impl block (after existing methods)
impl<'c> PlanBuilder<'c> {
    // ... existing methods ...

    // NEW: Add this method
    /// Mark plan as volatile and add explanatory Step::Info
    fn mark_volatile(&mut self, reason: &str) {
        if !self.is_volatile {
            self.is_volatile = true;
            self.plan.steps.push(Step::Info(reason.to_string()));
        }
    }
}
```

**Validation:**
```bash
# Check compilation
cargo check -p liquers-core

# Create unit test for mark_volatile
cargo test -p liquers-core --lib plan::tests::test_plan_builder_mark_volatile
```

**Rollback:**
```bash
git checkout liquers-core/src/plan.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** Read `liquers-core/src/plan.rs` PlanBuilder impl block
- **Rationale:** Simple helper method with clear semantics.

---

### Step 8: Implement PlanBuilder.is_action_volatile() Helper

**File:** `liquers-core/src/plan.rs`

**Action:**
- Add `is_action_volatile(command_key: &CommandKey) -> bool` method
- Looks up command in registry and checks `metadata.volatile` flag

**Code changes:**
```rust
// Add to PlanBuilder impl block
impl<'c> PlanBuilder<'c> {
    // ... existing methods ...

    // NEW: Add this method
    /// Helper: check if action command is volatile via CommandMetadata
    fn is_action_volatile(&self, command_key: &CommandKey) -> bool {
        if let Some(metadata) = self.command_registry.get(command_key) {
            metadata.volatile
        } else {
            false
        }
    }
}
```

**Validation:**
```bash
# Check compilation
cargo check -p liquers-core

# Create unit test with mock volatile command
cargo test -p liquers-core --lib plan::tests::test_is_action_volatile
```

**Rollback:**
```bash
git checkout liquers-core/src/plan.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** Read `liquers-core/src/plan.rs` and `liquers-core/src/command_metadata.rs`
- **Rationale:** Requires understanding CommandMetadataRegistry API and CommandKey construction. Sonnet for registry interaction logic.

---

### Step 9: Update PlanBuilder.build() to Set Plan.is_volatile

**File:** `liquers-core/src/plan.rs`

**Action:**
- Modify `PlanBuilder::build()` method (line ~758)
- Set `plan.is_volatile = self.is_volatile` before returning

**Code changes:**
```rust
// Line ~758 - Modify build() method
impl<'c> PlanBuilder<'c> {
    pub fn build(&mut self) -> Result<Plan, Error> {
        let query = self.query.clone();
        self.plan.query = query.clone();
        self.process_query(&query)?;

        // NEW: Set is_volatile field from builder state
        self.plan.is_volatile = self.is_volatile;

        Ok(self.plan.clone())
    }
}
```

**Validation:**
```bash
# Check compilation
cargo check -p liquers-core

# Test that plan.is_volatile is set correctly
cargo test -p liquers-core --lib plan::tests::test_plan_builder_sets_is_volatile
```

**Rollback:**
```bash
git checkout liquers-core/src/plan.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** Read `liquers-core/src/plan.rs` build() method
- **Rationale:** Simple one-line addition to existing method.

---

### Step 10: Implement Phase 1 Volatility Detection in PlanBuilder

**File:** `liquers-core/src/plan.rs`

**Action:**
- Intercept 'v' instruction at query-level (BEFORE action processing), similar to how 'q' and 'ns' are handled
- Add volatility checks for volatile commands during action processing
- The 'v' instruction should be handled in the query parsing/interpretation phase, not as a regular action

**IMPORTANT:** The 'v' instruction uses action syntax (`/v`) but is NOT a regular command. It's a query-level instruction like 'q' (query encoding) and 'ns' (namespace). It will be processed in `process_action_request`, but should:
1. Mark the plan as volatile
2. Either return early (not creating a Step), OR create only Step::Info
3. NOT create a Step::Action for command execution

**NOTE:** Query segments do NOT have an `is_instruction()` method. The 'v' instruction is handled at the action request level by checking the action name.

**Code changes:**
```rust
// Handle in action processing with early return (CORRECT APPROACH)
impl<'c> PlanBuilder<'c> {
    fn process_action_request(&mut self, action: &ActionRequest) -> Result<(), Error> {
        let action_name = &action.action_name;

        // NEW: Intercept 'v' instruction BEFORE normal action processing
        if action_name == "v" {
            self.mark_volatile("Volatile due to instruction 'v'");
            return Ok(());  // Don't create Step::Action for 'v'
        }

        // ... existing parameter resolution logic ...

        // Build CommandKey from action
        let command_key = CommandKey::new(
            action.realm.clone(),
            action.ns.clone(),
            action_name.clone(),
        );

        // NEW: Check if command is volatile
        if self.is_action_volatile(&command_key) {
            self.mark_volatile(&format!(
                "Volatile due to command '{}/{}/{}'",
                action.realm, action.ns, action_name
            ));
        }

        // ... existing step creation and addition logic ...
        Ok(())
    }
}
```

**NOTE:** Agent must examine actual query parsing code to determine correct interception point. The 'v' instruction may need special handling in the query parser itself if it's structurally different from regular actions.

**Validation:**
```bash
# Check compilation
cargo check -p liquers-core

# Examine actual query structure for 'v' instruction
# Test query: "data/v/to_text" should mark plan as volatile

# Create unit tests for 'v' instruction and volatile command detection
cargo test -p liquers-core --lib plan::tests::test_v_instruction_marks_volatile
cargo test -p liquers-core --lib plan::tests::test_volatile_command_marks_volatile

# Verify 'v' does NOT create a Step::Action
cargo test -p liquers-core --lib plan::tests::test_v_instruction_no_action_step
```

**Rollback:**
```bash
git checkout liquers-core/src/plan.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:** Read full `liquers-core/src/plan.rs`, `liquers-core/src/query.rs`, `liquers-core/src/parse.rs` to understand query structure and instruction handling
- **Rationale:** Requires understanding query parsing flow AND determining correct interception point for 'v' instruction. Sonnet for integration complexity and architectural decision.

---

### Step 10.5: Check ResolvedParameterValues Links for Volatility

**File:** `liquers-core/src/plan.rs`

**Action:**
- When processing action parameters in PlanBuilder, check if ResolvedParameterValues contains links
- If a link parameter points to a volatile query/key, mark the plan as volatile
- This ensures volatility propagates through parameter links

**Code changes:**
```rust
// In PlanBuilder, after resolving parameters for an action:
impl<'c> PlanBuilder<'c> {
    fn process_action_request(&mut self, action: &ActionRequest) -> Result<(), Error> {
        // ... existing parameter resolution ...

        // NEW: Check resolved parameters for links to volatile queries/keys
        for (_param_name, param_value) in &resolved_params {
            match param_value {
                ResolvedParameterValue::Link(query) => {
                    // Check if linked query is volatile
                    // Build a plan for the linked query and check its volatility
                    let mut link_pb = PlanBuilder::new(query.clone(), self.command_registry);
                    let link_plan = link_pb.build()?;
                    if link_plan.is_volatile {
                        self.mark_volatile(&format!(
                            "Volatile due to link parameter to volatile query: {:?}",
                            query
                        ));
                    }
                }
                ResolvedParameterValue::KeyLink(key) => {
                    // Check if linked key has a volatile recipe
                    // This will be checked later in Phase 2 (has_volatile_dependencies)
                    // No action needed here
                }
                _ => {
                    // Other parameter types don't affect volatility
                }
            }
        }

        // ... rest of action processing ...
        Ok(())
    }
}
```

**NOTE:** This is Phase 1 volatility detection (synchronous). Link parameters pointing to keys will be checked in Phase 2 via `has_volatile_dependencies()`.

**Validation:**
```bash
# Check compilation
cargo check -p liquers-core

# Create unit test for link parameter volatility propagation
cargo test -p liquers-core --lib plan::tests::test_link_parameter_volatile
```

**Rollback:**
```bash
git checkout liquers-core/src/plan.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:** Read `liquers-core/src/commands.rs` ResolvedParameterValue definition, understand parameter resolution in PlanBuilder
- **Rationale:** Requires understanding parameter resolution and nested plan building. Sonnet for integration complexity.

---

### **PHASE 3: Dependency Resolution**

---

### Step 11: Implement find_dependencies() Helper Function

**File:** `liquers-core/src/plan.rs`

**Action:**
- Add async function `find_dependencies<E: Environment>(envref, plan, stack, cwd) -> Result<HashSet<Key>, Error>`
- Recursively finds all asset dependencies (Keys) from Plan
- Detects circular dependencies using stack tracking
- Tracks current working directory (cwd) for relative key resolution
- Returns error with specific key that caused cycle if detected

**Code changes:**
```rust
// Add to plan.rs module (after PlanBuilder impl, before tests)
use std::collections::HashSet;
use crate::context::EnvRef;
use crate::context::Environment;

/// Helper function: Find all asset dependencies of a plan (direct and indirect)
/// Returns Error with specific key if circular dependency detected
///
/// # Dependency Semantics
///
/// - **UseKeyValue**: Does NOT create dependency. Creates a value with the key,
///   but does not fetch the resource. No attempt to get the resource is made.
///
/// - **GetAssetRecipe**: Does NOT create circular dependency risk. Asset recipe
///   is associated with the key, but it's a separate resource. In a dependency
///   tree it is a leaf. Recipe does not have further dependencies.
///
/// - **GetResource**: Ambiguous. Fetches data directly from the store, bypassing
///   dependency controls. Treated as no dependency for now, but flagged here.
///   Using the store rather than assets bypasses the asset dependency system.
///
/// - **SetCwd**: Does NOT create dependency on its own, but impacts relative links.
///   Requires complex evaluation: when a key/query is examined for circular dependency,
///   must find valid Cwd (previous SetCwd step in the plan), expand the query to
///   absolute form, and then assess the expanded query/key.
///
/// # Parameters
/// - `cwd`: Optional current working directory for resolving relative keys
pub(crate) async fn find_dependencies<E: Environment>(
    envref: EnvRef<E>,
    plan: &Plan,
    stack: &mut Vec<Key>,
    cwd: Option<Key>,
) -> Result<HashSet<Key>, Error> {
    let mut dependencies = HashSet::new();
    let mut current_cwd = cwd;

    for step in &plan.steps {
        match step {
            Step::GetAsset(key) => {
                // Resolve key relative to cwd if needed
                let resolved_key = if current_cwd.is_some() {
                    // TODO: Implement key.resolve_relative(cwd) or similar
                    key.clone()  // For now, use key as-is
                } else {
                    key.clone()
                };

                // Check for circular dependency
                if stack.contains(&resolved_key) {
                    return Err(Error::general_error(
                        format!("Circular dependency detected: key {:?} appears in dependency chain", resolved_key)
                    ).with_key(resolved_key));
                }

                // Add to dependencies
                dependencies.insert(resolved_key.clone());

                // Push onto stack
                stack.push(resolved_key.clone());

                // Get recipe for this key (if it exists)
                if let Some(recipe) = envref.get_recipe_provider()
                    .get_recipe(&resolved_key)
                    .await
                    .ok()
                    .flatten()
                {
                    // Recursively find dependencies of this recipe
                    if let Some(recipe_query) = &recipe.query {
                        // Build plan from recipe query
                        let cmr = envref.get_command_metadata_registry();
                        let mut pb = PlanBuilder::new(recipe_query.clone(), cmr);
                        let recipe_plan = pb.build()?;
                        let indirect_deps = find_dependencies(envref.clone(), &recipe_plan, stack, current_cwd.clone()).await?;
                        dependencies.extend(indirect_deps);
                    }
                }

                // Pop from stack
                stack.pop();
            }
            Step::UseKeyValue(_key) => {
                // Does NOT create dependency - just creates a value with the key
                // No attempt to fetch the resource is made
            }
            Step::GetAssetRecipe(_key) => {
                // Does NOT create circular dependency risk
                // Recipe is a leaf in the dependency tree, has no further dependencies
            }
            Step::GetResource(_key) => {
                // Ambiguous: fetches directly from store, bypassing dependency controls
                // Treated as no dependency for now
                // TODO: Consider flagging this as potential dependency bypass
            }
            Step::SetCwd(key) => {
                // Update current working directory for subsequent relative key resolution
                current_cwd = Some(key.clone());
                // Does not create dependency on its own
            }
            Step::Evaluate(query) | Step::UseQueryValue(query) => {
                // Resolve query relative to cwd if needed
                let resolved_query = if current_cwd.is_some() {
                    // TODO: Implement query.resolve_relative(cwd) or similar
                    query.clone()  // For now, use query as-is
                } else {
                    query.clone()
                };

                // Convert query to plan, find its dependencies
                let cmr = envref.get_command_metadata_registry();
                let mut pb = PlanBuilder::new(resolved_query, cmr);
                let eval_plan = pb.build()?;
                let query_deps = find_dependencies(envref.clone(), &eval_plan, stack, current_cwd.clone()).await?;
                dependencies.extend(query_deps);
            }
            Step::Plan(nested_plan) => {
                // Find dependencies of nested plan
                let nested_deps = find_dependencies(envref.clone(), nested_plan, stack, current_cwd.clone()).await?;
                dependencies.extend(nested_deps);
            }
            Step::Action { .. } => {
                // No dependencies
            }
            Step::Info(_) => {
                // No dependencies
            }
            Step::Query(_) => {
                // No dependencies (just metadata)
            }
            Step::Key(_) => {
                // No dependencies (just metadata)
            }
            Step::SetMetadata { .. } => {
                // No dependencies
            }
            Step::Value(_) => {
                // No dependencies
            }
            // IMPORTANT: No default match arm - all Step variants must be explicit
        }
    }

    Ok(dependencies)
}
```

**Validation:**
```bash
# Check compilation
cargo check -p liquers-core

# Create unit tests for circular dependency detection
cargo test -p liquers-core --lib plan::tests::test_find_dependencies_circular_a_b_a
cargo test -p liquers-core --lib plan::tests::test_find_dependencies_circular_a_b_c_a
cargo test -p liquers-core --lib plan::tests::test_find_dependencies_no_cycle
```

**Rollback:**
```bash
git checkout liquers-core/src/plan.rs
```

**Agent Specification:**
- **Model:** opus
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:** Read `liquers-core/src/plan.rs`, `liquers-core/src/query.rs`, `liquers-core/src/recipes.rs`
- **Rationale:** Complex recursive graph traversal with cycle detection. Opus for correctness in critical algorithm.

---

### Step 12: Implement has_volatile_dependencies() Function

**File:** `liquers-core/src/plan.rs`

**Action:**
- Add async function `has_volatile_dependencies<E>(envref, plan) -> Result<bool, Error>`
- Calls `find_dependencies()` to get all dependencies
- Checks each key's recipe for volatility
- Updates `plan.is_volatile` and adds Step::Info if volatile dependency found

**Code changes:**
```rust
// Add to plan.rs module (after find_dependencies)

/// Check if plan has volatile dependencies (Phase 2 check)
/// Returns true if any dependency recipe is volatile
pub(crate) async fn has_volatile_dependencies<E: Environment>(
    envref: EnvRef<E>,
    plan: &mut Plan,
) -> Result<bool, Error> {
    // Only check if plan is not already marked volatile
    if plan.is_volatile {
        return Ok(true);
    }

    // Find all dependencies (no initial cwd)
    let mut stack = Vec::new();
    let dependencies = find_dependencies(envref.clone(), plan, &mut stack, None).await?;

    // Check each dependency key for volatility
    for key in dependencies {
        if let Some(recipe) = envref.get_recipe_provider()
            .get_recipe(&key)
            .await
            .ok()
            .flatten()
        {
            if recipe.volatile {
                plan.is_volatile = true;
                plan.steps.push(Step::Info(
                    format!("Volatile due to dependency on volatile key: {:?}", key)
                ));
                return Ok(true);
            }
        }
    }

    Ok(false)
}
```

**Validation:**
```bash
# Check compilation
cargo check -p liquers-core

# Create unit tests
cargo test -p liquers-core --lib plan::tests::test_has_volatile_dependencies_none
cargo test -p liquers-core --lib plan::tests::test_has_volatile_dependencies_one_volatile
cargo test -p liquers-core --lib plan::tests::test_has_volatile_dependencies_transitive
```

**Rollback:**
```bash
git checkout liquers-core/src/plan.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:** Read `liquers-core/src/plan.rs` find_dependencies implementation, recipes API
- **Rationale:** Builds on Step 11, requires understanding recipe volatility. Sonnet sufficient for this logic.

---

### Step 13: Update make_plan() to be Async and Audit ALL Call Sites

**File:** `liquers-core/src/interpreter.rs` + all call sites

**Action:**
- Change `make_plan()` signature from sync to async
- Call `has_volatile_dependencies()` after building plan (Phase 2 check)
- **COMPREHENSIVE AUDIT:** Find and update ALL make_plan() call sites including:
  - Direct calls in interpreter.rs
  - Calls in assets.rs (AssetManager)
  - Calls in tests
  - **CRITICAL:** Calls within `IsVolatile<E>` trait implementations (especially `IsVolatile<E> for Query`, `IsVolatile<E> for &str`, etc.)
  - Any other indirect callers

**Code changes:**
```rust
// Line ~20 - Modify make_plan signature and implementation
use crate::plan::{find_dependencies, has_volatile_dependencies};

pub async fn make_plan<E: Environment, Q: TryToQuery>(
    envref: EnvRef<E>,
    query: Q,
) -> Result<Plan, Error> {
    let rquery = query.try_to_query();
    let cmr = envref.get_command_metadata_registry();
    let mut pb = PlanBuilder::new(rquery?, cmr);

    // Phase 1: Build plan, check commands and 'v' instruction
    let mut plan = pb.build()?;

    // NEW: Phase 2: Check asset dependencies for volatility
    has_volatile_dependencies(envref, &mut plan).await?;

    Ok(plan)
}
```

**CRITICAL AUDIT LOCATIONS:**

1. **IsVolatile trait implementations in interpreter.rs:**
   ```rust
   // FIND ALL IMPLEMENTATIONS (lines ~316-422)
   impl<E: Environment> IsVolatile<E> for Query {
       async fn is_volatile(&self, env: EnvRef<E>) -> Result<bool, Error> {
           // LIKELY CALLS make_plan() - must add .await
           let plan = make_plan(env, self).await?;  // <-- ADD .await
           Ok(plan.is_volatile)
       }
   }

   impl<E: Environment> IsVolatile<E> for &str {
       async fn is_volatile(&self, env: EnvRef<E>) -> Result<bool, Error> {
           // LIKELY CALLS make_plan() - must add .await
           let query = parse_query(self)?;
           let plan = make_plan(env, query).await?;  // <-- ADD .await
           Ok(plan.is_volatile)
       }
   }

   // Check ALL IsVolatile implementations - each may call make_plan()
   ```

2. **Direct callers in interpreter.rs:**
   ```bash
   rg "make_plan\(" liquers-core/src/interpreter.rs
   ```

3. **AssetManager in assets.rs:**
   ```bash
   rg "make_plan\(" liquers-core/src/assets.rs
   ```

4. **Tests:**
   ```bash
   rg "make_plan\(" liquers-core/tests/
   rg "make_plan\(" liquers-core/src/ | rg "#\[cfg\(test\)\]" -A 50
   ```

5. **Recursive calls in plan.rs:**
   ```bash
   # Check if find_dependencies or has_volatile_dependencies call make_plan
   rg "make_plan\(" liquers-core/src/plan.rs
   ```

**Validation:**
```bash
# Check compilation (will fail on all make_plan() call sites)
cargo check -p liquers-core 2>&1 | grep "make_plan"

# Systematically fix each location:
# 1. interpreter.rs - IsVolatile implementations
# 2. interpreter.rs - direct calls
# 3. assets.rs - AssetManager calls
# 4. plan.rs - any recursive calls
# 5. tests - test calls

# After fixing ALL callers:
cargo check -p liquers-core
cargo test -p liquers-core --lib

# DOUBLE-CHECK: no sync calls remain
rg "make_plan\([^)]+\)[^.]" liquers-core/src/  # Should find NO matches (all should have .await)
```

**Rollback:**
```bash
git checkout liquers-core/src/interpreter.rs
git checkout liquers-core/src/assets.rs
git checkout liquers-core/src/plan.rs
git checkout liquers-core/tests/
```

**Agent Specification:**
- **Model:** opus
- **Skills:** rust-best-practices
- **Knowledge:** Search for all `make_plan` callers across entire codebase, focusing on IsVolatile trait implementations
- **Rationale:** Breaking change affecting multiple call sites INCLUDING trait implementations (easy to miss). Opus for exhaustive audit and ensuring no calls are overlooked. CRITICAL that IsVolatile implementations are updated correctly.

---

### Step 13.5: Audit Direct PlanBuilder Usage (Bypassing make_plan)

**File:** Multiple (entire codebase)

**Action:**
- Search for places where `PlanBuilder` is used directly without calling `make_plan()`
- Assess each location to determine if Phase 2 volatility check is needed
- If async context is available and envref is available, convert to use `make_plan()`
- If async is not possible or envref is not available, document as known limitation

**Code changes:**
```bash
# Search for direct PlanBuilder usage
rg "PlanBuilder::new" liquers-core/src/

# For each location found:
# 1. Determine if this is inside make_plan() itself (if yes, ignore)
# 2. Check if async context is available
# 3. Check if envref is available
# 4. Assess if Phase 2 check (has_volatile_dependencies) is needed

# Example locations to check:
# - find_dependencies() - builds plans for recursion (Phase 2 not needed, would cause infinite loop)
# - has_volatile_dependencies() - builds plans for validation (Phase 2 not needed, we ARE Phase 2)
# - Step 10.5 - builds plans for link parameter checking (Phase 2 not needed, Phase 1 only)
# - Tests - may need updating depending on what they test

# Document findings in a list:
# Location | Async? | EnvRef? | Phase 2 Needed? | Action
# ---------|--------|---------|-----------------|--------
# find_dependencies | Yes | Yes | No (would recurse) | No change
# has_volatile_dependencies | Yes | Yes | No (IS Phase 2) | No change
# Step 10.5 link check | No | No | No (Phase 1 only) | No change
# ... (continue for all locations)
```

**OUTPUT:** Create a document listing all direct PlanBuilder usage sites with assessment. Save as `specs/volatility-system/planbuilder-audit.md`.

**Validation:**
```bash
# Verify all PlanBuilder::new call sites have been audited
rg "PlanBuilder::new" liquers-core/src/ | wc -l

# Check audit document exists
cat specs/volatility-system/planbuilder-audit.md
```

**Rollback:**
```bash
# No code changes, just documentation
rm specs/volatility-system/planbuilder-audit.md
```

**Agent Specification:**
- **Model:** opus
- **Skills:** rust-best-practices
- **Knowledge:** Search entire codebase for PlanBuilder usage, understand make_plan flow, know when Phase 2 check is required
- **Rationale:** Comprehensive audit requiring architectural understanding of when volatility checks are needed vs. would cause issues (infinite loops, wrong results). Opus for careful analysis and documentation.

**NOTE:** Most direct PlanBuilder usage is legitimate and should NOT call make_plan() because:
1. They're inside make_plan() itself (find_dependencies, has_volatile_dependencies)
2. They need Phase 1 only (no async or envref available)
3. Calling make_plan() would cause infinite recursion

---

### Step 14: Update IsVolatile<E> for Plan Implementation

**File:** `liquers-core/src/interpreter.rs`

**Action:**
- Find `IsVolatile<E> for Plan` implementation
- Change to return cached `self.is_volatile` field instead of computing

**Code changes:**
```rust
// Find existing IsVolatile implementation for Plan (likely around line 316-422)
// MODIFY existing implementation:

impl<E: Environment> IsVolatile<E> for Plan {
    async fn is_volatile(&self, _env: EnvRef<E>) -> Result<bool, Error> {
        // CHANGE: Return cached value
        // Plan.is_volatile is always set during plan building (make_plan)
        Ok(self.is_volatile)
    }
}
```

**Validation:**
```bash
# Check compilation
cargo check -p liquers-core

# Test IsVolatile trait usage
cargo test -p liquers-core --lib interpreter::tests
```

**Rollback:**
```bash
git checkout liquers-core/src/interpreter.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** Read `liquers-core/src/interpreter.rs` IsVolatile implementations
- **Rationale:** Simple change to existing trait implementation.

---

### Step 14.5: Recipe Provider Volatility and Circularity Guarantees

**File:** `liquers-core/src/recipes.rs`

**Action:**
- Add `has_circular_dependencies: bool` field to Recipe struct
- Add `circular_dependency_key: Option<Key>` field to Recipe struct
- Recipe provider should check all recipes for circularity and volatility on load/update
- Add notes to recipe description: "Recipe is VOLATILE due to ..." or "ERROR: Recipe has circular dependencies due to: ..."
- Prevents unnecessary re-evaluation of circularity checks

**Code changes:**
```rust
// Modify Recipe struct
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Recipe {
    pub key: Key,
    pub query: Option<Query>,
    pub volatile: bool,

    // NEW: Add these fields
    /// If true, this recipe has circular dependencies
    /// Set by recipe provider during validation
    #[serde(default)]
    pub has_circular_dependencies: bool,

    /// If has_circular_dependencies is true, this holds the key that caused the cycle
    #[serde(default)]
    pub circular_dependency_key: Option<Key>,

    // ... existing fields ...
}

// Recipe provider should implement validation logic
// Example (conceptual - exact implementation depends on RecipeProvider trait):
impl RecipeProvider {
    /// Validate all recipes for circularity and volatility
    /// Should be called on load or when recipes are updated
    pub async fn validate_recipes<E: Environment>(&mut self, envref: EnvRef<E>) -> Result<(), Error> {
        for recipe in self.all_recipes_mut() {
            // Check for circular dependencies
            if let Some(query) = &recipe.query {
                let cmr = envref.get_command_metadata_registry();
                let mut pb = PlanBuilder::new(query.clone(), cmr);
                let plan = pb.build()?;

                let mut stack = Vec::new();
                match find_dependencies(envref.clone(), &plan, &mut stack, None).await {
                    Ok(_) => {
                        recipe.has_circular_dependencies = false;
                        recipe.circular_dependency_key = None;
                    }
                    Err(err) => {
                        recipe.has_circular_dependencies = true;
                        // Extract key from error (assuming error.key() method exists)
                        recipe.circular_dependency_key = err.key().cloned();
                        // Add note to recipe description
                        recipe.description = format!(
                            "{}\n\nERROR: Recipe has circular dependencies due to: {:?}",
                            recipe.description,
                            recipe.circular_dependency_key
                        );
                    }
                }

                // Check for volatility and add note
                if plan.is_volatile || recipe.volatile {
                    recipe.volatile = true;
                    // Add note to recipe description if not already present
                    if !recipe.description.contains("VOLATILE") {
                        recipe.description = format!(
                            "{}\n\nRecipe is VOLATILE due to: {}",
                            recipe.description,
                            if plan.is_volatile { "query contains volatile operations" } else { "marked as volatile" }
                        );
                    }
                }
            }
        }
        Ok(())
    }
}
```

**IMPORTANT:** The exact implementation depends on the RecipeProvider trait and how recipes are managed. This step provides the conceptual approach. The recipe provider must guarantee that:
1. `Recipe.volatile` is accurate (checked via plan building)
2. `Recipe.has_circular_dependencies` and `Recipe.circular_dependency_key` are set correctly
3. Recipe descriptions contain helpful error/warning messages

**Validation:**
```bash
# Check compilation
cargo check -p liquers-core

# Test recipe validation
cargo test -p liquers-core --lib recipes::tests::test_recipe_circular_dependency_detection
cargo test -p liquers-core --lib recipes::tests::test_recipe_volatility_detection
```

**Rollback:**
```bash
git checkout liquers-core/src/recipes.rs
```

**Agent Specification:**
- **Model:** opus
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:** Read `liquers-core/src/recipes.rs`, understand RecipeProvider trait, know find_dependencies implementation from Step 11
- **Rationale:** Significant architectural change requiring understanding of recipe management lifecycle. Opus for careful design of validation mechanism.

---

### **PHASE 4: Asset Management**

---

### Step 15: Modify AssetManager to Check Volatility and Skip Caching

**File:** `liquers-core/src/assets.rs`

**Action:**
- Find `AssetManager::get_asset_from_query()` method
- Before caching check, call `make_plan()` to get plan
- If `plan.is_volatile`, create new AssetRef and skip cache entirely
- Set `AssetData.is_volatile` field during initialization

**Code changes:**
```rust
// Find AssetManager impl block and modify get_asset_from_query method
impl<E: Environment> DefaultAssetManager<E> {
    pub async fn get_asset_from_query(&self, query: &Query) -> Result<AssetRef<E>, Error> {
        // NEW: Build plan to check volatility BEFORE cache lookup
        let plan = crate::interpreter::make_plan(self.envref.clone(), query).await?;

        if plan.is_volatile {
            // Volatile asset: ALWAYS create new AssetRef, NEVER cache
            let asset_ref = self.create_asset_from_query(query.clone(), plan.clone()).await?;

            // Set is_volatile flag in AssetData
            {
                let mut data = asset_ref.write().await;
                data.is_volatile = true;
            }

            return Ok(asset_ref);
        }

        // Non-volatile: use existing cache logic
        // ... existing caching code ...
    }

    pub async fn get_asset(&self, key: &Key) -> Result<AssetRef<E>, Error> {
        // Similar logic: check recipe volatility before caching
        if let Some(recipe) = self.envref.get_recipe_provider()
            .get_recipe(key)
            .await?
        {
            if recipe.volatile {
                // Create new AssetRef, skip cache
                let asset_ref = self.create_asset_from_key(key.clone(), recipe).await?;
                {
                    let mut data = asset_ref.write().await;
                    data.is_volatile = true;
                }
                return Ok(asset_ref);
            }
        }

        // Non-volatile: use existing cache logic
        // ... existing caching code ...
    }
}
```

**Validation:**
```bash
# Check compilation
cargo check -p liquers-core

# Create integration test for no-cache behavior
cargo test -p liquers-core --lib assets::tests::test_volatile_asset_no_cache
```

**Rollback:**
```bash
git checkout liquers-core/src/assets.rs
```

**Agent Specification:**
- **Model:** opus
- **Skills:** rust-best-practices
- **Knowledge:** Read full `liquers-core/src/assets.rs`, understand AssetManager caching logic
- **Rationale:** Critical caching behavior change. Opus for correctness and careful cache bypass logic.

---

### Step 16: Implement AssetRef::to_override() Method

**File:** `liquers-core/src/assets.rs`

**Action:**
- Add `to_override()` async method to `AssetRef<E>` impl block
- Handles all status transitions per Phase 2 spec
- Uses existing cancellation mechanism for in-flight assets

**Code changes:**
```rust
// Add to AssetRef impl block
impl<E: Environment> AssetRef<E> {
    /// Convert asset status to Override, preventing re-evaluation.
    /// Behavior depends on current status:
    /// - Directory, Source: No change (ignored)
    /// - None, Recipe, Submitted, Dependencies, Processing, Error, Cancelled:
    ///   Cancel if necessary, set value to Value::none(), set status to Override
    /// - Partial, Storing, Expired, Volatile, Ready:
    ///   Keep existing value, set status to Override
    pub async fn to_override(&self) -> Result<(), Error> {
        let mut data = self.write().await;

        match data.status {
            // Ignore these - no change
            Status::Directory | Status::Source => {
                // No-op
            }

            // In-progress or failed states: cancel, set to none value, mark Override
            Status::None | Status::Recipe | Status::Submitted |
            Status::Dependencies | Status::Processing |
            Status::Error | Status::Cancelled => {
                // Use existing cancel() method for in-flight evaluations
                // Drop the write lock before calling cancel() to avoid deadlock
                drop(data);

                // Cancel using AssetRef::cancel() method
                self.cancel().await;

                // Re-acquire write lock to set Override state
                let mut data = self.write().await;

                data.data = Some(Arc::new(E::Value::none()));
                data.binary = None;
                data.status = Status::Override;
                if let Metadata::MetadataRecord(ref mut mr) = data.metadata {
                    mr.status = Status::Override;
                }
            }

            // States with data: keep value, mark Override
            Status::Partial | Status::Storing | Status::Expired |
            Status::Volatile | Status::Ready => {
                data.status = Status::Override;
                if let Metadata::MetadataRecord(ref mut mr) = data.metadata {
                    mr.status = Status::Override;
                }
            }

            // Already Override - no-op (idempotent)
            Status::Override => {
                // No-op
            }
        }

        Ok(())
    }
}
```

**Validation:**
```bash
# Check compilation
cargo check -p liquers-core

# Create unit tests for all status transitions
cargo test -p liquers-core --lib assets::tests::test_to_override_from_volatile
cargo test -p liquers-core --lib assets::tests::test_to_override_from_processing
cargo test -p liquers-core --lib assets::tests::test_to_override_from_ready
cargo test -p liquers-core --lib assets::tests::test_to_override_idempotent
```

**Rollback:**
```bash
git checkout liquers-core/src/assets.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:** Read `liquers-core/src/assets.rs` AssetRef impl and Status enum
- **Rationale:** Complex state machine with multiple transitions. Sonnet for careful status handling.

---

### Step 17: Update AssetData to Set Metadata is_volatile

**File:** `liquers-core/src/assets.rs`

**Action:**
- Find where `MetadataRecord` is created/initialized for new assets
- Set `is_volatile` field based on `AssetData.is_volatile`
- Ensure consistency between AssetData and Metadata

**Code changes:**
```rust
// Find asset creation/initialization code (likely in create_asset or similar)
// When creating MetadataRecord or updating status:

impl<E: Environment> AssetData<E> {
    // Find initialization code and ensure is_volatile is set
    // Example pattern:
    fn initialize_metadata(&mut self) {
        if let Metadata::MetadataRecord(ref mut mr) = self.metadata {
            mr.is_volatile = self.is_volatile;
        }
    }

    // When status changes to Volatile
    fn set_status_to_volatile(&mut self) {
        self.status = Status::Volatile;
        if let Metadata::MetadataRecord(ref mut mr) = self.metadata {
            mr.status = Status::Volatile;
            mr.is_volatile = true;
        }
    }
}
```

**Validation:**
```bash
# Check compilation
cargo check -p liquers-core

# Test metadata consistency
cargo test -p liquers-core --lib assets::tests::test_volatile_metadata_consistency
```

**Rollback:**
```bash
git checkout liquers-core/src/assets.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** Read `liquers-core/src/assets.rs` metadata initialization code
- **Rationale:** Simple field synchronization between AssetData and Metadata.

---

### **PHASE 5: Interpreter Integration**

---

### Step 18: Update All Context::new() Call Sites

**File:** Multiple files (search across codebase)

**Action:**
- Search for all `Context::new()` calls
- Add `is_volatile` parameter (default to `false` unless context requires it)
- Update each call site appropriately

**Code changes:**
```bash
# Search for all Context::new calls
rg "Context::new\(" liquers-core/

# For each call site, update:
# OLD: Context::new(assetref).await
# NEW: Context::new(assetref, false).await  // Or true if in volatile context

# Example locations to check:
# - liquers-core/src/interpreter.rs (evaluate_plan, apply_plan)
# - liquers-core/src/assets.rs (asset service loop)
# - Any command execution code
```

**Validation:**
```bash
# Check compilation across all files
cargo check -p liquers-core

# Run all tests to ensure no regressions
cargo test -p liquers-core --lib
```

**Rollback:**
```bash
# Revert all modified files
git checkout liquers-core/src/
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** Search entire liquers-core for Context::new usage
- **Rationale:** Multi-file change requiring careful tracking of all call sites. Sonnet for systematic updates.

---

### Step 19: Update Asset Evaluation to Set Status::Volatile

**File:** `liquers-core/src/assets.rs`

**Action:**
- Find where asset status is set to `Status::Ready` after successful evaluation
- Check if `AssetData.is_volatile` is true
- If true, set status to `Status::Volatile` instead of `Status::Ready`

**Code changes:**
```rust
// Find asset evaluation completion code (likely in asset service loop or apply_plan)
// When setting status after successful evaluation:

// Pattern to find and modify:
// OLD:
// data.status = Status::Ready;
// data.metadata.set_status(Status::Ready);

// NEW:
if data.is_volatile {
    data.status = Status::Volatile;
    if let Metadata::MetadataRecord(ref mut mr) = data.metadata {
        mr.status = Status::Volatile;
        mr.is_volatile = true;
    }
} else {
    data.status = Status::Ready;
    if let Metadata::MetadataRecord(ref mut mr) = data.metadata {
        mr.status = Status::Ready;
    }
}
```

**Validation:**
```bash
# Check compilation
cargo check -p liquers-core

# Integration test: volatile query results in Status::Volatile
cargo test -p liquers-core --test volatility_integration test_volatile_status_set
```

**Rollback:**
```bash
git checkout liquers-core/src/assets.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** Read asset evaluation completion code in assets.rs
- **Rationale:** Simple conditional status assignment.

---

### Step 20: Context.is_volatile - Preparation for Future Side-Effects Feature

**File:** `liquers-core/src/context.rs`

**Action:**
- **NO CHANGES TO Context::evaluate()** in this step
- Context.is_volatile is a PREPARATION for future side-effects feature (out of scope)
- Side-effect assets (assets stored from a context in the asset manager/store) will become volatile in the future
- For now, Context.is_volatile is just stored and passed through, not actively used

**IMPORTANT:** The original plan to make assets volatile in `Context::evaluate()` was a mistake. The `is_volatile` field in Context is for future side-effect handling, which is out of scope for this implementation.

**Code changes:**
```rust
// NO CHANGES TO Context::evaluate() METHOD
// The Context.is_volatile field was added in Step 4
// It will be used in a future feature for side-effect asset tracking

// For now, Context.is_volatile is:
// 1. Set during Context::new() (Step 4)
// 2. Propagated via with_volatile() (Step 4)
// 3. NOT used to mark assets as volatile (that happens via Plan.is_volatile)
```

**Validation:**
```bash
# Check compilation - no changes needed
cargo check -p liquers-core

# Verify Context has is_volatile field and with_volatile method (added in Step 4)
rg "is_volatile" liquers-core/src/context.rs
```

**Rollback:**
```bash
# No changes made, nothing to rollback
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** Read `liquers-core/src/context.rs` to verify Step 4 changes are sufficient
- **Rationale:** Verification-only step, no implementation needed.

---

### **PHASE 6: Testing & Validation**

---

### Step 21: Create Unit Tests for Status::Volatile

**File:** `liquers-core/src/metadata.rs` (in `#[cfg(test)] mod tests`)

**Action:**
- Add unit tests for Status::Volatile enum behavior
- Test `has_data()`, `is_finished()`, `can_have_tracked_dependencies()`
- Test serialization/deserialization

**Code changes:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_volatile_has_data() {
        let status = Status::Volatile;
        assert!(status.has_data());
    }

    #[test]
    fn test_status_volatile_is_finished() {
        let status = Status::Volatile;
        assert!(status.is_finished());
    }

    #[test]
    fn test_status_volatile_cannot_track_dependencies() {
        let status = Status::Volatile;
        assert!(!status.can_have_tracked_dependencies());
    }

    #[test]
    fn test_status_volatile_serialization() {
        let status = Status::Volatile;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"Volatile\"");
        let deserialized: Status = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, Status::Volatile);
    }

    #[test]
    fn test_metadata_record_is_volatile_helper() {
        let mut mr = MetadataRecord::default();
        mr.is_volatile = true;
        assert!(mr.is_volatile());

        mr.is_volatile = false;
        mr.status = Status::Volatile;
        assert!(mr.is_volatile());
    }
}
```

**Validation:**
```bash
cargo test -p liquers-core --lib metadata::tests::test_status_volatile
cargo test -p liquers-core --lib metadata::tests::test_metadata_record_is_volatile
```

**Rollback:**
```bash
git checkout liquers-core/src/metadata.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** liquers-unittest
- **Knowledge:** Read `liquers-core/src/metadata.rs` Status enum
- **Rationale:** Straightforward unit tests for enum variant.

---

### Step 22: Create Unit Tests for Plan/PlanBuilder Volatility

**File:** `liquers-core/src/plan.rs` (in tests module)

**Action:**
- Test 'v' instruction marks plan volatile
- Test volatile command marks plan volatile
- Test circular dependency detection
- Test has_volatile_dependencies logic

**Code changes:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_is_volatile_field() {
        let mut plan = Plan::new();
        assert!(!plan.is_volatile());

        plan.set_volatile(true);
        assert!(plan.is_volatile());
    }

    #[test]
    fn test_plan_volatile_serialization() {
        let mut plan = Plan::new();
        plan.set_volatile(true);

        let json = serde_json::to_string(&plan).unwrap();
        let deserialized: Plan = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.is_volatile, true);
    }

    #[tokio::test]
    async fn test_plan_builder_marks_volatile_for_v_instruction() {
        // Create mock environment and registry
        let query = crate::parse::parse_query("data/v/to_text").expect("parse");
        let cmr = CommandMetadataRegistry::new();
        let mut pb = PlanBuilder::new(query, &cmr);
        let plan = pb.build().expect("build");

        assert!(plan.is_volatile);
        // Verify Step::Info contains "Volatile due to instruction 'v'"
        let has_info = plan.steps.iter().any(|s| {
            matches!(s, Step::Info(msg) if msg.contains("instruction 'v'"))
        });
        assert!(has_info);
    }

    #[tokio::test]
    async fn test_plan_builder_volatile_command_marks_volatile() {
        // Create mock environment and registry with a volatile command
        // Build plan and verify is_volatile = true
    }

    #[tokio::test]
    async fn test_v_instruction_edge_case_with_q() {
        // CRITICAL TEST: "action/v" is volatile, but "action/v/q" is NOT volatile
        // "action/v/q" evaluates to Query("action/v"), which is a non-volatile query value

        // Test 1: "action/v" should be volatile
        let query1 = crate::parse::parse_query("action/v").expect("parse");
        let cmr = CommandMetadataRegistry::new();
        let mut pb1 = PlanBuilder::new(query1, &cmr);
        let plan1 = pb1.build().expect("build");
        assert!(plan1.is_volatile, "action/v should be volatile");

        // Test 2: "action/v/q" should NOT be volatile
        let query2 = crate::parse::parse_query("action/v/q").expect("parse");
        let mut pb2 = PlanBuilder::new(query2, &cmr);
        let plan2 = pb2.build().expect("build");
        assert!(!plan2.is_volatile, "action/v/q should NOT be volatile (evaluates to Query value)");
    }

    // More tests for circular dependency detection, volatile dependencies, etc.
}
```

**Validation:**
```bash
cargo test -p liquers-core --lib plan::tests
cargo test -p liquers-core --lib plan::tests::test_v_instruction_edge_case_with_q
```

**Rollback:**
```bash
git checkout liquers-core/src/plan.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** liquers-unittest
- **Knowledge:** Read Phase 3 test specifications, `liquers-core/src/plan.rs`
- **Rationale:** Complex test scenarios requiring mock environment setup. Sonnet for test design.

---

### Step 23: Create Integration Tests

**File:** `liquers-core/tests/volatility_integration.rs` (new file)

**Action:**
- Create comprehensive integration tests per Phase 3 spec
- Test full pipeline: Query → Plan → Asset → State
- Test AssetManager no-cache behavior
- Test circular dependency detection
- Test serialization round-trip

**Code changes:**
```rust
// Create new file: liquers-core/tests/volatility_integration.rs

use liquers_core::*;

#[tokio::test]
async fn test_volatile_query_to_asset_simple() -> Result<(), Box<dyn std::error::Error>> {
    // Setup environment with volatile command
    // Build plan and verify is_volatile
    // Evaluate and verify Status::Volatile
    Ok(())
}

#[tokio::test]
async fn test_asset_manager_volatile_no_cache() -> Result<(), Box<dyn std::error::Error>> {
    // Test that requesting same volatile query 3 times returns 3 different AssetRef IDs
    Ok(())
}

#[tokio::test]
async fn test_circular_dependency_direct_detection() -> Result<(), Box<dyn std::error::Error>> {
    // Create circular recipes: A→B, B→A
    // Attempt to make plan - should detect circular dependency
    Ok(())
}

// ... more tests from Phase 3 spec ...
```

**Validation:**
```bash
cargo test -p liquers-core --test volatility_integration
```

**Rollback:**
```bash
rm liquers-core/tests/volatility_integration.rs
```

**Agent Specification:**
- **Model:** opus
- **Skills:** liquers-unittest, rust-best-practices
- **Knowledge:** Read Phase 3 integration test specs, all volatility design docs
- **Rationale:** Critical integration tests validating entire feature. Opus for comprehensive coverage.

---

### Step 24: Final Validation and Cleanup

**File:** Multiple (entire codebase)

**Action:**
- Run full test suite
- Run clippy with warnings as errors
- **COMPREHENSIVE VERIFICATION:** Check implementation against actual codebase
- Check for any missed Status match arms
- Verify no breaking changes to public API (except documented ones)
- Update CLAUDE.md with volatility patterns if needed

**Code changes:**
```bash
# Full validation checklist
cargo check -p liquers-core
cargo test -p liquers-core --lib
cargo test -p liquers-core --tests
cargo clippy -p liquers-core -- -D warnings

# CRITICAL: Verify no default match arms remain (should have been removed in Steps 0 and 0.5)
rg "match.*[Ss]tatus" liquers-core/src/ -A 20 | rg "_ =>" || echo "✓ No default Status arms in liquers-core"
rg "match.*[Ss]tatus" liquers-lib/src/ -A 20 | rg "_ =>" || echo "✓ No default Status arms in liquers-lib"

# CRITICAL: Verify all make_plan() calls are awaited
rg "make_plan\([^)]+\)(?!\.await)" liquers-core/src/ || echo "✓ All make_plan() calls are awaited"

# CRITICAL: Verify Context does not implement Clone
rg "impl.*Clone.*for.*Context" liquers-core/src/context.rs && echo "✗ ERROR: Context should NOT implement Clone" || echo "✓ Context correctly does not implement Clone"

# Verify no regressions in other crates
cargo test -p liquers-lib
cargo test -p liquers-store

# Check documentation builds
cargo doc -p liquers-core --no-deps

# COMPREHENSIVE CODEBASE VERIFICATION
echo "=== Codebase Verification Checklist ==="

# 1. Verify Status enum has 15 variants (14 existing + 1 new Volatile)
echo "Status variants count:"
rg "pub enum Status" liquers-core/src/metadata.rs -A 20 | rg "^\s+[A-Z]" | wc -l

# 2. Verify Plan struct has is_volatile field
rg "pub struct Plan" liquers-core/src/plan.rs -A 10 | rg "is_volatile: bool" || echo "✗ ERROR: Plan missing is_volatile field"

# 3. Verify MetadataRecord struct has is_volatile field
rg "pub struct MetadataRecord" liquers-core/src/metadata.rs -A 30 | rg "is_volatile: bool" || echo "✗ ERROR: MetadataRecord missing is_volatile field"

# 4. Verify Context::new signature includes is_volatile parameter
rg "pub async fn new.*is_volatile.*bool" liquers-core/src/context.rs || echo "✗ ERROR: Context::new missing is_volatile parameter"

# 5. Verify AssetData struct has is_volatile field
rg "pub struct AssetData" liquers-core/src/assets.rs -A 20 | rg "is_volatile: bool" || echo "✗ ERROR: AssetData missing is_volatile field"

# 6. Verify make_plan is async
rg "pub async fn make_plan" liquers-core/src/interpreter.rs || echo "✗ ERROR: make_plan should be async"

# 7. Verify find_dependencies exists
rg "async fn find_dependencies" liquers-core/src/plan.rs || echo "✗ ERROR: find_dependencies function not found"

# 8. Verify has_volatile_dependencies exists
rg "async fn has_volatile_dependencies" liquers-core/src/plan.rs || echo "✗ ERROR: has_volatile_dependencies function not found"

# 9. Verify AssetRef::to_override exists
rg "pub async fn to_override" liquers-core/src/assets.rs || echo "✗ ERROR: AssetRef::to_override method not found"

# 10. Check Phase 2 documentation corrections
echo "NOTE: Verify Phase 2 documentation has been corrected:"
echo "  - MetadataRecord.is_volatile: NO #[serde(default)]"
echo "  - Plan.is_volatile: NO #[serde(default)]"
echo "  - Context Clone: Document should clarify Context does NOT implement Clone"
```

**Validation:**
All checks pass, no warnings, full test coverage, codebase verification confirms implementation matches design.

**Rollback:**
Full feature rollback - see Section 6 below.

**Agent Specification:**
- **Model:** opus
- **Skills:** All (rust-best-practices, liquers-unittest)
- **Knowledge:** Full codebase, all specs (Phase 1-3), PROJECT_OVERVIEW.md, CLAUDE.md
- **Rationale:** Final validation requires understanding entire system AND verifying implementation matches actual codebase structure. Opus for exhaustive review and codebase alignment verification.

---

## 4. Testing Plan

### Unit Testing Checkpoints

**After Step 1 (Status::Volatile):**
- Run: `cargo test -p liquers-core --lib metadata::tests`
- Verify: All Status enum methods handle Volatile correctly

**After Step 3 (Plan.is_volatile):**
- Run: `cargo test -p liquers-core --lib plan::tests`
- Verify: Plan field serializes/deserializes correctly

**After Step 10 (Phase 1 volatility detection):**
- Run: `cargo test -p liquers-core --lib plan::tests::test_v_instruction_marks_volatile`
- Verify: PlanBuilder correctly detects 'v' instruction

**After Step 12 (has_volatile_dependencies):**
- Run: `cargo test -p liquers-core --lib plan::tests`
- Verify: Dependency volatility propagation works

**After Step 16 (to_override):**
- Run: `cargo test -p liquers-core --lib assets::tests`
- Verify: All status transitions work correctly

### Integration Testing Checkpoints

**After Step 15 (AssetManager caching):**
- Run: `cargo test -p liquers-core --lib assets::tests`
- Verify: Volatile assets never cached

**After Step 23 (Full integration tests):**
- Run: `cargo test -p liquers-core --test volatility_integration`
- Verify: All integration tests pass

### Validation Frequency

- **After each step:** `cargo check -p liquers-core` (must pass)
- **After each phase:** `cargo test -p liquers-core --lib`
- **Before completion:** `cargo clippy -p liquers-core -- -D warnings`

---

## 5. Agent Assignment Summary

| Step | Phase | Agent | Skills | Complexity | Time |
|------|-------|-------|--------|------------|------|
| 0 | Pre-Cleanup | sonnet | rust-best-practices | Medium | 1.5h |
| 0.5 | Pre-Cleanup | sonnet | rust-best-practices | Medium | 1h |
| 1 | Core Data | haiku | rust-best-practices | Low | 30min |
| 2 | Core Data | haiku | rust-best-practices | Low | 30min |
| 2.5 | Core Data | haiku | rust-best-practices | Low | 45min |
| 3 | Core Data | haiku | rust-best-practices | Low | 30min |
| 4 | Core Data | sonnet | rust-best-practices | Medium | 1h |
| 5 | Core Data | haiku | rust-best-practices | Low | 30min |
| 6 | Plan Building | haiku | rust-best-practices | Low | 30min |
| 7 | Plan Building | haiku | rust-best-practices | Low | 30min |
| 8 | Plan Building | sonnet | rust-best-practices | Medium | 1h |
| 9 | Plan Building | haiku | rust-best-practices | Low | 15min |
| 10 | Plan Building | sonnet | rust-best-practices, liquers-unittest | Medium | 2h |
| 10.5 | Plan Building | sonnet | rust-best-practices, liquers-unittest | Medium | 1.5h |
| 11 | Dependency | opus | rust-best-practices, liquers-unittest | High | 3h |
| 12 | Dependency | sonnet | rust-best-practices, liquers-unittest | Medium | 1.5h |
| 13 | Dependency | opus | rust-best-practices | High | 2h |
| 13.5 | Dependency | opus | rust-best-practices | High | 2h |
| 14 | Dependency | haiku | rust-best-practices | Low | 15min |
| 14.5 | Dependency | opus | rust-best-practices, liquers-unittest | High | 2h |
| 15 | Asset Mgmt | opus | rust-best-practices | High | 2h |
| 16 | Asset Mgmt | sonnet | rust-best-practices, liquers-unittest | Medium | 2h |
| 17 | Asset Mgmt | haiku | rust-best-practices | Low | 30min |
| 18 | Interpreter | sonnet | rust-best-practices | Medium | 1h |
| 19 | Interpreter | haiku | rust-best-practices | Low | 30min |
| 20 | Interpreter | haiku | rust-best-practices | Low | 30min |
| 21 | Testing | haiku | liquers-unittest | Low | 1h |
| 22 | Testing | sonnet | liquers-unittest | Medium | 2h |
| 23 | Testing | opus | liquers-unittest, rust-best-practices | High | 4h |
| 24 | Testing | opus | All | High | 2h |
| **Total** | | | | | **~38h** |

---

## 6. Rollback Plan

### Per-Step Rollback

Each step includes specific rollback instructions:
```bash
git checkout <file-path>
cargo check -p liquers-core
```

### Full Feature Rollback

```bash
# Create rollback branch
git checkout -b rollback-volatility <commit-before-feature>

# Or reset
git reset --hard <commit-before-feature>

# Verify
cargo check -p liquers-core
cargo test -p liquers-core
```

---

## 7. Documentation Updates

### Phase 2 Architecture Document (CRITICAL CORRECTIONS)
**File:** `specs/volatility-system/phase2-architecture.md`

**Required corrections:**
1. **MetadataRecord.is_volatile field:** Remove `#[serde(default)]` annotation. Document that this field is always required in serialized format (no default).
2. **Plan.is_volatile field:** Remove `#[serde(default)]` annotation. Document that this field is always required in serialized format (no default).
3. **Context Clone clarification:** Add explicit note that Context does NOT and CANNOT implement Clone trait (due to AssetRef). Document that `with_volatile()` manually constructs new Context instances.
4. **is_finished() method:** Verify documentation matches implementation - Status::Volatile returns `true` from is_finished().

### CLAUDE.md
- Add volatility patterns section
- Document 'v' instruction as query-level marker (like 'q' and 'ns')

### PROJECT_OVERVIEW.md
- Add volatility system overview
- Document two-phase volatility detection (commands + dependencies)

### ISSUES.md
- Mark Issue 1 (VOLATILE-METADATA) as resolved
- Document breaking changes: make_plan() async, Context::new() signature

---

## 8. Breaking Changes

1. **make_plan() is now async** - Add `.await` to all calls
2. **Context::new() signature changed** - Add `is_volatile` parameter
3. **Status enum has new variant** - Add `Status::Volatile` to all match arms

**Migration Guide:** See Phase 2 architecture doc for details.

---

## Document Metadata

**Version:** 1.2 (Additional User Review Issues Fixed)
**Date:** 2026-02-19
**Status:** Ready for execution
**Estimated Completion:** 9-10 days (38 agent-hours)

**Changelog:**
- **v1.2 (2026-02-19):** Fixed additional issues from user review:
  1. Added Step 2.5: Metadata.is_volatile() method with legacy support, AssetInfo.is_volatile field
  2. Revised Step 10: Removed incorrect Approach 1 (no is_instruction method), kept only Approach 2
  3. Enhanced Step 11: find_dependencies now includes cwd parameter, explicit Step handling (UseKeyValue, GetAssetRecipe, GetResource, SetCwd), improved error messages with specific key, no default match arms
  4. Updated Step 12: Pass cwd parameter to find_dependencies
  5. Added Step 10.5: Check ResolvedParameterValues links for volatility
  6. Enhanced Step 22: Added unit test for "action/v" vs "action/v/q" edge case
  7. Updated Step 16: Use existing cancel() method in to_override() implementation
  8. Verified Step 19: Correctly sets both is_volatile=true AND status=Volatile
  9. Revised Step 20: Context.is_volatile is for future side-effects feature (no changes to evaluate())
  10. Added Step 14.5: Recipe provider volatility and circularity guarantees (has_circular_dependencies, circular_dependency_key)
  11. Added Step 13.5: Audit direct PlanBuilder usage (bypassing make_plan)
  12. Updated overview: 26 steps → 30 steps, 32h → 38h, 8 days → 9-10 days
- **v1.1 (2026-02-17):** Fixed 7 critical issues identified in multi-agent review:
  1. Added Step 0: Remove default match arms in assets.rs (CRITICAL-3)
  2. Added Step 0.5: Update downstream crate Status matches (CRITICAL-4)
  3. Removed `#[serde(default)]` from MetadataRecord.is_volatile and Plan.is_volatile (CRITICAL-5)
  4. Revised Step 10 to handle 'v' instruction at query-level, not action-level (CRITICAL-2)
  5. Enhanced Step 13 to explicitly audit ALL make_plan() call sites including IsVolatile trait implementations (CRITICAL-1)
  6. Added clarifying note in Step 4 that Context does NOT implement Clone (CRITICAL-6)
  7. Enhanced Step 24 and Prerequisites with comprehensive codebase verification checks (CRITICAL-7)
- **v1.0 (2026-02-17):** Initial implementation plan from multi-agent drafting process
