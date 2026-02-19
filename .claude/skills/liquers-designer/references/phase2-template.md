# Phase 2: Solution & Architecture Template

## Purpose

Phase 2 defines **HOW** the feature will be implemented through data structures, interfaces, and function signatures. **NO implementation code**, only signatures and architecture decisions.

**Goal:** Create a compilable architecture where:
- All data structures are defined (fields, types, ownership)
- All trait implementations are planned (which traits, bounds)
- All function signatures are specified (parameters, return types)
- All integration points are documented (which modules, dependencies)

**Duration:** 1-2 hours (including rust-best-practices auto-invoke)

**Output:** An architecture document that a developer can use to implement the feature without making architectural decisions.

## Auto-Invoke: rust-best-practices Skill

Before finalizing Phase 2, this skill **automatically invokes** the rust-best-practices skill to validate:
- Ownership patterns (Arc, Box, borrowed references)
- Trait bounds are minimal and justified
- No anti-patterns (e.g., unnecessary cloning, default match arms)
- Compilation feasibility

**You do not need to manually invoke this skill.**

## Template

Use this template for your `phase2-architecture.md`:

```markdown
# Phase 2: Solution & Architecture - <Feature Name>

## Overview

<2-3 sentences summarizing the architectural approach>
<Example: "Parquet support implemented as two commands in liquers-lib. Uses parquet crate for serialization, integrates with existing DataFrame handling.">

## Data Structures

### New Structs

#### StructName1
```rust
pub struct StructName1 {
    field1: Type1,
    field2: Arc<Type2>,  // Ownership: Arc for shared access
    // ... more fields
}
```

**Ownership rationale:**
- `field1` is owned because <reason>
- `field2` is Arc-wrapped because <reason>

**Serialization:**
- Derives: `Serialize, Deserialize`
- Special handling: <if any fields need #[serde(skip)] or custom serialization>

#### StructName2
... (repeat for each struct)

### New Enums

#### EnumName1
```rust
pub enum EnumName1 {
    Variant1(Type),
    Variant2 { field: Type },
    Variant3,
}
```

**Variant semantics:**
- `Variant1`: <When used>
- `Variant2`: <When used>
- `Variant3`: <When used>

**No default match arm:** All match statements on this enum must be explicit.

### ExtValue Extensions (if applicable)

```rust
// In liquers-lib/src/value/mod.rs
pub enum ExtValue {
    // ... existing variants
    NewVariant { value: Arc<NewType> },
}
```

**Rationale:** <Why a new variant is needed vs. using existing variants>

## Trait Implementations

### Trait: TraitName

**Implementor:** `StructName1`

```rust
impl TraitName for StructName1 {
    fn method1(&self, param: Type) -> Result<ReturnType, Error> {
        // Signature only; implementation in Phase 4
    }

    fn method2(&mut self) {
        // Signature only
    }
}
```

**Bounds:** None / `where Self: Clone + Send` (justify if bounds are required)

**Default methods:** <Which trait methods use default implementations?>

### Trait: AnotherTrait
... (repeat for each trait implementation)

## Generic Parameters & Bounds

### Generic Struct: GenericStruct<T>

```rust
pub struct GenericStruct<T>
where
    T: SomeTrait + Send + Sync + 'static,
{
    data: Arc<T>,
}
```

**Bound justification:**
- `SomeTrait`: <Why this bound is required>
- `Send + Sync`: <If the struct will be shared across threads>
- `'static`: <If the struct will be stored in long-lived contexts>

**Avoid over-constraining:** Only add bounds that are strictly necessary.

## Sync vs Async Decisions

### Async Functions

| Function | Async? | Rationale |
|----------|--------|-----------|
| `fn read_parquet` | Yes | Performs I/O via AsyncStore |
| `fn parse_parquet` | No | CPU-bound parsing, no I/O |
| `fn to_parquet` | No | Serialization is in-memory |

**Default to async** for:
- I/O operations (file, network, database)
- Functions called from async contexts

**Use sync** for:
- Pure computation (no I/O)
- Functions that will be called from sync contexts (e.g., Python bindings)

**Pattern:** Async implementation with sync wrapper if needed (see AsyncStoreWrapper pattern).

## Function Signatures

### Module: liquers_lib::parquet

