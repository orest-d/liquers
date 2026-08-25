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

## Design input: typed command arguments (2026-08-18)

Recorded during `value-type-system` Phase 2, at the user's request. Not part of that project.

**The idea.** A command written in ordinary Rust types

```rust
fn use_df(state: &State<V>, df: polars::DataFrame) -> Result<V, Error>
```

should register `df` under the type identifier `polars_dataframe`, and the framework should
**convert the incoming value to that Rust type automatically** — rather than the command declaring
`ArgumentType::Any` and unwrapping by hand.

This is why the `ArgumentType` change was moved out of `value-type-system` into
`COMMAND-METADATA-ENHANCEMENTS`: the *declaration* belongs there, the *conversion* belongs here,
and the two must be designed together.

**The governing rule, settled with the user:**

> **Rust types in Rust code and in command registration. Type identifiers in the registries** —
> which exist to integrate with other languages and other realms.

`value-type-system` supplies the single bridge, `to_type_identifier::<T>() -> &'static str`, a
`const fn` over `TypeIdentified::TYPE_IDENTIFIER`. Registration derives the identifier at compile
time from the same type token the macro already forwards (`registration.rs:492` emits
`let #var_name: #ty = arguments.get(#i, #name)?;`), so the extraction type and the recorded
identifier come from one `T` and **cannot disagree**. No lookup, no data file, no runtime check.
Earlier drafts of this note proposed a runtime `get_typed(.., "polars_dataframe")` and a
`CommandRegistryIssue` guarding the two against each other; both are unnecessary and are dropped.

The reverse direction — identifier to Rust type — is never needed inside Rust. Deserialization is
not a counterexample: bytes plus an identifier produce a `Value`, which is dispatch within the
value type, not resolution of a Rust type.

**What is left for this issue: the conversion itself.** `arguments.get::<T>(..)` must produce a
`polars::DataFrame` from whatever the value actually holds. That is an automatic conversion at
argument-binding time, so it is governed by this issue's central rule — automatic conversion
refuses `Lossy` and `Fallible` edges. A `df: polars::DataFrame` parameter succeeds against a
list-of-dictionaries value if that edge is `Structural`, and fails against an `i64` with a typed
error naming both the source type and the declared target. The trait — `FromValue<V>`, or whatever
it ends up called — is this issue's to design.

**Open, for whoever takes this up.** Whether a parameter can declare a *purpose* (`table`,
satisfied by several types) rather than one concrete Rust type. A purpose cannot be a Rust
parameter type directly, so it needs a wrapper — `Table<T>`, or an `impl TableLike` bound — and
that is the form in which conversion earns its keep.
