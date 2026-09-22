---
id: ASSETS-CANNOT-BE-DECLARED-NON-PERSISTENT
kind: feature
title: Assets cannot be declared non-persistent
status: draft
priority: P2
complexity: L
area: [core/assets, core/commands, macro]
design: 
created: 2026-09-18
github:
---
## Problem

There is a class of asset Liquers cannot express: **deterministic, cheap to produce, and not worth
storing**. A report or dashboard rendered from already-calculated data is the motivating case. It is
not volatile — it stays valid exactly as long as its inputs do — but persisting it wastes store
space and buys nothing, because reproducing it is cheaper than reading it back.

Today an asset is either volatile (never cached, re-evaluated every time, no version registered) or
it is persisted. There is no third position, and the vocabulary that would express one is present
but inert:

- **`CommandMetadata.cache: bool`** exists, is documented as "if true, then the result of the
  command can be cached", defaults to `true`, and is serialized
  (`liquers-core/src/command_metadata.rs:1011`). **Nothing reads it.** Outside a round-trip
  assertion in `command_declaration.rs:987` and a debug display in
  `liquers-lib/src/egui/widgets.rs:736`, no planner, interpreter or asset-manager code consults the
  field.
- **`register_command!` cannot set it.** The macro's metadata statements are `volatile`, `payload`,
  `label`, `doc`, `namespace`/`ns`, `realm`, `preset`, `next`, `filename`, `expires` and `version`
  (`liquers-macro/src/registration.rs:835-886`). There is no `cache`, so even the inert flag is
  unreachable from the normal registration path.
- **`PersistenceStatus` has no word for it.** Its variants are `None`, `Persisted`,
  `NonSerializable` and `NotPersisted`, and `NotPersisted` means "persistence was attempted but
  failed" (`assets.rs:395`). A deliberately unstored asset would be indistinguishable from a failed
  write.

Declaring such an asset `volatile` is the available workaround and it is wrong in a way that
matters: a volatile asset registers no version (version registration is gated on
`Status::Ready | Source | Override`, `assets.rs:5583` and `:5715`), so everything downstream loses
the ability to tell whether it changed. A non-persistent asset's version is perfectly well defined —
it is a function of its dependencies' versions — and declaring it volatile throws that away.

## Impact

The workaround is to let it persist, which costs store space and requires the value to be
serializable, so nothing is blocked. That is why this is P2 rather than higher.

What it costs is a whole category of cheap derived views. The pattern — precalculated data plus a
cheap presentation layer — is the normal shape of a reporting or dashboard system, and Liquers
currently asks such a system to either write every rendering to a store or give up change detection.

`design/store-and-asset-search/` is a concrete consumer. Its `indexation-policy.md` distinguishes
three classes of document for indexing, and this is the class where producing content at indexation
time is unambiguously *correct*: the version is derivable from dependencies without producing
anything, so staleness is decidable cheaply, and the produced content stays valid exactly as long as
the inputs do. Volatile documents, by contrast, can only ever be snapshotted. Without a way to
declare non-persistence, an author wanting this behaviour must mark the asset volatile, which moves
it into the weaker regime.

## Expected behaviour

An asset can be declared **computed but not stored**: evaluated on demand, versioned from its
dependencies, eligible for in-memory reuse, and never written to a store.

Three pieces, and the first is largely already present:

1. **Honour `CommandMetadata.cache`**, or replace it with a better-named declaration if `cache`
   conflates in-memory reuse with durable storage — which it probably does, since those are
   different questions.
2. **Expose it in `register_command!`**, beside `volatile` and `expires`, and propagate it through
   plans and recipes the way volatility already propagates.
3. **Give `PersistenceStatus` a deliberate variant**, so "not stored because it was not meant to be"
   is distinguishable from "the write failed".

Questions for the design:

- Whether *not persisted* and *not cached in memory* are one declaration or two. They are
  independent: an asset may be worth holding for the next request and not worth writing to disk.
- Whether the declaration belongs on the command, the recipe, or both, and how it combines when a
  plan mixes commands that disagree.
- Whether such an asset registers a version — it should, which is the whole point — and what it is
  computed over when no bytes are ever written, since `Version::from_bytes(content)` is the current
  derivation. A hash over the plan's dependency versions **plus the command implementation
  versions** is the candidate, and the command half is not optional: with no stored content to
  compare, a changed command is the only thing that says the output would differ. The same answer
  has to serve **non-keyed** assets — an ad-hoc report identified by a query and never stored, whose
  `metadata.version()` is `None` today, so query dependencies are recorded at an unknown version
  (`assets.rs:1647`).
- How it interacts with `SERIALIZED-BINARY-RETAINED-WITH-NO-DISPOSAL-POLICY` and with eviction: an
  asset that is cheap to reproduce is the best candidate to evict first.
- What a store read of such a key does — `Status::Recipe` and produce on demand, presumably, which
  makes this partly a question about how the asset manager already treats unstored keys.

## Discovery