```rust
// Command function (registered via register_command!)
pub fn to_parquet(state: &State<Value>) -> Result<Value, Error> {
    // Implementation in Phase 4
}

pub fn from_parquet(state: &State<Value>) -> Result<Value, Error> {
    // Implementation in Phase 4
}

// Helper functions
pub fn dataframe_to_parquet_bytes(df: &DataFrame) -> Result<Vec<u8>, Error> {
    // Implementation in Phase 4
}

pub fn parquet_bytes_to_dataframe(bytes: &[u8]) -> Result<DataFrame, Error> {
    // Implementation in Phase 4
}
```

**Parameter choices:**
- `state: &State<Value>` - Borrowed because commands don't own state
- `df: &DataFrame` - Borrowed to avoid cloning large DataFrames
- Return `Vec<u8>` (owned) - Caller needs ownership of bytes

### Module: liquers_axum::response (if applicable)

```rust
pub fn parquet_content_type() -> &'static str {
    "application/vnd.apache.parquet"
}
```

## Integration Points

### Crate: liquers-lib

**File:** `liquers-lib/src/parquet/mod.rs` (new module)

**Exports:**
- `pub fn to_parquet(...)`
- `pub fn from_parquet(...)`
- Helper functions (pub(crate) or private)

**Registration:** In `liquers-lib/src/commands.rs`, register commands:
```rust
register_command!(cr, fn to_parquet(state) -> result namespace: "polars")?;
register_command!(cr, fn from_parquet(state) -> result namespace: "polars")?;
```

### Crate: liquers-axum (if applicable)

**File:** `liquers-axum/src/response.rs` (modify existing)

**Change:** Add Parquet Content-Type mapping:
```rust
match extension {
    "json" => "application/json",
    "csv" => "text/csv",
    "parquet" => "application/vnd.apache.parquet",  // NEW
    // ...
}
```

### Dependencies

**Add to `liquers-lib/Cargo.toml`:**
```toml
[dependencies]
parquet = "52.2.0"  # Or latest compatible with polars
```

**Version rationale:** Must match Polars' parquet dependency to avoid conflicts.

## Relevant Commands

### New Commands

List all new commands that will be introduced by this feature, with full signatures:

```rust
// Command: <namespace>/<command_name>
// Registered via register_command!
pub fn command_name(state: &State<Value>, param1: Type1) -> Result<Value, Error> {
    // Signature only
}
```

| Command | Namespace | Parameters | Description |
|---------|-----------|------------|-------------|
| `to_parquet` | `polars` | state | Convert DataFrame to Parquet binary |
| `from_parquet` | `polars` | state | Parse Parquet binary into DataFrame |

### Relevant Existing Namespaces

List existing command namespaces from liquers-lib that are relevant to this feature (used in queries, interact with new commands, or provide context):

| Namespace | Relevance | Key Commands |
|-----------|-----------|--------------|
| `polars` | New commands registered here | `filter`, `select`, `to_csv`, `from_csv` |
| `core` | Base state operations | `text`, `json`, `yaml` |

**Ask user:** Are these the right namespaces? Any missing ones?

## Web Endpoints (if applicable)

### Endpoint: GET `/api/query/<query>`

**Behavior change:**
- If query ends with `~to_parquet`, set Content-Type: `application/vnd.apache.parquet`
- If file extension is `.parquet`, trigger Parquet deserialization

**Example:**
```
GET /api/query/-/data/sales.parquet~to_parquet
→ Response: Parquet binary, Content-Type: application/vnd.apache.parquet
```

**No new routes:** Uses existing query execution endpoint.

## Error Handling

### New Error Types (if needed)

**Do NOT create new error types.** Use existing `liquers_core::error::Error` with appropriate `ErrorType`.

### Error Constructors

```rust
// Use typed constructors, NOT Error::new
Error::general_error(format!("Parquet parsing failed: {}", e))
Error::from_error(ErrorType::General, parquet_error)
```

**Error propagation:**
- `?` operator for Result types
- Wrap external errors with `Error::from_error(ErrorType::General, external_error)`

### Error Scenarios

| Scenario | ErrorType | Example |
|----------|-----------|---------|
| Parquet parsing fails | `ErrorType::General` | `Error::general_error("Invalid Parquet file")` |
| DataFrame conversion fails | `ErrorType::General` | `Error::from_error(ErrorType::General, e)` |
| Unsupported Parquet feature | `ErrorType::General` | `Error::general_error("Nested types not supported")` |

