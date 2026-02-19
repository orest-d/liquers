# Issues and Open Problems

This document tracks small issues, open problems, and enhancement ideas for the Liquers project.

## Issue Index

| #  | ID                            | Status       | Summary |
|----|-------------------------------|--------------|---------|
| 1  | VOLATILE-METADATA             | **Closed**   | State metadata lacks volatility information |
| 2  | METADATA-CONSISTENCY          | Open         | MetadataRecord fields need consistency validation |
| 3  | CANCEL-SAFETY                 | **Closed**   | Cancelled flag needed to prevent writes from orphaned tasks |
| 4  | NON-SERIALIZABLE              | Open         | Support for non-serializable data in set_state() |
| 5  | STICKY-ASSETS                 | Open         | Source/Override assets need eviction resistance for reliable storage |
| 6  | UPLOAD-SIZE-LIMIT             | Open         | Configurable size limits for set() binary uploads |
| 7  | KEY-LEVEL-ACL                 | Open         | Access control for set()/set_state() operations |
| 8  | VALUE-LIST-SUPPORT            | **Closed**   | ValueInterface may need extension for returning lists of integers from lui commands |
| 9  | IMAGE-DIMENSIONS-METADATA     | Open         | Store image dimensions in State metadata for efficient queries |
| 10 | ENUM-ARGUMENT-TYPE            | **Closed**   | Support EnumArgumentType in register_command! macro |
| 11 | PAYLOAD-INJECTION             | **WONT_FIX** | Payload field extraction syntax in register_command! macro |
| 12 | PAYLOAD-INHERITANCE           | **WONT_FIX** | Payload inheritance in nested evaluations |
| 13 | CONTEXT-PARAM-ORDER           | **Closed**   | Parameter index misalignment with injected parameters in register_command! |
| 14 | KEYBOARD-SHORTCUT-ABSTRACTION | Open         | Platform-agnostic keyboard shortcut system for multiple UI backends |
| 15 | PRESET-NAMESPACE              | **Closed**   | CommandPreset missing namespace field |

---

## Issue 1: VOLATILE-METADATA

**Status:** **Closed**

**Summary:** State metadata does not indicate if the State originates from a volatile asset.

### Problem

Currently, volatility is computed dynamically via the `IsVolatile<E>` trait by inspecting:
- `CommandMetadata.volatile` flag
- `Recipe.volatile` flag
- Query/Plan structure

However, `MetadataRecord` (which is stored in `State<V>`) contains no volatility information. This means:
1. Consumers of a `State` cannot determine if it came from a volatile source without access to the original recipe/command metadata
2. Caching decisions require re-computation of volatility from the query

### Proposed Solutions

**Option A: Add `Volatile` status**
- Extend the `Status` enum to include a `Volatile` variant
- Semantically equivalent to `Ready`, but indicates the value expires immediately after use
- Status progression: `Volatile` behaves like `Ready` but signals "do not cache"

**Option B: Add `volatile` flag to `MetadataRecord`**
- Add `pub volatile: bool` field to `MetadataRecord`
- Set during State construction when the source is known to be volatile
- More explicit than status, allows `Ready` + `volatile: true` combination

**Option C: Both**
- Use `Volatile` status for assets that are inherently volatile
- Use `volatile` flag for metadata propagation and caching hints
- Provides flexibility for different use cases

### Considerations

- The `Status` enum currently has: `Unknown`, `None`, `Ready`, `Stale`, `Scheduled`, `Processing`, `Error`, `Expired`, `External`, `Recipe`
- Adding `Volatile` status fits the pattern of describing asset lifecycle states
- A metadata flag provides explicit control independent of status
- Need to decide if volatility should propagate through transformations (if State A is volatile and transformed to State B, is B also volatile?)

### Affected Files

- `liquers-core/src/metadata.rs` - `Status` enum and `MetadataRecord` struct
- `liquers-core/src/state.rs` - State construction and metadata handling
- `liquers-core/src/interpreter.rs` - Volatility computation and propagation
- `liquers-core/src/assets.rs` - Asset caching decisions

### Related

- `IsVolatile<E>` trait in `interpreter.rs`
- `CommandMetadata.volatile` in `command_metadata.rs`
- `Recipe.volatile` in `recipes.rs`

---

## Issue 2: METADATA-CONSISTENCY

**Status:** Open

**Summary:** MetadataRecord fields (`data_format`, `type_identifier`, `media_type`) need consistency validation.

