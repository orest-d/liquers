# Phase 1: High-Level Design — composite-variadic-arguments

Rewritten on 2026-11-24 and revised on 2026-11-25 after Orest's review. The first version
(2026-10-20) covered tuple elements only. Scenarios AC-1 to AC-8 keep their ids; AC-9 onward are
new; AC-11 is withdrawn. Scope changes are listed at the end.

## Purpose

A variadic argument can hold only a flat list of scalars. A command that needs something richer
in that position, a list of pairs, a dictionary or a JSON value, has to take
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
#3 'row' was supplied`. Wanted: `add` declares
`layout: BTreeMap<String, serde_json::Value> multiple` and receives
`{row: 0, col: 8, colspan: 4}` with `0`, `8` and `4` as numbers; `…-row-0-row-1` and
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
  THEN the argument has `multiple: true` and a `shape`: `{"tuples": ["string", "string"]}`,
  `"map"` or `"json"`; a plain list has no `shape`; all deserialize back unchanged
- **AC-6** Misdeclarations are refused
  WHEN an element has fewer than 2 or more than 4 components, a component is not a scalar type
  (`Vec`, `Option`, tuple), or a shape appears on an argument that is not `multiple`
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
  WHEN `layout: BTreeMap<String, serde_json::Value> multiple` receives
  `add-child-current-row-0-col-8-role-sidebar`
  THEN the command gets a map with `row: 0`, `col: 8` and `role: "sidebar"`. A parameter that
  reads as a number is a number, anything else is a string; there are no booleans, because
  `true` cannot be told from a string. `BTreeMap<String, String>` gets every value as text
- **AC-10** Duplicate keys are refused
  WHEN a dictionary argument receives the same literal key twice
  THEN planning fails, positioned at the second key, naming the key and the argument
- **AC-11** Withdrawn 2026-11-25: there is no separate `Scalar` type
  WHEN a dictionary value or a JSON value is read from a parameter
  THEN it is the JSON scalar of AC-9, a number or a string, and no other scalar type exists
- **AC-12** A command author's own element type
  WHEN a type implements the element trait with arity 2 and is declared as `Vec<Point> multiple`
  THEN `curve-0-0-1-2.5` gives two points, and the metadata shows a tuple of two floats
- **AC-13** The argument editor shows the shape
  WHEN an action with a shaped argument is opened in the query editor
  THEN tuples are rows with one widget per component; a dictionary is a list of pairs where a
  pair is added with its key and value together, so an incomplete pair cannot be written, and a
  duplicate key is refused; a JSON value has a choice of none, scalar, list or dictionary; the
  edited query equals the one typed by hand
- **AC-14** Component labels
  WHEN the declaration names the components (`columns: ["Old name", "New name"]`)
  THEN the metadata carries the names and the editor uses them as column headers

- **AC-15** A JSON value from the parameters
  WHEN `data: serde_json::Value multiple` receives its parameters
  THEN none gives `null`; one parameter gives a number or a string; a sequence starting with a
  number gives a list; a sequence starting with an empty string gives the rest as a list; a
  sequence starting with any other string gives a dictionary of key and value pairs
- **AC-16** JSON value errors
  WHEN the pairs of AC-15 are incomplete or repeat a literal key
  THEN planning fails as in AC-3 and AC-10; when the first parameter is a link, the shape is
  decided, and the same errors raised, at run time
- **AC-17** A JSON object
  WHEN `layout: serde_json::Map<String, serde_json::Value> multiple` is declared
  THEN it behaves as the dictionary of AC-9

Non-goals: new query syntax or a delimiter between elements; **a composite value in a
non-variadic position** (Orest, 2026-11-25: not allowed); nested shapes (a list of dictionaries, a
dictionary of lists); booleans in a JSON value; `Option` or enum components; named struct fields.

## Core Interactions

Query: no syntax change. Plan: parameters are typed by their position in the element, counts and
duplicate keys are checked. Commands: a shape on a variadic argument, grouped retrieval, a trait
for custom types. Macro: tuple, map, JSON and custom element types. Recipes: override shapes. MCP: tool
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
- **Automatic fixing:** not-eligible. It adds fields to `ArgumentInfo`, public traits and
  `register_command!` syntax across four crates.
