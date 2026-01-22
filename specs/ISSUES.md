# Issues and Open Problems

This document tracks small issues, open problems, and enhancement ideas for the Liquers project.

## Issue Index

| # | ID | Status | Summary |
|---|-----|--------|---------|
| 1 | VOLATILE-METADATA | Open | State metadata lacks volatility information |
| 2 | METADATA-CONSISTENCY | Open | MetadataRecord fields need consistency validation |
| 3 | CANCEL-SAFETY | Open | Cancelled flag needed to prevent writes from orphaned tasks |
| 4 | NON-SERIALIZABLE | Open | Support for non-serializable data in set_state() |
| 5 | SOURCE-EVICTION | Open | Non-serializable Source assets lost permanently on eviction |
| 6 | UPLOAD-SIZE-LIMIT | Open | Configurable size limits for set() binary uploads |
| 7 | KEY-LEVEL-ACL | Open | Access control for set()/set_state() operations |

---

## Issue 1: VOLATILE-METADATA

**Status:** Open

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

**Status:** Open

**Summary:** A `cancelled` flag is needed on AssetData to prevent orphaned tasks from writing after cancellation.

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

## Issue 5: SOURCE-EVICTION

**Status:** Open

**Summary:** Non-serializable Source assets are lost permanently when evicted from memory.

### Problem

When `set_state()` is called without a recipe (creating a Source asset) and the data is non-serializable:
1. The State exists only in memory (AssetRef)
2. It cannot be persisted to store
3. When LRU eviction removes the AssetRef, the data is gone
4. Next `get()` fails - no store data to reload, no recipe to re-execute

Currently: `get()` returns an error indicating data was lost permanently.

This is a data loss scenario that users may not expect.

### Proposed Solutions

**Option A: Pin Source assets**
- Non-serializable Source assets are never evicted (pinned in memory)
- Risk: OOM if many large non-serializable Sources accumulate
- Benefit: No unexpected data loss

**Option B: Require serializable for Source**
- `set_state()` without recipe (Source) requires data to be serializable
- Returns error if attempting to set non-serializable data without recipe
- Rationale: If there's no recipe to regenerate, data MUST be persistable

**Option C: Explicit acknowledgment**
- Add parameter: `set_state(key, state, acknowledge_volatile: bool)`
- If non-serializable + no recipe + `acknowledge_volatile = false` → error
- Caller must explicitly acknowledge potential data loss

**Option D: Transient status**
- Add `Status::Transient` for non-serializable, non-recipe assets
- Clear semantics: "this data exists only in memory and may be lost"
- `get()` after eviction returns specific error about Transient asset

### Considerations

- Should there be monitoring/warnings when non-serializable Source assets are created?
- Should eviction trigger a warning/event before data is lost?
- Could there be a "serialize or lose" callback that attempts one last serialization before eviction?

### Affected Files

- `liquers-core/src/assets.rs` - Eviction logic, set_state() validation
- `liquers-core/src/metadata.rs` - Potentially new Status variant

### Related

- Issue 4 (NON-SERIALIZABLE) - parent issue for non-serializable support
- Memory management and eviction strategies

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
