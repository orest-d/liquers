---
id: SKILL-REFERENCES-TEACH-REMOVED-SYNC-STORE-WRAPPER
kind: issue
title: liquers-project references still teach the removed AsyncStoreWrapper and old design paths
status: draft
priority: P2
complexity: S
area: [docs]
design: 
created: 2026-10-08
github:
---
## Problem

**Example.** A designer following `.claude/skills/liquers-project/references/liquers-patterns.md`
§"Async Pattern" proposes a sync `AsyncStoreWrapper` around an `AsyncStore` for a Python binding.
That type no longer exists (`grep -rn AsyncStoreWrapper liquers-core/src` finds nothing), and
`CLAUDE.md` says the sync `Store` trait is obsolete and there is no wrapper. Expected: the skill
teaches the current rule, async only.

Stale guidance in the skill's references:

- `references/liquers-patterns.md`: lines ~209, ~397-440 (the "Default to Async, Sync Wrappers
  When Needed" pattern with an `AsyncStoreWrapper` code sample), ~637 and ~740.
- `references/phase1-template.md` (~236), `references/phase2-template.md` (~189, ~458) and
  `references/review-checklist.md` (~72) ask "async default, with sync wrappers if needed".
- `SKILL.md` §"Integration with Other Skills" cites `specs/<feature>/phase2-architecture.md`
  rather than `specs/design/<slug>/…`, and §"Migration Note" refers to the pre-2026-08 flat
  `specs/` layout.
- `references/liquers-patterns.md` §"UI Element Pattern (Phase 1 Established)" names a long-past
  phase of the UI work.

## Impact

Designs produced with the skill can specify a type that does not exist and contradict
`CLAUDE.md`. Reviewers catch it, but only after a phase has been written around it. No runtime
impact.

## Expected behaviour

The references state the current async-only rule (or point to `CLAUDE.md` instead of restating
it), and every path uses `specs/design/<slug>/`.

## Discovery

Found during a review of the `liquers-project` skill on 2026-10-08. The closed
`DOCS-ASYNC-STORE-WRAPPER-NO-LONGER-EXISTS` corrected three `specs/` documents but not the skill's
own references, so this is the same stale fact surviving in files that issue did not cover.
