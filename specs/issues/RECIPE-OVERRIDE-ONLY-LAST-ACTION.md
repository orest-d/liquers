---
id: RECIPE-OVERRIDE-ONLY-LAST-ACTION
kind: issue
title: Argument overrides can target only the last action of a query
status: draft
priority: P3
complexity: M
area: [core/plan]
design: recipe-override-addressing
created: 2026-10-17
github:
---

## Problem

`Plan::override_value` and `Plan::override_link` change only the last action of a plan, as the
module docs of `liquers-core/src/plan.rs` state. `Recipe::to_plan` (`liquers-core/src/recipes.rs`)
looks argument names up in the last action's command and refuses any other name with
`not_in_last_action`. A value that cannot be written as query text, such as a list, a map or a long
text, can therefore be passed to the last command of a query and to no earlier one.

**Example (verified at HEAD 18732dd, 2026-10-20).** A `recipes.yaml` entry
`-R/data/sales.csv/-/ns-pl/from_csv/is_in-region/head-10/eu.csv` with `arguments: {values: [EU, US]}`
fails `liquers-validate --recipes` with `ParameterError: Argument 'values' is not an argument of the
last action`; the list is meant for `is_in`. Should: an argument name can address `is_in`.

## Impact

`liquers-mustache` turns a non-scalar placeholder into an override (design AC-16). Until this is
fixed, such a placeholder in an earlier action is an error, so a template author must split the
query or move the command to the end. HTTP named arguments (`design/axum-query-named-arguments/`)
have the same limit. Workaround: put the command that needs the structured argument last, and
continue in a second query that links the first.

## Expected behaviour

An override can address an action other than the last one. Positional addressing (the slot of a
given parameter) fits templates, because a placeholder knows its action and parameter index.
Addressing by name needs a way to say which action, since names repeat across actions.

## Discovery

Verified on 2026-10-17 while revising `liquers-mustache` Phase 1, after the maintainer noted that
overrides are "currently only supported in the last action".

## Design

Designed together with `RECIPE-CANNOT-PARAMETERIZE-RESOURCE-KEY` in
`specs/design/recipe-override-addressing/` (bulk design 2026-10-20; strong overlap T2/T3). Still
reproduces at HEAD: `Plan::override_value` / `override_link` (`liquers-core/src/plan.rs`) and
`Recipe::to_plan` / `not_in_last_action` (`liquers-core/src/recipes.rs`) are unchanged.
