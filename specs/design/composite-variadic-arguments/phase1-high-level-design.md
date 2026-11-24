# Phase 1: High-Level Design — composite-variadic-arguments

Rewritten on 2026-11-24. The first version (2026-10-20) covered tuple elements only. Scenarios
AC-1 to AC-8 keep their ids; AC-9 onward are new. Scope changes are listed at the end.

## Purpose

A variadic argument can hold only a flat list of scalars. A command that needs something richer
in that position, a list of pairs, a dictionary or a small structured value, has to take
`Vec<String>` and decode it by hand, and then neither the plan, nor the registry, nor a GUI knows
the shape. Let a variadic argument **declare the complex value it carries**, so that the plan
checks it, the command receives it typed, and an editor shows it as rows. The query language does
not change: the value is still written as dash-separated parameters.

## Problem Example

Verified at HEAD on 2026-11-24. Two cases.

**Pairs.** A command author can write only this today:

```rust
fn rename_pairs(state: &State<Value>, names: Vec<String>) -> Result<Value, Error> {
    if names.len() % 2 != 0 { return Err(/* hand-written */); }
    for pair in names.chunks(2) { /* old = pair[0], new = pair[1] */ }
}
register_command!(cr, fn rename_pairs(state, names: Vec<String> multiple) -> result)?;
```

`rename_pairs-a-x-b-y` works, but the metadata says "a list of strings", `rename_pairs-a-x-b`
fails only inside the command, and `pairs: Vec<(String, String)> multiple` does not compile.

**A dictionary of scalars.** The UI redesign (`design/ui-redesign/master-design.md` §10.9) places
a child element with layout data that only its parent interprets:

```
ns-lui/add-child-current-row-0-col-8-colspan-4
```

Today `liquers-validate` answers `Too many parameters for command 'add': accepts 2, but parameter
#3 'row' was supplied`. Wanted: `add` declares `layout: BTreeMap<String, Scalar> multiple` and
receives `{row: 0, col: 8, colspan: 4}` with `0`, `8` and `4` as integers; `…-row-0-row-1` and
`…-row` fail at plan time, positioned at the offending parameter.

## Scope and Acceptance Criteria

- **AC-1** Pairs from a flat query
  WHEN `pairs: Vec<(String, String)> multiple` receives `rename_pairs-a-x-b-y`
  THEN the command gets `[("a","x"), ("b","y")]`
- **AC-2** Component types are checked per position
  WHEN `weights: Vec<(String, i64)> multiple` receives `w-a-1-b-x`
  THEN planning fails with a conversion error positioned at `x`
- **AC-3** An incomplete element is a plan error
  WHEN the number of literal parameters is not a multiple of the element's arity
  THEN planning fails, positioned at the first parameter of the incomplete element, and the message
  names the argument and the arity
- **AC-4** Empty is allowed
  WHEN no parameter is given
  THEN the command gets an empty list or an empty dictionary
- **AC-5** Metadata shows the shape
  WHEN the command's metadata is exported or serialized
  THEN the argument's type is `{"Tuple": ["string", "string"]}` with `multiple: true`, a
  dictionary is `{"Map": ["string", "scalar"]}`, and both deserialize back unchanged
- **AC-6** Misdeclarations are refused
  WHEN an element has fewer than 2 or more than 4 components, a component is not a scalar type
  (`Vec`, `Option`, tuple), or a `Tuple` or `Map` type appears on an argument that is not `multiple`
  THEN the macro refuses it at compile time, and `CommandMetadata::check` refuses hand-built metadata
- **AC-7** Links and overrides
  WHEN an element parameter is a link, or a recipe overrides the argument with JSON
  THEN elements are grouped after links are materialised (and expanded, per
  `variadic-link-array-expansion`). A flat array or an array of arrays both work as an override;
  a dictionary also accepts a JSON object. An incomplete group is then a run-time error naming
  the argument
