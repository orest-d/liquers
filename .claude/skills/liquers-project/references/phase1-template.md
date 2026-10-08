# Phase 1 template: high-level design

**What and why.** One page, excluding the problem example and the readiness section. No code
structure; that is Phase 2. File: `specs/design/<slug>/phase1-high-level-design.md`. For a compact
design use [`compact-design-template.md`](compact-design-template.md) instead.

```markdown
# Phase 1: High-Level Design — <name>

## Purpose
<1-3 sentences: the problem this solves and for whom.>

## Problem Example
<One concrete case: the input, query, call or scenario; what happens today; what should happen.
For a new capability: what the user cannot do today, and how it reads once it exists.>

## Scope and Acceptance Criteria
- <Testable criterion. Each one gets at least one Phase 3 test.>
- Non-goals: <what this deliberately does not do>

## Core Interactions
<Only the systems this touches, one line each: Query, Store, Commands, Assets, Value types, Web/API,
UI, bindings. Omit the ones it does not touch.>

## Crate Placement
<Crate(s) and why, respecting the dependency flow in CLAUDE.md.>

## Documentation Intent
- Reference: <new / extend <path> / none — why>
- Guide: <new / extend <path> / none — why>
- Other documents: <list or none>
- Documents to update: <paths or none>

## Open Questions
1. <Question, with a recommended answer when there is one.>

## References
- <Related issues, designs, reference documents.>
```

Add when they apply:

- `## Design Readiness`: required when `DESIGN.md` carries `readiness` (bulk design, triage,
  compaction), and used as the decision log under `proceed all`. Fields and question tiers:
  `specs/guides/autonomous_bulk_design.md` §3, plus the **Automatic fixing** line
  ([`auto-fix.md`](auto-fix.md)). It replaces `## Open Questions`.
- `## Design Dependencies`: other designs this requires, is required by, is covered by, or overlaps
  (bulk-design guide §6).
- `## Scope Changes`: one entry per extension after the design started ([`issue-triage.md`](issue-triage.md)
  case 1 or 2). Give the date, what was added and why, its example, the effect on criteria and size,
  the phases updated, and the approval decision.
- `## Consolidated Findings`: written at the final review (bulk-design guide §10).
