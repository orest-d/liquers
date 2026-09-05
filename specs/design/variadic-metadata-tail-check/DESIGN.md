---
id: VARIADIC-METADATA-TAIL-CHECK
kind: design
title: Runtime validation of variadic command metadata
workflow: liquers-project
status: complete
area: [core/commands, core/context, core/validate]
issues: [VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS]
affects_docs: [specs/reference/COMMAND_DECLARATION.md, specs/guides/COMMAND_REGISTRATION_GUIDE.md, specs/guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md]
gh_pr: []
created: 2026-08-29
superseded_by:
---
# variadic-metadata-tail-check Design Tracking

The completed `variadic-arguments-declaration` design guards macro-registered commands. This
design covers the remaining hand-built and deserialized `CommandMetadata` path.

## Phase Status

- [x] Phase 1: High-Level Design
- [x] Phase 2: Solution & Architecture
- [x] Phase 3: Examples and Tests
- [x] Phase 4: Implementation
- [x] Phase 5: Documentation

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
