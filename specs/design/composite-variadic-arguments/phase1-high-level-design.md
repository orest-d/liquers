# Phase 1: High-Level Design — composite-variadic-arguments

## Purpose

A variadic argument can hold only scalars. A command that takes pairs, such as `rename` from old
names to new names, or a key-value mapping, has to declare a flat `Vec<String>` and pair the
elements itself. That is the kind of in-command splitting `design/variadic-arguments-declaration/`
removed for flat lists. Let an argument declare a tuple element type, so that the plan and the
retrieval code agree on how consecutive parameters form elements, and so that a UI can render one
row per element.

## Problem Example

Verified at HEAD (18732dd). What a command author can write today:

```rust
fn rename_pairs(state: &State<Value>, names: Vec<String>) -> Result<Value, Error> {
    if names.len() % 2 != 0 { return Err(/* hand-written */); }
    for pair in names.chunks(2) { /* old = pair[0], new = pair[1] */ }
}
register_command!(cr, fn rename_pairs(state, names: Vec<String> multiple) -> result)?;
```

`rename_pairs-a-x-b-y` works, but the metadata says "a list of strings". A UI cannot show pairs.
`rename_pairs-a-x-b` gets past the plan and fails only inside the command. A `Vec<(String, i64)>`
cannot type its second component. `pairs: Vec<(String, String)> multiple` does not compile:
`variadic_element_type` (`liquers-macro/src/registration.rs` ≈541) accepts the tuple as the
element type, but no `ArgumentType` represents a tuple, and the element extractor has no impl for it.

Wanted: `register_command!(cr, fn rename_pairs(state, pairs: Vec<(String, String)> multiple) ->
result)?`. Then `rename_pairs-a-x-b-y` gives `[("a","x"), ("b","y")]`, and `rename_pairs-a-x-b`
fails at plan time, pointing at `b`, with "`pairs` takes elements of 2 parameters".

## Scope and Acceptance Criteria

- **AC-1** Pairs from a flat query
  WHEN `pairs: Vec<(String, String)> multiple` receives `rename_pairs-a-x-b-y`
  THEN the command gets `[("a","x"), ("b","y")]`
- **AC-2** Component types are checked per position
  WHEN `weights: Vec<(String, i64)> multiple` receives `w-a-1-b-x`
  THEN planning fails with a conversion error positioned at `x`
- **AC-3** An incomplete element is a plan error
  WHEN the number of literal parameters is not a multiple of the tuple's arity
  THEN planning fails, positioned at the first parameter of the incomplete element, and the message
  names the argument and the arity
- **AC-4** Empty is allowed
  WHEN no parameter is given
  THEN the command gets an empty `Vec`
- **AC-5** Metadata shows the shape
  WHEN the command's metadata is exported or serialized
  THEN the argument's type is `{"Tuple": ["string", "string"]}` with `multiple: true`, and it
  deserializes back unchanged
- **AC-6** Misdeclarations are refused
  WHEN a tuple has fewer than 2 or more than 4 components, a component is not a scalar type
  (`Vec`, `Option`, tuple), or a `Tuple` type appears on an argument that is not `multiple`
  THEN the macro refuses it at compile time, and `CommandMetadata::check` refuses hand-built metadata
- **AC-7** Links and overrides
  WHEN an element parameter is a link, or a recipe overrides the argument with a JSON array
  THEN elements are grouped after links are materialised (and expanded, per
  `variadic-link-array-expansion`). A flat array or an array of arrays both work as an override.
  An incomplete group is then a run-time error naming the argument
- **AC-8** Existing variadics unchanged
  WHEN a scalar `Vec<T> multiple` argument is used
  THEN plans, retrieval and the exported registry are unchanged

Non-goals: a GUI row editor (`UI-VARIADIC-ARGUMENT-LIST-EDITOR`); map types (`HashMap`,
`BTreeMap`), which a command builds from pairs; new query syntax; named fields (structs); `Option`
or enum components.

