---
id: TEMPLATE-DIAGNOSTICS-CHECK-COMMANDS-A-Q-QUOTES
kind: issue
title: Template diagnostics report unknown commands in a query part that a later q quotes, which the planner never resolves
status: draft
priority: P3
complexity: S
area: [core/query, lib/commands]
design:
created: 2026-10-26
github:
---

## Problem

The planner does not resolve the actions before a `q` instruction: `q` turns the query so far into
a value (`Step::UseQueryValue`), and "its commands are neither resolved nor validated (`nosuch/q`
plans without error)" (`RECIPES_AND_PLANS.md` §Planner instructions, "`q`").
`liquers-template/src/diagnostics.rs` (`unknown_commands`, `template_commands`) checks every action
of a placeholder query against the registry regardless of a later `q`, so it reports an error the
query does not have.

## Problem Example

`{{$ nosuch/q}}` in a template: `validate_template` reports "unknown command 'nosuch'", while
`liquers-validate -- 'nosuch/q'` is `Ok` and the placeholder renders the quoted query. Found by
reading the code at `70b9005`; not reproduced with a template test.

## Impact

A false error in the template editor and the `ns-template` validation for templates that pass a
quoted query to a command. Rare. Workaround: ignore the diagnostic.

## Expected behaviour

Actions of a segment that a later `q` in the same query quotes are not looked up (an `ns` among
them still applies to the actions after the `q`, as in the planner). Fits the per-segment walk that
`template-diagnostics-namespace-scope` introduces; that design leaves it out of scope.

## Discovery

2026-10-26, bulk design of `TEMPLATE-DIAGNOSTICS-RESOLVE-NAMESPACES-FROM-THE-WHOLE-QUERY`
(`template-diagnostics-namespace-scope`), comparing the diagnostics walk with the planner's
instruction rules.
