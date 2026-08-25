---
id: CORE-VALUE-ENUM-OVERSIZED
kind: issue
title: Every Value occupies 704 bytes because three variants are stored unboxed
status: draft
priority: P2
complexity: M
area: [core/value, lib/value]
design:
created: 2026-08-18
github:
---
## Problem

A Rust enum is as large as its largest variant, and `liquers_core::value::Value` stores several
large structs inline. Measured at HEAD with `std::mem::size_of`:

| Type | Bytes |
|---|---|
| **`Value`** | **704** |
| `MetadataRecord` | 704 |
| `CommandMetadata` | 688 |
| `AssetInfo` | 656 |
| `Recipe` | 256 |
| `Query` | 64 |
| `Key` | 24 |
| `String` | 24 |
| `Arc<MetadataRecord>` | 8 |

So `Value::Metadata(MetadataRecord)` (`value.rs:31`) sets the size of the whole enum, and every
`Value::None`, `Value::Bool(_)` and `Value::I32(_)` occupies **704 bytes**. A `Vec<Value>` of ten
thousand integers costs about 7 MB to hold 40 KB of data.

`Value::AssetInfo(Vec<AssetInfo>)` is already indirect (a `Vec` is 24 bytes), so the three
offenders are `Metadata`, `CommandMetadata` and, at a lower level, `Recipe`.

## Impact

Every clone of a `Value` copies 704 bytes regardless of what it holds, and `Value` is cloned freely
— it is `Clone` by contract and `ValueInterface` returns it by value throughout. The cost falls
hardest on exactly the values that should be cheapest: scalars, and collections of them via
`Value::Array(Vec<Value>)`.

It also sets a bad default for the value types built on top: `liquers-lib`'s `ExtValue` correctly
puts every payload behind `Arc` (`value/mod.rs:24-47`), while core does not, so the convention is
contradicted by the type it is supposed to be modelled on.

## Expected behaviour

Large variants hold `Arc<T>` rather than `T`:

```rust
Metadata(Arc<MetadataRecord>),
CommandMetadata(Arc<CommandMetadata>),
Recipe(Arc<Recipe>),
```

`Arc` rather than `Box` because these are cloned far more often than mutated, and `Arc::clone` is a
refcount bump where `Box` would deep-copy — the same reasoning `ExtValue` already applies.

That takes `Value` from 704 bytes to roughly the size of `Query` (64) plus a discriminant — about a
**tenfold reduction** — and makes the cost of a `Value` independent of which variant it holds.

The change is mechanical but not local: it touches every construction and every `match` arm that
binds those payloads by value, in `liquers-core`, `liquers-lib`, `liquers-py` and `liquers-web`.
`SimpleValue` (`liquers-lib/src/value/simple.rs:16`) mirrors the same variant set and has the same
problem.

## Discovery

Measured on 2026-08-18 during `value-type-system` Phase 2, while deciding what discipline should
govern what a value variant may hold. The user raised the concern; the numbers above are from a
`size_of` probe against HEAD, not an estimate.