## Core Interactions

Query: no syntax change. Plan: `ParameterValue::pop_value` types each parameter by its position
in the tuple and checks the count. Commands: `ArgumentType::Tuple`, `CommandArguments` grouped
retrieval. Macro: tuple element types. MCP: the tool schema of a tuple argument. Display helpers in
`liquers-lib`: one more `ArgumentType` arm.

## Crate Placement

`liquers-core` (type, plan, retrieval, recipes, check), `liquers-macro` (DSL), `liquers-mcp`
(schema), `liquers-lib` (two display `match`es). Within the dependency flow.

## Documentation Intent

- Reference: extend `reference/REGISTER_COMMAND_FSD.md` §Variadic Parameters and §Supported Types.
- Guide: extend `guides/COMMAND_REGISTRATION_GUIDE.md` §"Accepting a variable number of parameters".
- Other documents: `reference/COMMAND_DECLARATION.md` (a declared `Tuple` type);
  `reference/api/DOC_08_RECIPES_PLANS.md` (override shapes).
- Documents to update: the four above, and the `specs/command_registry.yaml` changelog only if a
  shipped command adopts it (none planned).

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. It adds an `ArgumentType` variant, a `pub` trait and
  `register_command!` syntax across four crates (rules 1, 3 and 4).
- **Leading issue:** **Open design - whether to build it now:** no in-repo command needs pairs at
  HEAD. The issue itself suggests weighing a documented `key=value` string convention first.
- **Explanation:** a working design exists under the recommended answers below, and Phases 2-4 are
  written for it. Each answer changes a serialized format or a public type, so each is a
  maintainer decision, not an implementation detail.
- **Open questions:**
  1. **Open design - whether to build it now.** Recommended: approve the design but schedule it
     when the first command needs pairs. A `key=value` convention needs `=` escaped in queries and
     cannot be typed per component. Consequence of waiting: none today.
  2. **Open design - layering.** Recommended: grouping is a *metadata and retrieval* concept. The
     query stays flat, and the plan keeps one flat `MultipleParameters` list. Nested
     `MultipleParameters` stays illegal, so the three places that reject it today keep doing so.
     The alternative, a query-level delimiter, changes the language for one feature.
  3. **Open design - representation.** Recommended: `ArgumentType::Tuple(Vec<ArgumentType>)`,
     restricted to scalar components. The alternative, an `element_types` field on `ArgumentInfo`,
     avoids new `match` arms but lets `argument_type` and `element_types` disagree.
  4. **Proposed resolution - arity bounds.** 2-4 components, the same as the `FromParameterGroup`
     impls Phase 2 adds.

## Design Dependencies

- `variadic-link-array-expansion` — **requires** (weak): AC-7 groups after expansion. If that
  design is decided "convert" instead, the run-time arity check still applies, on unexpanded
  elements.
- `UI-VARIADIC-ARGUMENT-LIST-EDITOR` (issue) — `required-by`: a row editor needs the tuple shape.
- `variadic-arguments-declaration` (complete, E4): origin. Its flat-list contract is kept (AC-8).
- `macro-argument-hints-and-query-literals` — `overlaps` (weak): same parser, different statements.

## Consolidated Findings

- Exhaustive `ArgumentType` matches exist in 9 files (core: `plan.rs`, `recipes.rs`,
  `command_metadata.rs`, `command_declaration.rs`; `liquers-macro`; `liquers-lib`: `commands.rs`,
  `search/command_catalog.rs`, `egui/widgets.rs`; `liquers-mcp`: `schema.rs`, `server.rs`). Each
  gets an explicit `Tuple` arm. No `_ =>` is added.
- Plan-time arity checking is complete only when no element is a link. With links, the check moves
  to `get_multiple_grouped`, which is why AC-3 and AC-7 differ.
- The serialized form adds a variant. Older readers (an older `liquers-validate` reading a registry
  that uses it) fail on it. No committed registry uses it until a command adopts it.