## Serialization Strategy

### Serde Annotations

**Structs:**
```rust
#[derive(Serialize, Deserialize)]
pub struct ParquetMetadata {
    pub schema: String,
    #[serde(skip)]  // Not serializable
    pub row_groups: Vec<RowGroup>,
}
```

**Use `#[serde(skip)]` for:**
- Non-serializable types (file handles, Arc<dyn Trait>)
- Large temporary data
- Runtime-only state

### Round-trip Compatibility

**Test plan (Phase 3):** Serialize → Deserialize → Serialize should produce identical output.

## Concurrency Considerations

### Thread Safety

**Structs that will be shared across threads:**
- Wrap in `Arc<Mutex<T>>` or `Arc<RwLock<T>>`
- Ensure all fields are `Send + Sync`

**Example:**
```rust
// If ParquetCache will be shared across async tasks:
pub struct ParquetCache {
    data: Arc<RwLock<HashMap<Key, Vec<u8>>>>,
}
```

**No locks needed if:**
- Data is immutable after creation
- Each thread/task has its own instance

## Compilation Validation

Before requesting approval, ensure:

- [ ] All type signatures are specified
- [ ] All trait bounds are minimal and justified
- [ ] No use of `unwrap()` or `expect()` in signatures (return `Result` instead)
- [ ] All imports are documented (which crates, which modules)
- [ ] Generic parameters have clear purpose

**Run:** `cargo check --all-features` (mentally - actual compilation in Phase 4)

**Expect:** No compilation errors, only missing implementations (which is correct at this stage).

## References to liquers-patterns.md

Before finalizing, cross-check against `references/liquers-patterns.md`:

- [ ] Crate dependencies follow one-way flow
- [ ] ExtValue extensions in liquers-lib only
- [ ] Commands registered via `register_command!` macro
- [ ] AsyncStore pattern followed for stores
- [ ] UIElement pattern followed for UI (if applicable)
- [ ] Error handling uses typed constructors
- [ ] Async is default, sync wrappers if needed

```

## Example: Parquet File Support Architecture

Here's a real example following the template:

```markdown
# Phase 2: Solution & Architecture - Parquet File Support

## Overview

Parquet support implemented as two commands in the `polars` namespace within liquers-lib. Uses the `parquet` crate (same version as Polars dependency) for serialization/deserialization. Integrates with existing DataFrame handling; no new value types needed.

## Data Structures

### No New Structs Required

Parquet handling is stateless; uses functions only.

If we needed configuration:
```rust
pub struct ParquetConfig {
    pub compression: CompressionType,
    pub row_group_size: usize,
}
```

But for Phase 1, we'll use sensible defaults (Snappy compression, 1M row groups).

### ExtValue Extensions

**Not needed.** Use existing `ExtValue::DataFrame` for in-memory representation.

Parquet bytes stored as `Value::Bytes`.

## Trait Implementations

**None required.** Commands are registered functions, not trait implementations.

## Generic Parameters & Bounds

**Not applicable.** All functions are concrete (no generics).

## Sync vs Async Decisions

| Function | Async? | Rationale |
|----------|--------|-----------|
| `fn to_parquet` | No | In-memory serialization, no I/O |
| `fn from_parquet` | No | In-memory deserialization, no I/O |
| `fn dataframe_to_parquet_bytes` | No | CPU-bound, called from sync command |
| `fn parquet_bytes_to_dataframe` | No | CPU-bound, called from sync command |

**Decision:** All sync. File I/O happens via AsyncStore (before/after these functions).

**Pattern:**
1. AsyncStore reads bytes asynchronously
2. `from_parquet` deserializes bytes synchronously
3. `to_parquet` serializes DataFrame synchronously
4. AsyncStore writes bytes asynchronously

## Function Signatures

### Module: liquers_lib::parquet

```rust
// Command functions (registered via register_command!)
pub fn to_parquet(state: &State<Value>) -> Result<Value, Error> {
    // Extract DataFrame from state
    // Call dataframe_to_parquet_bytes
    // Wrap bytes in Value::Bytes
}

pub fn from_parquet(state: &State<Value>) -> Result<Value, Error> {
    // Extract bytes from state
    // Call parquet_bytes_to_dataframe
    // Wrap DataFrame in ExtValue::DataFrame
}

// Helper functions
fn dataframe_to_parquet_bytes(df: &DataFrame) -> Result<Vec<u8>, Error> {
    // Use parquet::file::writer::SerializedFileWriter
}

fn parquet_bytes_to_dataframe(bytes: &[u8]) -> Result<DataFrame, Error> {
    // Use parquet::file::reader::SerializedFileReader
}
```

