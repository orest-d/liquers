---
id: VALUE-CONVERSION-CAPABILITY
kind: feature
title: Values cannot be converted between types, automatically or explicitly
status: draft
priority: P2
complexity: L
area: [core/value, core/commands, lib/value]
design:
created: 2026-08-18
github:
---
## Problem

There is no way to convert a value from one type to another, and no way to ask whether such a
conversion is possible. Neither direction exists:

- **Explicit.** A user with a `polars_dataframe` who needs a list of dictionaries, or an `i64` that
  must become a `str`, has no command and no query syntax to say so. Every conversion is hand-coded
  inside whichever command happens to need it.
- **Automatic.** A command that needs "a table" must declare `ArgumentType::Any` and sort it out
  itself — see the `// TODO: add support for value with type_identifier` markers at
  `liquers-core/src/command_metadata.rs:73` and `:152`. A UI widget that renders tables cannot
  accept a Polars frame and a list of dictionaries through one path. A web client sending
  `Accept: text/csv` gets no negotiation.

The **purpose axis** — "can this value be used as a table / an image / JSON?" — was designed
alongside the `value-type-system` project and deliberately deferred here, because a purpose
vocabulary without conversion has no consumer: a caller could ask whether a value is a `table` and
still have no way to obtain one. The two are one feature.

## Impact

Conversion logic is duplicated inside individual commands, with each one choosing its own rules for
lossy cases. Nothing in the system can state that `f64 → f32`, or `i64 → JSON number` above 2⁵³,
loses information — so those conversions happen silently wherever someone wrote them by hand. Value
types cannot be substituted for one another, which pushes command libraries towards accepting
`Any` and validating by hand.

## Expected behaviour

A conversion registry alongside the type registry, keyed by source and target, where each edge
carries a classification (`Exact`, `Widening`, `Lossy`, `Fallible`, `Structural`) and a conversion
function. Explicit conversion permits any edge, since the user asked. Automatic conversion —
argument binding, UI rendering, web negotiation — refuses `Lossy` and `Fallible` edges, so
information is never lost without someone saying so. `can_convert` answers without doing the work,
so a value can advertise its reachable purposes cheaply.

Wants a design: it spans the command metadata, the value types in three crates, and the web layer.

## Discovery

Designed out of scope during `value-type-system` Phase 1, 2026-08-18, at the user's direction. The
full proposal — purpose model, conversion classes, the two-kinds table, and the relationship to the
encoding axis — is written up in `specs/design/value-type-system/type-conversion-draft.md`, with
the nine-ecosystem correspondence table it builds on in
`specs/design/value-type-system/prior-art.md` §9. Start there rather than from scratch.

---

## Design input: the type identifier as a command argument type (2026-08-18)

Recorded during `value-type-system` Phase 2, at the user's request. Not part of that project.

**The idea.** `ArgumentInfo` should be able to name a **type identifier** as its argument type, and
the framework should then convert the incoming value automatically to the Rust type the variant
carries — so a command written as

```rust
fn describe(state, frame: PolarsDataFrame) -> result
```

declares "this argument is a `pl:dataframe`" and receives the concrete Rust value, rather than
declaring `ArgumentType::Any` and unwrapping by hand.

This is why the `ArgumentType` change was moved out of `value-type-system` and into
`COMMAND-METADATA-ENHANCEMENTS`: the *declaration* belongs there, but the **automatic conversion**
belongs here, and the two must be designed together.

**What it needs: a correspondence between type identifiers and Rust types.**

The DSL slot carries the **type identifier**, not a Rust path, and the identifier is defined
somewhere the macro cannot see — `polars_dataframe` is declared by `liquers-lib`, or by a
downstream crate entirely outside this repository, while `liquers-macro` depends on neither.

The tempting answer is a data file both `liquers-lib` and `liquers-macro` can read. **It does not
work, and it is not needed.**

*Why it does not work.* A file shipped with Liquers cannot describe types defined in a downstream
user crate, which is the case that matters most. And a proc-macro's filesystem reads are not
tracked by cargo for rebuild purposes, so an expansion goes stale silently when the file changes —
a failure mode considerably worse than the staleness `specs/command_registry.yaml` already has to
be defended against by a dedicated test.

*Why it is not needed.* **The macro never has to resolve an identifier to a Rust type, because the
Rust type comes from the command function's own signature.** This is not a new mechanism — it is
exactly what the macro does today. `registration.rs:492` generates

```rust
let df__par: #ty = arguments.get(#i, #name_str)?;
```

where `arguments.get` is generic and `#ty` is a token the macro *forwards* from the DSL without
ever interpreting it. In the identifier-based form the annotation simply moves: the macro emits an
unannotated binding, and the generated call to the user's function pins the type.

```rust
// register_command!(cr, fn use_df(state, df: polars_dataframe) -> result)
// against  fn use_df(state: &State<V>, df: polars::DataFrame) -> Result<V, Error>

let df__par = arguments.get_typed(0usize, "df", "polars_dataframe")?;
use_df(state, df__par)      // <- infers df__par: polars::DataFrame
```

`get_typed<T: FromValue<V> + TypeIdentified>(idx, name, declared_identifier)` receives the
identifier as ordinary data and the Rust type by inference. `liquers-macro` gains no dependency, no
file, and no knowledge of `polars`.

**Where the agreement is checked.** `T::TYPE_IDENTIFIER` and the declared string must match, or the
author wrote `df: polars_dataframe` against a parameter of some other type. The natural home is the
existing `CommandRegistryIssue` mechanism — `CommandMetadata::check()`
(`command_metadata.rs:427`, `:965`) already exists to report exactly this class of
metadata-versus-reality disagreement, and registration has the `TypeRegistry` in hand to also
reject an identifier no build registers. A per-monomorphization `const` assertion inside
`get_typed` would give a compile-time error instead, but post-monomorphization errors are poor
diagnostics; the registry issue is the better default.

**The data file still has a job — just not this one.** `liquers-validate` and non-Rust clients need
to know which identifiers exist without linking `liquers-lib`, and that is the
`export-command-registry` pattern applied to types. Its consumer is tooling, not the macro.

**The extraction half is squarely this issue's.** `FromValue<V>` — or whatever it is called — is an
automatic conversion at argument-binding time, so it is governed by this issue's central rule:
automatic conversion refuses `Lossy` and `Fallible` edges. A command declaring
`df: polars_dataframe` against a value that is a list of dictionaries succeeds if that edge is
`Structural`; against an `i64` it fails with a typed error naming both the source type and the
declared target.

**Open, for whoever takes this up.** Whether the declared argument type is a *type identifier*
(exact, one variant) or a *purpose* (`table`, satisfied by several). Both are useful and they are
not the same declaration; the purpose form is the one that makes conversion pull its weight.
