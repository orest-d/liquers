---
id: CORE-VALUE-ENUM-OVERSIZED
kind: issue
title: Every Value occupies over 700 bytes because three variants are stored unboxed
status: draft
priority: P2
complexity: M
area: [core/value, lib/value]
design: core-value-enum-oversized
created: 2026-08-18
github:
---
## Problem

A Rust enum is as large as its largest variant, and `liquers_core::value::Value` stores several
large structs inline. Measured with `std::mem::size_of` on x86_64 (first on 2026-08-18; re-measured
at HEAD on 2026-10-12, when `CommandMetadata` had grown past `MetadataRecord`):

| Type | 2026-08-18 | 2026-10-12 |
|---|---|---|
| **`Value`** | **704** | **752** |
| `MetadataRecord` | 704 | 624 |
| `CommandMetadata` | 688 | **736** |
| `AssetInfo` | 656 | 576 |
| `Recipe` | 256 | 256 |
| `Query` | 64 | 64 |
| `Key` | 24 | 24 |
| `String` | 24 | 24 |
| `Arc<MetadataRecord>` | 8 | 8 |

So the inline `Value::CommandMetadata(CommandMetadata)` and `Value::Metadata(MetadataRecord)`
(`value.rs`) set the size of the whole enum, and every `Value::None`, `Value::Bool(_)` and
`Value::I32(_)` occupies **752 bytes**. A `Vec<Value>` of ten thousand integers costs about 7.5 MB
to hold 40 KB of data.

`Value::AssetInfo(Vec<AssetInfo>)` is already indirect (a `Vec` is 24 bytes), so the three
offenders are `CommandMetadata`, `Metadata` and, at a lower level, `Recipe`. Putting only
`Metadata` behind `Arc` would leave the enum at 736 bytes.

## Problem Example

```rust
let v: Vec<Value> = (0..10_000).map(Value::from_i32).collect();
assert_eq!(std::mem::size_of::<Value>(), 752);   // passes today
```

Today the vector's buffer is 7,520,000 bytes. With the three large payloads behind `Arc`,
`size_of::<Value>()` is 64 and the buffer is 640,000 bytes.

## Impact

Every clone of a `Value` copies 752 bytes regardless of what it holds, and `Value` is cloned freely
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
Query(Arc<Query>),      // added 2026-10-12: brings Value to 32 bytes
```

`Arc` rather than `Box` because these are cloned far more often than mutated, and `Arc::clone` is a
refcount bump where `Box` would deep-copy — the same reasoning `ExtValue` already applies.

That takes `Value` from 752 bytes to exactly the size of `Query` (64; the discriminant fits in a
niche) — more than a **tenfold reduction** — and makes the cost of a `Value` independent of which
variant it holds.

The change is mechanical but not local: it touches every construction and every `match` arm that
binds those payloads by value. Counted on 2026-10-12, that is about 7 sites in
`liquers-core/src/value.rs` and 10 in `liquers-lib/src/value/simple.rs` when the `ValueInterface`
signatures keep taking and returning owned payloads; `liquers-py` (its own `Value` enum) and
`liquers-web` then need no edit.
`SimpleValue` (`liquers-lib/src/value/simple.rs:16`) mirrors the same variant set and has the same
problem.

## Discovery

Measured on 2026-08-18 during `value-type-system` Phase 2, while deciding what discipline should
govern what a value variant may hold. The user raised the concern; the numbers above are from a
`size_of` probe against HEAD, not an estimate.
