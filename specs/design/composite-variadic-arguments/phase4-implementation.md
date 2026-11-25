# Phase 4: Implementation Plan — composite-variadic-arguments

Rewritten on 2026-11-25 for the scope of Phases 1 to 3 as revised that day.

## Overview

Metadata first (the `shape` field), then the plan, retrieval and overrides in `liquers-core`,
then the macro, then the consumers: MCP, the display of an argument, the editor core and its two
views. `ArgumentType` is not changed, so no step breaks an exhaustive match elsewhere. Every step
leaves scalar variadics exactly as they are (AC-8); until a command declares a shape, no observable
behaviour depends on any step. `variadic-link-array-expansion` is not a prerequisite: link tests
are written against materialised elements as they are today.

## Progress

- [x] Step 1: `VariadicShape`, `ArgumentInfo::shape` and `columns`, `check` rules (`c754b472`)
- [x] Step 2: Plan typing and checks in `pop_value` (`9786df9e`)
- [x] Step 3: `FromParameterGroup`, `json_scalar`, grouped, map and JSON retrieval (`46f50d1f`)
- [x] Step 4: Override conversion and declarations (`103ef3c3`)
- [x] Step 5: Registration macro support and rejections (`c2e1ceec`)
- [x] Step 6: Integration tests (`c2e1ceec`)
- [x] Step 7: MCP schema and input (`b8dbf7dd`)
- [x] Step 8: Display of a shaped argument and language bindings (`92db59d9`)
- [x] Step 9: Editor core: shape, components, row commands (`34362203`)
- [x] Step 10: Editor views (egui, web) (`3d09b846`)
- [ ] Step 11: Build matrix and full test loops
- [ ] Step 12: Documents and records

## Deviations during implementation

Recorded 2026-11-25. None changes a decision of Phase 1 or 2.

- **MCP input (Step 7).** `liquers-mcp/src/server.rs` did not need the conversion Phase 2
  describes: tool arguments are recipe overrides, so the override conversion of Step 4 already
  takes an object, nested arrays and a JSON value. Only the schema and the command reference
  changed.
- **A JSON value override is one element (Step 4).** An override such as `[1, "a"]` is kept whole
  instead of being flattened with a leading empty string, so a list is never mistaken for pairs.
  A string element is still read as a query parameter would be: an override `"8"` is the number 8.
- **Pipelines and URL overrides (Steps 4, 8), not listed in Phase 2.** `liquers-core/src/pipeline/plan.rs`
  types each position of a shaped argument and takes a whole value as an override does;
  `liquers-axum/src/api_core/overrides.rs` reads a JSON value argument from repeated URL
  parameters with `commands::json_value_from_texts`.
- **`Vec<P>` with a type of the caller's (Step 5).** The macro reads `Vec<T> multiple` as a plain
  list only when `T` is a number, `bool`, `String`, `Key`, `Query`, `Value` or `Option`; any other
  element type must implement `FromParameterGroup`. No command in the repository is affected; a
  command elsewhere with its own `FromParameterValue` element type would need the new trait.
- **Registration check (Step 5).** A command with a shaped argument runs
  `CommandMetadata::check` when it is registered, which is where `columns` are compared with a
  custom element type.
- **JavaScript (Step 8).** `LiquersArgument` has no variadic argument at all, so there was nothing
  to extend: `WEB-COMMAND-CANNOT-DECLARE-VARIADIC-ARGUMENT`. Python exposes `shape`,
  `shape_components` and `columns` read-only.
- **Rows of a JSON value (Step 10).** `InsertRow` and `RemoveRow` also apply to a JSON value, as
  pairs, so its dictionary form is edited like a dictionary.
- **Command registry.** `specs/command_registry.yaml` was regenerated after all: `commands_doc`
  has `version: auto` and its body changed (the shape in its Multiplicity column), so its
  `impl_version` moved. No signature changed.
- **Module docs.** `plan.rs` and `liquers-mcp/src/schema.rs` still lack an example section
  (`MODULE-DOCS-DO-NOT-FOLLOW-THE-API-DOC-RULES`); the three modules Phase 1 names pass
  `check_api_docs.py`.

## Implementation Steps

### Step 1: `VariadicShape`, `ArgumentInfo::shape` and `columns`, `check` rules
- Files / symbols: `liquers-core/src/command_metadata.rs` `VariadicShape`, `ArgumentInfo`,
  `ArgumentInfo::check`, builders (`with_shape`, `with_columns`)
- Change: the enum and two fields of Phase 2; a shape other than `list` requires `multiple`;
  `tuples` needs 2 to 4 scalar components; `columns` is empty or of the shape's arity
- Depends on: none
- Proof: `cargo test -p liquers-core --lib shape_`; `cargo test -p liquers-lib --test
  registry_export` (the committed registry is unchanged)
- Rollback: revert; nothing reads the fields yet

### Step 2: Plan typing and checks in `pop_value`
- Files / symbols: `liquers-core/src/plan.rs` `ParameterValue::pop_value` (multiple branch)
- Change: per-position element type for `tuples` and `map`; incomplete-element and
  duplicate-key errors for literals; the `json` pair checks
- Depends on: Step 1
- Proof: `cargo test -p liquers-core --lib shaped_variadic_`; `scalar_variadic_plan_unchanged`
- Rollback: revert; the `list` path is untouched

