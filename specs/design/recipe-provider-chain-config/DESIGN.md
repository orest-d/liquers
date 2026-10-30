---
id: RECIPE-PROVIDER-CHAIN-CONFIG
kind: design
title: The recipe provider chain is configured as an ordered list of provider entries
workflow: liquers-project
status: in_review
phase: architecture
area: [core/context, lib/commands, records, py, web]
issues: [RECIPE-PROVIDER-CHAIN-NOT-CONFIGURABLE, CONFIGURED-RECIPE-CHOICE-DROPS-MANIFEST-PROVIDER, PY-RECIPE-PROVIDER-NOT-CONFIGURABLE]
merged: 2026-10-30
affects_docs: [specs/reference/ENVIRONMENT_CONFIG.md, specs/reference/RECIPES_AND_PLANS.md, specs/reference/PYTHON_INTEGRATION.md, specs/guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md, specs/guides/PYTHON_GUIDE.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, specs/reference/JAVASCRIPT_INTEGRATION.md]
created: 2026-10-30
---
# The recipe provider chain is configured as an ordered list of provider entries

Raised by Orest on 2026-10-30 in place of the Pre-M4 fixes `kind-aware-recipe-provider-choice` and
`py-recipe-provider-choice`. Phase 1 approved 2026-10-30 (`proceed`), with every open question resolved as recommended. Phase 2 written and awaiting its gate.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
