---
id: PLAN-DEFAULT-LINK-PROMOTION-USES-THE-QUERYS-LAST-NS
kind: issue
title: Relative default links are promoted using the query's last ns, not each action's own
status: draft
priority: P3
complexity: S
area: [core/plan]
design:
created: 2026-11-01
github:
---

## Problem

`liquers-core/src/plan.rs` `promote_relative_default_links` walks every action of every transform
segment, but resolves each one with `namespaces_for_query(query, cmr)` on the **whole** query. That
uses the last `ns` of the query's last transform segment for every action, while the planner uses
the last `ns` before the action in its own segment (`RECIPES_AND_PLANS.md` §Planner instructions).

**Example.** A command `pl/f` with a relative default link argument (`dir: Key = query "-R-key/."`),
in `-R/a/-/ns-pl/f/-/to_text`: the last segment has no `ns`, so `f` is looked up in the default
chain only, is not found, and its relative default link is not promoted. Expected: `f` is found in
`pl` (as the planner finds it) and its default link is promoted and frozen. Not reproduced by a
test; found by reading the code.

## Impact

A relative default link of a namespaced command in a non-final segment stays implicit, so the
asset cache key does not record it, which is the hazard the promotion exists to remove. Rare:
it needs a namespaced command with a relative default link, used with `ns`, before a `-/`.

## Expected behaviour

Resolve each action with the explicit namespaces of its own segment up to that action, as
`PlanBuilder` and `liquers-template`'s diagnostics do.

## Discovery

Read while implementing `design/plan-default-namespaces/`, 2026-11-01.