### Problem

When using `set()` to store binary data with metadata, the system relies on metadata fields for later deserialization:
- `data_format` - determines how to deserialize binary back to Value
- `type_identifier` - determines what Value type to deserialize into
- `media_type` - HTTP content type, should be consistent with data_format

Currently there is no validation that these fields are:
1. Present (non-empty) when required
2. Consistent with each other
3. Valid/recognized values

### Scenarios Requiring Validation

**set() operation:**
- `data_format` must be present (mandatory for deserialization)
- `type_identifier` must be present (mandatory for deserialization)
- These should be consistent (e.g., `data_format: "json"` should match appropriate type_identifiers)

**Consistency rules to consider:**
- `data_format: "json"` → `media_type` should be `application/json`
- `data_format: "csv"` → `type_identifier` should be a table/dataframe type
- `data_format: "bin"` → generic binary, `type_identifier` could be `bytes`
- `data_format: "png"` → `type_identifier` should be an image type

### Proposed Solutions

**Option A: Validation function**
- Add `MetadataRecord::validate() -> Result<(), Error>` method
- Called by `set()` before storing
- Returns specific errors for missing/inconsistent fields

**Option B: Builder pattern with enforcement**
- Create `MetadataRecordBuilder` that enforces required fields
- `set()` accepts only validated metadata (via newtype wrapper)

**Option C: Auto-inference with validation**
- If `media_type` is missing, infer from `data_format`
- If `type_identifier` is missing, infer from `data_format` (with default)
- Validate consistency after inference

### Questions to Resolve

1. Should `set()` accept `MetadataRecord` only (not `Metadata` enum) to ensure structure?
2. What is the canonical list of valid `data_format` values?
3. Should there be a registry mapping `data_format` ↔ `type_identifier` ↔ `media_type`?
4. How strict should validation be? Warn vs. error for inconsistencies?

### Affected Files

- `liquers-core/src/metadata.rs` - MetadataRecord validation
- `liquers-core/src/assets.rs` - set() operation validation
- Potentially `liquers-core/src/value.rs` - type_identifier registry

### Related

- Issue 1 (VOLATILE-METADATA) - also concerns MetadataRecord fields
- Asset set operations in ASSET_SET_OPERATION_CHANGES.md

---

## Issue 3: CANCEL-SAFETY

**Status:** Closed

**Summary:** A `cancelled` flag is needed on AssetData to prevent orphaned tasks from writing after cancellation.

### Resolution

Implemented cancel-safety checks in `liquers-core/src/assets.rs`:

1. **`evaluate_and_store()` method**: Added check for `is_cancelled()` flag before calling `save_to_store()`. If cancelled, the store write is silently skipped and the method returns successfully.

2. **`save_to_store()` method**: Added two cancelled checks:
   - At the start of the method before any work begins
   - After serialization (which can be slow), before the actual store write

   This double-check pattern ensures that cancellation requests that arrive during serialization are still honored.

3. **API Endpoint**: Added `POST /api/assets/cancel/{*query}` endpoint specification to `specs/WEB_API_SPECIFICATION.md`. The endpoint:
   - Initiates cancellation for assets in cancellable states (Submitted, Dependencies, Processing)
   - Returns success with `cancelled: true` or `cancelled: false` depending on asset state
   - Returns 404 if asset not found

The existing infrastructure (`cancelled` flag, `set_cancelled()`, `is_cancelled()`, `cancel_evaluation()`) was already in place. This fix adds the missing write-prevention checks that were specified in the original issue.

### Problem

Commands can be long-running and non-cooperative (e.g., ML training in Python running in blocking mode). When a cancellation is requested:

1. Normal flow: cancellation signal received → command checks signal → stops before `ValueProduced` → no store write
2. Problem flow: command is blocking and doesn't check cancellation → eventually produces value → attempts to write to store

If the cancellation was triggered by `set()` or `set_state()`, the orphaned task's write would overwrite the freshly set data, causing inconsistency.

### Proposed Solution

Add a `cancelled: bool` flag to `AssetData`:

```rust
pub struct AssetData<E: Environment> {
    // ... existing fields ...

    /// If true, this asset has been cancelled and should not write results.
    /// Any ValueProduced or store write attempts should be silently dropped.
    cancelled: bool,
}
```

