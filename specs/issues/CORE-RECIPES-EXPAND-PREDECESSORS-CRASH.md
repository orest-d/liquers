---
id: CORE-RECIPES-EXPAND-PREDECESSORS-CRASH
kind: issue
title: `disable_expand_predecessors` crashes an evaluation test
status: draft
priority: P1
complexity: M
area: [core/assets, core/plan]
design: 
created: 2026-08-08
github:
---
## Problem

`liquers-core/src/recipes.rs:157` keeps a call commented out:
`// .disable_expand_predecessors(); // TODO: fix - evaluate_immediately unittest is crashing with
this option`.

## Impact

A supported plan-builder option cannot be used from the recipe path, and the reason is a crash
nobody has diagnosed. The commented-out line is the only record that the option is broken.

## Expected behaviour

Either the crash is fixed and the call restored, or the option is removed. A permanently
commented-out call with a TODO is neither.

## Discovery

Migration triage, 2026-08-08. Source: `todo20260219.md` #15, work package WP-7. Verified against HEAD: the commented call is still at `recipes.rs:157`. See `specs/archive/2026-08-08-docs-migration-plan.md` §4.0c.
