---
id: COMMAND-COMPOSITE-VARIADIC-ARGUMENTS
kind: feature
title: A variadic argument cannot carry a complex value such as pairs, tuples or a dictionary
status: draft
priority: P3
complexity: L
area: [core/commands, macro, lib/ui, lib/egui]
design: composite-variadic-arguments
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

A cheaper interim answer exists and should be weighed first (2026-11-24: weighed and not chosen,
see the update below; the UI layout data needs typed values and a GUI): a scalar variadic argument of strings
plus a documented `key=value` convention parsed inside the command. That is a workaround, with the
same drawbacks as the `split('-')` workaround this family of issues has been removing, but it costs
nothing and may be sufficient.

## Discovery

Raised by the user during `specs/design/variadic-arguments-declaration/` Phase 1 review, as the
direction variadic arguments may take once flat lists work: "In the future, variadic arguments may
support types combining multiple values — tuples, key-value pairs (allowing to specify
dictionaries). Such complex arguments also would require specific gui elements." Recorded so the
flat-list design does not foreclose it.

## Triage 2026-10-20

Still true at HEAD (18732dd). `ArgumentType` (`command_metadata.rs` ≈172) has no composite variant.
`variadic_element_type` (`liquers-macro/src/registration.rs` ≈541) only peels `Vec<T>`, and no
`FromParameterValue` impl exists for a tuple, so `Vec<(String, String)> multiple` does not compile.
Nested `MultipleParameters` is still refused in `plan.rs` `pop_value`, `commands.rs`
`convert_multiple_element` and `FromParameterValue<Vec<V>>`. No in-repo command pairs elements by
hand today. The origin design `design/variadic-arguments-declaration/` is complete; this feature is
now designed in `design/composite-variadic-arguments/` (needs-decision: a flat query, a
`Tuple` argument type, grouping at retrieval; the first question is whether to build it before a
command needs it).

## Update 2026-11-24: a first user, the older Python mechanism, and a wider scope

Discussed with Orest on 2026-11-24, during the UI redesign (`design/ui-redesign/master-design.md`
§10.9).

**A first user exists.** The UI redesign gives a child element optional layout data that only
its parent interprets: a flat map from a key to a scalar. In a UISpec it is
`layout: {row: 0, col: 8, colspan: 4}`. In a command it is a trailing list of key and value
pairs:

```
ns-lui/add-child-current-row-0-col-8-colspan-4
ns-lui/add-child-root-role-sidebar-size-240
```

This needs two things beyond pairs of declared types:

- **A dictionary shape**: keys are unique, and the command wants a map, not a list of pairs.
- **A scalar whose type is read from its text**: `8` is a number, `true` a boolean, `sidebar` a
  string. No single declared component type fits.

**The general pattern.** Orest: the pairs are "a convention that allows to represent
dictionaries or scalars as a variadic argument", and one example of a more general pattern:
*represent a complex type as a variadic argument*. It should be supported in the command
metadata and in the registration macro, with GUI support.

**The older Python mechanism.** The Python `liquer` had argument parsers (`ArgumentParser` in
`liquer/commands.py`): a parser consumed one or more dash-separated parameters from the front of
the list and returned a value and the rest; parsers were combined with `+`
(`SequenceArgumentParser`), and a parser with `is_argv` took all that remained. In principle a
2D vector could be one argument, and a list of 2D vectors a variadic one. It was never used
widely. The proposal here is simpler: grouping happens only in the variadic position, and the
shape is declared in the metadata, so that the plan, the registry and a GUI all see it.

**Scope after this discussion** (design `composite-variadic-arguments`, Phases 1 and 2
rewritten on 2026-11-24):

1. Tuple elements: `Vec<(A, B)> multiple` (the original scope).
2. A dictionary: `BTreeMap<String, V> multiple`, with duplicate keys refused.
3. A scalar component whose type is read from its text.
4. A trait, so that a command author's own type can be an element or a whole variadic value.
5. Registration macro support for all of the above, with compile-time rejections.
6. GUI support: the argument editor shows one row per element, with a column per component.

Open questions are listed in the design's Phase 1, `## Design Readiness`.