**Cancellation flow:**
1. Set `cancelled = true` on AssetData
2. Send cancellation signal via service channel
3. Remove AssetRef from AssetManager immediately
4. Proceed with set()/set_state() operation
5. Orphaned task eventually completes:
   - Checks `cancelled` flag before writing
   - If `cancelled == true`, silently drops result
   - Resources freed when task ends

**Write prevention points:**
- `ValueProduced` handler must check `cancelled` flag
- Store write operations must check `cancelled` flag
- Status updates must check `cancelled` flag

### API Endpoint

Add `/api/assets/cancel` endpoint to WEB_API_SPECIFICATIONS:

```
POST /api/assets/{key}/cancel
```

Response:
- 200 OK - cancellation initiated
- 404 Not Found - asset not found
- 409 Conflict - asset not in cancellable state

### Affected Files

- `liquers-core/src/assets.rs` - AssetData.cancelled flag, cancellation logic
- `liquers-axum/` - Cancel endpoint
- `specs/WEB_API_SPECIFICATIONS.md` - Document cancel endpoint

### Considerations

- Should cancelled assets be logged/tracked for monitoring?
- Timeout for cancellation before considering task "stuck"?
- Should there be a way to list cancelled/orphaned tasks?

---

## Issue 4: NON-SERIALIZABLE

**Status:** Open

**Summary:** `set_state()` must support non-serializable data that cannot be persisted to store.

### Problem

Some Value types cannot be serialized:
- Live database connections
- GPU tensors / CUDA memory
- File handles
- Python objects with native resources
- Callback functions / closures

For these values:
- `set_state(key, state)` should work (keeps State in memory via AssetRef)
- Serialization to store should be skipped or fail gracefully
- Retrieval must come from memory (AssetRef), not store

### Current Behavior

`set_state()` is specified to:
1. Create new AssetRef with State in memory
2. Serialize and store to persistent store

Step 2 will fail for non-serializable data.

### Proposed Solutions

**Option A: Try-serialize approach**
- Attempt serialization; if it fails, store metadata only (no binary)
- Mark in metadata that binary is not available (`binary_available: false`)
- Asset only retrievable while AssetRef exists in memory

**Option B: Explicit flag**
- Add parameter: `set_state(key, state, persist: bool)`
- If `persist = false`, skip serialization entirely
- Or: check `type_identifier` against known non-serializable types

**Option C: Metadata-driven**
- Add `serializable: bool` field to MetadataRecord
- `set_state()` checks this before attempting serialization
- Types self-declare serializability

### Considerations

- What happens when AssetRef is evicted from memory but asset is non-serializable?
  - Return error on next get()?
  - Keep non-serializable AssetRefs pinned in memory?
- Should non-serializable assets have a different Status? (e.g., `Transient`)
- How does this interact with volatility?

### Affected Files

- `liquers-core/src/assets.rs` - set_state() serialization logic
- `liquers-core/src/metadata.rs` - potential new fields
- `liquers-core/src/value.rs` - serializability trait/check

### Related

- Issue 1 (VOLATILE-METADATA) - transient/volatile concepts overlap
- set_state() specification in ASSET_SET_OPERATION_CHANGES.md

---

## Issue 5: STICKY-ASSETS

**Status:** Open

**Summary:** Source and Override status assets need eviction resistance to prevent data loss and enable reliable AppState storage.

### Problem

**Core issue:** AssetManager's LRU eviction can remove Source and Override assets from memory, causing permanent data loss.

**Scenario 1: Non-serializable Source assets**
When `set_state()` creates a Source asset with non-serializable data:
1. State exists only in memory (AssetRef)
2. Cannot be persisted to store (e.g., database connections, GPU tensors)
3. LRU eviction removes the AssetRef
4. Next `get()` fails - no store data to reload, no recipe to re-execute
5. Data lost permanently

**Scenario 2: UI AppState storage** (see UI_INTERFACE_FSD.md)
The UI interface design uses AssetManager's hierarchical structure for application state:
- Each UIElement is a separate Source/Override asset
- Handle = asset key path (e.g., `/ui/elements/window1/left_pane`)
- Eviction of UIElement asset breaks UI state
- Benefits (concurrency, transparency) require assets to stay in memory

**Common cause:** Source assets have no recipe to regenerate. Override assets represent user modifications. Both are non-derivable - losing them = data loss.

### Proposed Solution

Make `Source` and `Override` status assets eviction-resistant by default:

