---
id: CORE-PLAN-RELATIVE-RESOLUTION-MISSING
kind: issue
title: Queries are not resolved relative to the current working directory
status: draft
priority: P1
complexity: M
area: [core/plan]
design: 
created: 2026-08-08
github:
---
## Problem

`liquers-core/src/plan.rs:1857` — `// TODO: Implement query.resolve_relative(cwd) or similar`. The
plan builder resolves *keys* against the current working directory (`:1739`) but not *queries*, so
`SetCwd` affects one and not the other.

## Impact

A recipe that sets a working directory and then refers to a nested query by a relative path
resolves it against the wrong base. The failure is silent — it produces a plan for a different
asset rather than an error.

## Expected behaviour

`Query::resolve_relative(cwd)` exists and the plan builder applies it wherever it applies the key
equivalent, with `SetCwd` affecting both consistently.

## Discovery

Migration triage, 2026-08-08. Source: `todo20260219.md` #7, work package WP-7. Verified against HEAD: marker present at `plan.rs:1857`. See `specs/DOCS_MIGRATION_PLAN.md` §4.0c.
