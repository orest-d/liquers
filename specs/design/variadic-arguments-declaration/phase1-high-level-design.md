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
rejecting unknown identifiers instead of swallowing them. The macro also rejects a `multiple`
argument that is not last, which closes `VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS` for every
macro-registered command (decision 1).

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

No metadata change: `ArgumentGUIInfo` keeps describing **one element's** widget, and `multiple` tells
a renderer to repeat it as a list. That is the contract this design states in the reference.

No renderer changes, because there is none to change: `gui_info` has **zero consumers** outside
`liquers-core` — grepping `liquers-lib`, `liquers-web`, `liquers-axum` and `liquers-py` finds no
read of it. The one `multiple` mention in the UI (`liquers-lib/src/egui/widgets.rs:733`) is a
read-only registry *inspector* that prints a "multiple" badge, not an argument entry form. The
list editor with add / delete / reorder is therefore new UI work with no existing form to extend,
and is filed rather than built here (decision 5).

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
| `specs/issues/VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS.md` | Narrowed to hand-built metadata (decision 1), not closed |
| `specs/issues/UI-VARIADIC-ARGUMENT-LIST-EDITOR.md` | New, filed at Phase 2 (decision 5) |
| `specs/issues/COMMAND-COMPOSITE-VARIADIC-ARGUMENTS.md` | New, filed at Phase 2 (decision 5) |

**Audience and outcome.** A command author writing `register_command!`, and a coding agent adding
a polars-style command. After this lands they should be able to declare a variable-length argument
from the reference and the guide alone, and should find no document still claiming it is
impossible.

## Design Decisions

Resolved with the user after the first Phase 1 review. Each is settled unless Phase 2 finds a
contradiction in the code.

1. **`multiple` must be the last argument, enforced in the macro.** Rejecting the declaration at
   compile time closes `VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS` for every macro-registered
   command without needing a runtime caller for `CommandMetadata::check()`, and without waiting on
   `COMMAND-REGISTRY-ISSUE-NAMESPACE-NAME-SWAPPED`. **Injected arguments are exempt**: they consume
   no query parameter, so the rule is that no *non-injected* argument may follow a variadic one.
   The registry-level check stays filed as that issue, which this design narrows rather than closes
   — it still covers hand-built metadata such as `liquers-py`'s `argv`
   (`liquers-py/src/commands.rs:220`).

2. **Element types.** `Vec<String>` is the driver; the accessor is generic over
   `T: FromParameterValue<T>`, so `Vec<i64>`, `Vec<i32>`, `Vec<f64>`, `Vec<bool>` and the rest of
   the scalar family come with it at no extra cost.

3. **`ArgumentType` follows the element type** — `Vec<String>` → `String`, `Vec<i32>` → `Integer`,
   and so on. This matters beyond metadata cosmetics: `from_string` (`plan.rs:~600`) parses each
   action parameter *through* `ArgumentType`, so leaving it `Any` would deliver every element as a
   JSON string and `Vec<i32>` would fail at retrieval.

   **The `multiple` keyword stays mandatory, and `Vec<T>` alone does not imply it.** The user's
   instinct is right and the code confirms it: `impl<V: ValueInterface> FromParameterValue<Vec<V>>`
   (`commands.rs:269`) already reads a JSON array out of a *single* parameter, so `Vec<T>` is
   genuinely ambiguous between "one parameter carrying a list" and "the remaining parameters". A
   link argument resolving to an array is the second spelling the user anticipated, and it is the
   existing behaviour rather than a future one. Making `Vec` imply variadic would silently
   reinterpret it. Phase 2 should additionally make the macro reject `multiple` on a non-`Vec` type,
   so the keyword and the Rust type cannot disagree.

