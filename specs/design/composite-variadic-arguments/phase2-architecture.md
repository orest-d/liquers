# Phase 2: Solution & Architecture — composite-variadic-arguments

Rewritten on 2026-11-24 for the wider scope of Phase 1. Written under the recommended answers to
Phase 1 questions 2 to 9; none of them is decided yet.

## Overview

A variadic argument declares a **shape** in its `argument_type`. The shape says how consecutive
parameters form one element:

| Shape | Declared as | Parameters per element | The command receives |
|---|---|---|---|
| Scalar list (today) | `Vec<T> multiple` | 1 | `Vec<T>` |
| Tuple list | `Vec<(A, B)> multiple`, or `Vec<P> multiple` with `P: FromParameterGroup` | 2 to 4 | `Vec<(A, B)>`, `Vec<P>` |
| Dictionary | `BTreeMap<String, V> multiple` | 2 (key, value) | `BTreeMap<String, V>` |

The query and the plan stay flat: the plan holds one `MultipleParameters` list, as today. Three
places learn the shape. The plan builder types parameter *k* by its position in the element and
checks counts and duplicate literal keys. Retrieval groups the flat list into elements. The
argument editor draws one row per element. `Scalar` is a new component type whose kind is read
from the text.

Rejected: a delimiter between elements in the query (a language change); nested
`MultipleParameters` (illegal in three places today); a `shape` field beside `argument_type` (two
fields that can disagree); a `key=value` string convention (untyped, nothing for a GUI); and
parser combinators as in the Python `liquer` (`ArgumentParser`, `+`, `is_argv`), which allow a
composite at any position but keep the shape in code, where the plan, the registry and a GUI
cannot see it.

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
pub enum ArgumentType {
    /* existing variants */
    /// A value whose kind is read from its text. Serialized as "scalar".
    Scalar,
    /// Element of a `multiple` argument: consecutive parameters form one element. 2..=4 scalar
    /// components. Serialized `{"Tuple": ["string", "int"]}`.
    Tuple(Vec<ArgumentType>),
    /// A `multiple` argument read as key and value pairs with unique keys.
    /// Serialized `{"Map": ["string", "scalar"]}`.
    Map(Box<ArgumentType>, Box<ArgumentType>),
}
impl ArgumentType {
    /// Parameters per element: Tuple(v) => v.len(), Map => 2, everything else => 1.
    pub fn arity(&self) -> usize;
    /// The type of component `k` of an element (k < arity).
    pub fn component(&self, k: usize) -> Option<&ArgumentType>;
}
pub struct ArgumentInfo {
    /* existing fields */
    /// Names of an element's components, for column headers. Empty when not given; then not
    /// serialized. When given, its length equals `argument_type.arity()`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<String>,
}

/// A value read from its text: `8`, `0.5`, `true`, `sidebar`. The text is kept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scalar { text: String, kind: ScalarKind }
pub enum ScalarKind { Integer(i64), Float(f64), Boolean(bool), String }
impl Scalar {
    pub fn parse(text: &str) -> Scalar;      // integer, then float, then true/false, else string
    pub fn as_str(&self) -> &str;            // always the original text
    pub fn kind(&self) -> &ScalarKind;
    pub fn to_json(&self) -> serde_json::Value;
}

// liquers-core/src/commands.rs
/// A fixed-size group of consecutive parameters, converted component-wise. Implemented for
/// tuples of 2 to 4 components; a command author implements it for an own type (AC-12).
pub trait FromParameterGroup: Sized {
    const ARITY: usize;
    /// The component types, for the metadata. Length ARITY.
    fn component_types() -> Vec<ArgumentType>;
    fn from_parameter_group(group: &[ParameterValue]) -> Result<Self, Error>;
}
impl FromParameterValue<Scalar> for Scalar { /* from a string or a JSON scalar */ }

impl<E: Environment> CommandArguments<E> {
    /// Like `get_multiple`, grouping the elements by `T::ARITY`. A remainder is an error naming
    /// the argument.
    pub fn get_multiple_grouped<T: FromParameterGroup>(&self, i: usize, name: &str)
        -> Result<Vec<T>, Error>;
    /// Key and value pairs as a map. A duplicate key is an error naming the key and the argument.
    pub fn get_multiple_map<V: FromParameterValue<V>>(&self, i: usize, name: &str)
        -> Result<BTreeMap<String, V>, Error>;
}
```

All synchronous. No new error type.

**Registration macro.** No new statement; the argument's Rust type selects the shape, and one
new per-argument key names the components:

```rust
register_command!(cr, fn rename_pairs(state,
    pairs: Vec<(String, String)> multiple (label: "Renames", columns: ["Old name", "New name"])
) -> result)?;
register_command!(cr, fn add(state, position_word: String, reference_word: String,
    layout: BTreeMap<String, Scalar> multiple) -> result)?;
