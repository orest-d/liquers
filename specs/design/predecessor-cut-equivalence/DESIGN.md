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
| A cut boundary is a cache entry, and a payload is deliberately not part of a cache key | 1 (`injection`) | **Not a defect** — a mis-declared command, fixed in the test. It exposes a real correctness question about *where* a boundary may go — §2 |
| A test asserts the *expanded* plan's step shape | 1 (`--lib`) | **Not a defect.** Measured equivalent in value — §3 |

A fifth cause was found later, by reading rather than by the suite: a recipe's own
`volatile:` / `expires:` does not travel into a boundary query, so a volatile recipe's
predecessor is computed once and cached. No existing test combines the two, which is why it is
not in the table. See `analysis.md` Cause 4 and open question 1.

A latent instance of the first cause's *shape* — a plan mutated through a subset of
coupled fields — sits in `Plan::split`; it is in scope for the same reason (§1b).

The first cause is a genuine equivalence bug and is the whole reason the two
`recipe_cwd_resolution` failures looked CWD-shaped. It was not "a nested keyed recipe
re-deriving its own working key", as the issue speculated; it is one missing cursor advance.

## Decisions

1. **`Plan` records its prologue explicitly** (`prologue_steps: usize`) rather than three
   places each inferring where the recipe prefix ends. Verified fix.
2. **A boundary is cut in front of the payload need, not across it.** The need is declared
   on command metadata and must not be inferred from `injected`, which may be satisfied from
   the environment. `Plan::payload_required` is a whole-query flag and is the wrong
   granularity: `cut_predecessor` instead builds each candidate boundary's own plan,
   recursing back a level while that plan requires a payload, and cuts at the first
   payload-free candidate — or not at all. This is a correctness rule, so it is in scope
   here, not deferred. Verified against the builder for all four chain shapes. No command
   declaration changes and no new `Plan` field; E8 stands as Phase 3 wrote it.
3. **The equivalence suite gains a CWD axis.** The present harness always builds a recipe
   with no `cwd:` and passes `cwd: None`, so it structurally cannot reach the defect this
   design exists to fix. Every shape runs under three conditions: no CWD, a recipe `cwd:`,
   and a provider (keyed) recipe.
4. **Coupled plan fields are carried by construction.** `Plan::split` drops `frozen_cwd`,
   `predecessor` and `predecessor_steps` because it copies a field list rather than cloning.
   In scope, not because that function has a caller — it has none outside tests — but because
   the field list is the shape of every defect this lineage has found, two of which shipped.
   The first half is exactly the predecessor's steps, so the fields are cleared rather than
   copied; `frozen_cwd` is carried.
5. **The default stays expanded.** Flipping it is `CORE-PLAN-POLICY-AND-DEFAULTS`; this
   design only makes the flip safe to consider. It does settle *where* a boundary goes when
   one is cut, because that turns out to be a correctness question (decision 2) rather than
   a policy one.

## Open questions

One is a decision the author owns; the rest are things to measure at implementation time rather
than unknowns about the approach.

| # | Question | Blocking? |
|---|---|---|
| 1 | **A recipe's own `volatile:` / `expires:` does not cross a boundary** (`analysis.md` Cause 4). Decline the cut for such a recipe, or accept it as the meaning of the flag? Propagating is not available — a boundary is shared by query identity and cannot carry a per-recipe policy. Recommendation (a), decline; it is the reversible one. | Yes — semantic call, `solution.md` §2b |
| 2 | §2's step-count assumption was measured on **raw** queries; the real input is promoted and frozen. Should hold, is guarded, but is reasoning rather than measurement. Failure mode is a silently lost boundary, not a wrong value. | No — first implementation step |
| 3 | `with_placeholders_allowed()` on §2's rebuild is in the sketch but not established. A recorded predecessor should be placeholder-free, since overrides patch the tail. | No |
| 4 | "Equivalent" is defined as Phase 3's four properties. Cutting changes asset count, dependency edges and metadata *by design*; §4 now says so explicitly rather than leaving the next author to discover it. | No — confirm at review |

Nothing here changes the shape of the solution. Question 1 changes whether one class of recipe
is cuttable at all.

## Links

- [Analysis](./analysis.md) — the measurement, the three causes, and the worked example
- [Solution](./solution.md) — the change set, in landing order
- Predecessor design: [`specs/design/plan-cwd-freeze/`](../plan-cwd-freeze/DESIGN.md)
- Reference: `specs/reference/api/DOC_08_RECIPES_PLANS.md`, "Predecessor boundaries"
