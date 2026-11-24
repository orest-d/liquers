# Phase 2: Solution & Architecture — composite-variadic-arguments

Rewritten on 2026-11-24 for the wider scope of Phase 1 and revised on 2026-11-25 after Orest's
review. Written under the recommended answers to Phase 1's open questions.

## Overview

A variadic argument declares a **shape**: how its dash-separated parameters form one value. The
shape is a field of the argument, valid only with `multiple`; it is not an argument type
(Orest, 2026-11-25).

| Shape | Declared as | The command receives |
|---|---|---|
| `list` (today, the default) | `Vec<T> multiple` | `Vec<T>` |
| `tuples` | `Vec<(A, B)> multiple`, or `Vec<P> multiple` with `P: FromParameterGroup` | `Vec<(A, B)>`, `Vec<P>` |
| `map` | `BTreeMap<String, V> multiple`, `serde_json::Map<String, Value> multiple` | a map with unique keys |
| `json` | `serde_json::Value multiple` | null, a scalar, a list or an object |

`argument_type` keeps its meaning: the type of one element (`list`) or of one value (`map`). A
value of type `any` is a **JSON scalar**: a number when the text reads as one, otherwise a string.
There are no booleans.

The `json` shape is decided by the data (Orest's rules):

| Parameters | Value |
|---|---|
| none | `null` |
| one | a number or a string |
| a sequence starting with a number | a list |
| a sequence starting with an empty string | the rest, as a list |
| a sequence starting with any other string | key and value pairs: an object |

One empty string alone is the empty list (Phase 1 question 4).

The query and the plan stay flat: the plan holds one `MultipleParameters` list, as today. The
plan builder types the parameters and checks counts and duplicate literal keys, retrieval builds
the value, and the argument editor draws the shape.

Rejected: a delimiter between elements in the query (a language change); nested
`MultipleParameters` (illegal in three places today); new `ArgumentType` variants for shapes (a
shape is only valid on a variadic argument); a separate `Scalar` type (the JSON scalar covers it);
a `key=value` string convention (untyped, nothing for a GUI); and parser combinators as in the
Python `liquer` (`ArgumentParser`, `+`, `is_argv`), which allow a composite at any position but
keep the shape in code, where the plan, the registry and a GUI cannot see it.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `LINK-IN-VARIADIC-DOES-NOT-EXPAND` | draft | P3 | element count with links (AC-7) | Preferably | No | designed in `variadic-link-array-expansion` |
| `PY-COMMAND-DECORATOR-CANNOT-DECLARE-ARGUMENT-METADATA` | draft | P3 | Python cannot declare a shape | No | No | the "later" decision of Phase 1 joins it |

Namespaces involved: none changed here. The first adopter is `lui` (`add`, a new `layout`), in
the UI redesign.

## Interfaces

```rust
// liquers-core/src/command_metadata.rs
/// How the parameters of a `multiple` argument form its value.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum VariadicShape {
    /// One element per parameter, of `argument_type`.
    #[default]
    List,
    /// Consecutive parameters form one element; 2..=4 scalar component types.
    Tuples(Vec<ArgumentType>),
    /// Key and value pairs with unique string keys; values are of `argument_type`.
    Map,
    /// A JSON value, its form decided by the parameters.
    Json,
}
impl VariadicShape {
    pub fn is_list(&self) -> bool;
    /// Parameters per element: Tuples(v) => v.len(), Map => 2, List and Json => 1.
    pub fn arity(&self) -> usize;
}
pub struct ArgumentInfo {
    /* existing fields */
    /// Only with `multiple`. Omitted when `List`, so existing registries are unchanged.
    #[serde(default, skip_serializing_if = "VariadicShape::is_list")]
    pub shape: VariadicShape,
    /// Names of a tuple's components, for column headers. Empty or of length `shape.arity()`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<String>,
}

// liquers-core/src/commands.rs
/// A fixed-size group of consecutive parameters, converted component-wise. Implemented for
/// tuples of 2 to 4 components; a command author implements it for an own type (AC-12).
pub trait FromParameterGroup: Sized {
    const ARITY: usize;
    fn component_types() -> Vec<ArgumentType>;
    fn from_parameter_group(group: &[ParameterValue]) -> Result<Self, Error>;
}
/// A number when the text reads as one (integer, then float), otherwise a string.
pub fn json_scalar(text: &str) -> serde_json::Value;

impl<E: Environment> CommandArguments<E> {
    pub fn get_multiple_grouped<T: FromParameterGroup>(&self, i: usize, name: &str)
        -> Result<Vec<T>, Error>;
    pub fn get_multiple_map<V: FromParameterValue<V>>(&self, i: usize, name: &str)
        -> Result<BTreeMap<String, V>, Error>;
    /// The five rules of the `json` shape.
    pub fn get_multiple_json(&self, i: usize, name: &str) -> Result<serde_json::Value, Error>;
}
impl FromParameterValue<serde_json::Value> for serde_json::Value { /* json_scalar of a string */ }
```

All synchronous. No new error type. A remainder or a duplicate key is an error naming the
argument.

**Registration macro.** No new statement; the argument's Rust type selects the shape, and one
new per-argument key names a tuple's components:

```rust
register_command!(cr, fn rename_pairs(state,
    pairs: Vec<(String, String)> multiple (label: "Renames", columns: ["Old name", "New name"])
) -> result)?;
register_command!(cr, fn add(state, position_word: String, reference_word: String,
    layout: BTreeMap<String, serde_json::Value> multiple) -> result)?;
register_command!(cr, fn set_data(state, data: serde_json::Value multiple) -> result)?;
```

| Declared type | `shape` | `argument_type` | Extractor |
|---|---|---|---|
| `Vec<(A, B, …)>` | `tuples` of the component types | `any` | `get_multiple_grouped` |
| `Vec<P>`, `P` not a known scalar | `tuples` of `P::component_types()`, at registration | `any` | `get_multiple_grouped::<P>` |
| `BTreeMap<String, V>` | `map` | from `V` (`String` → `string`, `serde_json::Value` → `any`) | `get_multiple_map` |
| `serde_json::Map<String, serde_json::Value>` | `map` | `any` | `get_multiple_map`, collected |
| `serde_json::Value` | `json` | `any` | `get_multiple_json` |

Compile-time rejections: a tuple of fewer than 2 or more than 4 components; a `Vec`, `Option`
or tuple as a component; a tuple, map, JSON or `FromParameterGroup` type without `multiple` (so
no composite in a non-variadic position); a map whose key is not `String`; a default on a shaped
argument; `columns:` on a shape other than tuples, or of the wrong length (checked at
registration for a custom type, whose arity the macro cannot see).

## Integration Points

- `liquers-core/src/plan.rs` `ParameterValue::pop_value`, multiple branch: for `tuples`, parse
  parameter *j* with component `j % n`; for `map`, even positions are keys and odd positions are
  parsed with `argument_type`. When every parameter is a literal: an incomplete element is a
  positioned error at its first parameter (AC-3), and a repeated key a positioned error at the
  second one (AC-10). For `json`, the same two checks apply when the first parameter is a
  non-empty literal that is not a number (AC-16); otherwise nothing is checked at plan time.
- `liquers-core/src/command_metadata.rs` `ArgumentInfo::check`: a shape other than `list`
  requires `multiple`; `tuples` requires 2 to 4 scalar components; `columns` is empty or of the
  shape's arity (AC-6). `ArgumentType` is not changed.
- `liquers-core/src/recipes.rs` override conversion: `tuples` accepts a flat array or an array of
  arrays; `map` accepts a JSON object or a flat array of pairs; `json` accepts any JSON value
  without nested containers.
- `liquers-core/src/command_declaration.rs`: a declared `shape` and `columns`.
- `liquers-macro/src/registration.rs`: `variadic_element_type` also recognises `BTreeMap<K, V>`,
  `serde_json::Map` and `serde_json::Value`; the table above; `columns:` beside `label:` and
  `gui:`; the rejections.
- `liquers-mcp/src/schema.rs`: `argument_property` wraps every `multiple` argument in an array
  today. It keeps doing so for `list`; `tuples` is an array of `prefixItems` arrays; `map` is
  `{"type": "object", "additionalProperties": …}`, not wrapped; `json` is unconstrained, not
  wrapped. `server.rs` turns an object into pairs and a list into parameters, adding the
  empty-string marker when a list does not start with a number.
- Display of an argument (`liquers-lib/src/commands.rs`, `search/command_catalog.rs`,
  `egui/widgets.rs`, `liquers-py/src/command_metadata.rs`, `liquers-web/src/typescript.rs`): show
  the shape beside "multiple". No exhaustive `ArgumentType` match changes.

**GUI: the argument editor.** The editor core (`liquers-lib/src/ui/editor/`) keeps the flat item
list it has for variadics (`FieldValue::Items`); a shape is a view of it.

```rust
// liquers-lib/src/ui/editor/tree.rs
pub struct EditorField {
    /* existing fields */
    pub shape: VariadicShape,
    /// One entry per component of an element (tuples: the components; map: key and value).
    pub components: Vec<ComponentField>,
}
pub struct ComponentField { pub label: String, pub type_name: String, pub widget: ArgumentGUIInfo }
// liquers-lib/src/ui/editor/command.rs — EditorCommand gains
InsertRow { path: NodePath, arg: String, at: usize, texts: Vec<String> },
RemoveRow { path: NodePath, arg: String, at: usize },
MoveRow { path: NodePath, arg: String, from: usize, to: usize },
SetItems { path: NodePath, arg: String, texts: Vec<String> },   // replace the whole value
```

- Row *r*, component *c* is item `r * n + c`, so `SetItem` edits a cell unchanged. The row
  commands change `n` items at once, so the query never holds a partial element.
- **Dictionary**: existing pairs are listed with their key fixed and their value editable. A
  pair is added through one form that takes key and value together and sends one `InsertRow`; a
  duplicate key is refused there. An incomplete pair cannot be written (Orest, 2026-11-25).
- **JSON value**: a choice of none, scalar, list or dictionary, then the matching editor. The
  editor writes the empty-string marker itself when a list does not start with a number, and
  refuses a dictionary key that reads as a number. Changing the form sends one `SetItems`.
- Views: `liquers-lib/src/egui/editor.rs` `items_ui` and `liquers-lib/src/ui/web/editor.rs`.

## Error Handling

- Component conversion at plan time: the existing `from_string` errors, positioned (AC-2).
- Incomplete element at plan time: `Error::conversion_error_with_message(…).with_position(pos)`
  naming the argument and the arity (AC-3).
- Duplicate literal key at plan time: the same constructor, positioned at the second key (AC-10).
- Remainder or duplicate key at run time (links, overrides): `Error::general_error(…)` with the
  action position, naming the argument (AC-7).
- Bad metadata: an issue from `ArgumentInfo::check` (AC-6).

## Relevant Commands

None added or changed by this design. Namespaces untouched. The first adopter (`ns-lui/add` with
layout data) regenerates `specs/command_registry.yaml` in its own design.

## Documentation Architecture

`reference/REGISTER_COMMAND_FSD.md` (§Supported Types, §Variadic Parameters, the `columns:` key,
the rejection table), `guides/COMMAND_REGISTRATION_GUIDE.md` (§Accepting a variable number of
parameters: "Elements made of several parameters", "A dictionary", "Your own element type"),
`reference/COMMAND_DECLARATION.md`, `reference/RECIPES_AND_PLANS.md`. Module docs:
`liquers-core/src/commands.rs`, `liquers-core/src/command_metadata.rs`,
`liquers-lib/src/ui/editor/mod.rs`. User guide: the commands chapter of `docs/guides/rust/`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | about 15 across core, macro, mcp, lib, py and web |
| Existing tests likely to change | none; scalar variadics keep their exact plans (AC-8) |
| New validation | Phase 3 tests; `bash scripts/check-build-matrix.sh` (egui and web arms are gated) |
| Compatibility / data | two new optional fields on an argument; `ArgumentType` is unchanged, so an older reader fails only on a registry that uses a shape |
| JSON value ambiguity | the form depends on the first parameter: a numeric key, a boolean and an empty string scalar cannot be written; a link in first place defers every check to run time |
| Recovery | revert; no stored data uses the variants until a command adopts one |
| Certainty | medium: the tuple and map paths fit the existing flat path; the JSON rules, the custom trait and the shaped editor are new and unproven |
