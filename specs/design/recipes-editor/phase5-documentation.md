# Phase 5: Documentation - recipes-editor

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated (the maintainer's two Phase 1 reviews)
- [ ] All review comments are answered or incorporated (PR review pending)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical

## Implementation Summary

Two UI elements edit recipes, as approved:

- `RecipesElement` (`lui/recipes_editor`) is a narrow side panel over a folder's `recipes.yaml`. Its
  backend-free core, `RecipesState`, keeps the working copy as a `TreeEditor`, so renames rewrite
  references, deletes are refused while referenced, and an edit adding a check error changes
  nothing. Rows show the type icon, the name (filename, or node name in a tree), title, a status
  circle or percentage, findings and tooltips; the label is configurable (name, title, both). The
  right-click menu (egui) or `⋯` menu (browser) has View, Metadata, Edit, Evaluate, Rename,
  Duplicate, Add child, Set as result, Save as, Export as `<format>` and Delete. The optional
  dependencies view puts roots on top with direct dependencies beneath, repeating shared nodes and
  stopping at cycles. A YAML window highlights the file and applies only text that parses. Edits
  are written at once or on Update (`continuous`).
- `RecipeElement` (`lui/recipe_editor`) is one recipe as a form with the query editor inside, bound
  through the new `acc/recipe_file-<filename>` accessor, which finds the recipe by filename.

The recipe list stays a document (YAML text over accessors), not a `Value` variant, as recommended
in Phase 1. New runner messages: `AppMessage::Evaluate` (start without waiting) and
`AppMessage::SaveAs` (evaluate, serialize, write a local file, report a notice); `NodeContext`
gains the folder's asset information, the formats and a notice. Core gained `RecipeFileStep` and
`TreeEditor::add`, `duplicate`, `set_recipe`, `set_result`, `replace`, `child_slots`.

Deviations, each recorded in Phase 4: Save as asks for a typed path instead of an `rfd` dialog;
`add` / `add_child` tolerate the new node's own `MissingSource`; `Recipe` no longer serializes
three default fields; the browser shows one form at a time.

## Documentation Delivered

### New Reference Documents
None: the editors extend existing contracts.

### New Guide Documents
None.

### Existing Documents Reviewed or Updated
- `specs/reference/UI_INTERFACE_FSD.md`: new §Editors › Recipe editors (elements, panel, protocol,
  renderers); History row; `reviewed:` already 2026-11-24.
- `specs/reference/RECIPE_TREES.md`: the new `TreeEditor` methods and the unstored-source rule;
  §In the editors gains the panel and its dependencies view; History row; `reviewed:` 2026-11-24.
- `specs/reference/VALUE_ACCESSORS.md`: `RecipeFileStep` and `recipe_file-<filename>`; History
  row; `reviewed:` 2026-11-24.
- `specs/command_registry.yaml`: regenerated (195 commands) with a CHANGELOG line.

### Module Docs
`ui/recipes/mod.rs` (new, all five parts), `egui/recipes.rs` (new), `ui/web/recipes.rs` (new),
`ui/commands.rs`, `accessor/mod.rs`, `accessor/steps.rs`. `scripts/check_api_docs.py` on these and
on `ui/runner.rs`, `ui/message.rs`, `ui/editor/tree.rs`, `recipe_tree.rs`, `recipes.rs`:
0 warnings, 0 errors each.

### User-Guide Chapters
`docs_index.py chapters-for` lists `04-values`, `05-commands`, `07-assets`, `11-ui` and
`e-web-api` of the three guides. `11-ui` (all three) includes `_shared/ui-editors.md`, which gains
a "Recipe editors" subsection (queries checked with `liquers-validate`). The others were reviewed:
still true, no change. A row per review is in each guide's `HISTORY.md`. `liquers-docs check`:
0 errors, 0 warnings.

### Language Integration
- Integration guide: no new feature ID, as decided; `recipe_file` is a command reachable by query.
- Python: not needed, as decided; the elements come through `lui` queries.
- JavaScript: the browser renderer shipped (`ui::web::recipes`); no new JavaScript API. Browser
  downloads for Save as / Export are later: `WEB-RECIPES-EDITOR-CANNOT-DOWNLOAD`.

### Links and Capability Map
`specs/README.md` line for the design updated to complete.

## Issues Filed

- `WEB-RECIPES-EDITOR-CANNOT-DOWNLOAD`: Save as and Export are disabled in a browser.
- `RECIPES-EDITOR-SAVE-AS-HAS-NO-FILE-DIALOG`: a typed path instead of a native file dialog.
- `DUPLICATE-OF-EXTERNAL-RESOURCE-RECIPE-KEEPS-ITS-FILENAME`: Duplicate is refused for a recipe that
  is only a resource elsewhere.

`NO-RECIPE-OR-RECIPES-EDITOR-UI` is closed by this design.

## Important Learning

- A panel over a document should keep the document as the source of truth and make the typed
  model a working copy: a file that does not parse still loads, read-only, with the YAML window open
  to repair it. Adding a `Value` variant for the list would have cost a ripple through ~25 files
  and lost that.
- Forms keyed by filename must be re-keyed *before* an edit reloads them; otherwise a rename closes
  the window it came from (fixed in `RecipesState::rename`).
- `TreeEditor`'s "no added error" rule needs one exception for creation: a new recipe is a source
  declaration until its query is written.
- Serializing a struct with `#[serde(default)]` fields but no `skip_serializing_if` adds them to a
  hand-written file on the first write; check that before writing a user's file back.

## Conformance and Remaining Work

Requested: a single recipe editor, a list editor (outline, icon, open on click, new), a tree editor
(node names, rename, optional dependencies view), editors' own accessors by filename, a highlighted
YAML editor; then a narrow side panel with a configurable label, status, tooltips, a right-click
menu and YAML in a window. All are implemented. Remaining: the three issues above.

## Validation

- `cargo test -p liquers-core --tests` and `cargo test -p liquers-lib --lib --tests`: 118 suites,
  0 failures (includes the 26 `ui::recipes` unit tests, 4 egui/web renderer tests and
  `tests/ui_recipes_editor.rs`, 4 tests through the runner).
- `cargo run -p liquers-lib --example ui_spec_editor -- --check`: the panel follows an edit made by
  another editor.
- `bash scripts/check-build-matrix.sh` (after merging main): all 49 configurations OK.
- `python3 scripts/docs_index.py --check`: 0 errors. `cargo run -p liquers-docs -- check`: 0 errors.
