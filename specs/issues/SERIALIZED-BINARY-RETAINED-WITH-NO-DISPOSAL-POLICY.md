---
id: SERIALIZED-BINARY-RETAINED-WITH-NO-DISPOSAL-POLICY
kind: issue
title: A serialized binary is cached on the asset forever, with no policy for releasing it
status: draft
priority: P2
complexity: M
area: [core/assets]
design:
created: 2026-09-05
github:
---

## Problem

`AssetRef::serialize_to_binary` (`liquers-core/src/assets.rs:2697`) caches the bytes it produces on
the asset:

```rust
let binary = data.as_bytes()?;
let mut lock = self.data.write().await;
let arc_binary = Arc::new(binary);
lock.binary = Some(arc_binary.clone());
```

Nothing ever releases that cache while the asset lives. `lock.binary` is cleared only when the
asset is being *replaced or invalidated* — `reset` (`:1334`), the error branch of `evaluate`
(`:2572`), `set_value` (`:3318`), `set_state` (`:3359`), the cancellation path (`:2208`) — never
because the bytes have served their purpose. So a keyed asset that persists once holds both its
deserialized value and a full serialized copy of itself for as long as the asset manager keeps it,
roughly doubling its resident cost for as long as it is cached.

There is no policy to consult. The asset carries no size accounting, no last-used marker for the
binary, and no notion of when a cached binary is worth keeping (a repeatedly-served large table)
versus dropping (a one-shot write of a value that is cheap to re-serialize).

## Impact

Memory, not correctness — the cached bytes are always consistent with the value that produced them,
except in the separate case recorded by `EVALUATE-DOES-NOT-CLEAR-CACHED-BINARY`. The cost scales
with the number of cached keyed assets and with value size, so it is invisible on small workloads
and material on the ones the store exists for. P2 on that basis: real, bounded by eviction, and
with no wrong answer attached.

It becomes more prominent under `keyed-expiry-cascade-fix`, which serializes keyed non-volatile
assets at status finalization so their content hash can be computed, and reuses the same bytes for
persistence. That does not enlarge the retained set — those are the same assets that persist today
— but it does make "we hold a serialized copy of every cached keyed asset" a deliberate part of the
design rather than a side effect of one function, which is what makes the missing policy worth
recording.

## Expected behaviour

A stated policy for the cached binary, and code that implements it. The shape is open; plausible
options, not mutually exclusive:

- **Drop after a successful persist.** Simplest, and correct whenever the store is the cache. Costs
  a re-serialization for any later reader of the bytes.
- **Keep under a budget.** A per-manager byte budget with an eviction order, which needs size
  accounting the asset does not have today.
- **Keep by policy declared on the asset**, from the recipe or type — the same place volatility and
  expiration are already declared, so it composes with the existing vocabulary.

Whichever is chosen, `poll_binary` / `binary_unchecked` consumers must tolerate a `None` they would
have got `Some` for before, since dropping the cache is exactly that transition.

## Discovery

Recorded on 2026-09-05 by the project owner during Phase 1 of `keyed-expiry-cascade-fix`, when
deciding that the version hash and the persisted bytes should come from one serialization: "the
binary may be disposed later based on the policy". Explicitly scoped out of that design.
