---
id: COMPOSITE-COMMAND-DEFINITION
kind: design
title: Composite commands and query placeholders for aliases, recipes and the web API
workflow: liquers-project
status: in_review
phase: high-level
area: [core/commands, core/plan, core/query, core/assets, axum]
issues: [COMMAND-CANNOT-BE-DEFINED-AS-A-COMMAND-SEQUENCE]
affects_docs: [reference/COMPOSITE_COMMANDS.md, reference/COMMAND_ALIASES.md, reference/RECIPES_AND_PLANS.md, reference/WEB_API_SPECIFICATION.md, reference/PROJECT_OVERVIEW.md, reference/COMMAND_DECLARATION.md, guides/COMMAND_REGISTRATION_GUIDE.md, guides/COMMAND_DESIGN_GUIDE.md, guides/WEB_API_GUIDE.md]
created: 2026-11-01
---
# Composite commands and query placeholders

A command defined as a sequence of other commands, with `$name` placeholders that may stand for any
parameter, `$state` to pass the input state to an argument, and a leading `$name/…` to start from an
argument. The same placeholders let recipes bind complex values anywhere in their query and let
HTTP requests bind them, including the value a query starts from.

Phase 2 was drafted together with Phase 1, before Phase 1's gate, so the three uses (registry,
recipes, web API) can be reviewed against one mechanism. Phase 1 is the phase awaiting approval;
answers to its open questions may reopen Phase 2.

## Phase Status

- [ ] Phase 1: High-Level Design (in review)
- [ ] Phase 2: Architecture (drafted, not yet at its gate)
- [ ] Phase 3: Examples and Tests
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
