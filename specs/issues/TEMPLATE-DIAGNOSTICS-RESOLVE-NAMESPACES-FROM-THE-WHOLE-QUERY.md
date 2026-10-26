---
id: TEMPLATE-DIAGNOSTICS-RESOLVE-NAMESPACES-FROM-THE-WHOLE-QUERY
kind: issue
title: Template diagnostics look every command up in one namespace list for the whole query
status: draft
priority: P3
complexity: S
area: [core/query, lib/commands]
design:
created: 2026-10-26
github:
---

## Problem

`liquers-template/src/diagnostics.rs` checks the commands of a `{{$ query}}` placeholder against the
registry, but chooses namespaces differently from the planner (`RECIPES_AND_PLANS.md` §Planner
instructions: only the last `ns` *before an action, in the same transform segment* applies):

- `unknown_commands` computes one list, `query.last_ns()` (the last `ns` of the **last** segment)
  plus the defaults, and checks every action of every segment against it;
- `template_commands` (placeholder bodies) inserts every literal `ns` parameter of every segment in
  front of the defaults, so all of them apply everywhere.

Inferred from reading the code at cd6d5da; not reproduced with a test.

## Problem Example

`head` is registered in namespace `pl` only. The placeholder `{{$ ns-pl/head-3/ns-img/x}}` plans
(the planner looks `head` up in `pl`), but `unknown_commands` looks it up in `[img, defaults]` and
reports "unknown command 'head'". Likewise `{{$ ns-pl/head-3/-/x}}`: the last segment has no `ns`,
so `head` is looked up in the defaults only. In the other direction, a template body with
placeholders accepts a command from any `ns` anywhere in the query, which the planner would refuse.

## Impact

False "unknown command" errors for valid multi-`ns` and multi-segment queries in templates, and
missed errors in placeholder bodies. Templates using a single leading `ns` are unaffected.

## Expected behaviour

Resolve each action's namespaces as the planner does: the parameters of the last `ns` before it in
its transform segment, then the registry's defaults. Either a shared public helper in `liquers-core`
(the planner's `namespaces_for_query` applied to the action's prefix), or the same rule written
once in `diagnostics.rs`. `find_next_presets` has the same rule
(`next-presets-follow-planner-rules`).

## Discovery

Found on 2026-10-26 during the bulk design of `PLANNER-INSTRUCTION-NAMES-DUPLICATED` and
`NEXT-PRESETS-MISREAD-PLANNER-INSTRUCTIONS`, while listing every copy of the planner's instruction
handling. Related: `planner-instruction-names-public` (same file, instruction names only).
