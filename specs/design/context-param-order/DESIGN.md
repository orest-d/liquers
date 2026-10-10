---
id: CONTEXT-PARAM-ORDER
kind: design
title: Context parameter at any position in register_command!
workflow: liquers-project
status: complete
area: [core/commands, macro]
issues: [COMMAND-CONTEXT-PARAM-ORDER, MACRO-TESTS-PRINT-TO-STDOUT]
merged: 2026-10-10
affects_docs: [REGISTER_COMMAND_FSD, COMMAND_REGISTRATION_GUIDE, RECORD_STREAM_GUIDE]
created: 2026-10-10
---
# Context parameter at any position in register_command!

`context` may appear anywhere in a `register_command!` signature, with compile-time errors for
placements the macro cannot honour. Merged in: the macro's tests assert instead of printing,
argument `hint` options are rejected instead of discarded, and command-argument errors number
arguments from 1.

The folder dates from 2026-03-02. It was rewritten from scratch on 2026-10-10 under the
`liquers-project` workflow (hence `created`); its earlier findings and solution are archived as
`specs/archive/2026-09-02-context-param-order-{findings,solution}.md`. It began in the compact form
and was converted to the full form when its scope became cross-crate (Phase 1, Scope Changes).

Pre-approved after Phase 2 on 2026-10-10 (`proceed all`).

## Phases

- [x] [Phase 1: High-Level Design](phase1-high-level-design.md) — approved 2026-10-10
- [x] [Phase 2: Solution & Architecture](phase2-architecture.md) — approved 2026-10-10
- [x] [Phase 3: Examples and Tests](phase3-examples.md) — pre-approved
- [x] [Phase 4: Implementation Plan](phase4-implementation.md) — pre-approved; implemented
- [x] [Phase 5: Documentation](phase5-documentation.md) — approved 2026-10-10