register_command!(cr, fn curve(state, points: Vec<Point> multiple) -> result)?;
```

`Vec<(A, B, …)>` gives `Tuple([…])` and `get_multiple_grouped`; `BTreeMap<String, V>` gives
`Map(String, V's type)` and `get_multiple_map`; `Vec<P>` with `P` not a known scalar gives
`Tuple(P::component_types())`, built at registration.

Compile-time rejections: a tuple of fewer than 2 or more than 4 components; a `Vec`, `Option`
or tuple as a component; a tuple, map or `FromParameterGroup` type without `multiple`; a map
whose key is not `String`; a default on a shaped argument; `columns:` whose length differs from
the arity (checked at registration for a custom type, whose arity the macro cannot see).

## Integration Points

- `liquers-core/src/plan.rs` `ParameterValue::pop_value`, multiple branch: parse parameter *j*
  with `argument_type.component(j % n)`. After the loop, when every parameter was a literal:
  `count % n != 0` is a positioned error at the first parameter of the incomplete element (AC-3);
  for `Map`, a repeated literal key is a positioned error at the second one (AC-10). `from_string`
  gets a `Scalar` arm (a string value; the kind is read at retrieval) and explicit `Tuple` / `Map`
  arms that error.
- `liquers-core/src/command_metadata.rs` `ArgumentInfo::check`: `Tuple` and `Map` require
  `multiple`, scalar components and the arity bounds; `columns` empty or of length arity (AC-6).
- `liquers-core/src/recipes.rs` override conversion: a flat array, an array of arrays, and for a
  `Map` a JSON object (keys in sorted order); `argument_type_name` arms.
- `liquers-core/src/command_declaration.rs`: declared `Tuple`, `Map`, `scalar` and `columns`.
- `liquers-macro/src/registration.rs`: `variadic_element_type` also peels `BTreeMap<K, V>`; the
  element-type mapping, the extractor choice, `columns:` beside `label:` and `gui:`, and the
  rejections above.
- `liquers-mcp/src/schema.rs` `type_schema`: a tuple element is an array with `prefixItems`; a
  map is `{"type": "object", "additionalProperties": …}`; a scalar is
  `{"type": ["string", "number", "boolean"]}`. `argument_property` wraps every `multiple` argument in an array today; it must leave a `Map` unwrapped and keep the wrapper for scalar and tuple lists. `server.rs` converts an object argument to pairs.
- `liquers-lib/src/commands.rs`, `search/command_catalog.rs`, `egui/widgets.rs`,
  `liquers-py/src/command_metadata.rs`, `liquers-web/src/typescript.rs`: display arms.

**GUI: the argument editor.** The editor core (`liquers-lib/src/ui/editor/`) keeps the flat item
list it has for variadics; rows are a view of it.

```rust
// liquers-lib/src/ui/editor/tree.rs
pub struct EditorField {
    /* existing fields */
    /// One entry per component of an element; empty for a scalar list.
    pub components: Vec<ComponentField>,
    /// Keys must be unique (a dictionary).
    pub unique_keys: bool,
}
pub struct ComponentField { pub label: String, pub type_name: String, pub widget: ArgumentGUIInfo }
// liquers-lib/src/ui/editor/command.rs — EditorCommand gains
InsertRow { path: NodePath, arg: String, at: usize, texts: Vec<String> },
RemoveRow { path: NodePath, arg: String, at: usize },
MoveRow { path: NodePath, arg: String, from: usize, to: usize },
```

- `state.rs` fills `components` from `argument_type` and `columns`; a component's widget comes
  from its type through `effective_gui`. Row *r*, component *c* is item `r * n + c`, so `SetItem`
  edits a cell unchanged. The row commands change `n` items at once, so the query never holds a
  partial element.
- A new row lives in the editor's state until every cell is filled, then one `InsertRow` writes
  it (Phase 1 question 7). The view marks an incomplete row and a duplicate key.
- `liquers-lib/src/egui/editor.rs` `items_ui` and `liquers-lib/src/ui/web/editor.rs`: when
  `components` is not empty, a table with a header from the labels, one widget per cell, and
  add, remove and move controls per row. A `Scalar` cell is a text field that shows the kind read.

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
| Files likely to change | about 18 across core, macro, mcp, lib, py and web |
| Existing tests likely to change | none; scalar variadics keep their exact plans (AC-8) |
| New validation | Phase 3 tests; `bash scripts/check-build-matrix.sh` (egui and web arms are gated) |
| Compatibility / data | three new serde variants and one field: an older reader cannot read a registry that uses them |
| Scalar ambiguity | `1` meant as a string is read as an integer; `as_str` returns the text |
| Recovery | revert; no stored data uses the variants until a command adopts one |
| Certainty | medium: the tuple path fits the existing flat path; the scalar type, the custom trait and the row editor are new and unproven |
