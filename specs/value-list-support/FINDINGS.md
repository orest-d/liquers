# Issue 8 Findings: VALUE-LIST-SUPPORT

## Scope Investigated

- `specs/ISSUES.md` (Issue 8)
- `liquers-core/src/value.rs` (`Value`, `ValueInterface`)
- `liquers-core/src/commands.rs` (`FromParameterValue<Vec<V>>`)
- `liquers-lib/src/ui/commands.rs` (`lui` `children`, `roots`)
- `liquers-lib/src/value/simple.rs` and `liquers-lib/src/value/extended.rs`
- `specs/UI_INTERFACE_PHASE1_FSD.md` command contract

## Summary

Issue 8 is still relevant, but the current behavior is more nuanced than the issue text suggests.

1. The issue text says `Value::List`; current code uses `Value::Array`.
2. `lui/children` and `lui/roots` currently return comma-separated strings, not numeric arrays.
3. Core `Value` and `SimpleValue` can represent arrays, but there is no ergonomic `From<Vec<i64>>` path.
4. Generic `ValueInterface` does not require array/object conversion support; default rejects JSON arrays/objects.

## Current Behavior Details

### A. `lui` commands return CSV-like strings today

- `children` converts handles to strings and returns `Value::from(handles_str.join(","))`:
  - `liquers-lib/src/ui/commands.rs:123`
  - `liquers-lib/src/ui/commands.rs:124`
- `roots` does the same:
  - `liquers-lib/src/ui/commands.rs:234`
  - `liquers-lib/src/ui/commands.rs:235`

This matches the current Phase 1 FSD command table (`comma-separated handles`):
- `specs/UI_INTERFACE_PHASE1_FSD.md:1213`
- `specs/UI_INTERFACE_PHASE1_FSD.md:1219`

### B. Array support exists in concrete value types

- `Value` supports `Array(Vec<Value>)` and JSON-array roundtrip:
  - `liquers-core/src/value.rs:24`
  - `liquers-core/src/value.rs:456`
- `SimpleValue` supports `Array { value: Vec<SimpleValue> }` and JSON-array roundtrip:
  - `liquers-lib/src/value/simple.rs:40`
  - `liquers-lib/src/value/simple.rs:292`

### C. Generic `ValueInterface` array conversion is optional by design

Default `ValueInterface::try_from_json_value` rejects arrays/objects unless overridden:
- `liquers-core/src/value.rs:222`
- `liquers-core/src/value.rs:226`

This means generic code cannot assume list support for arbitrary custom value implementations.

### D. Parameter input side already supports vectors

`FromParameterValue<Vec<V>>` already accepts JSON arrays and maps each element via `V::try_from_json_value`:
- `liquers-core/src/commands.rs:271`
- `liquers-core/src/commands.rs:275`

So the main gap is on the output ergonomics/contract, not parsing vector parameters.

### E. Ergonomic conversion gap

There is no `impl From<Vec<i64>> for Value` (nor equivalent for `SimpleValue` / `CombinedValue`), so returning integer lists cleanly requires manual construction (`Value::Array(...)` or JSON conversion helper).

## Key Mismatch to Resolve

There are two conflicting contracts:

- Issue 8 + UI navigation intent: `children` / `roots` should return list-of-integers.
- Current FSD + implementation: they return comma-separated string.

Any change should explicitly choose one contract and document compatibility behavior.