```rust
impl AssetManager {
    fn is_evictable(&self, asset: &AssetData) -> bool {
        match asset.status {
            Status::Source | Status::Override => false,  // Never evict
            Status::Ready | Status::Stale => true,       // Normal eviction
            Status::Processing | Status::Scheduled => false,  // Active work
            Status::Error | Status::Expired => true,     // Can evict
            _ => true,
        }
    }
}
```

**Rationale:**
- `Source` assets: No recipe to regenerate, user-provided data
- `Override` assets: User modifications that override computed values
- Both represent mutable, non-derivable state
- `Ready`/`Stale` assets are derivable from recipes, safe to evict

Assets can still be removed explicitly via `remove()` or `clear()` operations.

### Alternative Approaches

**Option A: Explicit sticky flag**
```rust
pub struct AssetData<E: Environment> {
    sticky: bool,  // If true, resist LRU eviction
}
```
More flexible but requires callers to remember to set it.

**Option B: Require serializable for Source**
- `set_state()` without recipe requires data to be serializable
- Returns error for non-serializable data without recipe
- Too restrictive - blocks valid use cases (UI state, live connections)

**Option C: Transient status**
- Add `Status::Transient` for non-serializable, non-recipe assets
- Clear semantics but doesn't prevent eviction issue

### Benefits

**For non-serializable data:**
- No unexpected data loss from eviction
- Reliable in-memory state management
- Supports live resources (connections, handles)

**For UI AppState:**
- UIElement assets persist reliably in memory
- Multi-threaded UI can access different elements concurrently
- UI state visible in asset inspection tools
- Asset cache provides optional persistence

### Considerations

**Memory pressure:** What happens when sticky assets consume all available memory?
- Option A: Allow eviction when critically low memory (with warning)
- Option B: Return error when creating new sticky asset if memory full
- Option C: Configurable max sticky asset count/size

**Monitoring:**
- Should sticky asset count/size be tracked and exposed?
- Warning when non-serializable Source assets are created?
- Event/log before attempting eviction of sticky asset?

**Scope:**
- Should both Source and Override be sticky, or only Source?
  - Source = user-provided data (definitely sticky)
  - Override = user override of computed value (also sticky for UX)

### Affected Files

- `liquers-core/src/assets.rs` - AssetManager eviction logic, `is_evictable()` method
- `liquers-lib/src/ui/asset_provider.rs` - UIElement asset creation
- Asset lifecycle documentation in specs/ASSETS.md

### Related

- Issue 4 (NON-SERIALIZABLE) - Support for non-serializable data in set_state()
- UI_INTERFACE_FSD.md - AppState design using asset hierarchy
- specs/ASSETS.md - Asset status and lifecycle documentation

---

## Issue 6: UPLOAD-SIZE-LIMIT

**Status:** Open

**Summary:** Need configurable size limits for `set()` binary uploads to prevent memory/performance issues.

### Problem

`set()` accepts arbitrary binary data. Without limits:
- Large uploads could exhaust server memory
- Could be used for DoS attacks
- May exceed store backend limits

### Proposed Solution

Add configurable `max_binary_size` setting:
- Default: reasonable value (e.g., 100MB or 1GB)
- Configurable per-environment or per-store
- `set()` checks size before processing; rejects with error if exceeded

### Considerations

- Should limit apply to total size or per-request?
- Should different limits apply to different key patterns?
- For very large files, streaming upload may be needed (future feature)
- How does this interact with store backend limits?

### API Impact

- `set()` returns new error type: `BinaryTooLarge { size: usize, limit: usize }`
- HTTP API returns 413 Payload Too Large

### Affected Files

- `liquers-core/src/assets.rs` - Size check in set()
- `liquers-core/src/error.rs` - New error variant
- Configuration system - New setting
- `liquers-axum/` - HTTP 413 response

---

## Issue 7: KEY-LEVEL-ACL

**Status:** Open

**Summary:** Access control needed for `set()` and `set_state()` operations to restrict who can modify which keys.

### Problem

Currently `set()` and `set_state()` have no access control. Any caller can set any key. This is problematic for:
- Multi-tenant environments
- Production systems with sensitive data
- Preventing accidental overwrites
- Audit and compliance requirements

### Requirements

- Control which principals (users, services) can write to which keys
- Key pattern matching (e.g., `/user/*/private/*` restricted to owner)
- Integration with existing authentication mechanisms
- Read vs write permissions may differ

### Proposed Solutions

