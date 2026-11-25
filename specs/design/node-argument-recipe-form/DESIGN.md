---
id: NODE-ARGUMENT-RECIPE-FORM
kind: design
title: A Node argument can pass a node's recipe (form recipe)
form: compact
workflow: liquers-project
status: approved
phase: implementation
readiness: ready
autofix: not-eligible
area: [core/commands, macro, core/assets]
issues: [NODE-ARGUMENT-CANNOT-PASS-A-RECIPE]
created: 2026-11-25
---
# A Node argument can pass a node's recipe (form recipe)

Pre-approved after Phase 1 on 2026-11-25 (`proceed all`); the three open questions take their
recommended answers (decisions 1–3 below).

## Phase 1: High-Level Design

### Purpose

A Node argument (`liquers-core/src/command_metadata.rs` `NodeArgument`, `NodeLinkForm`) lets a
recipe name a sibling file of its folder by a bare name, and decides what the command receives:
the node's value (`resource`, `-R/./<name>.<ext>`) or its key (`key`, `-R-key/./<name>.<ext>`).
This design adds a third form, `recipe`: the link `-R-recipe/./<name>.<ext>`, so the command
receives the node's recipe. Commands that describe, compare or re-plan the nodes of a recipe tree
can then be used in a strict folder and get the node selector in the editors.

### Problem Example

A command `describe(state, part: Value)` summarises the recipe of a part. In
`scenes/car/recipes.yaml` (strict) a recipe wants the description of `wheel.glb`:

```
-R-cwd/scenes/car/-R/./body.glb/-/describe-~X~-R-recipe/./wheel.glb~E      (today)
-R-cwd/scenes/car/-R/./body.glb/-/describe-wheel                            (wanted)
```

Both validate (`liquers-validate --command describe`, encoded as written). Today the first is the
only way. The strict check refuses it (`StrictLink`: a recipe passes no argument as a link) and
the editor shows a raw link instead of a node selector. Declaring `part: Value (node: local, ext: "glb", form: recipe)`
should make the second query mean the first.

While checking the tree code a related gap was confirmed (throwaway test against HEAD): a
`form: key` Node name produces **no edge** in `TreeGraph`, because `recipe_tree.rs`
`walk_parameter` takes the target from `Query::key()`, which is `None` for a non-trivial header
(`-R-key/…`). In a strict folder `locate-gone` (no `gone.glb` anywhere) is accepted with no
`Dangling` finding. The same code would ignore every `form: recipe` name, so this design fixes it
for both forms (scope change below).

### Scope and Acceptance Criteria

- **AC-1** A bare name becomes a recipe link
  WHEN a command declares `part: Value (node: local, ext: "glb", form: recipe)` and a query in a folder with a recipe for `wheel.glb` calls `describe-wheel`
  THEN the command receives that recipe as a `Value::Recipe`, the same value as `describe-~X~-R-recipe/./wheel.glb~E`
- **AC-2** Metadata carries the form
  WHEN the command's `ArgumentInfo` is serialized (JSON, YAML, the exported registry)
  THEN its type is `{node: {kind: local, extension: glb, form: recipe}}`, and it reads back equal
- **AC-3** The macro checks the type
  WHEN `form: recipe` is declared on an argument that is not a `Value`
  THEN `register_command!` fails to compile with a message naming the allowed type; an unknown form lists `resource`, `key` and `recipe`
- **AC-4** The editors show a node name
  WHEN a recipe-form argument holds the link `-R-recipe/./wheel.glb`
  THEN `NodeArgument::name_of` returns `wheel` and the editor shows the node selector with `wheel`
- **AC-5** The tree sees key and recipe Node names
  WHEN a recipe of a folder names a node through a `key` or `recipe` form argument
  THEN `TreeGraph` has a `NodeArg` edge to `./<name>.<ext>`, so dangling, undeclared, strict and orphan checks apply as for the `resource` form
- **AC-6** Rename follows
  WHEN `TreeEditor::rename` renames a node named through a `recipe` form argument
  THEN the Node name in the referring recipe is rewritten (`Rewriter::source` is form-independent already; this is a guard)
- **AC-7** Editing the named recipe expires the result
  WHEN the recipe of `wheel.glb` changes after `describe-wheel` was evaluated
  THEN the result is expired through its `-R-recipe/` dependency (`specs/reference/DEPENDENCIES_STATUS.md`)
