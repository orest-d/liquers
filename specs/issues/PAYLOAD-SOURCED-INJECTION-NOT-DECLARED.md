---
id: PAYLOAD-SOURCED-INJECTION-NOT-DECLARED
kind: issue
title: An injection that reads the payload is indistinguishable from one that does not
status: draft
priority: P3
complexity: M
area: [core/plan, core/commands]
design: predecessor-cut-equivalence
created: 2026-08-26
github:
---
## Problem

`injected` on a command argument means `InjectedFromContext`: the value is produced from the
`Context`, which may mean the evaluation payload *or* the environment alone — `()` implements
the trait and reads no payload, and any environment-derived service is injected the same way.
The registration surface records only `ArgumentInfo::injected: bool`
(`liquers-core/src/command_metadata.rs:391`), so nothing downstream can tell the two apart.

That is deliberate and correct as far as it goes: an injected parameter must **not** imply
`payload_required`, because most injections are not payload reads, and `payload: required`
also sets `volatile` and makes `Recipe::to_plan_for_key` reject the recipe.

The gap is that there is no way to say the other thing either. A caller that needs to know
"does this step read the payload?" has only the over-approximation "does this step inject
anything?".

## Impact

One consumer today, and it is the reason this is filed rather than left implicit.
`predecessor-cut-equivalence` §2 forbids cutting an evaluation boundary across a
payload-sensitive step, because a boundary is a cache entry and a payload is not part of a
cache key. With only `injected` to go on, `PlanBuilder` must treat every injecting action as
payload-sensitive, so a command injecting an environment-only value blocks a boundary that
would have been perfectly safe.

The cost is a lost caching and parallel-scheduling opportunity, never a wrong result — the
over-approximation errs in the safe direction. Hence P3.

## Expected behaviour

The plan can distinguish a payload-sourced injection from an environment-sourced one, with
the fact declared where it is known rather than inferred from a type the macro cannot
inspect. Sketches, none preferred yet:

- A separate trait — `InjectedFromPayload` alongside `InjectedFromContext` — with the macro
  keying on which one the argument's type implements.
- A source marker in the DSL: `user_id: UserId injected from payload`, recorded as an
  `ArgumentInfo` field.
- Deriving it per parameter *type* in the registry rather than per command.

Whichever is chosen, the consumer changes by one predicate at the point that already holds
the metadata.

## Discovery

`predecessor-cut-equivalence` analysis, 2026-08-26. Filed first as
`INJECTED-PARAMETER-DOES-NOT-IMPLY-PAYLOAD-REQUIREMENT`, proposing the opposite — that
`injected` should imply the requirement. Corrected by the author: injection may be from the
environment only, and the right response to payload processing is to leave that part of the
plan expanded rather than to declare a requirement that does not hold. This issue is what
remains of the real gap after that correction.
