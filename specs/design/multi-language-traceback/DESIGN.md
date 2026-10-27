---
id: MULTI-LANGUAGE-TRACEBACK
kind: design
title: Traceback segments for errors that cross several languages
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/error, core/value, core/plan, py, web]
issues: [MULTI-LANGUAGE-TRACEBACK-IS-A-FLAT-STRING]
affects_docs: [specs/reference/PYTHON_INTEGRATION.md, specs/guides/PYTHON_GUIDE.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md]
created: 2026-10-27
---
# Traceback segments for errors that cross several languages

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-27: Phases 1-4 written and
reviewed without phase approval. Nothing is implemented. Recommended model: an ordered list of
`TracebackSegment { language, command, text }` beside `Error::traceback`, which stays as the
display string rendered from it; the interpreter attributes each segment to its command. The
maintainer's position of 2026-10-19 ("a flat string for now") makes the shape a decision
(Phase 1, Design Readiness Q1).

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