**Parameter choices:**
- `state: &State<Value>` - Standard command signature
- `df: &DataFrame` - Borrowed to avoid cloning (DataFrames can be large)
- Return `Vec<u8>` - Caller needs ownership of serialized bytes
- Return `DataFrame` - Caller needs ownership of deserialized DataFrame

## Integration Points

### Crate: liquers-lib

**New file:** `liquers-lib/src/polars/parquet.rs`

**Modify:** `liquers-lib/src/polars/mod.rs`
```rust
pub mod dataframe;
pub mod parquet;  // NEW MODULE
```

**Modify:** `liquers-lib/src/commands.rs`
```rust
use crate::polars::parquet::{to_parquet, from_parquet};

register_command!(cr, fn to_parquet(state) -> result
    namespace: "polars"
    label: "To Parquet"
    doc: "Convert DataFrame to Parquet binary format"
    filename: "data.parquet"
)?;

register_command!(cr, fn from_parquet(state) -> result
    namespace: "polars"
    label: "From Parquet"
    doc: "Parse Parquet binary into DataFrame"
)?;
```

### Crate: liquers-axum

**Modify:** `liquers-axum/src/response.rs`

Add Parquet Content-Type:
```rust
fn extension_to_content_type(extension: &str) -> &'static str {
    match extension {
        "json" => "application/json",
        "csv" => "text/csv",
        "parquet" => "application/vnd.apache.parquet",  // NEW
        _ => "application/octet-stream",
    }
}
```

### Dependencies

**Add to `liquers-lib/Cargo.toml`:**
```toml
[dependencies]
polars = { version = "0.44", features = ["parquet"] }
# Note: parquet crate is already included via polars dependency
```

**No new direct parquet dependency needed** - use polars' re-exported parquet module.

## Web Endpoints

### Endpoint: GET `/api/query/<query>`

**No new routes.** Existing query endpoint handles Parquet automatically:

**Example queries:**
```
GET /api/query/-/data.parquet
→ Reads data.parquet from store, deserializes via from_parquet implicitly

GET /api/query/-/df~to_parquet
→ Executes df query, converts to Parquet via to_parquet command
→ Response Content-Type: application/vnd.apache.parquet
```

**File extension routing:** `.parquet` extension triggers Parquet deserialization (existing pattern in liquers-axum).

## Error Handling

### Error Scenarios

| Scenario | Constructor | Example |
|----------|-------------|---------|
| Invalid Parquet file | `Error::general_error` | `Error::general_error("Invalid Parquet file structure".to_string())` |
| Schema mismatch | `Error::general_error` | `Error::general_error(format!("Schema mismatch: expected {:?}, got {:?}", expected, actual))` |
| Unsupported feature | `Error::general_error` | `Error::general_error("Nested Parquet types not supported".to_string())` |
| Conversion failure | `Error::from_error` | `Error::from_error(ErrorType::General, polars_error)` |

### Error Propagation

```rust
fn dataframe_to_parquet_bytes(df: &DataFrame) -> Result<Vec<u8>, Error> {
    let mut buf = Vec::new();
    ParquetWriter::new(&mut buf)
        .finish(df)
        .map_err(|e| Error::from_error(ErrorType::General, e))?;
    Ok(buf)
}
```

## Serialization Strategy

**Not applicable** - Parquet is the serialization format itself.

No Serde derives needed (functions are stateless).

## Concurrency Considerations

### Thread Safety

**No shared state.** All functions are stateless, operating on borrowed or owned data.

**Safe to call from multiple threads** - each invocation is independent.

**No locks needed.**

## Compilation Validation

**Expected to compile:** Yes (after adding parquet dependency)

**Potential issues:**
- Polars version mismatch - ensure `liquers-lib/Cargo.toml` uses compatible polars version
- Feature flags - ensure `polars` is imported with `features = ["parquet"]`

**Check:**
```bash
cargo check -p liquers-lib --features polars
```

## References to liquers-patterns.md

