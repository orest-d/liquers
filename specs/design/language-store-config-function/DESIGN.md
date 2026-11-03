---
id: LANGUAGE-STORE-CONFIG-FUNCTION
kind: design
title: store_config handlers defined in Python and JavaScript
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/store, store/config, py, web]
issues: [LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE]
affects_docs: [specs/reference/STORE_CONFIG_FSD.md, specs/guides/STORE_CONFIG_GUIDE.md, specs/reference/PYTHON_INTEGRATION.md, specs/guides/PYTHON_GUIDE.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md]
created: 2026-10-27
---
# store_config handlers defined in Python and JavaScript

**Review 2026-11-01.** Orest decided the placement of new registration calls: an `Environment` method, refused with `not_supported` on an
explicit `new Environment()` instance rather than acting on the singleton
(`WEB-EXPLICIT-ENVIRONMENT-CONFIGURES-THE-SINGLETON`), until instances can be configured. So the JavaScript entry point is
`Environment.registerStoreConfigHandler(declaration)`, not the module-level
`liquers.registerStoreConfigHandler` of Phases 1-4, which are to be updated when Phase 1 is taken
up. Also stale since 2026-11-01: `liquers-py` now has Python stores (`liquers-py-store`), so the
non-goal "a Python language store" no longer applies; a Python handler over a Python store should
be one of the tests, since that is the pair `STORE16` and `STORE18` are about.

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-27: Phases 1-4 written and
reviewed without phase approval. Nothing is implemented. Model: `StepBuilder` becomes an `Arc`'d
closure (source-compatible for every `fn` builder), and each binding registers its handlers into
`command_registry.store_config` on every build — `@env.store_config_handler` in Python,
`liquers.registerStoreConfigHandler` in JavaScript. Open: the binding API and the late-registration
rule (Phase 1, Design Readiness).

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
