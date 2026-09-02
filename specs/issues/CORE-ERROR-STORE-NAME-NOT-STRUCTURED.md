---
id: CORE-ERROR-STORE-NAME-NOT-STRUCTURED
kind: issue
title: A store's name is interpolated into the error message instead of being a payload field
status: draft
priority: P2
complexity: S
area: [core/error, core/store]
design:
created: 2026-09-02
github:
---
## Problem

Three of the store error constructors take the store's name and bake it into the message string:

```rust
pub fn key_not_supported(key: &Key, store_name: &str) -> Self { …format!("Key '{}' not supported by store {}", key, store_name)… }
pub fn key_read_error(key: &Key, store_name: &str, message: &…) -> Self { … }
pub fn key_write_error(key: &Key, store_name: &str, message: &…) -> Self { … }
```

`ErrorPayload` (`liquers-core/src/error.rs:58`) carries `query`, `key`, `position` and `command_key`
as **fields**, each with a builder — `with_query`, `with_key`, `with_position`, `with_command_key`
(`:143-160`). The store name is the one piece of provenance that is prose instead of data.

Two consequences:

1. **Only code holding `&self` can raise a store error.** A helper that knows a key is unusable —
   a path-mapping function, a key guard, a validator — cannot construct the error, because it has no
   name to pass. It must either take the name as a parameter (and `store_name()` returns an owned
   `String`, so that allocates per call on paths `CLAUDE.md` lists as performance-sensitive) or hand
   the decision back to its caller as a predicate.
2. **Nothing can ask an error which store it came from** without parsing the message. Tests assert
   on `error_type` and cannot assert on provenance; the web API cannot surface it as a field.

## Impact

Low but recurring. It surfaced in `design/opendal-path-mapping/` Phase 4 (refinement R1): the
`PathMap` type that concentrates the key→path rules cannot itself refuse a key with
`key_not_supported`, so the refusal rule sits in a predicate and the error is raised one level up in
the store. That split is defensible on its own — `is_supported` returns `bool` and needs a predicate
regardless — but it means `PathMap::data` does **not** enforce the rule it documents, and a future
call site that uses `PathMap::data` directly would bypass it.

## Expected behaviour

`store: Option<String>` on `ErrorPayload`, with `with_store_name(&self, name: &str) -> Self`
following the existing builders. The three constructors keep their signatures and additionally set
the field, so nothing breaks; a helper with no name raises the error unattributed and whoever has a
name enriches it:

```rust
// in a helper that has no &self
Err(Error::key_not_supported_unattributed(key))
// at the store boundary
.map_err(|e| e.with_store_name(&self.store_name()))
```

`ErrorPayload` derives `Serialize`/`Deserialize`, so the field is additive and `Option`, and existing
payloads deserialize unchanged. The payload is boxed (`error.rs:95`, with the reasoning recorded
there), so one more `Option<String>` does not widen `Result<T, Error>`.

## Discovery

Raised on 2026-09-02 in the Phase 4 gate of
[`design/opendal-path-mapping/`](../design/opendal-path-mapping/), on the observation that the
awkwardness R1 works around is an `Error` API gap rather than a fact about path mapping.