### Step 3: `FromParameterGroup`, `json_scalar`, grouped, map and JSON retrieval
- Files / symbols: `liquers-core/src/commands.rs` `FromParameterGroup` (impls for tuples of 2 to
  4), `json_scalar`, `FromParameterValue<serde_json::Value>`,
  `CommandArguments::{get_multiple_grouped, get_multiple_map, get_multiple_json}`
- Change: as Phase 2; run-time remainder and duplicate-key errors
- Depends on: Step 1
- Proof: `cargo test -p liquers-core --lib get_multiple_`
- Rollback: revert

### Step 4: Override conversion and declarations
- Files / symbols: `liquers-core/src/recipes.rs` (the variadic branch of override conversion),
  `liquers-core/src/command_declaration.rs` (declared `shape`, `columns`)
- Change: flat and nested arrays for `tuples`; an object or pairs for `map`; a flat JSON value
  for `json`
- Depends on: Steps 1, 2
- Proof: `cargo test -p liquers-core --lib shaped_variadic_override`
- Rollback: revert

### Step 5: Registration macro support and rejections
- Files / symbols: `liquers-macro/src/registration.rs` `variadic_element_type` and its caller,
  the per-argument metadata parser (`columns:`), the extractor generation
- Change: the table and rejections of Phase 2
- Depends on: Steps 1, 3
- Proof: `cargo test -p liquers-macro variadic_shape`
- Rollback: revert; existing variadics untouched

### Step 6: Integration tests
- Files / symbols: `liquers-core/tests/composite_variadic.rs`
- Change: the Phase 3 integration tests
- Depends on: Steps 2 to 5
- Proof: `cargo test -p liquers-core --test composite_variadic`
- Rollback: delete the file

### Step 7: MCP schema and input
- Files / symbols: `liquers-mcp/src/schema.rs` `argument_property`, `type_schema`;
  `liquers-mcp/src/server.rs` (arguments to parameters)
- Change: schemas per shape; an object becomes pairs; a list for `json` gets the leading empty
  string
- Depends on: Step 1
- Proof: `cargo test -p liquers-mcp`
- Rollback: revert

### Step 8: Display of a shaped argument and language bindings
- Files / symbols: `liquers-lib/src/commands.rs`, `liquers-lib/src/search/command_catalog.rs`,
  `liquers-lib/src/egui/widgets.rs`, `liquers-py/src/command_metadata.rs`,
  `liquers-web/src/typescript.rs`
- Change: show the shape beside "multiple"; expose `shape` and `columns` read-only
- Depends on: Step 1
- Proof: `cargo test -p liquers-lib --lib --tests`; `cargo check -p liquers-py`
- Rollback: revert

### Step 9: Editor core: shape, components, row commands
- Files / symbols: `liquers-lib/src/ui/editor/tree.rs` `EditorField`, `ComponentField`;
  `command.rs` `EditorCommand::{InsertRow, RemoveRow, MoveRow, SetItems}`; `state.rs`
- Change: as Phase 2; row commands apply to `n` items at once
- Depends on: Step 1
- Proof: `cargo test -p liquers-lib --lib shaped_`
- Rollback: revert; list editing untouched

### Step 10: Editor views (egui, web)
- Files / symbols: `liquers-lib/src/egui/editor.rs` `items_ui`; `liquers-lib/src/ui/web/editor.rs`
- Change: tuple rows with column headers; dictionary pairs with an add form; the JSON form choice
- Depends on: Step 9
- Proof: `cargo test -p liquers-lib --lib --tests`; `cargo check -p liquers-lib
  --no-default-features --features webui --tests`
- Rollback: revert; a shaped argument then shows as a flat list, which still edits correctly

### Step 11: Build matrix and full test loops
- Files / symbols: none
- Change: none
- Depends on: Steps 1 to 10
- Proof: `bash scripts/check-build-matrix.sh`; `cargo test -p liquers-core`; `cargo test -p
  liquers-macro`; `cargo test -p liquers-mcp`; `cargo test -p liquers-lib --lib --tests`
- Rollback: n/a

### Step 12: Documents and records
- Files / symbols: `REGISTER_COMMAND_FSD.md`, `COMMAND_REGISTRATION_GUIDE.md`,
  `COMMAND_DECLARATION.md`, `RECIPES_AND_PLANS.md` (History rows, `reviewed:`); module docs of
  `commands.rs`, `command_metadata.rs`, `ui/editor/mod.rs`; the commands chapter of
  `docs/guides/rust/`; issue and design status
- Change: as Phase 2's documentation architecture
- Depends on: Step 11
- Proof: `python3 scripts/docs_index.py` then `--check`; `python3 scripts/check_api_docs.py` on
  the three modules; `cargo run -p liquers-docs -- check`
- Rollback: n/a

## Testing Plan

Unit tests land with their step. The integration file is Step 6. The feature matrix runs once in
Step 11, because Steps 8 and 10 touch feature-gated code (`egui`, `webui`). The wasm conformance
loop of `liquers-web` is not run: only its TypeScript text changes, which the matrix compiles.

## Rollback Plan

One commit per step. Reverting Step 1 requires reverting everything after it. Steps 7 to 10 are
independent of each other and can be reverted alone.

## Documentation Updates

`affects_docs` as in Phase 2. `specs/command_registry.yaml` is not regenerated: no shipped command
adopts a shape here (the first adopter is the UI redesign). `specs/index.csv` is regenerated in
Step 12.

## Phase 5 Entry Criteria

None — a four-phase design. Step 12 does the documentation work and the records.
