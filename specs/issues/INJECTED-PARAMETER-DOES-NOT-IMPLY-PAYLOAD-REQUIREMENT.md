---
id: INJECTED-PARAMETER-DOES-NOT-IMPLY-PAYLOAD-REQUIREMENT
kind: issue
title: Injected parameter does not imply payload requirement
status: draft
priority: P2
complexity: M
area: [core/plan, core/commands]
design: predecessor-cut-equivalence
created: 2026-08-26
github:
---
## Problem

`register_command!(cr, fn f(state, user_id: UserId injected) -> result)` registers a command
that reads the evaluation payload, but `CommandMetadata::payload_required` stays
`PayloadRequirement::None` unless the author also writes `payload: required`. The two facts
are independent where they should be one.

The declaration is what every payload-aware code path keys on: `Plan::payload_required`,
`Context::apply`, `Context::schedule_dependency_asset` and `Recipe::to_plan_for_key` all ask
the plan whether a payload is required and take a different route when it is. A command with
an injected parameter and no declaration is therefore invisible to all of them, and works
only by accident — because it happens to run in a context that already holds a payload.

It fails as soon as it does not. `liquers-core/tests/injection.rs::test_chained_commands_with_payload`
evaluates `/-/first_cmd/second_cmd/third_cmd`, where `first_cmd` takes an injected `UserId`
and declares nothing. Inlined it works; behind an evaluation boundary it fails with
`Command 'first_cmd' failed: No payload for UserId at position 4`, because the boundary is
scheduled as an ordinary payload-free dependency.

## Impact

A latent, position-dependent failure: the same command works or fails depending on where in a
query it lands, with a runtime error rather than a registration or planning one. Today the
boundary that exposes it is not switched on (`PREDECESSOR-CUT-NOT-YET-EQUIVALENT`), so the
exposure is limited to explicit nested evaluation — but it is exactly the class of defect
that appears only after someone else changes something unrelated.

## Expected behaviour

Not simply "infer it". Three obstacles make inference a design question rather than a
one-line change, and they are the reason this is filed instead of fixed:

1. `injected` means `InjectedFromContext`, which is *context* injection, not necessarily
   payload injection — `()` implements it and needs no payload at all. Inference would have
   to distinguish injections that read the payload from those that do not, which the trait
   does not currently express.
2. `payload: required` also sets `volatile` at registration time. Inference would silently
   make a large class of commands volatile, changing their caching.
3. `Recipe::to_plan_for_key` rejects any payload-requiring plan, on the sound ground that a
   key names one shared asset while a payload is per evaluation. Inference would start
   rejecting stored recipes that use injected parameters and work today.

A resolution therefore needs a way to say "this injection reads the payload" that is separate
from the volatility and keyed-recipe consequences — for instance splitting the trait, or
deriving the requirement per parameter type rather than per command.

## Discovery

`predecessor-cut-equivalence` analysis, 2026-08-26, measured by forcing `cut_predecessor` on
in `finalize_plan`. See that design's `analysis.md` §"Cause 2" for why forwarding the payload
across a boundary is unsound and the boundary is declined instead.
