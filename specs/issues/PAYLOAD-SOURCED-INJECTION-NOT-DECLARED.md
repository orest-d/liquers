---
id: PAYLOAD-SOURCED-INJECTION-NOT-DECLARED
kind: issue
title: An injection that reads the payload is indistinguishable from one that does not
status: rejected
priority: P3
complexity: M
area: [core/plan, core/commands]
design: predecessor-cut-equivalence
created: 2026-08-26
github:
---
## Problem

As filed: `injected` on a command argument means `InjectedFromContext`, which may be satisfied
from the evaluation payload *or* from the environment alone, and `ArgumentInfo` records only
`injected: bool` (`liquers-core/src/command_metadata.rs:391`). Nothing downstream can tell the
two apart, so a consumer asking "does this step read the payload?" has only the
over-approximation "does this step inject anything?".

## Why this is rejected

**The question was already answered elsewhere, exactly.** The payload need is declared on
command metadata as `CommandMetadata::payload_required`, read by
`PlanBuilder::action_payload_requirement` (`liquers-core/src/plan.rs:1268`) and ORed up into
`Plan::payload_required`. A command that reads the payload declares `payload: required`; that
is the existing "declare it, or lose it" rule, documented in
`specs/reference/api/DOC_08_RECIPES_PLANS.md` and pinned by `plan-cwd-freeze` Phase 3 as E8.

Injection is the *mechanism* by which an argument arrives, not evidence about the payload in
either direction. Nothing needs to derive one from the other, so there is no gap between them
to close.

The sole consumer this issue was filed for — `predecessor-cut-equivalence`, deciding where
an evaluation boundary may be cut — now reads the declaration directly, by building each
candidate boundary's plan and inspecting its `payload_required`. That is exact, so the
over-approximation this issue proposed to remove no longer exists to be removed.

A command that reads the payload without declaring it is a defect **in that command**, and
making the plan compensate for it would hide the defect rather than surface it. Should
enforcement of the declaration itself ever be wanted, that is a different issue — about
checking a declaration against an implementation, not about distinguishing injection sources.

## Discovery

Filed 2026-08-26 during `predecessor-cut-equivalence` analysis, first as
`INJECTED-PARAMETER-DOES-NOT-IMPLY-PAYLOAD-REQUIREMENT` proposing that `injected` should imply
the requirement, then reframed to this. Rejected the same day by the author: the payload need
is indicated on command metadata and there is no need to approximate it from injection.
