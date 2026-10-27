---
id: RECIPE-CWD-ERROR-DOES-NOT-NAME-THE-RECIPE
kind: issue
title: The explicit-cwd recipe error names the directory but not the offending recipe
status: closed
priority: P3
complexity: S
area: [core/assets]
design: recipe-list-set-cwd-validation
created: 2026-10-20
github:
---
## Problem

When a `recipes.yaml` entry sets `cwd`, `RecipeList::set_cwd` (`liquers-core/src/recipes.rs`)
returns `NotSupported` ("CWD can't be explicitly specified in a recipe") and writes a diagnostic to
stderr with `eprintln!`. The error names neither the offending recipe nor its position;
`DefaultRecipeProvider` adds only the directory key.

## Problem Example

Re-checked at HEAD (`cd6d5da`, 2026-10-26): unchanged. With
`recipes: [{query: -R/data/sales.csv/-/to_text/sales.txt}, {query: -R/data/sales.csv/-/to_text/explicit.txt, cwd: other}]`
in `data/recipes.yaml`, every lookup in `data` fails with `NotSupported` "CWD can't be explicitly
specified in a recipe" (key `data`), and stderr gets `Recipe already has CWD set to Some("other")`.
Neither names `recipes[1]`. Designed together with `RECIPE-LIST-SET-CWD-MUTATES-BEFORE-FAILING`
(same function).

## Impact

Every lookup in that directory fails, and in a long `recipes.yaml` the author has to search for the
entry that caused it. The stderr line does not reach a web or Python caller.

## Expected behaviour

Name the offending recipe (its query or filename, or its index) in the error, and drop the stderr
line in favour of the error itself.

## Discovery

Found while reviewing `reference/api/DOC_08_RECIPES_PLANS.md` against the code at `96279f14` before promoting it to [`reference/RECIPES_AND_PLANS.md`](../reference/RECIPES_AND_PLANS.md) (2026-10-20). Split off `RECIPE-LIST-SET-CWD-MUTATES-BEFORE-FAILING`, which shares the code but not the problem.

## Resolution

Fixed 2026-10-27 by `design/recipe-list-set-cwd-validation/`. `RecipeList::set_cwd` (`liquers-core/src/recipes.rs`) checks the whole list before mutating: when any entry already has a `cwd` it returns `NotSupported` "CWD can't be explicitly specified in a recipe: recipes[i] (query '…') sets cwd '…'" for every offender and leaves the list unchanged, and it no longer writes to stderr. Evidence: `set_cwd_tests` (`set_cwd_failure_leaves_the_list_unchanged`, `set_cwd_error_names_every_offending_recipe`, `set_cwd_sets_every_recipe`) and `explicit_cwd_in_recipes_yaml_names_the_recipe`.