**Option A: Key pattern ACL**
- Configuration maps key patterns to allowed principals
- Checked before `set()`/`set_state()` proceeds
- Example: `{ pattern: "/admin/**", write: ["admin-service"] }`

**Option B: Store-level permissions**
- Each store has its own ACL configuration
- Simpler but less granular

**Option C: Policy engine integration**
- Integrate with external policy engine (OPA, Cedar)
- Maximum flexibility but adds dependency

### Considerations

- How are principals identified? (tokens, certificates, headers)
- Should ACL be checked synchronously or asynchronously?
- Caching of ACL decisions for performance
- Audit logging of access decisions
- Default policy: allow-all vs deny-all

### Affected Files

- `liquers-core/src/assets.rs` - ACL check in set()/set_state()
- New ACL module in `liquers-core/` or `liquers-lib/`
- Configuration system - ACL configuration
- `liquers-axum/` - Principal extraction from requests

### Related

- Authentication/authorization system design (broader scope)

---

## Issue 8: VALUE-LIST-SUPPORT

**Status:** **Closed**

**Summary:** ValueInterface may need extension to support returning lists of integers from lui navigation commands.

### Problem

The `lui` namespace commands `children` and `roots` need to return lists of UIHandle values (as integers). The `Value` type supports lists (`Value::List`), but `ValueInterface` may not provide convenient methods for constructing lists of integers or extracting them.

### Context

Navigation commands in the `lui` namespace (Phase 1 UI Interface) return:
- Single handles as `i64` (e.g., `parent`, `next`, `prev`)
- Lists of handles as lists of `i64` (e.g., `children`, `roots`)

These return values may be consumed by embedded queries, so they need to flow cleanly through the value system.

### Investigation Needed

1. Can `Value::from(vec![1i64, 2, 3])` produce a `Value::List`?
2. Does `ValueInterface` have `from_list` / `try_into_list` methods?
3. If not, what methods need to be added?
4. Does `ExtValue` (in liquers-lib) need corresponding extensions?

### Affected Files

- `liquers-core/src/value.rs` - ValueInterface trait, Value enum
- `liquers-lib/src/value/mod.rs` - ExtValue extensions

### Related

- UI Interface Phase 1 FSD - lui namespace commands

---

## Issue 9: IMAGE-DIMENSIONS-METADATA

**Status:** Open

**Summary:** Store image dimensions (width, height) in State metadata for efficient queries without deserializing image data.

### Problem

Currently, retrieving image dimensions requires loading and deserializing the full image. For large images or metadata-only queries, this is inefficient. Dimensions should be stored in `State.metadata` alongside the image data.

### Proposed Solutions

**Option A: Type-specific fields**
- Add `metadata.image_width` and `metadata.image_height` fields to `Metadata` struct
- Explicit, but requires schema changes for each value type

**Option B: Generic properties map**
- Use `metadata.properties["width"]` and `metadata.properties["height"]`
- Flexible, but less type-safe

**Option C: Dedicated metadata struct**
- Add `metadata.image_info` struct with width/height/color_type/format fields
- Clean separation, extensible for other image properties

### Considerations

- **Population timing**: During image load commands and transformations that change dimensions
- **Consistency**: How to ensure metadata stays in sync with actual image after transformations
- **Backward compatibility**: Handle existing assets without metadata (compute on-demand and cache)
- **Other metadata**: Color type, file format, EXIF data, compression settings
- **Generalization**: Similar pattern could apply to Polars DataFrames (row/column count, schema)
- **Lazy vs eager**: Compute dimensions immediately or on first request

### Affected Files

- `liquers-core/src/metadata.rs` - Metadata structure
- `liquers-lib/src/commands.rs` - Image command library
- `specs/IMAGE_COMMAND_LIBRARY.md` - Image command design

### Related

- Issue 2 (METADATA-CONSISTENCY) - Metadata field validation
- Polars DataFrames may need similar metadata enhancement

---

## Issue 10: ENUM-ARGUMENT-TYPE

**Status:** **Closed**

**Summary:** Add enum-style arguments to register_command! macro for type-safe parameter validation.

### Problem

Many commands need parameters that accept one of a predefined set of string values (e.g., resize method: "nearest", "lanczos3", etc.). Currently, these are received as `String` and manually validated in each command, which is repetitive and doesn't provide command metadata about valid values.

