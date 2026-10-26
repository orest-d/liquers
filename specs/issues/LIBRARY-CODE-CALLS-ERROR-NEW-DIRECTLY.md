---
id: LIBRARY-CODE-CALLS-ERROR-NEW-DIRECTLY
kind: issue
title: Library code constructs errors with Error::new, which CLAUDE.md forbids
status: draft
priority: P3
complexity: M
area: [core/value, lib/value, lib/polars]
design: error-type-taxonomy
created: 2026-10-20
github:
---

## Problem

`CLAUDE.md` (Code Conventions, Error Handling; Constraints) forbids `Error::new` in favour of the
typed constructors. At HEAD (cd6d5da) 19 library call sites outside `error.rs` still use it — the
"about 50" first counted included `syn::Error::new` in `liquers-macro` and
`nom::error::Error::new` in `parse.rs`, which are other types:

| File | Sites | Type |
|---|---|---|
| `liquers-core/src/value.rs` (`Value::as_bytes`, `deserialize_from_bytes` refusals) | 5 | `SerializationError` |
| `liquers-core/src/plan.rs` (`ParameterValue::to_result`) | 1 | `ArgumentMissing` |
| `liquers-lib/src/value/simple.rs` (`as_bytes`, `json` arm and fallback) | 12 | `SerializationError` |
| `liquers-lib/src/image/serde.rs`, `liquers-lib/src/polars/serde.rs` (`serialization_error` helpers) | 2 | `SerializationError` |

`liquers-py/src/error.rs` `Error::new` (the Python constructor `lq.Error(type, message)`) is a
binding of the generic form and legitimately takes any type. There is no typed
`SerializationError` constructor, and `Error::from_error(ErrorType::X, msg)` is `Error::new` under
another name, so swapping one for the other satisfies the letter of the rule only. Nothing stops
new sites: `Error::new` is `pub`.

## Problem Example

`liquers-core/src/value.rs`, `as_bytes` with an unsupported format:

```rust
_ => Err(Error::new(
    ErrorType::SerializationError,
    format!("Unsupported format {}", data_format),
)),
```

Today: compiles and is copied by every new serializer arm beside it (`simple.rs` repeats it 12
times). Should be: a typed constructor that also states which of the two meanings of
`SerializationError` applies (here the capability refusal "format not supported for this type"),
and `Error::new` unreachable outside `error.rs`.

## Impact

Low: the errors carry the right `ErrorType` today. The rule exists so error construction stays
uniform and reviewable; every new arm written beside these (the serialization designs of
2026-10-20 add several) is tempted to copy the local pattern.

## Expected behaviour

Each site uses a typed constructor matching its meaning; `plan.rs` and the serializers need typed
constructors that do not exist yet. The rule is enforced by the compiler rather than by review.

## Discovery

Noticed 2026-10-20 in a bulk-design run (cluster C5a) while reading the serializer arms for
`specs/design/core-value-json-identifier-dispatch/` and `specs/design/polars-dataframe-ipc/`.

2026-10-26 (bulk design, compaction): sites re-counted (19, not about 50). Designed together with
`ERROR-TYPES-TOO-COARSE-AND-GENERAL-OVERUSED` in `specs/design/error-type-taxonomy/`: the typed
constructors these sites need, and the split of `SerializationError` they fall under, are that
design's contract (`overlap.md` T2), and its migration rewrites the same arms (T3). Not eligible
for automatic fixing on its own (two crates, rule 6), so exclusion E1 does not keep it separate.