- **AC-8** A node with no recipe
  WHEN the named node is a stored file with no recipe
  THEN the command receives the none value, as `-R-recipe/` gives today, and no new tree finding is reported

Out of scope: other `NodeKind`s, a typed `Recipe` argument (Q1), new `-R-…` headers.

### Language Integration

No change to `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`: this extends an existing argument
type, not a feature of the catalogue. **Python: not needed**, Python commands cannot declare Node
arguments; `ArgumentInfo.argument_type` already returns the serde JSON, which gains
`"form": "recipe"` with no binding change. **JavaScript: not needed**, for the same reason;
recipe trees as a whole are absent from both bindings (`RECIPE-TREES-NOT-IN-LANGUAGE-BINDINGS`).

### Documentation Intent

- API docs: `liquers-core/src/command_metadata.rs` (module doc names the Node argument forms) and
  `liquers-core/src/recipe_tree.rs` (which references are edges); `liquers-macro` registration
  docs for `form:`.
- User guides: `docs/guides/rust/05-commands.md` (the Nodes paragraph). Python and web: none,
  the feature cannot be declared there.
- Specs: `specs/reference/RECIPE_TREES.md` §Node arguments, `specs/reference/REGISTER_COMMAND_FSD.md`
  if it lists `form`, `specs/guides/COMMAND_REGISTRATION_GUIDE.md` §1.

### Scope Changes

- 2026-11-25: AC-5 adds key-form Node names to the tree graph. It is the same root cause and the
  same change site as the recipe form (triage case 1, overlap T1/T3), so no separate issue.

### Design Dependencies

- requires: `recipe-tree` (complete), `recipe-dependency-versions` (complete, gives AC-7).
- overlaps: `RECIPE-TREES-NOT-IN-LANGUAGE-BINDINGS` (weak: same area, independent change).

### Design Readiness

Ready. Decisions, accepted with the pre-approval on 2026-11-25:


1. **Argument type.** Recommended: `form: recipe` accepts only `Value` (it receives
   `Value::Recipe` or none). A typed `recipe: Recipe` argument would need a `FromParameterValue`
   for `Recipe` and a `try_into_recipe` on `ValueInterface`; it can follow later if wanted.
2. **Cycles.** Recommended: key and recipe Node names count as edges for the cycle check too, the
   same rule the tree applies today to a `-R-recipe/./x` segment and a `-R-key/./x` query
   (`RefKind::Resource` / `KeyValue`). The alternative, excluding them because nothing is
   evaluated, would make `-R-recipe/` segments and Node names disagree.
3. **Default widget.** Recommended: keep `NodeSelector` for all three forms.

## Phase 2: Architecture

### Solution

Add a variant `NodeLinkForm::Recipe` and let every place that matches on the form handle it. The
link is the existing `-R-recipe/` header, so planning (`ParameterValue::NodeLink`), evaluation
(`Step::GetAssetRecipe`, cwd-resolved), dependency recording (`-R-recipe/<key>`, versioned) and the
editors (`name_of`) need no new mechanism. In `recipe_tree.rs` `walk_parameter`, a `NodeLink`
takes its target from `Query::header_key()` instead of `Query::key()`, which gives key- and
recipe-form names an edge (AC-5); all edges count for cycles (decision 2).

Rejected: a separate `ArgumentType` for recipe arguments (duplicates the Node machinery); a typed
`Recipe` argument (decision 1, needs `FromParameterValue` for `Recipe`); excluding non-resource
edges from the cycle check (decision 2).

Known-issue preflight: `RECIPE-OVERRIDES-MISS-A-FINAL-PIPELINE-COMMAND` and
`RECIPE-TREES-NOT-IN-LANGUAGE-BINDINGS` touch the same area and neither blocks. No open issue
names `NodeLinkForm`. Command namespaces involved: none (no command is registered by this design).

### Changes

- `liquers-core/src/command_metadata.rs`: `NodeLinkForm::Recipe` (serde `recipe`, doc
  `-R-recipe/./<name>.<ext>`); `NodeArgument::link` and `name_of` gain the `-R-recipe/./` arm.
  Module doc: names the three forms.
