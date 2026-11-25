---
id: NODE-ARGUMENT-RECIPE-FORM
kind: design
title: A Node argument can pass a node's recipe (form recipe)
form: compact
workflow: liquers-project
status: in_review
phase: high-level
area: [core/commands, macro, core/assets]
issues: [NODE-ARGUMENT-CANNOT-PASS-A-RECIPE]
created: 2026-11-25
---
# A Node argument can pass a node's recipe (form recipe)

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

### Open Questions

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
<Chosen approach; rejected alternatives in one line each; known issues that block or must go first.>
### Changes
<Files and symbols; new or changed signatures only; errors; sync/async; commands; documents and
the module docs that change.>
### Risks
<Files, tests likely to change, compatibility, recovery, certainty, in a few lines.>

## Phase 3: Examples and Tests
### Examples
### Tests
<Exact test names, each citing the scenarios it proves (AC-1, …); the command to run them.>

## Phase 4: Implementation Plan
### Steps
<A checklist, one item per step: `- [ ] 1. <file and symbol> — <change> — <proof command>`.
Tick each step when its proof passes and append the commit.>
### Validation
<Final checks, `check_api_docs.py` on the touched files among them; documents and generated files
to update.>

## Phase 5: Documentation
<Added after implementation, only for `workflow: liquers-project`: what was built versus approved,
documents and module docs reviewed, issues closed and filed.>
