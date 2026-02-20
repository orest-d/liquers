# METADATA-CONSISTENCY Proposed Plan

## Goals
1. Enforce predictable metadata semantics for `set()` and `set_state()`.
2. Prevent write-time acceptance of metadata that cannot be read back.
3. Enforce State-level metadata/value consistency at creation and mutation points.
4. Ensure both `type_identifier` and `type_name` are set correctly in metadata via State setters.
5. Keep migration risk controlled.

## Solution Options

## Option A: Strict Validation (Fail Fast)
Description:
- Add `MetadataRecord::validate_consistency(policy)`.
- Call it from `set()` and `set_state()` before persistence.
- Reject missing/inconsistent/unsupported metadata.

Pros:
- Strong correctness guarantees.
- Easy to reason about and test.

Cons:
- May break existing callers that rely on implicit defaults.

## Option B: Normalize + Warn
Description:
- Add `MetadataRecord::normalize()` and `validate_warnings()`.
- Fill missing fields (`data_format`, `media_type`) from filename/data defaults.
- Keep write successful; append warning log entries for mismatches.

Pros:
- Lowest compatibility impact.

Cons:
- Allows ambiguous metadata; problems can persist silently.

## Option C: Hybrid (Recommended)
Description:
- Phase 1: canonical read/write path + hard errors for mandatory fields.
- Phase 2: keep permissive normalize+warn behavior as the single mode.

Why recommended:
- Fixes concrete bug immediately.
- Minimizes regressions while moving toward strong guarantees.

## Recommended Implementation (Option C)

### Phase 1 (immediate)
1. Remove dead deserialization helper.
- `deserialize_from_binary()` has no call sites and should be removed to reduce misleading duplicate logic.
- File: `liquers-core/src/assets.rs`.

2. Add minimal hard validation on write paths (`set`, `set_state`) for non-error statuses.
- Require non-empty `type_identifier`.
- Require non-empty `type_name` (new metadata field).
- Keep `data_format`/`media_type` checks lightweight (consistency by derivation/sync only).
- Do NOT enforce value-type vs `data_format` compatibility in metadata validation.

3. Keep normalization only at State level.
- Implement normalization through State constructor/setter flow (no metadata-level normalize API).
- State-level normalization should infer/sync `media_type` from extension-derived format when appropriate.

4. Add warnings for soft mismatches.
- If filename extension and `data_format` differ, add warning log entry and do not fail.

5. Keep one deserialization source of truth.
- Preserve/strengthen deserialization in active store-load path (`try_fast_track`) which already uses `metadata.get_data_format()`.
- Add regression test ensuring load path respects `data_format` over filename extension assumptions.

6. Introduce State consistency hook and apply it broadly.
- Add a central method in `State`, e.g. `ensure_metadata_consistency(self) -> Result<Self, Error>` (and/or mutable variant).
- It should:
  - force `type_identifier` from `data.identifier()`,
  - force `type_name` from `data.type_name()` (new metadata field),
  - keep `data_format`/`media_type` consistent via filename/extension derivation rules,
  - avoid strict type-vs-format compatibility checks.
- Call it from:
  - `State::from_value_and_metadata(...)`,
  - `State::with_data(...)`,
  - `State::with_metadata(...)`,
  - key asset paths that assign state/metadata directly (notably in `assets.rs`).

### Phase 2 (strictness hardening)
1. Keep single permissive behavior (no policy enum/config).
- Normalize metadata where possible.
- Do not fail on soft mismatches (e.g. extension vs data_format).
- Emit warnings when mismatch is detected and context allows warning propagation.

2. Reduce bypasses of State invariants.
- Make direct `State { data, metadata }` construction rare by policy:
  - add lint/documentation rule for core code: prefer constructor/helpers.
  - keep `State` field visibility unchanged in this issue; non-public fields are a separate refactor.

3. `type_name` policy.
- Add `type_name` to metadata and keep it synchronized from `ValueInterface::type_name()` through State methods.
- `type_name` is informational only; deserialization behavior remains based on `type_identifier` and format implementation.

## API/Code Changes

1. `MetadataRecord`
- Add required `type_name: String` field and include it in serialization/asset-info projections as appropriate.
- Implement Metadata-level accessors following existing conventions:
  - `Metadata::type_name(&self) -> Result<String, Error>`
  - `Metadata::with_type_name(&mut self, type_name: String) -> &mut Self`
  - Support both `MetadataRecord` and `LegacyMetadata(serde_json::Value::Object)` paths similarly to existing `type_identifier` handling.

2. `assets.rs`
- Apply mandatory-field checks in `set()` and `set_state()` before final persistence.
- Remove unused `deserialize_from_binary()`.
- Refactor manual `State { ... }` constructions to use State consistency helpers where practical.
- When inconsistencies are detected in asset context (`AssetData`/`AssetRef` operations), append warnings to metadata log (instead of failing for soft mismatches).

3. `state.rs`
- Add consistency API and wire it into constructors/mutators:
  - `from_value_and_metadata`
  - `with_data`
  - `with_metadata`
- Keep fields public in this issue scope; private fields + safe builders are tracked as separate refactor.
- Ensure consistency logic computes both metadata types from value:
  - `type_identifier <- identifier()`
  - `type_name <- type_name()`
- Keep normalization implementation at State level only (single source of truth for metadata reconciliation).

4. `metadata.rs` setter sync hardening
- Update `set_filename()` / `set_extension()` to keep `media_type` synchronized with extension-derived mapping.
- Keep existing `get_data_format()` fallback behavior.

## Test Plan

1. Unit tests in `liquers-core/src/metadata.rs`:
- missing `type_identifier` -> error (hard check)
- missing `type_name` -> error (hard check once field is added)
- empty `media_type` inferred from `data_format`
- extension/data_format mismatch -> warning in permissive mode

2. Unit/integration tests in `liquers-core/src/assets.rs`:
- `set()` rejects empty `type_identifier`
- `set()` rejects empty `type_name`
- `set_state()` enforces identifier+type_name synchronization through State-level consistency
- deserialization uses `data_format` when filename extension differs
- soft mismatches produce warning log entries when asset context is available

3. Unit tests in `liquers-core/src/state.rs`:
- `with_data` updates/normalizes metadata consistency.
- `from_value_and_metadata` reconciles mismatched input metadata.
- setting new metadata preserves value-consistent `type_identifier`.
- setting new metadata preserves value-consistent `type_name`.
- `type_name` remains informational only for deserialization behavior.

4. Regression tests:
- existing `set`/`set_state` tests continue passing after adding required metadata in fixtures.

5. Pattern conformance checks (core):
- Replace high-value manual constructions in `assets.rs`/`interpreter.rs` with consistent helper path where possible.

## Migration Notes

1. Migration path:
- Use permissive mode with hard checks only for mandatory fields (`type_identifier`, `type_name`).
- Keep soft mismatches as warnings only.

## Open Questions (need your decision)

1. Should `media_type` inconsistency be auto-corrected, or should we preserve caller-provided `media_type` and only warn?
2. Decision: filename-extension mismatch with `data_format` is warning-only.
3. Decision: `State` field visibility changes are out of scope here and handled in a separate refactor.
4. Decision: `type_name` field is required `String`.
