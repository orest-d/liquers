---
id: VALUE-TYPE-DEFINITION-MACRO
kind: feature
title: Value types and their registry entries are hand-written instead of generated
status: draft
priority: P2
complexity: L
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

Separately, it leaves a real capability out of reach. `register_command!` cannot resolve a **type
identifier** to the Rust type it carries — the case `fn use_df(state, df: polars_dataframe)` needs —
because `liquers-macro` depends on neither `liquers-lib` nor a downstream user crate.

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
divergent arm, `TypeIdentified` impls, and `type_descriptions() -> Vec<TypeInfo>`.

**How it reaches `register_command!` — and why no data file is needed.** Proc-macros hold no
reliable state between invocations, so `register_command!` cannot *read* what `define_value_types!`
declared; compilation order, parallelism and incremental builds all make cross-invocation state
unsound. The channel is not macro state but **generated code**: `define_value_types!` emits a
module of type aliases and constants named after each identifier, and `register_command!` expands
`df: polars_dataframe` into a path referencing that alias. Ordinary name resolution does the
lookup, in the compiler, at the definition site. An unknown identifier becomes a name-resolution
error rather than a runtime miss, and a downstream crate defining its own types is covered by the
same mechanism — which a data file shipped with Liquers can never be.

Two constraints this places on the design:

1. **Identifiers must be Rust-identifier-safe, or be mangled deterministically.** `polars_dataframe`
   is fine; a namespaced `py:int` is not, and needs an agreed mangling (`py__int`) with the
   unmangled form kept as the string that appears in metadata and on the wire.
2. **The generated module must be in scope where `register_command!` is invoked**, so either the
   expansion uses an absolute path with a configurable crate root, or the type-defining crate
   re-exports the module and command crates import it.

## Discovery

Proposed by the user during `value-type-system` Phase 2, 2026-08-18, as a better answer than a
shared data file to the question of how `register_command!` learns about types defined outside
`liquers-macro`. The evidence that hand-written arms drift was found in the same phase:
`COMBINED-VALUE-DEFAULT-EXTENSION-NOT-DELEGATED`.

Related: `VALUE-CONVERSION-CAPABILITY` owns the extraction half — turning the value into the
declared Rust type — and `COMMAND-METADATA-ENHANCEMENTS` owns the `ArgumentType` variant that
carries a type identifier. `value-type-system` defines `TypeInfo`, `TypeRegistry` and
`TypeIdentified` in shapes a generator can emit: a builder rather than struct literals, so a later
field is additive.