Raised while designing `store-and-asset-search`'s indexation policy, 2026-09-18, working out which
classes of document need content produced at indexation time. Verified at HEAD: `cache` is defined
at `command_metadata.rs:1011` and consulted nowhere; the macro's statement list at
`registration.rs:835-886` omits it; `PersistenceStatus::NotPersisted` is defined as a failure at
`assets.rs:395`; version registration is gated on `Ready | Source | Override` at `assets.rs:5583`
and `:5715`.

## Update 2026-09-22 — a concrete consumer, and a complication

`record-streams` gives this a concrete consumer. A record stream's manifest carries `stored:` and
`cached:` flags, and `stored: false, cached: false` is exactly the class this issue describes: chunks
that are deterministic, cheap to reproduce, valid as long as their inputs are, and not worth keeping.

Marking such chunks **volatile was considered and rejected**, and the reasoning is the clearest
statement of what this issue is for. A volatile result is one that cannot be trusted to be the same
next time; a not-kept chunk is deterministic and valid exactly as long as its inputs are. Those are
different claims, and `assets.rs:169` makes the difference expensive: *"Volatility is contagious…
An asset that depends on volatile input also produces a volatile result."* A report built over such
a stream would inherit a label that is simply false about it, and lose caching it was entitled to.

So the record design **rejects that combination at manifest load** until this issue lands, rather
than approximating it. What it needs is exactly what this issue describes: **an asset that does not
keep the value but still tracks expiration, and is therefore not contagious.**

A complication worth checking when work starts. `assets.rs:90-97` states
`stored => keyed`, `persistent => stored`, and then: *"A volatile keyed asset **is** keyed, so it is
stored — it is simply not persistent."* If exact, marking a keyed asset volatile does **not** stop
its bytes being written; it stops them being read back. A design for this issue should say plainly
whether a non-persistent asset writes nothing or writes-and-ignores, because the record design needs
the former and the current vocabulary may only offer the latter.

## Update 2026-09-22 — a specified mechanism

The gap now has a proposed mechanism, and it fits the existing structure more neatly than expected.

### Two independent flags, carried by four types

`stored` and `cached` become first-class, on `MetadataRecord`, `Metadata`, `AssetInfo` and `Recipe`.
Both default to `true`, so legacy data and existing recipes keep today's behaviour — the same
`#[serde(default)]` treatment `MetadataRecord::type_name` already uses.

| Flag | Means | Default |
|---|---|---|
| `stored` | the asset manager writes the produced value to the store | `true` |
| `cached` | the asset manager registers the asset for reuse | `true` |

Neither is `volatile`, which stays a separate and stronger claim: *this result cannot be trusted to
be the same next time*. An asset with both flags false is still deterministic and still tracks
expiration — it is simply not retained, and **must not be contagious**.

### The two operations are already adjacent and separable

`DefaultAssetManager`'s finish path does them one after the other (`assets.rs:5694-5712`):

```rust
// 6. Store in assets map
assert!(self.try_insert_key_asset(key, asset_ref.clone()).await);   // <- cached

// 7/8. Try to serialize and store
store.set(key, &binary, &metadata.clone().into()).await?;            // <- stored
```

So `cached: false` skips step 6 and `stored: false` skips steps 7–8. No restructuring; two
conditions at a site that already separates the concerns.

### The module documentation already sanctions `cached: false`

`assets.rs:100` says so in as many words:

> *"Whether the manager registers a keyed asset in its key map is a separate caching-and-sharing
> decision that belongs to the manager: **declining to register a non-volatile keyed asset still
> produces correct results**."*

An unregistered asset is re-evaluated per request rather than shared. That is a **deduplication**
loss, not a correctness one: two concurrent requests both compute, and both get the right answer.

### `stored: false` suppresses writing, not reading

Deliberately asymmetric. An asset declared `stored: false` does not write its value, but an existing
stored copy remains readable. Two consequences worth having:

- Turning the flag on or off **does not invalidate data already on disk**.
- A value stored under an older configuration keeps serving until something replaces it.

**Open:** whether a stored copy should be *preferred* over recomputation for a `stored: false` asset.
Reading it is cheaper; recomputing is more clearly correct when the flag says the store is not
authoritative for this asset. This needs deciding before implementation, not after.

### Recipes get the same flags

`Recipe` already carries `volatile` and `expires`, so `stored` and `cached` sit beside them and are
folded into the plan the same way `Recipe::to_plan` folds volatility (`recipes.rs:281`). A recipe
author can then say "produce this, do not keep it" without claiming it is unstable.

### The consumer

`record-streams`' manifest carries the same two flags per chunk and currently **rejects**
`stored: false, cached: false` at load, because the class does not exist. That rejection is removed
when this lands. See `specs/design/record-streams/manifest-format.md` §4b.

### Readiness

This has moved from "a gap with a motivating example" to "a mechanism with named call sites, a
backward-compatibility story and a consumer waiting on it". It is ready for a design folder when
work starts; the open question above is the one thing a design must settle first.