- `liquers-macro/src/registration.rs`: `NodeFormSpec::Recipe`; `form: recipe` parses, emits
  `NodeLinkForm::Recipe`, and requires the argument type `Value` (error: "a Node argument with
  `form: recipe` must be a `Value`"); the unknown-form error lists the three forms.
- `liquers-core/src/recipe_tree.rs` `walk_parameter`: `NodeLink` uses `header_key()`. Module doc:
  Node names of every form are edges.
- No change to `plan.rs`, `interpreter.rs`, the editors, `liquers-py` or `liquers-web`; no error
  type is added. Everything stays sync except the existing async evaluation.
- Docs: `specs/reference/RECIPE_TREES.md` §Node arguments, `specs/reference/REGISTER_COMMAND_FSD.md`
  (the `form:` row), `specs/guides/COMMAND_REGISTRATION_GUIDE.md` §1, `docs/guides/rust/05-commands.md`
  (Nodes). `specs/command_registry.yaml` is unchanged: no registered command uses a Node argument.

### Risks

Small. A serialized `ArgumentType` with `form: recipe` cannot be read by an older build (no
compatibility is required). A folder with a key-form Node name to a missing file now fails the
check (AC-5); that was the bug. Recovery: revert the commit.

## Phase 3: Examples and Tests

### Examples

```rust
fn describe(part: Value) -> Result<Value, Error> {
    match part {
        Value::Recipe(r) => Ok(Value::from(r.title.clone())),
        other if other.is_none() => Ok(Value::from("none")),
        _ => Err(Error::general_error("not a recipe".to_owned())),
    }
}
register_command!(cr, fn describe(part: Value (node: local, ext: "glb", form: recipe)) -> result)?;
```

With `scenes/car/recipes.yaml` declaring `{query: make_part/wheel.glb, title: Wheel}`,
`-R-cwd/scenes/car/-/describe-wheel` gives `Wheel`, the same as
`-R-cwd/scenes/car/-/describe-~X~-R-recipe/./wheel.glb~E`; for the stored-only `body.glb` it gives
`none`. Edge case: `describe-a/b` fails as today (`invalid_token`, not one resource name).

### Tests

- `liquers-core/src/command_metadata.rs`: `node_argument_serde_round_trip_recipe` (AC-2);
  `node_link_builds_recipe_form` — `-R-recipe/./w.glb`, `name_of` inverts it and refuses the other
  forms' links (AC-1, AC-4).
- `liquers-macro/src/registration.rs`: `test_parse_node_argument` gains the recipe form;
  `test_node_argument_errors` gains `part: Key (… form: recipe)` and `form: other` (AC-3).
- `liquers-core/src/recipe_tree.rs`: `key_and_recipe_node_names_are_edges` — a strict folder with
  `locate-gone` and `describe-gone` reports two `Dangling` findings, and existing names give
  `NodeArg` edges (AC-5); `rename_rewrites_recipe_form_node_name` (AC-6).
- `liquers-core/tests/node_argument.rs`: `recipe_form_name_gives_the_recipe` (AC-1, AC-8);
  `recipe_form_result_expires_when_the_recipe_changes`, on both managers (AC-7).

```bash
cargo test -p liquers-core --lib node_ recipe_form key_and_recipe
cargo test -p liquers-core --test node_argument
cargo test -p liquers-macro --lib node_argument
```

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `command_metadata.rs` `NodeLinkForm::Recipe`, `link`, `name_of`, module doc, two unit tests — `cargo test -p liquers-core --lib node_`
- [ ] 2. `registration.rs` `NodeFormSpec::Recipe`, parse and type check, tests — `cargo test -p liquers-macro --lib node_argument`
- [ ] 3. `recipe_tree.rs` `walk_parameter` `header_key()`, module doc, two unit tests — `cargo test -p liquers-core --lib recipe_tree`
- [ ] 4. `tests/node_argument.rs` recipe-form evaluation and expiry tests — `cargo test -p liquers-core --test node_argument --test recipe_tree`
- [ ] 5. Reference, guide and user-guide updates (History rows, `reviewed:`) — `cargo run -p liquers-docs -- check`, `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-core`, `cargo test -p liquers-macro`, `python3 scripts/check_api_docs.py`
on `command_metadata.rs` and `recipe_tree.rs`, and `cargo test -p liquers-lib --lib --tests`
(the editors use `name_of`). No `#[cfg]` changes, so the build matrix is not needed.

## Phase 5: Documentation
<Added after implementation, only for `workflow: liquers-project`: what was built versus approved,
documents and module docs reviewed, issues closed and filed.>