**Use cases:**
- Image resize methods: `resize-800-600-lanczos3` (5 valid values)
- Color formats: `color_format-rgba8` (8 valid values)
- Rotation methods: `rotate-45-bilinear` (2 valid values)
- Blur methods: `blur-gaussian-2.5` (3+ valid values)

### Proposed Solution

Add inline enum definition syntax to register_command! macro:

```rust
register_command!(cr,
    fn resize(state, width: u32, height: u32,
              method: enum("nearest", "triangle", "catmullrom", "gaussian", "lanczos3") = "lanczos3"
                     (label: "Interpolation Method")) -> result
    label: "Resize Image"
    namespace: "img"
)?;
```

**Macro generates:**
1. Validation code that checks value against enum list
2. `ArgumentType::Enum { values: vec![...] }` in metadata
3. Auto-selected GUI based on value count (2-3 values: radio buttons, 4+ values: dropdown)
4. Clear error messages listing all valid values

### Considerations

- **String validation**: Case-sensitive matching recommended for consistency
- **GUI auto-selection**: 2-3 options → VerticalRadioEnum, 4+ options → EnumSelector
- **Error messages**: Include parameter name and list of valid values
- **Default values**: Support defaults like other parameter types
- **Metadata storage**: Expose enum values via command introspection for help/docs
- **Backward compatibility**: Existing String parameters continue to work

### Affected Files

- `liquers-core/src/command_metadata.rs` - Add `ArgumentType::Enum` variant
- `liquers-macro/src/lib.rs` - Parse enum syntax and generate validation code
- `specs/REGISTER_COMMAND_FSD.md` - Document enum syntax
- `specs/COMMAND_REGISTRATION_GUIDE.md` - Add enum examples

### Related

- `specs/IMAGE_COMMAND_LIBRARY.md` - Primary use case for enum arguments
- Issue 10 blocks implementation of image command library

---

## Issue 11: PAYLOAD-INJECTION

**Status:** **WONT_FIX**

**Summary:** Add field extraction syntax to register_command! macro to simplify payload injection without manual trait implementations.

**WONT_FIX**: It is sufficient to access the payload via context.

### Problem

Currently, using `injected` parameters with payload types or newtypes requires manually implementing `InjectedFromContext` for each type. This is verbose and error-prone due to Rust's trait coherence rules.

**Current workaround:**
```rust
// 1. Define payload type
#[derive(Clone)]
pub struct MyPayload {
    pub user_id: String,
    pub window_id: u64,
}

impl PayloadType for MyPayload {}

// 2. Manually implement InjectedFromContext (required)
impl<E: Environment<Payload = MyPayload>> InjectedFromContext<E> for MyPayload {
    fn from_context(name: &str, context: Context<E>) -> Result<Self, Error> {
        context.get_payload_clone().ok_or(Error::general_error(format!(
            "No payload in context for injected parameter {}", name
        )))
    }
}

// 3. For newtypes, also implement InjectedFromContext
pub struct UserId(pub String);
impl ExtractFromPayload<MyPayload> for UserId { /* ... */ }
impl InjectedFromContext<MyEnvironment> for UserId { /* ... */ }
```

### Proposed Solution

Add field extraction syntax to register_command! macro:

```rust
register_command!(cr, fn my_cmd(
    state,
    user_id: String injected from payload.user_id,
    window_id: u64 injected from payload.window_id
) -> result)?;
```

This eliminates the need for newtypes and manual `InjectedFromContext` implementations. The macro generates wrapper code to extract fields from the payload.

### Benefits

1. **Less boilerplate**: No manual trait implementations needed
2. **Type safety**: Compile-time field access validation
3. **Clearer intent**: Syntax explicitly shows field extraction
4. **Backward compatible**: Existing `injected` keyword still works for full payload

### Affected Files

- `liquers-macro/src/lib.rs` - Parse `injected from payload.field` syntax
- `specs/REGISTER_COMMAND_FSD.md` - Document field extraction syntax
- `specs/PAYLOAD_GUIDE.md` - Update user documentation

### Related

- `liquers-core/tests/injection.rs` - Test examples showing manual implementation

---

## Issue 12: PAYLOAD-INHERITANCE

**Status:** **WONT_FIX**


**Summary:** Payload is not automatically passed to nested queries executed via context.evaluate().
Option C is fine - or maybe Option A can be implemented when needed.

### Problem

