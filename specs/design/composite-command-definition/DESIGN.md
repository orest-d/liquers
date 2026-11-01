---
id: COMPOSITE-COMMAND-DEFINITION
kind: design
title: Pipeline commands and pipelines for aliases and recipes
workflow: liquers-project
status: in_review
phase: architecture
area: [core/commands, core/plan, core/assets]
issues: [COMMAND-CANNOT-BE-DEFINED-AS-A-COMMAND-SEQUENCE, RECIPE-OVERRIDE-ONLY-LAST-ACTION]
affects_docs: [reference/PIPELINE_COMMANDS.md, reference/COMMAND_ALIASES.md, reference/RECIPES_AND_PLANS.md, reference/PROJECT_OVERVIEW.md, reference/COMMAND_DECLARATION.md, guides/COMMAND_REGISTRATION_GUIDE.md, guides/COMMAND_DESIGN_GUIDE.md]
created: 2026-11-01
---
# Pipeline commands

A command defined as a sequence of other commands through a **pipeline** (a *pipeline command*): a structured,
serializable form of a query in which any argument of any step can be a value, a placeholder, a
link, a nested pipeline or the input state, and in which a value can become the current state.
Recipes use the same pipelines to pass complex data to any step. The planner mixes queries and
pipeline elements in one plan.

Decided on 2026-11-01: no placeholders in query text; the web API part moved to
`WEB-API-CANNOT-RUN-PIPELINES-OR-RECIPE-ENDPOINTS`. Phase 1 was approved on 2026-11-01, with the variant named
`CommandDefinition::Pipeline` (*pipeline command*; the folder keeps its original slug). Phase 2 is
in review.

## Phase Status

- [x] Phase 1: High-Level Design (approved 2026-11-01)
- [ ] Phase 2: Architecture (in review)
- [ ] Phase 3: Examples and Tests
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Pipelines, plans and recipes](./pipeline-and-plan.md) — pipeline vs plan, recipe order, lists, positional arguments (review 3, 2026-11-01)
- [Notation and integration analysis](./notation-and-integration.md) — syntax, pipeline notation, variadics, payload, recipes and web API (review of 2026-11-01)