- **AC-8** Existing variadics unchanged
  WHEN a scalar `Vec<T> multiple` argument is used
  THEN plans, retrieval and the exported registry are unchanged
- **AC-9** A dictionary from key and value pairs
  WHEN `layout: BTreeMap<String, Scalar> multiple` receives `add-child-current-row-0-col-8`
  THEN the command gets a map with `row` and `col`
- **AC-10** Duplicate keys are refused
  WHEN a dictionary argument receives the same literal key twice
  THEN planning fails, positioned at the second key, naming the key and the argument
- **AC-11** A scalar is read from its text
  WHEN a component of type `Scalar` receives `8`, `0.5`, `true` or `sidebar`
  THEN it is an integer, a float, a boolean or a string, and its original text stays available
- **AC-12** A command author's own element type
  WHEN a type implements the element trait with arity 2 and is declared as `Vec<Point> multiple`
  THEN `curve-0-0-1-2.5` gives two points, and the metadata shows a tuple of two floats
- **AC-13** The argument editor shows rows
  WHEN an action with a tuple or dictionary argument is opened in the query editor
  THEN each element is one row with one widget per component, rows can be added, removed and
  reordered, an incomplete row and a duplicate key are marked, and the edited query equals the one
  typed by hand
- **AC-14** Component labels
  WHEN the declaration names the components (`columns: ["Old name", "New name"]`)
  THEN the metadata carries the names and the editor uses them as column headers

Non-goals: new query syntax or a delimiter between elements; a composite value in a non-variadic
position (`origin: (f64, f64)` between other arguments, as the Python `liquer` parsers allowed);
nested shapes (a tuple of tuples, a dictionary of lists); `Option` or enum components; named
struct fields in the query.

## Core Interactions

Query: no syntax change. Plan: parameters are typed by their position in the element, counts and
duplicate keys are checked. Commands: two new argument types, grouped retrieval, a trait for
custom types. Macro: tuple, map and custom element types. Recipes: override shapes. MCP: tool
schema. UI: the argument editor (`liquers-lib/src/ui/editor/`, egui view) draws rows.

## Crate Placement

`liquers-core` (types, plan, retrieval, recipes, check), `liquers-macro` (DSL), `liquers-mcp`
(schema), `liquers-lib` (display arms; editor core and its egui view). Within the dependency flow.

## Language Integration

`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` `COMMAND` already covers variadics (`COMMAND04`);
it gains one sentence and no new feature id: a declared element shape is carried in the metadata.

- **Python (`liquers-py`): later.** Its metadata classes must compile with the new variants
  (in scope, display only). Declaring a shape from a Python decorator is a follow-up issue, to be
  filed in Phase 2's preflight under `COMMAND`.
- **JavaScript (`liquers-web`): later.** The TypeScript declarations get the new variants (in
  scope); declaring a shape from JavaScript is the same follow-up.

## Documentation Intent

- Reference: extend `reference/REGISTER_COMMAND_FSD.md` §Variadic Parameters and §Supported Types.
- Guide: extend `guides/COMMAND_REGISTRATION_GUIDE.md` §"Accepting a variable number of parameters".
- Other documents: `reference/COMMAND_DECLARATION.md` (declared `Tuple` and `Map` types);
  `reference/RECIPES_AND_PLANS.md` (override shapes).
- API docs: `liquers-core/src/commands.rs`, `liquers-core/src/command_metadata.rs`,
  `liquers-lib/src/ui/editor/mod.rs`.
- User guides: the commands chapter of `docs/guides/rust/`; none for Python and web until their
  integration lands.
- Documents to update: the four above; the `specs/command_registry.yaml` changelog when the first
  shipped command adopts a shape.

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. It adds `ArgumentType` variants, public traits and
  `register_command!` syntax across four crates.
- **Leading issue:** **Open design - the dictionary's value type** (question 3): it decides
  whether a new scalar type enters `liquers-core`.
