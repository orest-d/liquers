---
id: COMMAND-ALIAS-CONTRACT
kind: design
title: A specified, validated and exercised contract for command aliases
workflow: liquers-project
status: complete
area: [core/plan, core/commands]
issues: [COMMAND-ALIAS-DEFINITION-UNTESTED]
affects_docs: [specs/guides/COMMAND_REGISTRATION_GUIDE.md, specs/guides/COMMAND_DESIGN_GUIDE.md, specs/reference/COMMAND_DECLARATION.md, specs/reference/POLARS_COMMAND_LIBRARY.md, specs/reference/api/DOC_08_RECIPES_PLANS.md]
created: 2026-10-09
---
# A specified, validated and exercised contract for command aliases

Full form (converted from compact on 2026-10-10; see Phase 1 §Scope Changes).

## Phases

1. [High-level design](phase1-high-level-design.md) — approved 2026-10-10
2. [Architecture](phase2-architecture.md) — approved 2026-10-10
3. [Examples and tests](phase3-examples.md) — approved 2026-10-10
4. [Implementation plan](phase4-implementation.md) — implemented
5. [Documentation](phase5-documentation.md) — approved 2026-10-10

## Pre-approval

Pre-approved after Phase 3 on 2026-10-10 (`proceed all`): Phase 4, the implementation and Phase 5.

Decision log (none needs the user):
- Implementation detail: the full build matrix is replaced by the two liquers-lib configurations
  that bracket the one feature gate involved (Phase 4 §Testing Plan).
- Implementation detail: `liquers-py` is checked only if the toolchain can build it.
