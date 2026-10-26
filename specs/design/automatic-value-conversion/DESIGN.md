---
id: AUTOMATIC-VALUE-CONVERSION
kind: design
title: The interpreter converts a command's state and link arguments to their declared types
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/commands, core/value, core/plan]
issues: [VALUE-NOT-CONVERTED-AUTOMATICALLY]
affects_docs: [specs/reference/VALUE_TYPE_SYSTEM.md, specs/guides/COMMAND_REGISTRATION_GUIDE.md, specs/guides/TYPE_SYSTEM_GUIDE.md]
created: 2026-10-26
---
# The interpreter converts a command's state and link arguments to their declared types

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented.

The automatic half of `VALUE-CONVERSION-CAPABILITY`. It requires two unimplemented designs:
`value-conversion-registry` (the `ConversionRegistry` and `ConversionPolicy::Automatic`) and
`command-io-type-declarations` (`accepts` on the state and arguments). It covers the two core
sites, state and link arguments; the UI-widget and HTTP-negotiation sites the issue also named are
split off as `UI-WIDGET-DOES-NOT-CONVERT-ITS-VALUE` and `HTTP-ACCEPT-HEADER-NOT-NEGOTIATED`.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
