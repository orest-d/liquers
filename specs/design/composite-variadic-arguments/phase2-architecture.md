# Phase 2: Solution & Architecture — composite-variadic-arguments

## Overview

Written under the recommended answers to Phase 1 questions 2-4. A variadic argument whose
`argument_type` is `Tuple([T1, …, Tn])` consumes parameters exactly like a scalar variadic. The
difference is that parameter *k* is parsed as `T(k mod n)`. The plan keeps a flat
`MultipleParameters`. Retrieval groups the flat elements into `n`-tuples. The macro maps
`Vec<(A, B, …)> multiple` to that type and to grouped retrieval.

Rejected: a query delimiter for elements (language change); nested `MultipleParameters` (illegal
in three places today, and it would make every consumer handle depth); an `element_types` field
(question 3); grouping only inside the command (today's workaround).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `LINK-IN-VARIADIC-DOES-NOT-EXPAND` | draft | P3 | element count with links (AC-7) | Preferably | No | designed in `variadic-link-array-expansion` |
| `UI-VARIADIC-ARGUMENT-LIST-EDITOR` | draft | P3 | consumer of the shape | No | No | none |
| `REGISTRY-QUERY-DEFAULT-NOT-WRITABLE-AS-TEXT` | draft | P3 | same serialized registry, different field | No | No | none |

## Interfaces

```rust
// liquers-core/src/command_metadata.rs
pub enum ArgumentType {
    /* existing variants */
    /// Element type of a `multiple` argument: consecutive parameters form one element.
    /// Components are scalar types (not Enum, GlobalEnum, Tuple, Any, None); 2..=4 of them.
    /// Serialized like `Enum`: `{"Tuple": ["string", "int"]}`.
    Tuple(Vec<ArgumentType>),
}
impl ArgumentType {
    pub fn arity(&self) -> usize;          // Tuple(v) => v.len(), everything else => 1
}

// liquers-core/src/commands.rs
/// A fixed-size group of consecutive parameters, converted component-wise.
pub trait FromParameterGroup: Sized {
    const ARITY: usize;
    fn from_parameter_group(group: &[ParameterValue]) -> Result<Self, Error>;
}
// impls for (A, B), (A, B, C), (A, B, C, D) where each component: FromParameterValue<Self>

impl<E: Environment> CommandArguments<E> {
    /// Like `get_multiple`, grouping the elements by `T::ARITY`. A remainder is an error
    /// naming the argument (the run-time arity check of AC-7).
    pub fn get_multiple_grouped<T: FromParameterGroup>(&self, i: usize, name: &str)
        -> Result<Vec<T>, Error>;
}
```

Macro DSL: no new statement. `pairs: Vec<(String, String)> multiple` is recognised by extending
`variadic_element_type`'s caller. A `syn::Type::Tuple` element yields
`ArgumentType::Tuple(vec![…component types…])` and the extractor
`arguments.get_multiple_grouped(i, "pairs")?`. All sync.

## Integration Points

- `liquers-core/src/plan.rs` `ParameterValue::pop_value`, multiple branch: build one element
  `ArgumentInfo` per component (`argument_type = T_k`) and parse parameter *j* with component
  `j % n`. After the loop, when every parameter was a string and `count % n != 0`, return
  `Error::conversion_error_at_position`-style positioned error at the first parameter of the
  incomplete group. `from_string` gets an explicit `Tuple` arm that errors (a tuple on a scalar
  position cannot happen after `check`).
- `liquers-core/src/command_metadata.rs` `ArgumentInfo::check`: `Tuple` requires `multiple` and
  2-4 scalar components, else `Error`. `ArgumentType::is_option` gets an explicit `Tuple` arm.
- `liquers-core/src/recipes.rs` override conversion: for a `Tuple` variadic, accept a flat array or
  an array of arrays (each inner of length `n`), flatten one level, and convert entry *j* with
  component `j % n`. `argument_type_name` gets a `tuple(string, int)` arm.
- `liquers-core/src/command_declaration.rs`: the declared-type match accepts `{"Tuple": […]}`
  (serde already does) and names it in messages.
- `liquers-core/src/interpreter.rs`: no change. Expansion (other design) happens before grouping.
- `liquers-macro/src/registration.rs`: the element-type to `ArgumentType` mapping, the extractor
  choice, and four compile-time rejections (arity, non-scalar component, `Option` component, tuple
  without `multiple`).
- `liquers-mcp/src/schema.rs` `type_schema`: `Tuple(v)` → `{"type":"array","prefixItems":[…],
  "minItems":n,"maxItems":n}` for one element. The tool's list wraps it as today. `server.rs`
  display arm.
- `liquers-lib/src/commands.rs`, `search/command_catalog.rs`, `egui/widgets.rs`: display arms
  (`(String, String)`, `tuple(string,string)`).

## Error Handling

- Component conversion at plan time: the existing `from_string` errors, positioned (AC-2).
- Incomplete group at plan time: `Error::conversion_error_with_message(…).with_position(pos)`
  naming the argument and arity (AC-3).
- Remainder at run time (links): `Error::general_error(format!("Argument #{i} '{name}' takes
  elements of {n} parameters; {m} given"))` with the action position (AC-7).
- Bad metadata: `Issue::command_registry(Error, …)` from `ArgumentInfo::check` (AC-6).

## Relevant Commands

None added or changed. Namespaces untouched. An adopter later (for example a `ns-pl/rename`) would
regenerate `specs/command_registry.yaml`.

## Documentation Architecture

`reference/REGISTER_COMMAND_FSD.md` (§Supported Types, §Variadic Parameters, rejection table),
`guides/COMMAND_REGISTRATION_GUIDE.md` (§Accepting a variable number of parameters: "Elements made
of several parameters"), `reference/COMMAND_DECLARATION.md` (declared types),
`reference/api/DOC_08_RECIPES_PLANS.md` (override shapes). `affects_docs` lists these four. No
`specs/README.md` change.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | ≈12 across core, macro, mcp, lib (Integration Points) |
| Crates and workflows affected | command authors (opt-in), MCP tool listing, UI metadata display |
| Existing tests likely to change | none; scalar variadics keep their exact plans (AC-8) |
| New validation | Phase 3 tests; `bash scripts/check-build-matrix.sh` (egui display arm is feature-gated) |
| Compatibility / data | new serde variant: readers built before it cannot read a registry that uses it |
| Concurrency / performance | none; one modulo per parameter |
| Recovery | revert; no stored data uses the variant until a command adopts it |
| Certainty | medium-high: the mechanism fits the existing flat path. The three open decisions are about whether and how, not feasibility |