- **Leading issue:** **Open design - one empty string as a JSON value** (question 4).
- **Explanation:** the mechanism is known and fits the existing flat path. Orest decided most
  questions on 2026-11-25; the rest change a serialized format or a public type.
- **Decided:**
  1. **Build it** (2026-11-24). The UI layout data is the first user. A `key=value` string
     convention is not chosen.
  2. **No separate `Scalar` type** (2026-11-25). A dictionary's values are JSON scalars or
     strings.
  3. **The shape is not an `ArgumentType`** (2026-11-25): it only makes sense on a variadic
     argument.
  4. **Value types** (2026-11-25): string values, JSON values, and a JSON object.
  5. **The editor cannot write an incomplete pair** (2026-11-25): a dictionary editor takes key
     and value together.
  6. **No composite in a non-variadic position** (2026-11-25).
  7. **A JSON value type** with the five rules of AC-15 (2026-11-25, Orest's proposal).
- **Open questions:**
  1. **Open design - where the shape is stored.** Recommended: a `shape` field on `ArgumentInfo`
     beside `multiple`, valid only when `multiple` is true, omitted for a plain list. Then
     `argument_type` keeps its meaning, the type of one element or one dictionary value, and
     existing registries read unchanged. Alternative: turn `multiple` itself into the shape.
  2. **Open design - layering.** Recommended: grouping is a metadata and retrieval concept. The
     query stays flat and the plan keeps one flat `MultipleParameters` list.
  3. **Open design - which map types.** Recommended: `BTreeMap<String, V>` (sorted, stable
     output) and `serde_json::Map`. `HashMap` can follow.
  4. **Open design - one empty string as a JSON value.** It matches two rules of AC-15: a scalar
     empty string, or the list marker with nothing after it. Recommended: the empty list, since
     nothing else can write one; an empty string scalar is then not expressible.
  5. **Accepted limit to confirm - numeric keys.** In a JSON value `1-a` is the list `[1, "a"]`,
     never `{"1": "a"}`. A declared dictionary has no such limit.
  6. **Open design - the custom-type trait.** Recommended: one element trait with a fixed arity
     and declared component types (AC-12).
  7. **Open design - per-component widgets.** Recommended: the widget follows from the component
     type; `columns:` gives labels (AC-14).
  8. **Proposed resolution - arity bounds.** 2 to 4 components in a tuple.

## Design Dependencies

- `variadic-link-array-expansion` — **requires** (weak): AC-7 groups after expansion.
- `ui-redesign` — `required-by`: master design §10.9 uses AC-9 and AC-10 for layout data; until
  this ships, its commands take `Vec<String>` and pair the elements themselves.
- `UI-VARIADIC-ARGUMENT-LIST-EDITOR` (closed): the list editor AC-13 extends
  (`FieldValue::Items` in `liquers-lib/src/ui/editor/tree.rs`).
- `variadic-arguments-declaration` (complete): origin. Its flat-list contract is kept (AC-8).

## Scope Changes

- 2026-11-24: dictionaries (AC-9, AC-10), the scalar (AC-11), custom element types (AC-12), GUI
  support (AC-13, AC-14) and language integration decisions added. Map types and the row editor
  were non-goals before. AC-4, AC-5, AC-6 and AC-7 were widened to dictionaries.
- 2026-11-25 (Orest's review): the `Scalar` type withdrawn (AC-11); the shape moved from
  `ArgumentType` to a field of the argument (AC-5); the JSON value (AC-15, AC-16) and the JSON
  object (AC-17) added; the dictionary editor of AC-13 made unable to write an incomplete pair.

## References

- Issue `COMMAND-COMPOSITE-VARIADIC-ARGUMENTS`, update of 2026-11-24 (the discussion).
- Python `liquer`, `liquer/commands.py`: `ArgumentParser`, `SequenceArgumentParser`.
- `ArgumentType` is not changed, so its exhaustive matches (9 files) are untouched. The places
  that display an argument show the shape beside "multiple".
