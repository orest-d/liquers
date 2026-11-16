---
id: SETTABLE-VALUE-ACCESSOR
kind: design
title: Settable value accessor
workflow: liquers-project
status: complete
area: [core/value, core/assets, core/query, lib/ui, lib/value, records, axum, py, web]
issues: []
affects_docs: [specs/reference/VALUE_TYPE_SYSTEM.md, specs/reference/UI_INTERFACE_FSD.md, specs/reference/RECIPES_AND_PLANS.md, specs/reference/WEB_API_SPECIFICATION.md, specs/reference/PYTHON_INTEGRATION.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, specs/guides/PYTHON_GUIDE.md, specs/guides/JAVASCRIPT_GUIDE.md, specs/reference/VALUE_ACCESSORS.md, specs/guides/VALUE_ACCESSOR_GUIDE.md]
created: 2026-11-15
---
# Settable value accessor

A handle to the place a value lives: read it, write it, watch it, and project it to a part (a
recipe, a query step, an argument, a document field, a table cell). Supersedes `value-accessor`.

Phase 1 approved by Orest on 2026-11-15. Phase 2 approved, and Phases 3 and 4 pre-approved
(`proceed all`): Pre-approved after Phase 2 on 2026-11-16. Phase 5 approved by Orest on 2026-11-16;
the design is complete.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
