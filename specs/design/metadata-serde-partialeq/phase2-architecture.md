# Phase 2: Solution and Architecture - Serde and Equality for `Metadata`

## Chosen Solution

In `liquers-core/src/metadata.rs`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Metadata {
    LegacyMetadata(serde_json::Value),
    MetadataRecord(MetadataRecord),
}

/// Untagged: the same document `Metadata::to_json` writes.
impl serde::Serialize for Metadata {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Metadata::LegacyMetadata(value) => value.serialize(serializer),
            Metadata::MetadataRecord(record) => record.serialize(serializer),
        }
    }
}

/// Record first, legacy fallback: the same choice `Metadata::from_json_value` makes.
impl<'de> serde::Deserialize<'de> for Metadata {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        Metadata::from_json_value(value).map_err(serde::de::Error::custom)
    }
}
```

The `Version` type in the same file already carries a hand-written `impl serde::Serialize`
(≈161), so the pattern and imports (`serde::` paths, `extern crate serde` in `lib.rs`) are
established. `from_json_value` cannot fail in practice (its fallback deserializes any
`serde_json::Value`), but its `Result` is propagated rather than unwrapped.

## Rejected Alternatives

- **`#[derive(Serialize, Deserialize)]`** — externally tagged (`{"MetadataRecord": {…}}`), a
  second representation incompatible with every store's metadata. Rejected.
- **`#[serde(untagged)]` derive** — would need the variants reordered (an untagged
  `serde_json::Value` arm first swallows everything), and its error messages are worse. The
  explicit impl states the record-first rule in code next to `from_json_value`.
- **Implementing `to_json` via the new `Serialize`** — tempting, but unnecessary churn; leave
  `to_json` / `from_json` unchanged and test that both agree.

## Files and Symbols

| File | Symbol | Change |
|---|---|---|
| `liquers-core/src/metadata.rs` | `enum Metadata` | add `PartialEq` to derive |
| `liquers-core/src/metadata.rs` | new `impl serde::Serialize for Metadata`, `impl<'de> serde::Deserialize<'de> for Metadata` | after `impl Default for Metadata` |
| `liquers-core/src/metadata.rs` | `mod tests` | new tests (Phase 3) |
| `liquers-records/src/batch.rs` | `ChunkDescriptor` doc comment | drop the "until it is fixed" sentence; state that the remaining blocker is `query`'s serialized form |

## Ownership, Serialization, Errors, Sync/Async

No ownership change; `Serialize` borrows, `Deserialize` owns a buffered `Value`. Errors are
`D::Error::custom` from `serde_json::Error` — serde-level, so `liquers_core::error::Error` is not
involved. Entirely synchronous.

## API and Compatibility Effects

Additive trait impls on a public type. A downstream crate with its own `impl Serialize for
Metadata` cannot exist (orphan rule), so no conflict. `liquers-py` and `liquers-web` do not
implement these traits for `Metadata` (checked). `MetadataRecord: PartialEq` already exists,
so the derive compiles.

## Interactions

- `CORE-LEGACY-METADATA-ACCESSORS-RETURN-JSON` (closed): its fix (`#[serde(default,
  deny_unknown_fields)]` on `MetadataRecord`) is what makes the record-first deserialize correct;
  unchanged here.
- `record-streams`: `ChunkDescriptor` may derive the traits later; out of scope.

## Questions

- **Implementation detail — buffering through `serde_json::Value`:** resolved; the only formats
  in use are self-describing.
- **Implementation detail — impl placement:** next to `impl Default for Metadata`.

## Risk Table

| Aspect | Assessment |
|---|---|
| Likely files | `liquers-core/src/metadata.rs`; doc comment in `liquers-records/src/batch.rs` |
| Affected workflows/crates | none at runtime; compile-time for every crate (additive) |
| Existing-test impact | none expected; metadata tests at ≈3388-3420 exercise `from_json` / `to_json` and must stay green |
| New validation | five focused tests (Phase 3) |
| Compatibility/data | wire form identical to `to_json`; asserted by test |
| Concurrency/performance | none; deserialize buffers one `Value`, as `from_json_value` already does |
| Security | none |
| Recovery | revert the two impls and the derive word |
| Certainty | high |

## Review

Against Phase 1: every criterion maps to one impl. Against the code: signatures of
`from_json_value` (`serde_json::Result<Metadata>`), `MetadataRecord` derives, and the existing
`Version` impl were inspected at HEAD.
