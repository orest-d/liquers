---
id: LIQUERS-PY-RECIPES
kind: design
title: Recipe providers written in Python
workflow: liquers-project
status: in_review
phase: implementation
readiness: ready
autofix: not-eligible
area: [py, core/assets, core/context]
issues: [PY-NO-LANGUAGE-DEFINED-RECIPE-PROVIDER]
affects_docs: [specs/reference/PYTHON_INTEGRATION.md, specs/reference/ENVIRONMENT_CONFIG.md, specs/reference/RECIPES_AND_PLANS.md, specs/guides/PYTHON_GUIDE.md, specs/guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, liquers-py/README.md, specs/README.md]
created: 2026-10-31
---
# liquers-py-recipes — recipe providers written in Python

The `RECIPE` feature of liquers-py, milestone M4 of the liquers-py plan together with
[`liquers-py-store`](../liquers-py-store/DESIGN.md). It builds on the configurable recipe provider
chain (`recipe-provider-chain-config`, PR #224): Python registers recipe provider *factories*, each
an entry type of `config["recipes"]`, declared through `EnvironmentBuilder::with_recipe_provider_factory`
(the plan's M4 says `with_appended_recipe_provider`; PR #224 made the factory the route for a
configured entry, so this deliberately updates the plan). Full form since 2026-11-01: the source issue was `M` and is now `L` (Phase 1, Scope Changes).

Phase 1 approved by Orest on 2026-11-01 (`proceed all`). Pre-approved after Phase 1 on 2026-11-01: Phases 2-4, the implementation and Phase 5, with a stop before implementation only if the decision log needs a decision. (The skill offers `proceed all` at the Phase 2 gate; Orest gave it at Phase 1, so the remaining gates were run with their two reviewer passes and no stop.)

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
