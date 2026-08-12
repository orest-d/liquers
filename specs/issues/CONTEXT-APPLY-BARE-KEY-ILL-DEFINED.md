---
id: CONTEXT-APPLY-BARE-KEY-ILL-DEFINED
kind: issue
title: Context::apply with a bare key discards the input state and writes to the store
status: accepted
priority: P0
complexity: S
area: [core/assets]
design:
created: 2026-08-09
github:
---

## Problem

`Context::apply(&query, state)` (`liquers-core/src/context.rs:597`) applies a query to a supplied
input state as an ad-hoc asset. If the query is a **bare key** — `-R/some/file.txt`, no actions —
the result is not what the signature suggests:

1. `Recipe::key()` returns the key, so `evaluate_recipe` takes the keyed path and the supplied
   input state is never applied to anything. It is silently dropped.
2. The ad-hoc asset then delegates to the key's registered owner, which currently fails with a
   spurious cycle (`ASSET-KEYED-DELEGATION-ALWAYS-CYCLES`), or evaluates the key's recipe itself
   when nothing is registered.
3. Either way `evaluate_and_store` persists, and `save_to_store` targets
   `recipe.key().or(recipe.store_to_key())` (`:2105`) — so the ad-hoc result is **written to the
   store under that key**, with status `Ready`, which `try_fast_track` will later accept as the
   key's value.

So a call whose evident meaning is "apply this query to this state" instead reads (or recomputes) a
stored resource, ignores the state it was given, and may overwrite the resource.

## Impact

Low reachability, and no known caller does it — `Context::apply`'s in-tree uses all pass action
queries. But nothing rejects it, the failure is silent in the most important respect (the input
state simply vanishes), and step 3 is a **store write**, which is the part that turns a confused
call into a durable one.

## Expected behaviour

Pick one and make it explicit:

1. **Reject it.** `Context::apply` with a query whose `key()` is `Some` is a caller error;
   returning an `Error` naming the query is honest and costs nothing. A caller that wants the key's
   value should use `Context::evaluate` or the manager's `get`.
2. **Define it as "read the key".** Legitimate, but then the input state should be documented as
   ignored, and the ad-hoc asset must not persist — writing back a value it did not compute is
   never right.

(1) is preferable: the query language already has a way to ask for a key, and the second reading
gives `apply` two unrelated meanings depending on its argument.

## Discovery

Found on 2026-08-09 during `specs/design/keyed-recipe-ownership/`, while tracing which construction
sites can produce an asset whose recipe is a bare key. `Context::apply` is the only production
route to one. Recorded then rather than fixed, because the ownership change neither introduces nor
removes the behaviour — it only alters which of steps 2's two outcomes occurs.
