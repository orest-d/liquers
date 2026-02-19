# volatility-system Design Tracking

**Created:** 2026-02-16

**Status:** In Progress

## Phase Status

- [x] Phase 1: High-Level Design (Approved 2026-02-17)
- [x] Phase 2: Solution & Architecture (Approved 2026-02-17)
- [x] Phase 3: Examples & Testing (Approved 2026-02-17)
- [x] Phase 4: Implementation Plan (Critical Issues Fixed 2026-02-17)
- [ ] Implementation Complete

## Notes

**Phase 2 Key Decisions:**
- Two-phase volatility check (commands during build, dependencies after)
- Circular dependency detection via stack tracking
- `make_plan()` is now async (breaking change)
- Volatile assets never cached in AssetManager
- `to_override()` handles all status transitions comprehensively

**Phase 3 Deliverables:**
- 67 test specifications (44 unit + 23 integration)
- 12 conceptual examples (primary/advanced/edge cases)
- Multi-agent review scores: Phase 1 conformity 92%, Phase 2 architecture 90%, Codebase validation 75%
- Query syntax notes for Phase 4, API verification checklist ready

**Phase 4 Critical Issues Fixed (2026-02-17):**
- CRITICAL-1: Step 13 now explicitly audits ALL make_plan() call sites including IsVolatile trait implementations
- CRITICAL-2: Step 10 revised to handle 'v' instruction at query-level (like 'q'/'ns'), not as regular action
- CRITICAL-3: Added Step 0 to remove default match arms (`_ =>`) in assets.rs before adding Status::Volatile
- CRITICAL-4: Added Step 0.5 to update downstream crate Status matches (liquers-lib, liquers-py)
- CRITICAL-5: Removed `#[serde(default)]` from MetadataRecord.is_volatile and Plan.is_volatile (always required)
- CRITICAL-6: Added clarifying note that Context does NOT implement Clone (manual construction via with_volatile)
- CRITICAL-7: Enhanced Prerequisites and Step 24 with comprehensive codebase verification checks

**Phase 4 Additional User Review Issues Fixed (2026-02-19):**
- Added Metadata.is_volatile() method with legacy support, AssetInfo.is_volatile field (Step 2.5)
- Removed incorrect Step 10 Approach 1 (no is_instruction method exists)
- Enhanced find_dependencies with cwd parameter, explicit Step handling, improved errors, no default match (Step 11)
- Added ResolvedParameterValues link volatility checking (Step 10.5)
- Added unit test for "action/v" vs "action/v/q" edge case (Step 22)
- Updated to_override to use existing cancel() method (Step 16)
- Verified Step 19 correctly sets both is_volatile=true AND status=Volatile
- Clarified Step 20: Context.is_volatile is for future side-effects (no evaluate() changes)
- Added recipe provider circularity/volatility guarantees (Step 14.5)
- Added PlanBuilder usage audit for direct usage bypassing make_plan (Step 13.5)
- Updated plan: 26→30 steps, 32h→38h, 8→9-10 days

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
