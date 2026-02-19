# Issue 8 Proposed Solution: VALUE-LIST-SUPPORT

## Decision

Standardize `lui/children` and `lui/roots` on **JSON-compatible integer arrays** (logical type: `Vec<i64>`), with no backward-compatibility string mode.

## Proposed Changes

### 1. Modify existing `children` / `roots` to return typed arrays

Update:
- `liquers-lib/src/ui/commands.rs` `children`
- `liquers-lib/src/ui/commands.rs` `roots`

Behavior (direct replacement):
- Convert `UIHandle` list to `Vec<i64>` (or `u64` mapped carefully to i64 if guaranteed in-range).
- Return as value array instead of comma-separated string.

Implementation:
- Use `Value::from(vec_of_i64)` after adding conversion impls in step 2.

### 2. Add ergonomic list conversions (non-breaking)

Add conversion impls:
- `impl From<Vec<i64>> for liquers_core::value::Value`
- `impl From<Vec<i64>> for liquers_lib::value::simple::SimpleValue`
- `impl<B,E> From<Vec<i64>> for CombinedValue<B,E>` where `B: From<Vec<i64>>`

Optional but useful:
- `impl TryFrom<Value> for Vec<i64>` and equivalent for `SimpleValue` / `CombinedValue`.

This removes boilerplate and makes list-returning commands obvious and type-safe.

### 3. Make `ValueInterface::try_from_json_value` support arrays/lists

Update the default `ValueInterface::try_from_json_value` implementation in `liquers-core/src/value.rs` to parse:

- `serde_json::Value::Array` recursively into list values
- (optionally, for consistency) `serde_json::Value::Object` recursively as well

Rationale:
- `FromParameterValue<Vec<V>>` already relies on `V::try_from_json_value` for each item.
- Having default array support improves generic `ValueInterface` behavior and removes an unnecessary limitation.
- Concrete `Value`/`SimpleValue` already support arrays; this change aligns trait default behavior with practical usage.

## Required Documentation Updates

- `specs/ISSUES.md` Issue 8 text:
  - replace `Value::List` with `Value::Array`
  - note that `children`/`roots` are changed in-place to return arrays
- `specs/UI_INTERFACE_PHASE1_FSD.md` command table:
  - update `children` / `roots` return type from `comma-separated handles` to `array of handles (integers)`
- `specs/COMMAND_REGISTRATION_GUIDE.md` (if examples include `lui` navigation return forms)

## Test Plan

### Unit tests

1. `Value`/`SimpleValue` conversions:
- `From<Vec<i64>>` creates array values.
- `TryFrom<Value> for Vec<i64>` roundtrip (if implemented).

2. `lui` command behavior:
- `children` returns array value with expected numeric handles.
- `roots` returns array value with expected numeric handles.

### Integration tests

- End-to-end query including `children`/`roots` and downstream command consuming `Vec<i64>`.

## Risks and Notes

- Changing `children`/`roots` output type is externally visible behavior (intentional).
- `UIHandle` currently wraps `u64`; if converted to i64, define overflow policy explicitly (practically unlikely for UI handles, but should be documented).

## Suggested Rollout

1. Add conversions + tests.
2. Update `children` and `roots` implementations.
3. Update docs.
