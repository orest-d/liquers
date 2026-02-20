# METADATA-CONSISTENCY Findings

## Scope
Investigation of Issue `METADATA-CONSISTENCY` in `specs/ISSUES.md` with focus on `liquers-core` write/read paths for metadata fields:
- `type_identifier`
- `type_name` (runtime detailed type)
- `data_format`
- `media_type`

## Current Behavior

0. State-level consistency is not centralized
- `State` has public fields (`data`, `metadata`), so direct `State { ... }` construction bypasses normalization entirely: `liquers-core/src/state.rs:13`.
- `State::with_metadata(...)` enforces `type_identifier` from value: `liquers-core/src/state.rs:33`.
- `State::with_data(...)` does NOT update metadata (no type/data-format/media-type alignment): `liquers-core/src/state.rs:60`.
- `State::from_value_and_metadata(...)` stores metadata as-is (no consistency check): `liquers-core/src/state.rs:26`.

1. No validation on external writes (`set`, `set_state`)
- `set(...)` stores metadata without validating required fields or compatibility: `liquers-core/src/assets.rs:2050`.
- `set_state(...)` updates status/timestamp/log and persists, also without consistency validation: `liquers-core/src/assets.rs:2115`.

2. Deserialization path does not use `data_format`
- `deserialize_from_binary()` uses `metadata.extension()` (filename extension), not `metadata.get_data_format()`: `liquers-core/src/assets.rs:1285`.
- This is the largest consistency bug: metadata can declare one format while loader uses another.
- `deserialize_from_binary()` is currently unused in code paths (no call sites found via ripgrep). Active store-load deserialization happens in `AssetData::try_fast_track()` and already uses `metadata.get_data_format()`: `liquers-core/src/assets.rs:395`.

3. Serialization path uses `data_format`
- `State::as_bytes()` serializes using `metadata.get_data_format()`: `liquers-core/src/state.rs:75`.
- This creates asymmetry: write may use `data_format`, read may use filename extension.

4. Metadata helpers are permissive and can drift
- `get_data_format()` falls back to extension and then `bin`: `liquers-core/src/metadata.rs:850`.
- `get_media_type()` falls back to extension and then `application/octet-stream`: `liquers-core/src/metadata.rs:836`.
- `with_filename()` updates `media_type` from extension but does not set `data_format`: `liquers-core/src/metadata.rs:770`.
- `set_filename()` only sets filename (no media/data-format synchronization): `liquers-core/src/metadata.rs:809`.

5. Serializer support is finite, but metadata accepts arbitrary values
- Core serializer/deserializer supports a limited set (`json`, text-like, `bytes|b|bin`): `liquers-core/src/value.rs:760` and `liquers-core/src/value.rs:800`.
- No pre-check ensures metadata `data_format` and `type_identifier` are actually supported.

6. Existing specs already state stronger requirements than implementation
- `specs/ASSET_SET_OPERATION.md` states mandatory `data_format` and `type_identifier` for `set()`/`set_state()`, but this is not enforced in code.

6a. `type_name` vs `type_identifier` semantics are split and not captured in metadata
- `ValueInterface` has both:
  - `identifier()` = cross-platform/storage identifier (`type_identifier` candidate)
  - `type_name()` = detailed runtime/debug name
  (`liquers-core/src/value.rs:170` and `liquers-core/src/value.rs:175`).
- Metadata persists only `type_identifier`; no dedicated field for `type_name` today.
- Errors/diagnostics often use `type_name()`, while persistence/deserialization relies on `type_identifier`, so introspection/debug information can be inconsistent across runtime vs stored metadata.

6b. `data_format`/`media_type` consistency is mostly derived, with small setter gaps
- The effective behavior is largely extension-derived:
  - `get_data_format()` falls back to extension.
  - `get_media_type()` falls back to extension mapping.
  - `with_filename()` updates media type from extension.
- Gap found: `set_filename()` and `set_extension()` mutate filename/extension but do not proactively sync `media_type`.
- Conclusion: broad format/media redesign is not required; small setter-level synchronization is enough.

7. Current serialization and save-to-store flow in `assets.rs`
- Evaluation path:
  - `evaluate_and_store()` computes value and then triggers `save_to_store()` (sync or background): `liquers-core/src/assets.rs:1163`.
  - `save_to_store()` obtains binary from `poll_binary()` or `serialize_to_binary()`: `liquers-core/src/assets.rs:1217`.
  - `serialize_to_binary()` serializes `poll_state()` via `State::as_bytes()` using `metadata.get_data_format()`: `liquers-core/src/assets.rs:1301` and `liquers-core/src/state.rs:75`.
  - Then `store.set(key, data, metadata)` persists payload and metadata: `liquers-core/src/assets.rs:1239`.