- **Explanation:** the mechanism is known and fits the existing flat path. The questions below
  each change a serialized format or a public type, so each is a maintainer decision.
- **Open questions:**
  1. **Decided 2026-11-24 - whether to build it.** Orest asked for it; the UI layout data is the
     first user. The `key=value` string convention is not chosen: it cannot type values and gives
     a GUI nothing.
  2. **Open design - layering.** Recommended: grouping is a metadata and retrieval concept. The
     query stays flat and the plan keeps one flat `MultipleParameters` list.
  3. **Open design - the scalar.** Recommended: a small `Scalar` type in `liquers-core` that keeps
     the original text and its inferred kind (integer, float, boolean, string). Alternatives: the
     environment's `Value` (richer, but ties a dictionary to the value type), or strings only
     (the command converts). A string that looks like a number cannot be forced to stay a string;
     the kept text is the answer: a consumer that wants a string reads the text.
  4. **Open design - representation.** Recommended: `ArgumentType::Tuple(Vec<ArgumentType>)` and
     `ArgumentType::Map(Box<ArgumentType>, Box<ArgumentType>)`, scalar components only.
     Alternative: a `shape` field on `ArgumentInfo`, which avoids new `match` arms but lets two
     fields disagree.
  5. **Open design - which map types.** Recommended: `BTreeMap<String, V>` only (sorted, stable
     output). `HashMap` and an order-preserving map can follow. Should keys other than `String`
     be allowed?
  6. **Open design - the custom-type trait.** Recommended: one element trait with a fixed arity
     and declared component types (AC-12). A second trait for a whole variadic value of unbounded
     length is possible; is it wanted now, or only when a type needs it?
  7. **Open design - the editor's incomplete row.** A new row has empty cells. Recommended: the
     editor keeps the row in its own state and writes it to the query only when it is complete.
     Alternative: write empty parameters and mark the row.
  8. **Open design - per-component widgets.** Recommended: the widget follows from the component
     type; `columns:` gives labels (AC-14). Per-component `gui:` hints are left for later.
  9. **Proposed resolution - arity bounds.** 2 to 4 components.
  10. **Open - the non-variadic position.** A fixed-size composite between other arguments is a
      non-goal here. It would change how arguments count parameters. Is it wanted as a follow-up?

## Design Dependencies

- `variadic-link-array-expansion` — **requires** (weak): AC-7 groups after expansion.
- `ui-redesign` — `required-by`: master design §10.9 uses AC-9 to AC-11 for layout data; until
  this ships, its commands take `Vec<String>` and pair the elements themselves.
- `UI-VARIADIC-ARGUMENT-LIST-EDITOR` (closed): the list editor AC-13 extends
  (`FieldValue::Items` in `liquers-lib/src/ui/editor/tree.rs`).
- `variadic-arguments-declaration` (complete): origin. Its flat-list contract is kept (AC-8).

## Scope Changes

- 2026-11-24: dictionaries (AC-9, AC-10), the scalar (AC-11), custom element types (AC-12), GUI
  support (AC-13, AC-14) and language integration decisions added. Map types and the row editor
  were non-goals before. AC-4, AC-5, AC-6 and AC-7 were widened to dictionaries.

## References

- Issue `COMMAND-COMPOSITE-VARIADIC-ARGUMENTS`, update of 2026-11-24 (the discussion).
- Python `liquer`, `liquer/commands.py`: `ArgumentParser`, `SequenceArgumentParser`.
- Exhaustive `ArgumentType` matches exist in 9 files (core: `plan.rs`, `recipes.rs`,
  `command_metadata.rs`, `command_declaration.rs`; `liquers-macro`; `liquers-lib`: `commands.rs`,
  `search/command_catalog.rs`, `egui/widgets.rs`; `liquers-mcp`: `schema.rs`, `server.rs`), plus
  `liquers-py/src/command_metadata.rs` and `liquers-web/src/typescript.rs`. Each gets explicit
  arms. No `_ =>` is added.
