---
id: PREDECESSOR-CUT-EQUIVALENCE
kind: design
title: Make cutting a predecessor boundary observably equivalent to expanding it
status: in_review
phase: architecture
area: [core/plan, core/assets, core/context]
issues: [PREDECESSOR-CUT-NOT-YET-EQUIVALENT, CORE-PLAN-POLICY-AND-DEFAULTS]
gh_pr:
created: 2026-08-26
superseded_by:
---
# predecessor-cut-equivalence

Follow-on to `plan-cwd-freeze`, which built the boundary machinery
(`Plan::freeze_cwd`, `Plan::predecessor`, `Plan::cut_predecessor`) and left it switched off
because cutting still changes observable behaviour. This design closes the remaining
divergences and builds the suite that keeps them closed.

Simplified transitional design: no `workflow:` marker. It produces
`analysis.md` (measurement and root causes) and `solution.md` (the change set), not the
five-phase set — the architecture it needs was already decided in `plan-cwd-freeze` Phase 2
and this is the correction pass over it.

## Phase status

- [x] Analysis — divergences re-measured at HEAD, root-caused, one fix verified
- [ ] Architecture (this document set) — awaiting approval
- [ ] Implementation

## Summary of findings

Re-measured at `d1bd02e` by calling `cut_predecessor` from `finalize_plan`:
**4 divergences, from 3 distinct causes**, matching the issue's table exactly.

| Cause | Divergences | Verdict |
|---|---|---|
| The predecessor query is frozen against the *entry* CWD, one step before the recipe's `SetCwd` prologue | 2 (`recipe_cwd_resolution`) | **Defect.** Fix verified — see `solution.md` §1 |
| A cut boundary is a cache entry, and a payload is deliberately not part of a cache key | 1 (`injection`) | **Not a defect.** Guard the cut instead — §2 |
| A test asserts the *expanded* plan's step shape | 1 (`--lib`) | **Not a defect.** Measured equivalent in value — §3 |

The first cause is a genuine equivalence bug and is the whole reason the two
`recipe_cwd_resolution` failures looked CWD-shaped. It was not "a nested keyed recipe
re-deriving its own working key", as the issue speculated; it is one missing cursor advance.

## Decisions

1. **`Plan` records its prologue explicitly** (`prologue_steps: usize`) rather than three
   places each inferring where the recipe prefix ends. Verified fix.
2. **`cut_predecessor` declines to cut a predecessor that reads the payload** rather than
   requiring every such command to declare `payload: required`. Cutting is a policy, so
   declining is always safe; declaring stays the opt-in that gets the boundary back.
   This turns E8 from "the two forms differ" into "the cut is refused, and here is why",
   which is a stronger falsifiable claim than the one Phase 3 wrote down.
3. **The equivalence suite gains a CWD axis.** The present harness always builds a recipe
   with no `cwd:` and passes `cwd: None`, so it structurally cannot reach the defect this
   design exists to fix. Every shape runs under three conditions: no CWD, a recipe `cwd:`,
   and a provider (keyed) recipe.
4. **The default stays expanded.** Flipping it is `CORE-PLAN-POLICY-AND-DEFAULTS`; this
   design only makes the flip safe to consider.

## Links

- [Analysis](./analysis.md) — the measurement, the three causes, and the worked example
- [Solution](./solution.md) — the change set, in landing order
- Predecessor design: [`specs/design/plan-cwd-freeze/`](../plan-cwd-freeze/DESIGN.md)
- Reference: `specs/reference/api/DOC_08_RECIPES_PLANS.md`, "Predecessor boundaries"
