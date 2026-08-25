# Phase 1: High-Level Design - variadic-arguments-declaration

## Feature Name

Declarable variadic command arguments

## Purpose

`ArgumentInfo.multiple` — an argument that consumes every remaining action parameter — is
implemented end to end in the plan builder and interpreter, but a command author cannot declare one
through `register_command!` and the command framework cannot hand its value to a function. This
design makes the existing mechanism reachable: a `multiple` flag in the macro DSL, a retrieval path
that gets past the trait bounds blocking `Vec<T>`, and the two polars commands that have been
faking it with `split('-')` converted to use it.

## Core Interactions

### Query System

None. The query language already spells a variadic call — `select_columns-a-b-c` is three action
parameters — and parsing does not change. What changes is which command declarations can absorb
them.

### Store System

Not applicable.

### Command System

No new commands. Two existing ones change signature: `pl/select_columns` and `pl/drop_columns`
(`liquers-lib/src/polars/selection.rs:15`, `:37`) go from `columns: String` to a variadic
`columns: Vec<String>`, and their internal `split('-')` workaround is deleted. The `register_command!`
argument DSL gains a `multiple` flag alongside `injected`, and — because a second bare-identifier
flag makes a silent typo possible — the flag parser (`liquers-macro/src/registration.rs:1564`) starts
rejecting unknown identifiers instead of swallowing them.

`CommandArguments` gains an accessor for variadic arguments. `get` (`liquers-core/src/commands.rs:102`)
cannot serve them: its `T: FromParameterValue<T> + TryFrom<E::Value, Error = Error>` bounds are
unsatisfiable for `Vec<String>`, and the obvious blanket impl collides with the existing
`impl<V: ValueInterface> FromParameterValue<Vec<V>> for Vec<V>` under coherence.

### Asset System

Unaffected. A variadic argument may contain links, and `materialize_nested_parameter`
(`interpreter.rs:344`) already resolves them element-wise, so dependency collection
(`interpreter.rs:115`) needs no change.

### Value Types

No new `ExtValue` variants. `ParameterValue::MultipleParameters` already exists and already carries
resolved elements.

### Web/API

Not applicable, beyond `specs/command_registry.yaml` regenerating with `multiple: true` on the two
converted arguments — `ArgumentInfo.multiple` already serializes.

### UI

No new widget. `liquers-lib/src/egui/widgets.rs:733` already labels a `multiple` argument; making
one declarable turns that dormant branch live. Whether a variadic argument needs a list-entry widget
rather than a single text field is an open question below, not scope here.

## Crate Placement

- **liquers-core** — `commands.rs`: the new accessor. No trait added, no existing bound relaxed.
- **liquers-macro** — `registration.rs`: the `multiple` DSL flag, unknown-flag rejection, and
  emitting the new accessor for a variadic argument.
- **liquers-lib** — `polars/selection.rs`: convert the two commands and delete the workaround.

Follows the dependency flow: core provides, macro emits, lib consumes. Nothing in liquers-store,
liquers-axum or liquers-web changes.

## Documentation Intent

**Reference:** Extend `specs/reference/REGISTER_COMMAND_FSD.md` — the argument grammar at `:102`
and the attribute table at `:109` gain `multiple`, and the generated-`ArgumentInfo` example at
`:388` stops showing `multiple: false` as invariant. Also `specs/reference/POLARS_COMMAND_LIBRARY.md`,
which `design/excess-action-parameters-error/` set to the escaped `col1~_col2` spelling; it reverts
to the plain `col1-col2` and the arity-error note at `:90` becomes a note about escaping a column
name that genuinely contains a dash. No new reference: this is a gap in a documented mechanism, not
a new mechanism.

**Guide:** Extend `specs/guides/COMMAND_REGISTRATION_GUIDE.md` §"Accepting a variable number of
parameters" (`:156-175`). It currently says the feature *cannot be declared* and prescribes the
`~_` workaround; it becomes the how-to for declaring one, including the last-argument constraint.
No new guide — the material belongs in the command-registration guide the audience already reads.

**Other documents to create:** None. The design folder plus the two extended documents cover it.

**Specific documents to update:**

| Path | Change |
|---|---|
| `CLAUDE.md` | "DSL Syntax Reference" gains `multiple` beside `injected` |
| `specs/command_registry.yaml` | Regenerated (generated file; `registry_export` test enforces) |
| `specs/README.md` | New design folder listed, per the docs rules |
| `specs/issues/COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE.md` | `design:` set; status closed at Phase 5 |
| `specs/issues/VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS.md` | Same, if open question 1 pulls it in |

**Audience and outcome.** A command author writing `register_command!`, and a coding agent adding
a polars-style command. After this lands they should be able to declare a variable-length argument
from the reference and the guide alone, and should find no document still claiming it is
impossible.

## Open Questions

1. **Does this effort also fix `VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS`?** It becomes reachable
   the moment `multiple` is declarable: an argument declared after a variadic one silently takes its
   default, or reports "missing" for a value the caller did supply. Its own fix direction
   (`CommandMetadata::check()`) has two obstacles — `check()` has no caller anywhere in the
   workspace, and `CommandRegistryIssue::{warning,error}` transpose two fields
   (`COMMAND-REGISTRY-ISSUE-NAMESPACE-NAME-SWAPPED`). A cheaper guard is available: reject the
   declaration in the macro, at compile time, where no runtime caller is needed. Phase 2 should
   decide between the two; my inclination is the macro guard now and the registry check filed as
   remaining work.
2. **Which element types must a variadic argument support?** `Vec<String>` is what the polars
   commands need. `Vec<i64>`, `Vec<f64>`, `Vec<bool>` fall out of the same accessor for free if it
   is generic over `FromParameterValue`. `Vec<Value>` already works through the existing impl and
   must keep working — Phase 2 must check the two paths do not collide.
3. **What `ArgumentType` does a variadic argument report?** Today the macro infers from the Rust
   type and `Vec<String>` falls through to `ArgumentType::Any` (`registration.rs:658`), which loses
   the per-element type used by `from_string` when parsing each parameter. Should it infer `String`
   from the element type instead?
4. **Can a variadic argument carry a default?** `from_arginfo` (`plan.rs:481`) already expands an
   array default into `MultipleParameters`, but the DSL's `= <default>` accepts no array literal.
   Leave unsupported, or add?
5. **Does a variadic argument need its own `ArgumentGUIInfo`?** Out of scope unless Phase 2 finds
   the single text field actively wrong.

## References

- `specs/issues/COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE.md` — the issue this closes
- `specs/issues/VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS.md` — open question 1
- `specs/design/excess-action-parameters-error/` — where this was split out at Phase 2, and which
  set the `~_` spelling this design reverts
- `specs/reference/REGISTER_COMMAND_FSD.md`, `specs/guides/COMMAND_REGISTRATION_GUIDE.md`
