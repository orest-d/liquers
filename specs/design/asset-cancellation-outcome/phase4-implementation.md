# Phase 4: Implementation Plan — asset-cancellation-outcome

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
