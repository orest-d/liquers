---
id: RECIPE-CANNOT-PARAMETERIZE-RESOURCE-KEY
kind: issue
title: A recipe can leave an action parameter open for an override, but not the key of a resource segment
status: draft
priority: P3
complexity: L
area: [core/query, core/plan]
design: agent-memory-mvp
created: 2026-10-17
github:
---
# A recipe can leave an action parameter open for an override, but not the key of a resource segment

## Problem

`Recipe::to_plan` builds with `PlanBuilder::with_placeholders_allowed()`: a missing action
parameter becomes `ParameterValue::Placeholder(name)` (`liquers-core/src/plan.rs:422`) and a named
entry of `Recipe.arguments` fills it. There is no equivalent for the key of a resource segment.
A recipe or a tool that should act on "the document named by the caller" — `-R/<key>/-/ns-mem/read`
— cannot be expressed as one recipe with a hole; the caller has to build the query.

Building it by editing query text is unsafe: a key such as `x/-/ns-mem/write-…` changes the
command (query injection), and `..` or a `-`-prefixed segment changes what is addressed.

## Impact

`agent-memory-mvp` (Phase 2, decision 1) serves Liquers-style commands as MCP tools and needs the
tool's key argument to become the state's key. It designs around the gap: `liquers-mcp` parses the
tool query once and joins a validated `Key` onto its resource segment structurally (`fill_key`).
`liquers-template`'s hygienic placeholders do the same for one segment (P4). Two crates now hold
their own copy of a mechanism that belongs with `Recipe`.

## Expected behaviour

A recipe can declare a key placeholder in a resource segment, filled from `Recipe.arguments` with
the validation above, so a tool or template is a `Recipe` with named holes. Together with
`RECIPE-OVERRIDE-ONLY-LAST-ACTION` (filed with `liquers-mustache`, PR #139), which would let overrides reach any action, this would retire
`fill_key`. Open design questions: a text syntax for the hole (an encoding change, touching
`query.rs`, `parse.rs`, `plan.rs` and `PROJECT_OVERVIEW.md`) versus a structural declaration in
`Recipe`; whether a placeholder may stand for one segment or for a whole relative key.

## Discovery

Found 2026-10-17 while revising `agent-memory-mvp` Phase 2 decision 1 (option C in its discussion).
Excluded from that design for now by Orest.
