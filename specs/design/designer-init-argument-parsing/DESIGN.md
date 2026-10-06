---
id: DESIGNER-INIT-ARGUMENT-PARSING
kind: design
title: Design skill init scripts parse options and validate the slug
status: complete
readiness: ready
area: [docs]
issues: [DESIGNER-INIT-FEATURE-ACCEPTS-FLAGS-AS-NAMES]
created: 2026-10-06
---
# Design skill init scripts parse options and validate the slug

Produced under [`guides/autonomous_bulk_design.md`](../../guides/autonomous_bulk_design.md):
the first four phases, reviewed without phase approval. Not an approval and not an implementation.

## Phase Status

- [x] Phase 1: High-Level Design
- [x] Phase 2: Solution and Architecture
- [x] Phase 3: Examples and Tests
- [x] Phase 4: Implementation Plan

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)

## Implementation (2026-10-06)

Implemented as planned (Wave 0 of `archive/2026-10-06-p2-p3-s-implementation-order-revised.md`): `argparse` and the lowercase-kebab check in both skills' `init_feature.py` and both `validate_phase.py` (phase as `type=int` with `choices`). T1-T5 pass for both skills in a scratch directory; results are in the issue's resolution.
