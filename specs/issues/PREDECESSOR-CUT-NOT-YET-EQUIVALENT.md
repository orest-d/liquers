---
id: PREDECESSOR-CUT-NOT-YET-EQUIVALENT
kind: issue
title: Cutting a predecessor boundary is not yet equivalent to expanding it
status: draft
priority: P1
complexity: M
area: [core/plan, core/assets]
design: predecessor-cut-equivalence
created: 2026-08-15
github:
---
## Problem

`Plan::cut_predecessor` works and is covered, but turning it on for every recipe still changes
observable behaviour. Measured by calling it from `finalize_plan` and running
`cargo test -p liquers-core --tests --no-fail-fast`:

| Suite | Failures under the cut |
|---|---|
| `--lib` | 1 — `absolute_outer_resource_keeps_relative_link_on_live_cwd`, which asserts the **expanded** plan shape (`steps[1]` is a `GetAsset`, an `Evaluate` once cut). Not a defect. |
| `injection` | 1 — `test_chained_commands_with_payload` |
| `recipe_cwd_resolution` | 2 — `programmatic_and_provider_cwd_select_their_own_inputs`, `recursive_links_and_multiple_parameters_use_active_cwd` |

Both remaining `recipe_cwd_resolution` failures are CWD-related, which points at a nested **keyed**
recipe re-deriving its own working key behind the boundary rather than inheriting the frozen one.
The `injection` failure is a payload crossing one more boundary than before.

Down from 11 divergences when the design started. Fixed along the way, each found by comparison
rather than analysis:

- A stale `predecessor_steps` across the recipe CWD prefix made a cut plan run the predecessor's
  action **twice**, once in the boundary asset and once inline.
- `Query::predecessor` splits a trailing *filename* off as the remainder, and the builder recorded
  the predecessor at every level of its recursion, so the outermost level overwrote the inner one
  with the whole action chain. Cutting then swallowed the last action and a recipe's overrides had
  nothing to patch. Fixed by recording only when the remainder is a real action. This also cleared
  the three `expiration_integration` divergences.
- A dependency's error was replaced by "did not produce a value", so a boundary hid the diagnosis.
  Fixed by chaining the cause.

## Update, 2026-08-26 (`predecessor-cut-equivalence`)

Re-measured at `d1bd02e` and root-caused. The four divergences come from three causes, and
only one of them is a defect:

- **The two `recipe_cwd_resolution` failures are one bug**, and it is not the nested keyed
  recipe this issue guessed at. `Plan::freeze_cwd_with` resolves the recorded predecessor
  query from the cursor's *entry* state, but `Recipe::to_plan` prepends a `Step::SetCwd` for
  `recipe.cwd` that the builder never emitted. The step count is compensated
  (`predecessor_steps += 1`); the cursor is not. So the boundary query — the only thing a cut
  carries — is frozen one CWD short, and every relative operand in it loses its folder
  prefix. Fix verified: `Plan::prologue_steps`, advanced over before the predecessor is
  resolved. Both failures pass; `liquers-core` stays green with the cut off, and
  `liquers-lib --lib --tests` is green with it on.
- **The `injection` failure is a mis-declared command**, and is fixed in the test.
  `first_cmd` and `third_cmd` read the payload through injected parameters and declare no
  `payload: required`; that is the documented "declare it, or lose it" rule (E8), which a cut
  is simply where it first bites. Injection must not be read as evidence of a payload need in
  either direction — it may be satisfied from the environment alone — and it need not be, since
  the need is declared on command metadata. What the failure *does* expose is a correctness
  question the design now answers: a boundary must be cut in front of a payload need, never
  across it, and `Plan::payload_required` is the wrong granularity to decide that. The cut
  builds each candidate boundary's own plan and steps back a level while it requires a payload.
- **The `--lib` failure is a test asserting the expanded shape**, as this issue already said.
  Measured: with those two shape assertions relaxed, the test passes under the cut with the
  same value and the same context CWD.

A fifth cause, found by reading and then measured: a **recipe-level** `volatile:` (or
`expires:`) is not in the query text, so it does not reach a cut boundary — the parent re-runs
while the boundary it reads is cached. Measured over two evaluations, the prefix ran 2 times
expanded and 1 cut. A **command**-level `volatile: true` does not diverge (2 both ways): it
travels in the query, so the boundary's own plan is volatile and the manager evaluates it as a
volatile query. The design cuts only at a candidate that can be cached — not payload-requiring,
not volatile — and records a recipe-level flag on the plan, where no candidate query could
show it.

A latent instance of the first cause's *shape* — a plan mutated through a subset of
coupled fields — sits in `Plan::split`, which drops `frozen_cwd` and both predecessor fields.
It has no production caller, but two of the three instances of that shape shipped, so it is in
scope: `PLAN-SPLIT-DROPS-PREDECESSOR-FIELDS`.

Design: `specs/design/predecessor-cut-equivalence/`. One issue filed and rejected the same day,
`PAYLOAD-SOURCED-INJECTION-NOT-DECLARED`.

## Impact

None today: nothing calls `cut_predecessor`, so the shipped behaviour is unaffected and the default
stays expanded. It blocks `CORE-PLAN-POLICY-AND-DEFAULTS` from flipping that default, which is the
reason the boundary machinery exists — intermediates that are individually cached, independently
expiring and separately schedulable.

## Expected behaviour

Cutting and expanding produce the same value, the same `is_volatile` / `payload_required` /
`expires`, and the same surfaced error, for every query shape. `specs/design/plan-cwd-freeze/`
Phase 3 specifies the suite that should establish this (twelve shapes, E1-E12, with E8 pinning the
one documented divergence — a payload command that omits `payload: required`). The failures above
are its starting worklist.

## Notes

`plan-cwd-freeze` Phase 2 concluded that cutting was a policy choice rather than a correctness one,
on the grounds that payload, volatility, side effects and cycles all reduce to declaration defects.
All three differences listed above were ones that analysis did not anticipate — which is the case
for building the comparison harness rather than reasoning about equivalence. The harness lives in
`liquers-core/src/interpreter.rs` as `evaluate_both_ways`; extending it to the remaining shapes is
the work this issue tracks.

## Discovery

`specs/design/plan-cwd-freeze/` implementation, 2026-08-15, by calling `cut_predecessor` from
`finalize_plan` and running the suite.
