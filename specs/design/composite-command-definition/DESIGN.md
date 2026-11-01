---
id: COMPOSITE-COMMAND-DEFINITION
kind: design
title: Composite commands and pipelines for aliases and recipes
workflow: liquers-project
status: in_review
phase: high-level
area: [core/commands, core/plan, core/assets]
issues: [COMMAND-CANNOT-BE-DEFINED-AS-A-COMMAND-SEQUENCE, RECIPE-OVERRIDE-ONLY-LAST-ACTION]
affects_docs: [reference/PIPELINES_AND_COMPOSITES.md, reference/COMMAND_ALIASES.md, reference/RECIPES_AND_PLANS.md, reference/PROJECT_OVERVIEW.md, reference/COMMAND_DECLARATION.md, guides/COMMAND_REGISTRATION_GUIDE.md, guides/COMMAND_DESIGN_GUIDE.md]
created: 2026-11-01
---
# Composite commands and pipelines

A command defined as a sequence of other commands through a **pipeline**: a structured,
serializable form of a query in which any argument of any step can be a value, a placeholder, a
link, a nested pipeline or the input state, and in which a value can become the current state.
Recipes use the same pipelines to pass complex data to any step. The planner mixes queries and
pipeline elements in one plan.

Decided on 2026-11-01: no placeholders in query text; the web API part moved to
`WEB-API-CANNOT-RUN-PIPELINES-OR-RECIPE-ENDPOINTS`. Phase 1 is in review. The Phase 2 draft
predates those decisions and is rewritten after Phase 1's approval.

## Phase Status

- [ ] Phase 1: High-Level Design (in review)
- [ ] Phase 2: Architecture (superseded draft; rewritten after Phase 1)
- [ ] Phase 3: Examples and Tests
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Pipelines, plans and recipes](./pipeline-and-plan.md) — pipeline vs plan, recipe order, lists, positional arguments (review 3, 2026-11-01)
- [Notation and integration analysis](./notation-and-integration.md) — syntax, pipeline notation, variadics, payload, recipes and web API (review of 2026-11-01)
