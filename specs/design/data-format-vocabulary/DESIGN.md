---
id: DATA-FORMAT-VOCABULARY
kind: design
title: A data-format vocabulary in liquers-core, with write-only formats and typo detection
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/value, lib/value, core/assets]
issues: [DATA-FORMAT-CONSTANTS-AND-TOOLING, TYPE-INFO-CANNOT-DECLARE-WRITE-ONLY-FORMATS]
merged: 2026-10-20
affects_docs: [specs/reference/VALUE_TYPE_SYSTEM.md, specs/guides/TYPE_SYSTEM_GUIDE.md, .claude/skills/liquers-validate/SKILL.md]
created: 2026-10-20
---
# A data-format vocabulary in liquers-core, with write-only formats and typo detection

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented. The two sources are designed together by maintainer decision (backlog compaction D8,
2026-10-08): `TYPE-INFO-CANNOT-DECLARE-WRITE-ONLY-FORMATS` is solved inside
`DATA-FORMAT-CONSTANTS-AND-TOOLING`. The abandoned `design/type-info-write-only-formats/` is input.

This design is the **first slice** of `DATA-FORMAT-CONSTANTS-AND-TOOLING` (its points 1-2 and the
write-only declaration). Points 3-5 are proposed as follow-up issues (Phase 1, question 1).

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
