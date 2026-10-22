---
id: LIBRARY-CODE-CALLS-ERROR-NEW-DIRECTLY
kind: issue
title: Library code constructs errors with Error::new, which CLAUDE.md forbids
status: draft
priority: P3
complexity: M
area: [core/value, lib/value, lib/polars]
design:
created: 2026-10-20
github:
---

## Problem

`CLAUDE.md` (Code Conventions, Error Handling; Constraints) forbids `Error::new` in favour of the
typed constructors (`Error::from_error`, `Error::general_error`, …). At HEAD about 50 call sites in
library code still use it, outside `error.rs` itself: `liquers-core/src/value.rs`
(`Value::as_bytes` / `deserialize_from_bytes` refusals), `liquers-core/src/plan.rs`,
`liquers-core/src/parse.rs`, `liquers-lib/src/value/simple.rs` (the `json` arm of `as_bytes`),
`liquers-lib/src/image/serde.rs`, `liquers-lib/src/polars/serde.rs` (`serialization_error`).
Found with `grep -rn "Error::new(" liquers-core/src liquers-lib/src`.

## Impact

Low: the errors carry the right `ErrorType` today. The rule exists so error construction stays
uniform and reviewable; every new arm written beside these (the serialization designs of
2026-10-20 add several) is tempted to copy the local pattern.

## Expected behaviour

Each site uses the typed constructor matching its `ErrorType` (`Error::from_error(ErrorType::SerializationError, …)`
for the serializers; parse errors through the parse constructors). Mechanical; can be done file by
file, and `plan.rs` / `parse.rs` may need a typed constructor that does not exist yet (check first).

## Discovery

Noticed 2026-10-20 in a bulk-design run (cluster C5a) while reading the serializer arms for
`specs/design/core-value-json-identifier-dispatch/` and `specs/design/polars-dataframe-ipc/`.