4. **A variadic argument takes no default; the implicit default is the empty list.** This matches
   Python's `*args` and C varargs, and it is *already* what the code does: `from_arginfo`
   (`plan.rs:504`) maps `CommandParameterValue::None` on a variadic argument to
   `MultipleParameters(vec![])`, and `pop_value` returns the same when no parameters remain. So core
   needs nothing; the DSL simply rejects `= <default>` on a `multiple` argument.

   Consequence for Phase 2: `pl/select_columns` with no parameters is now well-formed at plan level
   and reaches the command with an empty list. The command must decide — an explicit "at least one
   column" error is the likely answer, rather than letting Polars produce an empty frame.

5. **GUI: same element widgets, list rendering — filed, not built.** `ArgumentGUIInfo` needs no new
   variant; it describes one element and `multiple` means "render a list of these, with add, delete
   and reorder". Nothing in the workspace reads `gui_info` yet, so that renderer is new UI work
   spanning `lib/ui` and `web`, well outside closing this issue.

   The design precedent is the user's earlier prototype, `orest-d/egui-midi-test`
   (`src/editor.rs`, HEAD `ed3fb10`), which is the only working `gui_info` consumer anywhere. It
   confirms the element-widget model — `edit_query` (`:328`) matches on `param.info.gui_info` and
   draws one egui widget per argument — and it makes the variadic gap concrete in a way worth
   carrying into the filed issue, because **repeating the widget is the smaller half of the work**:

   - *The editor addresses parameters positionally, and assumes argument slot == parameter
     position.* `extract_editor_records` (`:283`) zips `action.parameters` against
     `action_info.arguments.get(parameter_number)` and errors with "Extra parameter N" when the
     query has more parameters than the command declares — the same failure shape as the plan
     builder's arity error. A variadic argument breaks the bijection: one `ArgumentInfo` owns a
     *range* of parameter positions, so the editor needs an argument→parameter-range mapping.
   - *Editing is a query rewrite, and the available operation is too weak.*
     `set_parameter_value` (`:245`) can only overwrite in place or append at the end. Delete and
     reorder need insert / remove / move on `ActionRequest::parameters`, which does not exist yet.
   - *Type-directed parameter lookup breaks too.* `find_numeric_parameter` (`:41`) counts
     parameters by `argument_type` to map MIDI controls onto them; with a variadic numeric argument
     that count no longer identifies a unique slot.

   None of this blocks the present design — it is entirely downstream of `gui_info` — but it means
   the list editor is a query-manipulation feature, not a widget feature, and the issue should say
   so. Two issues to file at Phase 2:

   | Issue | Covers |
   |---|---|
   | `UI-VARIADIC-ARGUMENT-LIST-EDITOR` | Rendering a variadic argument as an editable list: argument→parameter-range mapping, and insert / remove / move on an action's parameters. Cites `egui-midi-test/src/editor.rs` as the prototype and the three gaps above |
   | `COMMAND-COMPOSITE-VARIADIC-ARGUMENTS` | Future tuple / key-value element types, giving dictionary-shaped arguments, and the GUI they would need |

## Open Questions

1. Does `Vec<Value>` remain retrievable? It is currently unreachable *through the macro*
   (`arguments.get::<Vec<Value>>` fails the `TryFrom<E::Value>` bound), but the impl exists for
   hand-built registrations. Phase 2 must confirm the new accessor does not displace it.
2. Does `specs/command_registry.yaml` round-trip an `ArgumentType::String` argument carrying
   `multiple: true`? Both fields already serialize; the combination has never existed.

## References

- `specs/issues/COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE.md` — the issue this closes
- `specs/issues/VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS.md` — open question 1
- `specs/design/excess-action-parameters-error/` — where this was split out at Phase 2, and which
  set the `~_` spelling this design reverts
- `specs/reference/REGISTER_COMMAND_FSD.md`, `specs/guides/COMMAND_REGISTRATION_GUIDE.md`
- [`orest-d/egui-midi-test`](https://github.com/orest-d/egui-midi-test/blob/master/src/editor.rs) —
  prototype query/parameter editor; the only existing `gui_info` consumer, and the reference point
  for decision 5
