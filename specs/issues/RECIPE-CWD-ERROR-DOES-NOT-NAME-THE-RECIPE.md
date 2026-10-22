---
id: RECIPE-CWD-ERROR-DOES-NOT-NAME-THE-RECIPE
kind: issue
title: The explicit-cwd recipe error names the directory but not the offending recipe
status: draft
priority: P3
complexity: S
area: [core/assets]
design:
created: 2026-10-20
github:
---
## Problem

When a `recipes.yaml` entry sets `cwd`, `RecipeList::set_cwd` (`liquers-core/src/recipes.rs`)
returns `NotSupported` ("CWD can't be explicitly specified in a recipe") and writes a diagnostic to
stderr with `eprintln!`. The error names neither the offending recipe nor its position;
`DefaultRecipeProvider` adds only the directory key.

## Impact

Every lookup in that directory fails, and in a long `recipes.yaml` the author has to search for the
entry that caused it. The stderr line does not reach a web or Python caller.

## Expected behaviour

Name the offending recipe (its query or filename, or its index) in the error, and drop the stderr
line in favour of the error itself.

## Discovery

Found while reviewing `reference/api/DOC_08_RECIPES_PLANS.md` against the code at `96279f14` before promoting it to [`reference/RECIPES_AND_PLANS.md`](../reference/RECIPES_AND_PLANS.md) (2026-10-20). Split off `RECIPE-LIST-SET-CWD-MUTATES-BEFORE-FAILING`, which shares the code but not the problem.
