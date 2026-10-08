# Phase 4 template: implementation plan

**Steps.** Three pages at most. Each step can be executed and checked on its own. Re-open every
signature a step depends on before writing it. File: `phase4-implementation.md`.

```markdown
# Phase 4: Implementation Plan — <name>

## Overview
<What gets built, in what order, and why that order. Prerequisites: issues to fix first.>

## Progress
- [ ] Step 1: <action>
- [ ] Step 2: <action>

## Implementation Steps
### Step 1: <action>
- Files / symbols: `<path>` `<symbol>`
- Change: <what changes; new or changed signatures only>
- Depends on: <earlier steps or none>
- Proof: `<cargo check / test command>` and the Phase 3 test it makes pass
- Rollback: <how to undo or contain it>

<Repeat. Mark steps that can run in parallel.>

## Testing Plan
<When each test group runs, and the final proportionate check (crate tests, feature matrix
`scripts/check-build-matrix.sh` when a `cfg(feature)` or optional dependency changes).>

## Rollback Plan
<Whole-change rollback, and what happens when only some steps landed.>

## Documentation Updates
<Documents to create or update (from Phase 2's documentation architecture), regenerated files
(`specs/index.csv`, `specs/command_registry.yaml`), History rows and `reviewed:` bumps.>

## Phase 5 Entry Criteria
- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
```

**Progress** (`DOCS_STRUCTURE_GUIDE.md` §5.2.1) has one item per `### Step N`, in the same order.
During implementation, tick a step when its proof passes and append the commit:
`- [x] Step 2: add the reader — a1b2c3d`. Ticking is progress, not a design change: it needs no
re-approval and is allowed after Phase 4 is approved. A session resuming the work, or a spin-off
taking over part of it, starts at the first unticked step. Before Phase 5 every step is ticked, or
the unfinished remainder has become an issue (§5.6) and the item says so.

Designs without `workflow: liquers-project` (bulk design) write `None — four-phase design` under
Phase 5 Entry Criteria.