- [x] Crate dependencies: liquers-lib only (correct)
- [x] Commands registered via `register_command!` macro
- [x] Error handling uses `Error::general_error()` and `Error::from_error()`
- [x] No unwrap/expect in function signatures
- [x] Functions follow Rust naming conventions (snake_case)
```

## Review Checklist

Before requesting user approval, validate Phase 2 using these criteria:

### Type Design
- [ ] All structs have documented fields with types
- [ ] Ownership is explicit (Arc, Box, owned, borrowed)
- [ ] Serialization strategy is defined (#[derive], #[serde(skip)])
- [ ] No `unwrap()` or `expect()` in signatures (return `Result` instead)

### Trait Implementations
- [ ] All trait implementations are listed
- [ ] Trait bounds are minimal and justified
- [ ] Generic parameters have clear purpose

### Match Statements
- [ ] Enum variants are fully documented
- [ ] No default match arms (`_ =>`) planned - all variants explicit

### Integration
- [ ] File paths are specified (which modules, which files)
- [ ] Dependencies are listed with versions
- [ ] Compatibility with existing crates verified

### Async/Sync
- [ ] Async decisions made with rationale
- [ ] AsyncStore pattern followed for stores (if applicable)

### Error Handling
- [ ] Uses `Error::typed_constructor()` (NOT `Error::new`)
- [ ] Error scenarios documented

### Relevant Commands
- [ ] New commands listed with full signatures
- [ ] Relevant existing namespaces identified
- [ ] User confirmed namespace selection

### Multi-Agent Review
- [ ] Reviewer A (Phase 1 conformity) launched and completed
- [ ] Reviewer B (Codebase alignment) launched and completed
- [ ] Sonnet fixer launched (if issues found) and completed
- [ ] All fixable issues resolved
- [ ] Remaining questions (if any) presented to user

### Approval Criteria
- [ ] All signatures compilable (at least in theory)
- [ ] High confidence in approach
- [ ] No major architectural unknowns remaining
- [ ] Relevant commands identified and confirmed by user
- [ ] Multi-agent review completed with no open issues
- [ ] User agrees with the design

**If any checklist items fail, revise Phase 2 before requesting approval.**

## Multi-Agent Review

After completing the Phase 2 document and running the inline review checklist, launch a **multi-agent review** before requesting user approval.

### Reviewer Agents (2 haiku, launched in parallel)

**Reviewer A — Phase 1 Conformity (haiku):**
- Skills: (none required)
- Knowledge: Phase 1 document, Phase 2 document
- Task: Check that Phase 2 architecture aligns with Phase 1 high-level design:
  - Scope hasn't drifted (no new features beyond Phase 1 scope)
  - All interactions identified in Phase 1 are addressed in Phase 2
  - No new unscoped features crept in
  - Feature purpose is preserved (not accidentally broadened or narrowed)

**Reviewer B — Codebase Alignment (haiku):**
- Skills: rust-best-practices
- Knowledge: Phase 2 document, integration point source files from codebase
- Task: Check Phase 2 against existing code at integration points:
  - Function signatures match existing code (parameter types, return types)
  - Trait bounds are compatible with existing trait definitions
  - Detect functionality that already exists under different names or with slightly different behavior
  - Identify missed reuse opportunities
  - Flag integration point inconsistencies

### Fixer Agent (1 sonnet, launched only if issues found)

**Sonnet Fixer:**
- Skills: rust-best-practices
- Knowledge: Phase 1 document, Phase 2 document, all reviewer outputs, relevant source files
- Task: Process all review findings and:
  1. Fix all fixable issues directly in the Phase 2 document
  2. Produce a summary with:
     - List of fixes made (what was changed and why)
     - List of remaining questions (genuine design decisions only)
  3. Ask user ONLY for decisions that can't be resolved from available context

### After Multi-Agent Review

- If no issues found: proceed directly to user approval gate
- If issues found and fixed: present fixed document + summary to user
- If unresolvable questions remain: ask user before requesting approval

## Next Steps

**STOP HERE.** Present Phase 2 to the user and WAIT for explicit approval.

The user must say "proceed" or "Proceed to next phase" before you start Phase 3. Any other response (feedback, questions, corrections, design changes, "looks good", "ok") is NOT approval — address the feedback and WAIT again.

After user says "proceed":
1. Start Phase 3: Examples & Use-cases
2. Use this Phase 2 architecture as the implementation blueprint in Phase 4
