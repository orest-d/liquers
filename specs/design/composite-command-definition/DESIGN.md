---
id: COMPOSITE-COMMAND-DEFINITION
kind: design
title: Pipelines - pipeline commands, recipe pipelines and the pipeline as query builder and editor
workflow: liquers-project
status: in_review
phase: high-level
area: [core/commands, core/plan, core/assets, core/query, core/validate, web]
issues: [COMMAND-CANNOT-BE-DEFINED-AS-A-COMMAND-SEQUENCE, QUERY-BUILDER-TOOLING, RECIPE-OVERRIDE-ONLY-LAST-ACTION]
merged: 2026-11-02
affects_docs: [reference/PIPELINES.md, guides/PIPELINE_GUIDE.md, reference/PROJECT_OVERVIEW.md, reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md, reference/COMMAND_ALIASES.md, reference/COMMAND_DECLARATION.md, reference/RECIPES_AND_PLANS.md, reference/TEMPLATE_LANGUAGE.md, guides/COMMAND_REGISTRATION_GUIDE.md, guides/COMMAND_DESIGN_GUIDE.md, guides/QUERY_ESCAPING_GUIDE.md]
created: 2026-11-01
---
# Pipelines

A command defined as a sequence of other commands through a **pipeline** (a *pipeline command*): a structured,
serializable form of a query in which any argument of any step can be a value, a placeholder, a
link, a nested pipeline or the input state, and in which a value can become the current state.
Recipes use the same pipelines to pass complex data to any step. The planner mixes queries and
pipeline elements in one plan.

Decided on 2026-11-01: no placeholders in query text; the web API part moved to
`WEB-API-CANNOT-RUN-PIPELINES-OR-RECIPE-ENDPOINTS`. Phase 1 was approved on 2026-11-01, with the variant named
`CommandDefinition::Pipeline` (*pipeline command*; the folder keeps its original slug).

**Scope extended on 2026-11-02.** The query builder (`QUERY-BUILDER-TOOLING`, design
`query-builder`, now superseded) is merged in: the pipeline converts to and from a query, lives in
its own module, and is edited through `PipelineEditor`, with `QueryEditor` as a wrapper that keeps
it convertible to a query. Phase 1 is back in review for the extension (AC-25–AC-36, questions
20–25). The Phase 2 draft predates it and is revised after re-approval.

## Phase Status

- [ ] Phase 1: High-Level Design (approved 2026-11-01; reopened 2026-11-02 for the scope extension)
- [ ] Phase 2: Architecture (draft; revised after Phase 1 re-approval)
- [ ] Phase 3: Examples and Tests
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md) — draft of 2026-11-01, before the scope extension
- Superseded and merged: [query-builder](../query-builder/DESIGN.md) (2026-11-02)
- [Pipelines, plans and recipes](./pipeline-and-plan.md) — pipeline vs plan, recipe order, lists, positional arguments (review 3, 2026-11-01)
- [Notation and integration analysis](./notation-and-integration.md) — syntax, pipeline notation, variadics, payload, recipes and web API (review of 2026-11-01)
