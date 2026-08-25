---
id: VALUE-TYPE-DEFINITION-MACRO
kind: feature
title: Value types and their registry entries are hand-written instead of generated
status: draft
priority: P2
complexity: XL
area: [macro, lib/value, core/value]
design:
created: 2026-08-18
github:
---
## Problem

Adding a value type today means hand-writing the same facts in many places. For one `ExtValue`
variant an author edits, at minimum, the enum, then a match arm in each of `identifier`,
`type_name`, `default_extension`, `default_filename`, `default_media_type` and `as_bytes`
(`liquers-lib/src/value/mod.rs:126-215`), then the corresponding `CombinedValue` delegations
(`value/extended.rs:136-168`), then the `ExtValueInterface` accessors — each with its own
`#[cfg]` guards. Nothing checks that the arms agree with one another.

They already do not. `COMBINED-VALUE-DEFAULT-EXTENSION-NOT-DELEGATED` is exactly this failure: one
of five sibling delegations returns the constant `"ext"` instead of delegating, so every extended
value reports `default_filename() == "image.png"` and `default_extension() == "ext"`. A hand-written
match arm is the only place that bug could have come from.

The arithmetic gets worse with the scalar widening `value-type-system` proposes: fifteen scalars
across roughly eight exhaustive match sites is on the order of 120 cfg-gated arms, all of them
mechanical, all of them a place for the same class of mistake.

## Impact

Adding a value type is tedious and error-prone, and the errors are silent — a wrong arm compiles
and produces a self-contradictory type description that only shows up when something downstream
tries to serialize. The cost also falls on every crate defining its own value types, which is the
documented extension path (`liquers-lib/src/value/extended.rs:12-20`).

## Expected behaviour

A function-like macro in `liquers-macro`, alongside `register_command!`, that takes a declarative
list of the types **not already in core** and generates the enum, every trait impl, and the
registry entries from one description:

```rust
define_value_types! {
    ExtValue {
        polars_dataframe: Arc<polars::frame::DataFrame> {
            extension: "csv", media_type: "text/csv",
            formats: ["csv", "parquet", "json"],
            cfg: feature = "polars",
        },
        image: Arc<image::DynamicImage> { extension: "png", media_type: "image/png", .. },
    }
}
```

generating the enum, the `ValueExtension` / `DefaultValueSerializer` impls with no possibility of a
divergent arm, `TypeIdentifiedIn<ExtValue>` impls, and `type_descriptions() -> Vec<TypeInfo>`.

**Relationship to command registration.** The generated `TypeIdentifiedIn<V>` impls are what let
`register_command!` record a type identifier for an argument written as an ordinary Rust type, via
`to_type_identifier::<V, T>()` (`value-type-system` Phase 2). This is a convenience, not a dependency:
a hand-written `impl TypeIdentifiedIn<MyValue> for MyType` works identically, which is why a downstream crate
defining its own types needs nothing from this macro and no shared data file exists anywhere in the
design. Note also that `register_command!` is expected to be redesigned; this macro should not
assume its current form.

## Included scope: the extended scalar set

`value-type-system` established, from a nine-ecosystem correspondence table
(`specs/design/value-type-system/prior-art.md` §9), that `Value` lacks fifteen scalars at least
five of the nine target systems represent distinctly: `i8, i16, i128, u8, u16, u32, u64, u128,
f32` (no dependency) and `decimal, date, time, datetime, duration, uuid` (`rust_decimal` and
`uuid`; `chrono` is already non-optional in both `liquers-core` and `liquers-lib`).

**Implementing them belongs here rather than there**, decided with the user on 2026-08-18. They are
this generator's first and best customer: written by hand they are ~120 mechanical cfg-gated match
arms that the generator would immediately delete, and every one of those arms is an opportunity for
the divergence that produced `COMBINED-VALUE-DEFAULT-EXTENSION-NOT-DELEGATED`. Declared through the
macro they are fifteen lines.

The rule for what counts as a Liquers scalar — Rust has it (or a canonical crate does), **and** at
least five of the nine ecosystems represent it distinctly — is settled and excludes `f16`,
`complex`, `char`, `isize`/`usize` and GlueSQL's `Inet`/`Point`. Feature gating (`ext-scalars`
without dependencies, `ext-temporal` with) is a starting proposal, not a decision.

## Discovery

Proposed by the user during `value-type-system` Phase 2, 2026-08-18, while working through how
`register_command!` learns about types defined outside `liquers-macro`. That question was
subsequently answered without any macro at all — Rust code names Rust types, and the identifier is
derived from them — so this issue stands on its own merit: the hand-written arms drift, and
`COMBINED-VALUE-DEFAULT-EXTENSION-NOT-DELEGATED`, found in the same phase, is the proof.

`DATA-FORMAT-CONSTANTS-AND-TOOLING` shares a boundary with this issue and should be designed with
it: the macro must let a type enable or disable the generic serde formats individually and supply
its own encoder and decoder for a format — that is how `polars.DataFrame` gets CSV and Parquet
while `Image` gets PNG — and `supported_data_formats` should be *derived* from those choices rather
than restated alongside them, so a declaration cannot disagree with its implementation.

Related: `VALUE-CONVERSION-CAPABILITY` owns the extraction half — turning the value into the
declared Rust type — and `COMMAND-METADATA-ENHANCEMENTS` owns the `ArgumentType` variant that
carries a type identifier. `value-type-system` defines `TypeInfo`, `TypeRegistry` and
`TypeIdentifiedIn<V>` in shapes a generator can emit — see its Phase 2 "Generator alignment" section for
the commitments it makes so that nothing here has to undo them.
