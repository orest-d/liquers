# Phase 4: Implementation Plan — Recipe and recipes editors

## Overview

Bottom-up: the core document edits first, since everything else uses them. Then the accessor
command, the backend-free state, the elements and runner messages, the two renderers, and the docs
last. No issue must be fixed first.

## Progress
- [x] Step 1: `RecipeFileStep` and the new `TreeEditor` edits (core) — `daf10125`
- [x] Step 2: `acc/recipe_file` command and registry — `da861144`
- [x] Step 3: `ui/recipes` state: rows, label, status, dependencies view, commands, effects — `da861144`
- [x] Step 4: `RecipeForm` and the YAML window state, with highlighting spans — `da861144`
- [x] Step 5: elements, `lui` commands, runner messages and `NodeContext` fields — `da861144`
- [x] Step 6: egui renderer, context menu, windows, Save as (typed path, see Deviations), example — `f364de22`
- [x] Step 7: browser renderer — `f364de22`
- [x] Step 8: integration tests, build matrix, docs and issues — `f364de22` and the documentation commit

## Deviations

- **Save as asks for a typed path**, not an `rfd` dialog (Q8): `rfd` needs GTK or the XDG portal on
  Linux, which the cloud and CI builds lack. Filed `RECIPES-EDITOR-SAVE-AS-HAS-NO-FILE-DIALOG`.
- **A new recipe is allowed while it is an unstored source.** New and Add child append a recipe
  whose query is its filename, which the check reports as `MissingSource`; `TreeEditor::commit`
  refused that as an added error, so `add` and `add_child` now tolerate that one finding for the
  node they add (`commit_new`). The panel shows the finding until the query is written.
- **`Recipe` skips default fields when serialized** (`has_circular_dependencies`,
  `circular_dependency_key`, a `never` expiration): otherwise the panel's first write added three
  noise lines per recipe to `recipes.yaml`. Deserialization is unchanged (all default).
- **The browser shows one recipe form at a time**, below the rows, so the field names its events
  carry stay unique.
- **Duplicate of a recipe that is only a resource elsewhere** (`-R/data/a.csv`) is refused by the
  check (two recipes produce one file): `DUPLICATE-OF-EXTERNAL-RESOURCE-RECIPE-KEEPS-ITS-FILENAME`.

## Implementation Steps

### Step 1: core edits
- Files: `liquers-core/src/accessor/steps.rs` (`RecipeFileStep`), `liquers-core/src/recipe_tree.rs`
  (`TreeEditor::{add, duplicate, set_recipe, set_result, replace}`).
- Proof: `cargo test -p liquers-core --lib accessor::steps recipe_tree`. Phase 3 core unit tests.
- Rollback: additive; revert the two files.

### Step 2: `acc/recipe_file`
- Files: `liquers-lib/src/accessor/mod.rs` (`recipe_file`, registration),
  `liquers-lib/tests/value_accessor_commands.rs`, `specs/command_registry.yaml`.
- Depends on: Step 1.
- Proof: `cargo test -p liquers-lib --test value_accessor_commands --test registry_export`.
- Rollback: remove the command and regenerate the registry.

### Step 3: panel state
- Files: `liquers-lib/src/ui/recipes/{mod,state,rows}.rs`, `ui/mod.rs`; `ui/editor/tree.rs`
  (`NodeContext` gains `assets`, `formats`, `notice`, and existing constructors are updated).
- Depends on: Step 1.
- Proof: `cargo test -p liquers-lib --lib ui::recipes` (AC-1–3, 7–11, 13–19 tests).
- Rollback: the module is new; revert the `NodeContext` fields.

### Step 4: form and YAML
- Files: `ui/recipes/{form,yaml}.rs`.
- Depends on: Step 3.
- Proof: the AC-4, AC-5, AC-6 and AC-12 unit tests.

### Step 5: elements, commands, runner
- Files: `ui/recipes/element.rs` (`RecipesElement`, `RecipeElement`), `ui/commands.rs`
  (`recipes_editor`, `recipe_editor`), `ui/message.rs` (`Evaluate`, `SaveAs`), `ui/runner.rs`
  (handlers; `node_context` fills assets and formats), `specs/command_registry.yaml`.
- Depends on: Steps 2–4.
- Proof: `cargo test -p liquers-lib --test ui_recipes_editor --test registry_export`.

### Step 6: egui
- Files: `liquers-lib/src/egui/recipes.rs`, `egui/mod.rs`, `liquers-lib/Cargo.toml` (`rfd`,
  optional, native, in feature `egui`), `examples/ui_spec_editor.rs` (a recipes panel).
- Depends on: Step 5.
- Proof: `cargo check -p liquers-lib --features egui`;
  `cargo run -p liquers-lib --example ui_spec_editor -- --check`.

### Step 7: browser
- Files: `liquers-lib/src/ui/web/recipes.rs`, `ui/web/mod.rs`; `render_web` of both elements.
- Depends on: Step 5.
- Proof: `cargo test -p liquers-lib --features webui --lib ui::web::recipes`.

### Step 8: finish
- Run `bash scripts/check-build-matrix.sh` (a new optional dependency and `cfg`s).
- Write the docs and the Phase 5 summary; file `WEB-RECIPES-EDITOR-CANNOT-DOWNLOAD`.
- Proof: `python3 scripts/check_api_docs.py` on the touched modules; `python3 scripts/docs_index.py --check`.

## Testing Plan

Each step runs its own tests. At the end: `cargo test -p liquers-core --lib`,
`cargo test -p liquers-lib --lib --tests`, the example check, and the build matrix.

## Rollback Plan

The change is additive. A partial landing is safe after any step, because no existing behaviour
depends on the new code. An unfinished step becomes an issue (§5.6).

## Documentation Updates

The documents in the Phase 2 documentation architecture, each with a History row and `reviewed:`
bumped. `specs/command_registry.yaml` is regenerated with a CHANGELOG line. The indexes are
regenerated.

## Phase 5 Entry Criteria
- [x] Implementation finished and validated
- [ ] User and review comments answered
- [x] Documentation checkable against implemented and tested behaviour
