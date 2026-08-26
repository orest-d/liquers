---
id: PY-VALUE-SERIALIZER-IS-A-STUB
kind: issue
title: liquers-py's value serializer writes almost nothing and reads nothing back
status: draft
priority: P2
complexity: M
area: [py, core/value]
design:
created: 2026-08-26
github:
---
## Problem

`impl DefaultValueSerializer for Value` (`liquers-py/src/value.rs:760`) is a stub.

**Writing** (`as_bytes`) accepts only `txt` and `html`, and only for `None`, `Bool`, `I32`, `I64`,
`F64`, `Text`, `Query` and `Key`. `Array`, `Object`, `Bytes`, `Metadata`, `AssetInfo`, `Recipe`,
`CommandMetadata` and `Py` fall through to a serialization error, as does every other format —
including `json`, which is the *declared default extension* of most of those variants.

**Reading** (`deserialize_from_bytes`) accepts nothing at all:

```rust
fn deserialize_from_bytes(b: &[u8], _type_identifier: &str, fmt: &str) -> Result<Self, Error> {
    match fmt {
        _ => Err(Error::new(ErrorType::SerializationError, …)),
    }
}
```

The `match` with only a wildcard arm is the shape of an unfinished function, and it also violates
the no-default-arm rule.

## Impact

**No Python value round-trips through a store.** A value can at best be written as text and never
read back, so an asset produced from Python cannot be reloaded as a value — only as bytes plus
metadata.

The type registry is honest about this as of 2026-08-26: `type_descriptions()` declares `txt`/`html`
for the three variants whose default is `txt` and which `as_bytes` implements, and **no formats at
all** for everything else, so those persist as metadata only. That is the correct description of
the current code, not a limitation of the type system — but it means the declarations must be
widened in step with the serializer, or they will understate it.

A second, smaller inconsistency: several variants declare `json` (or `pickle`, or `b`) as their
default extension while `as_bytes` cannot produce it. `State::as_bytes` on such a value resolves
the default format and then fails. Either the defaults or the codec should move.

## Expected behaviour

`as_bytes` and `deserialize_from_bytes` cover the same set of formats, that set includes each
type's own default, and `type_descriptions()` declares exactly it. Most of these types are plain
serde structures whose JSON support is a one-line `serde_json::to_vec`, so the bulk of this is
mechanical — see `DATA-FORMAT-CONSTANTS-AND-TOOLING`, which proposes a generic serde path that
would remove the hand-written arms entirely.

`Py` is the genuine exception: a retained Python object has no byte form unless pickling is
adopted deliberately, which is a decision rather than an omission.

## Discovery

Found on 2026-08-26 during `foreign-value-type-registration`, from a Codex review comment on PR
[#42](https://github.com/orest-d/liquers/pull/42) pointing out that the new `type_descriptions()`
advertised `json` for types the serializer cannot write. The declarations were corrected in that PR;
the serializer gap behind them is this issue.
