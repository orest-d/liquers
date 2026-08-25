---
id: COMMAND-COMPOSITE-VARIADIC-ARGUMENTS
kind: feature
title: A variadic argument cannot carry composite elements such as pairs or tuples
status: draft
priority: P3
complexity: L
area: [core/commands, macro, lib/ui]
design: variadic-arguments-declaration
created: 2026-08-25
github:
---
## Problem

A variadic argument is a flat list of scalars: `Vec<String>`, `Vec<i64>`, and so on. Each element
comes from exactly one action parameter, converted through `FromParameterValue`. There is no way to
declare an argument whose elements are **composite** — a pair, a tuple, or a key-value entry.

The immediate casualty is dictionary-shaped arguments. A command that wants
`rename-old1-new1-old2-new2` (or any mapping) must either declare a fixed number of scalar
arguments, or take a flat `Vec<String>` and pair the elements up itself — the same class of
in-command workaround that `COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE` removed for flat lists.

## Expected behaviour

An argument can declare a composite element type, for example:

```rust
fn rename(state, pairs: Vec<(String, String)> multiple) -> result
```

with the query language, the plan builder, and the retrieval path agreeing on how consecutive
action parameters group into elements.

## What this requires

Each layer needs a decision, and none of them is settled:

| Layer | Question |
|---|---|
| Query language | How are elements delimited? Consecutive parameters grouped by arity is the cheapest rule (`a-1-b-2` is two pairs), but it makes a missing parameter shift every later element |
| `ArgumentType` | It describes a single scalar type today. A composite element needs a representation — a tuple of `ArgumentType`, or a named-field descriptor for key-value entries |
| `ParameterValue` | `MultipleParameters(Vec<ParameterValue>)` is flat. Nested `MultipleParameters` is currently rejected explicitly in three places (`plan.rs:748`, `commands.rs:~330`, `pop_value`'s vector branch); grouping would make it legal in exactly one shape |
| `register_command!` | Element-type inference must handle `Vec<(A, B)>`, and the four compile-time rejections added by `variadic-arguments-declaration` need composite-aware equivalents |
| GUI | A composite element needs its own widget — a row of element widgets, not one. Compounds with `UI-VARIADIC-ARGUMENT-LIST-EDITOR` |

## Fix direction

Not proposed. The layering question — whether grouping is a query-language concept, a metadata
concept, or purely a retrieval concept — should be settled before any of the above is designed, and
it is the reason this is `complexity: L` rather than an extension of the flat case.

A cheaper interim answer exists and should be weighed first: a scalar variadic argument of strings
plus a documented `key=value` convention parsed inside the command. That is a workaround, with the
same drawbacks as the `split('-')` workaround this family of issues has been removing, but it costs
nothing and may be sufficient.

## Discovery

Raised by the user during `specs/design/variadic-arguments-declaration/` Phase 1 review, as the
direction variadic arguments may take once flat lists work: "In the future, variadic arguments may
support types combining multiple values — tuples, key-value pairs (allowing to specify
dictionaries). Such complex arguments also would require specific gui elements." Recorded so the
flat-list design does not foreclose it.
