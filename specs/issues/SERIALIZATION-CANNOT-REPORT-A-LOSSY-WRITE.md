---
id: SERIALIZATION-CANNOT-REPORT-A-LOSSY-WRITE
kind: issue
title: Serialization cannot report a lossy write, so a value silently loses data on its way to the store
status: draft
priority: P2
complexity: M
area: [core/value]
design: serialization-lossy-write-warnings
created: 2026-10-13
github:
---
## Problem

`DefaultValueSerializer::as_bytes(&self, data_format) -> Result<Vec<u8>, Error>`
(`liquers-core/src/value.rs`) has two possible outcomes: bytes, or an error. A write that succeeds
but loses information has no third outcome to report through. The asset layer calls it where the
metadata *is* in hand: `liquers-core/src/assets.rs`, the store write after `state.as_bytes()`, and
the `PreparedVersion` path. But it has nothing to put into the metadata log
(`MetadataRecord::warning` exists and would be the natural sink).

The first concrete case is `design/value-to-json-non-finite-float/`. By maintainer decision, a NaN
or ±∞ nested in a core `Value::Array` / `Value::Object` is written to JSON as `null`, because a
refused write that stores nothing is worse for a user than a few `null`s. The decision accepts that
the loss is silent, and records it as the compromise this issue would remove. Other lossy writes
of the same shape include text formats that cannot represent every value, and a future format with
lower precision.

### Problem example

```rust
let v = Value::Array(vec![Value::F64(1.0), Value::F64(f64::NAN)]);   // core Value
v.as_bytes("json")?;   // Ok(b"[1.0,null]"), and nothing else
```

A keyed asset with this value is stored as `[1.0,null]` and its stored metadata has no log entry
about it; after a restart it reads back as `[1.0, None]`. It should carry a `Warning` naming `json`
and the non-finite float.

## Impact

A value read back from the store can differ from the value that was computed, with no trace in the
asset's metadata, log or UI. Whether a user sees `None` or NaN then depends on whether the asset
came from the cache. The workaround is to choose a lossless format, which requires knowing about the
problem in the first place. Today `specs/guides/SERIALIZATION_GUIDE.md` (planned by the design
above) is the only place that says so.

## Expected behaviour

A lossy write is recorded as a `Warning` entry in the metadata log that is stored with the bytes,
so it is visible in the asset info and the UI. One shape that avoids changing `as_bytes`: a
`ValueInterface` / `DefaultValueSerializer` method with a default, such as
`fn serialization_warnings(&self, data_format: &str) -> Vec<String> { Vec::new() }`. The asset
layer calls it beside `as_bytes` and adds each result with `MetadataRecord::warning` before
`store.set`. Core `Value` overrides it to report nested non-finite floats in `json`. The design
owns the final signature.

**Preferred home: the serialization redesign.** The maintainer's view (2026-10-13) is that
serialization may need a redesign to handle reporting and warnings, not only streaming.
`VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` already proposes one (`SerializedBytes`), and
its 2026-10-13 update records this issue there. Whichever is designed first must account for the
other (overlap test T2, shared contract). The default-method sketch above is the interim fallback,
worth doing only if that redesign is far off.

## Discovery

Phase 1 of `design/value-to-json-non-finite-float/`, 2026-10-13. The maintainer chose `null` for
nested non-finite floats and noted that the right companion would be a warning, which cannot be
logged from `as_bytes` without widening its interface. Duplicate search: `docs_index.py new`
suggested `STORE-WRITE-HAS-NO-PRECONDITION` (a concurrency problem, unrelated) and
`VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` (does not cover it, but shares the contract; see
above).