When a command calls `context.evaluate()` to execute a nested query, the payload from the parent context is not automatically passed to the child query. Nested queries cannot access injected parameters.

**Example:**
```rust
async fn parent_cmd(
    _state: State<Value>,
    user_id: UserId,  // Has access to payload
    context: Context<E>,
) -> Result<Value, Error> {
    // Nested query - will NOT have access to payload
    let child = context.evaluate(&parse_query("/-/child_cmd")?).await?;
    // child_cmd cannot use injected parameters!
}
```

### Why This Happens

`context.evaluate()` goes through the standard asset creation pipeline, which doesn't have access to the parent command's payload because:
1. Assets are shared across multiple users/contexts
2. Asset manager is designed to work without execution-specific context
3. Caching would be impossible if assets depended on ephemeral payload data

### Proposed Solutions

**Option A: Add context.evaluate_with_payload()**
```rust
let child = context.evaluate_with_payload(
    &parse_query("/-/child_cmd")?,
    context.get_payload_clone()
).await?;
```

**Option B: Store payload in Context and thread through asset creation** (more invasive)

**Option C: Document as intentional limitation** (recommended)
- Encourage passing data through query parameters or state instead
- Simpler, avoids complexity with caching and asset sharing

### Recommended Approach

Option C (document as limitation). Payload inheritance is conceptually problematic for caching and asset sharing. Users should pass data through query parameters or state transformation.

### Affected Files

- `liquers-core/src/context.rs` - Context implementation
- `liquers-core/src/assets.rs` - AssetManager
- `liquers-core/tests/injection.rs` - Test documenting limitation

---

## Issue 13: CONTEXT-PARAM-ORDER

**Status:** **Closed** (workaround available)

**Summary:** register_command! macro has parameter index misalignment when Context is not the last parameter.

### Problem

`extract_all_parameters()` (line ~557 in liquers-macro) uses `enumerate()` over all parameters including `Context`. `command_arguments_expression()` (line ~774) uses `filter_map` to exclude `Context` from metadata. When `Context` is not the last parameter, the extractor index doesn't match the metadata/values index.

**Example:**
```rust
// BROKEN: context is not last
fn remove(state, context, target_word: String)
// Generates: arguments.get(1, "target_word")
// But metadata has target_word at index 0
```

### Fix

Use a separate counter for non-Context parameters in `extract_all_parameters()`:

```rust
let mut arg_index = 0;
for p in &self.parameters {
    extractors.push(p.parameter_extractor(arg_index));
    if !matches!(p, CommandParameter::Context) {
        arg_index += 1;
    }
}
```

### Workaround

Always place `context` last in the macro DSL:

```rust
// CORRECT: context last
register_command!(cr,
    async fn remove(state, target_word: String, context) -> result
)?;
```

### Affected Files

- `liquers-macro/src/lib.rs` - Parameter extraction logic

### Related

- Documented in MEMORY.md as known workaround
- `specs/REGISTER_COMMAND_FSD.md` - Macro syntax specification

---

## Issue 14: KEYBOARD-SHORTCUT-ABSTRACTION

**Status:** Open

**Summary:** Create platform-agnostic keyboard shortcut system to support multiple UI backends (egui, ratatui, dioxus).

### Problem

UISpecElement's keyboard shortcut parsing is tightly coupled to egui's API (`egui::Key`, `egui::KeyboardShortcut`, `egui::Modifiers`). This creates portability issues when supporting multiple UI backends.

**Current implementation** (egui-specific):
```rust
fn check_shortcut(&self, ui: &egui::Ui, shortcut_str: &str) -> bool {
    // Parses "Ctrl+S" using egui types
    let key_opt = egui::Key::from_name(part);
    let shortcut = egui::KeyboardShortcut::new(modifiers, key);
    ui.input_mut(|i| i.consume_shortcut(&shortcut))
}
```

**Problems:**
1. Backend coupling - cannot swap UI backends without rewriting shortcut logic
2. Duplicate code risk - each backend would need its own parser
3. Testing difficulty - cannot test shortcuts without egui context

### Proposed Solution

Create `liquers-shortcuts` crate with platform-agnostic abstraction:

```rust
// Core abstraction (platform-agnostic)
pub struct KeyboardShortcut {
    modifiers: Modifiers,
    key: Key,
}

impl KeyboardShortcut {
    pub fn parse(s: &str) -> Result<Self, Error>;
}

// Feature-gated backend support
#[cfg(feature = "egui")]
pub mod egui_backend {
    pub fn check_shortcut(ui: &egui::Ui, shortcut: &KeyboardShortcut) -> bool;
}

#[cfg(feature = "ratatui")]
pub mod ratatui_backend { /* ... */ }

#[cfg(feature = "dioxus")]
pub mod dioxus_backend { /* ... */ }
```

### Benefits

1. **Portability**: UISpecElement works with any UI backend
2. **Testability**: Shortcuts testable without UI framework
3. **Consistency**: Same shortcut format across all backends
4. **Maintainability**: Centralized shortcut logic

### Considerations

- **String format standard**: Use unified format ("Ctrl+S") across all backends
- **Platform differences**: Handle macOS Command vs Ctrl gracefully
- **Key name mapping**: Some keys may not exist on all platforms
- **Performance**: Cache parsed shortcuts

### Implementation Priority

- **Phase 1** (current): egui-specific implementation (get feature working)
- **Phase 2** (future): Abstract when adding ratatui support
- **Phase 3** (future): Support dioxus/other backends

### Affected Files

- `liquers-lib/src/ui/widgets/ui_spec_element.rs` - Current egui-specific implementation
- New `liquers-shortcuts/` crate (future)

### Related

- `specs/UI_RATATUI_DESIGN_NOTES.md` - Future ratatui support
- `specs/UI_WEB_DESIGN_NOTES.md` - Future web support (dioxus)

---

## Issue 15: PRESET-NAMESPACE

**Status:** **Closed**

**Summary:** CommandPreset lacks namespace field, causing wrong command resolution when preset namespace differs from query's active namespace.

### Problem

`CommandPreset` contains an `ActionRequest` (command name + parameters) but has no namespace field. When a preset is appended to a query, the namespace context is inherited from the preceding query's last `ns` action. If the preset's intended namespace differs, the wrong command may be resolved.

**Example:**
```rust
// Query: -R-bin/data/data.csv/-/ns-pl/from_csv
// Active namespace: pl (Polars)
// Command's next preset: CommandPreset { action: "add-child", label: "Show" }
// "add" is in lui namespace, not pl (enabled through ns-pl), nor in default root namespace.
// Appending: -R-bin/data/data.csv/-/ns-pl/from_csv/add-child
// PlanBuilder resolves "add" in [polars, "", root] — may find wrong command or fail to find it
// Desired behaviour:
// Append also namespace: -R-bin/data/data.csv/-/ns-pl/from_csv/ns-lui/add-child
```

### Root Cause

`CommandPreset` (line 652 in command_metadata.rs) stores only `ActionRequest`:

```rust
pub struct CommandPreset {
    pub action: ActionRequest,  // name + parameters, no namespace
    pub label: String,
    pub description: String,
}
```

### Proposed Fix

Add optional namespace field to `CommandPreset`:

```rust
pub struct CommandPreset {
    pub action: ActionRequest,
    /// Namespace for the action. If Some, preset should be preceded by `ns-<namespace>/`
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub ns: String,
    pub label: String,
    pub description: String,
}
```

In plan module make a general purpose method to append action (ActionRequest) to a query:
```rust
pub fn append_action(query:&Query, ns:&str, action:ActionRequest, cmr:&CommandMetadataRegistry) -> Result<Query, Error>
```
It should follow the logic:
1) try to append the action without the namespace (ns)
2) Using the same logic as in the PlanBuilder try to resolve the action and verify that it yields the same namespace. (Note that "" and "root" are considered the same namespace.) CommandKey can be used for the comparison.
3) If the action resolves to the same namespace, it is kept.
4) Otherwise prepend `ns-<namespace>/` before the action and add this to the query.

### Impact

- `CommandPreset::new()` unchanged (ns defaults to None for backward compatibility)
- `register_command!` macro's `next:` and `preset:` DSL may need syntax for specifying namespace
- `find_next_presets()` utility function should handle ns injection
- Serialization: backward compatible (skip_serializing_if + default)

### Workaround

Include `ns` action as part of preset string (fragile, may be lost during parsing).

### Affected Files

- `liquers-core/src/command_metadata.rs` - CommandPreset, CommandMetadata.next
- `liquers-core/src/plan.rs` - PlanBuilder namespace resolution
- `liquers-core/src/query.rs` - ActionRequest, Query.last_ns()

### Related

- `specs/query-console-element/phase2-architecture.md` - find_next_presets() design
