---
id: GENERIC-SERDE-DATA-FORMATS
kind: design
title: Generic serde data formats (YAML, CBOR, MessagePack) for serde-capable value types
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/value, lib/value]
issues: [SERDE-TYPES-HAVE-NO-GENERIC-DATA-FORMATS, NO-COMPACT-BINARY-DATA-FORMAT]
merged: 2026-10-26
affects_docs: [specs/guides/SERIALIZATION_GUIDE.md, specs/reference/VALUE_TYPE_SYSTEM.md, specs/guides/TYPE_SYSTEM_GUIDE.md]
created: 2026-10-26
---
# Generic serde data formats (YAML, CBOR, MessagePack) for serde-capable value types

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented.

The two sources strongly overlap (`references/overlap.md` T2 and T4):
`SERDE-TYPES-HAVE-NO-GENERIC-DATA-FORMATS` has to decide which generic formats exist, and
CBOR / MessagePack are both on its list. `NO-COMPACT-BINARY-DATA-FORMAT` asks for a compact binary
format, which is that same decision made for the binary case. The merged scope stays `L` (E2 does
not apply), because the compact-format evaluation reduces to choosing a self-describing format that
the generic path already carries. `SERDE-TYPES-HAVE-NO-GENERIC-DATA-FORMATS` is the leading source.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