- External write path:
  - `AssetManager::set()` writes provided binary+metadata directly to store: `liquers-core/src/assets.rs:2050`.
  - `AssetManager::set_state()` serializes `State::as_bytes()` when possible, else stores metadata only: `liquers-core/src/assets.rs:2115`.

8. State creation patterns are mixed (no single enforced pattern)
- Pattern A (better): build via `State::new().with_data(...).with_metadata(...)` in interpreter:
  - `liquers-core/src/interpreter.rs:56`
  - `liquers-core/src/interpreter.rs:293`
  - This eventually sets `type_identifier` via `with_metadata`.
- Pattern B (partial): `State::from_value_and_metadata(...)` with caller-managed metadata:
  - `liquers-core/src/assets.rs:1955`
  - `liquers-core/src/assets.rs:1974`
  - `liquers-core/src/cache.rs:250`
- Pattern C (manual struct literal): direct `State { data, metadata }`:
  - `liquers-core/src/assets.rs:482`
  - `liquers-core/src/assets.rs:1099`
  - plus several test-only locations in `liquers-lib`.
- Pattern D (data-only helper): `State::new().with_data(...)` appears heavily in `liquers-lib` image tests; metadata remains default unless later overridden.

Conclusion: there is no single common pattern that guarantees metadata/value consistency at State creation or mutation.

9. State creation inventory (current)
- Core production code:
  - `liquers-core/src/state.rs`: constructor/mutator methods (`new`, `from_value_and_metadata`, `with_metadata`, `with_data`, `from_error`, `with_string`)
  - `liquers-core/src/interpreter.rs:56` and `liquers-core/src/interpreter.rs:293` (builder chain)
  - `liquers-core/src/interpreter.rs:291` (input state via `State::new()`)
  - `liquers-core/src/assets.rs:236` and `liquers-core/src/assets.rs:241` (asset init with `State::new()`)
  - `liquers-core/src/assets.rs:482`, `liquers-core/src/assets.rs:494`, `liquers-core/src/assets.rs:505` (manual struct literals in `poll_state`)
  - `liquers-core/src/assets.rs:1099` (manual struct literal in `evaluate_recipe`)
  - `liquers-core/src/assets.rs:1955`, `liquers-core/src/assets.rs:1974` (`from_value_and_metadata` in apply paths)
  - `liquers-core/src/cache.rs:250` (`from_value_and_metadata` when reading cache)
- Non-core/test code with direct struct literals (bypass risk if copied into production):
  - `liquers-lib/src/utils.rs:138` (test helper)
  - `liquers-lib/src/ui/app_state.rs:1112`, `liquers-lib/src/ui/app_state.rs:1147`, `liquers-lib/src/ui/app_state.rs:1162` (tests)
  - `liquers-lib/tests/polars_commands.rs:28` (test helper)

## Risk Summary

1. Silent incompatibility
- Data may be serialized in one format and later deserialized with another due to extension-first behavior.

2. Delayed failure
- Invalid metadata is accepted at write time and fails only when asset is read/deserialized.

3. Cross-client inconsistency
- API/store clients can persist metadata that looks valid but is not executable by current `Value` serializer.

4. In-process inconsistency drift
- Value can be changed (`with_data`) without synchronizing metadata type/data-format/media-type.
- Direct field construction and assignment can bypass any future invariants.

## Candidate Invariants (to enforce)

1. For persisted binary/state data (non-error statuses):
- `type_identifier` must be non-empty.
- effective `data_format` must be non-empty and supported by active serializer.

2. `media_type` should match effective `data_format` unless explicitly overridden.

3. Deserialization must use effective `data_format` (not filename extension).

4. If filename extension conflicts with `data_format`, policy must be explicit:
- either canonicalize filename extension to data_format, or
- allow mismatch but warn/log.

5. State invariant:
- Any `State` creation or mutation must maintain metadata consistency with value:
  - `type_identifier == value.identifier()`
  - `type_name == value.type_name()` (for metadata field once added)
  - `data_format` and `media_type` remain extension-consistent by existing/default behavior + setter sync
  - do NOT enforce value-type vs data-format compatibility at metadata validation time

## Key Gaps to Resolve Before Implementation

1. Strictness policy
- Reject inconsistent metadata (`Error`) vs normalize-and-warn.

2. Compatibility policy
- Whether to introduce strict validation immediately or via phased rollout/feature flag.

3. Ownership of format/type registry
- Static map in `liquers-core` vs serializer-provided capability checks.
