# Bulk design

**Triggers:** "bulk-design" or "bulk design", in any case, with a hyphen or a space, followed by a
list of issue or feature IDs, a filter ("all draft S issues in `core/assets`"), or nothing (meaning
every eligible open issue and feature without a finished Phase 4).

The binding procedure is [`specs/guides/autonomous_bulk_design.md`](../../../../specs/guides/autonomous_bulk_design.md).
Read it completely before starting; this file does not restate it. It produces Phases 1-4 for each
item without phase approval, labels readiness, and **never implements**.

## What this skill adds to the guide

1. **Overlap at intake.** In preflight step 5 (grouping), apply [`overlap.md`](overlap.md) across the
   selected items and against open designs. Strongly overlapping sources share one design
   (`merged:` set, leading source first), and an item that strongly overlaps an open design is
   attached to it, as in [`issue-triage.md`](issue-triage.md) cases 2-3. Exclusions E1-E5 apply.
   Weak overlap is a Design Dependency, never a merge.
2. **Problem example.** Every Phase 1 has `## Problem Example` (see the Phase 1 template). An issue
   whose body lacks one gets it added in the same change.
3. **Automatic-fix label.** Every design's `## Design Readiness` carries the
   **Automatic fixing** line ([`auto-fix.md`](auto-fix.md)).
4. **All phases where possible.** Prefer carrying an open question through Phases 3-4 as an explicit,
   recommended assumption (readiness `needs-decision`) over stopping at `phase2-blocked`. Use
   `phase2-blocked` only when no working solution can be stated even under an assumption, which is
   what guide §8 already requires.
5. **Phase 1 template.** Use this skill's `references/phase1-template.md` sections, plus the guide's
   Design Readiness, Design Dependencies and Consolidated Findings sections. Designs from this
   procedure omit `workflow:` (guide §5).
6. **Report.** The guide's §13 closing report, plus two lists: items **eligible for automatic
   fixing**, and **decisions needed**, grouped by design with the recommended answer. Offer to fix
   the eligible ones as spin-offs ([`spin-off.md`](spin-off.md)), but do not start without the
   user's go-ahead, because bulk design does not authorize implementation.

## Scale

Work in bounded batches (guide §4). Where the host supports parallel agents or child sessions, one
agent per independent cluster is appropriate. Clusters that share a design or a `requires` edge go
to the same agent, in dependency order.
