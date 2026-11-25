---
id: NODE-ARGUMENT-CANNOT-PASS-A-RECIPE
kind: feature
title: A Node argument cannot pass a node's recipe
status: closed
priority: P3
complexity: S
area: [core/commands, macro, core/assets]
design: node-argument-recipe-form
created: 2026-11-25
github:
---
## Problem

A Node argument (`ArgumentType::Node`, `liquers-core/src/command_metadata.rs` `NodeArgument`,
`NodeLinkForm`) turns a bare node name into one of two links: `-R/./<name>.<ext>` (the node's
value, form `resource`) or `-R-key/./<name>.<ext>` (its key, form `key`). There is no form for the
node's **recipe** (`-R-recipe/./<name>.<ext>`, which `Step::GetAssetRecipe` already evaluates to a
`Value::Recipe`). A command that inspects a sibling's recipe (its title, query or arguments) must
take a plain link, `describe-~X~-R-recipe/./wheel.glb~E`. The editor then shows a raw link instead
of a node selector, and a strict folder refuses the recipe (`StrictLink`).

## Impact

Commands that document, compare or re-plan the nodes of a recipe tree cannot be written for
strict folders and get no node selector. Workaround: a plain link argument, outside strict mode.

## Expected behaviour

`form: recipe` in `register_command!` (and `NodeLinkForm::Recipe`) makes a bare name the link
`-R-recipe/./<name>.<ext>`; the command receives the node's recipe as a value.

## Discovery

Requested by the maintainer on 2026-11-25. Design: `specs/design/node-argument-recipe-form/`.

## Resolution

Fixed in PR #290 (design `node-argument-recipe-form`): `form: recipe` / `NodeLinkForm::Recipe`
makes a bare name the link `-R-recipe/./<name>.<ext>`. Key- and recipe-form Node names are now
edges of the recipe tree. Evidence: `recipe_form_name_gives_the_recipe`,
`key_and_recipe_node_names_are_edges` and the other tests listed in the design's Phase 5.
