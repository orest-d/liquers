---
title: "Phase 3: Examples and tests — Versions for computed keyed assets"
kind: design
audience: internal
area: [core/assets]
---
# Phase 3: Examples & Use-cases

## Introduction

Phase 1's purpose was one sentence: expiring a computed keyed asset invalidates no keyed dependent,
because such an asset never carries a version. **Measurement against HEAD shows that sentence is
not quite right, and the correction matters** — see the next section. Everything here exists to
make the corrected statement testable, and then to pin the behaviours the fix switches on so none
of them can regress quietly.

The progression is deliberate. **Scenario 1** is the defect itself — a three-link keyed chain of
computed assets, which is the shape no test in the repository has. **Scenario 2** builds on it to
show the two mechanisms that make the fix safe rather than merely effective: the version is the
hash of the bytes the store holds, and an unregistered dependency is treated as unverified rather
than stale. **Pitfalls** then covers what goes wrong if an implementer takes an obvious shortcut,
most of which are shortcuts that compile.

**Examples are runnable tests, not conceptual code.** This is a defect fix in `liquers-core`, so
the example *is* the regression test; there is no separate prototype worth writing. Every query
below was checked with `liquers-validate` before being written down.

## What HEAD Actually Does — measured, not inferred

A throwaway probe built the Scenario 1 fixture and ran it against HEAD before any of the tests
below were written. The output corrects the issue, Phase 1 and Phase 2, all three of which say that
**no** keyed dependent is reached:

```
PROBE c value                        = Ok("Hello, world!, world!")
PROBE a.version                      = None
PROBE b.version                      = None
PROBE stored a.txt bytes             = Ok("Hello")
PROBE statuses before expire         : a=Ready   b=Ready   c=Ready
PROBE statuses AFTER expire(a)       : a=Expired b=Expired c=Ready      <-- b IS invalidated
PROBE after recompute, then expire(b): b=Expired c=Expired              <-- one level again
PROBE n(.bin) status=Ready version=None persistence=NonSerializable stored=false
PROBE query-asset version            = None
```

**The corrected statement: invalidation reaches direct dependents and never propagates.** Each
explicit expiry moves exactly one level and stops.

The mechanism explains both halves. A keyed asset is recorded in *two* places when it depends on
another key: as a graph edge in `keyed_dependents`, and — because `Context::evaluate` calls
`add_dependent_asset` whenever the current asset is keyed — as a weak reference in
`dependent_assets`. `expire_internal` collects `dependent_assets` **outside** the `skip_cascade`
guard and traverses `keyed_dependents` **inside** it. So with `Version(0)`:

- `dependent_assets[a]` is collected → `b` is expired. One level, always.
- `keyed_dependents[a]` is never traversed → `b` never enters the BFS queue → `dependent_assets[b]`
  is never collected → `c` survives. No second level, ever.

**Why this correction is worth the words.** "No dependent is invalidated" would have been caught by
anyone writing a two-asset test. "Dependents are invalidated, but only the first" survives every
two-asset test, which is why it survived. It also changes what Scenario 1 must assert: the
assertion on `b` **passes at HEAD** and is a control, and only the assertion on `c` is the
regression test. A three-link chain is the minimum that fails, and that is exactly why no existing
test fails.

The issue, Phase 1 and Phase 2 are corrected to say this.

## Overview Table

| ID | Kind | Where | Demonstrates / checks | Fails at HEAD? |
|---|---|---|---|---|
| **S1** | Scenario | — | Keyed→keyed cascade through computed assets | — |
| **S2** | Scenario | — | Version is the hash of the stored bytes; unregistered ≠ stale | — |
| U1 | Unit | `dependencies.rs` | An unregistered **asset** dependency is registered provisionally, not expired (**rewrite** of `add_dependency_fails_unregistered_dep`) | changed |
| U2 | Unit | `dependencies.rs` | An unregistered **command** dependency still expires the dependent — the pair that keeps the two branches from being collapsed | new |
| U3 | Unit | `dependencies.rs` | Provisional entry equal to the later real registration ⇒ no cascade | new |
| U4 | Unit | `dependencies.rs` | Provisional entry differing from the later real registration ⇒ dependent expired | new |
| U5 | Unit | `dependencies.rs` | With real versions, `expire` reaches the whole keyed chain | new |
| U6 | Unit | `dependencies.rs` | `Version(0)` still stops a cascade — now the **policy** test, not an accident (`expire_skips_version_zero_cascade`, re-commented) | unchanged |
| U9 | Unit | `dependencies.rs` | `version_consistent` and `add_dependency` **deliberately disagree** about an unregistered asset key — pinned so the asymmetry cannot be "tidied up" | new |
| U7 | Unit | `metadata.rs` | `Version::new_unique` is distinct across a tight loop — what the counter buys and a bare clock would fail | new |
| U8 | Unit | `metadata.rs` | `Version::new_unique` / `from_time_now` produce non-zero, ordered-enough values from the chrono clock | new |
| I1 | Integration | new file | A computed keyed asset's version **equals** `Version::from_bytes` of what the store holds | yes |
| I2 | Integration | new file | **The defect.** Expiring `a.txt` expires `b.txt` (control — passes at HEAD) **and `c.txt`** (the regression) | **yes, on `c`** |
| I3 | Integration | new file | Identical content ⇒ identical version; changed content ⇒ changed version | yes |
| I4 | Integration | new file | A non-serializable keyed asset gets a unique fallback version, logged, and is not persisted | yes |
| I5 | Integration | `manager_parametric.rs` | A delegating asset reports the delegate's version (extends `scenario_keyed_delegation`) | yes |
| I6 | Integration | new file | A **query** asset gets no version — the no-op that keeps the common path cheap | yes |
| I7 | Integration | new file | A **volatile** keyed asset gets no version | yes |
| I8 | Integration | new file | Cold start: a dependent reloaded before its dependency is **not** expired | yes |
| I9 | Integration | new file | Cold start: a dependent whose dependency reloads at a different version **is** expired | yes |
| R1 | Regression | `expiration_integration.rs` | The existing 34 tests still pass, unchanged | must stay green |

"Fails at HEAD" is the property that makes a test worth writing here. Counted against the rows
above: one rewritten (U1), eight new unit tests (U2–U5, U7–U9), nine integration tests that fail at
HEAD (I1–I9), one unchanged but re-purposed (U6), and one regression guard (R1). **Eighteen tests
that either fail today or assert something nothing asserts.** The count is stated because a
reviewer caught an earlier one being wrong, and an unverified tally is a sign the accounting was
not done.

## Example: Scenario 1 — the defect, end to end

Three keyed assets, each computed from the previous one. This is the shape the repository has never
tested: `test_dependent_expiration` looks like it, but its dependent is
`envref.evaluate("-R/hello.txt/-/world")` — a **query** asset, invalidated through `dependent_assets`
outside the version guard. Swap that for a keyed dependent and the test stops passing.

```rust
// a.txt  <- hello
// b.txt  <- -R/a.txt/-/world/b.txt
// c.txt  <- -R/b.txt/-/world/c.txt
let mut recipe_list = RecipeList::new();
recipe_list.add_recipe(Recipe::new(
    "hello/a.txt".to_string(), "A".to_string(), "root of the chain".to_string())?);
recipe_list.add_recipe(Recipe::new(
    "-R/a.txt/-/world/b.txt".to_string(), "B".to_string(), "depends on a.txt".to_string())?);
recipe_list.add_recipe(Recipe::new(
    "-R/b.txt/-/world/c.txt".to_string(), "C".to_string(), "depends on b.txt".to_string())?);

let store = AsyncMemoryStore::new(&Key::new());
store.set(&parse_key("recipes.yaml")?, serde_yaml::to_string(&recipe_list)?.as_bytes(),
          &Metadata::new()).await?;
env.with_async_store(Box::new(store));
env.with_recipe_provider(Box::new(DefaultRecipeProvider));
let envref = env.to_ref();

// Build the whole chain by asking for its tip.
let c = envref.evaluate("-R/c.txt").await?;
assert_eq!(c.get().await?.try_into_string()?, "Hello, world!, world!");
let a = envref.evaluate("-R/a.txt").await?;
let b = envref.evaluate("-R/b.txt").await?;

a.expire().await?;

assert_eq!(a.status().await, Status::Expired);
assert_eq!(b.status().await, Status::Expired, "direct keyed dependent must be invalidated");
assert_eq!(c.status().await, Status::Expired, "invalidation must reach the whole chain");
```

**Why it fails at HEAD — and which assertion fails.** Measured: `b` reads `Expired` and `c` reads
`Ready`. The assertion on `b` is a **control**, not the regression: `b` is invalidated through the
weak-reference route, which runs outside the version guard. The assertion on `c` is the regression:
`a.txt` carries no `version`, so `track_asset` registered `Version(0)`, `expire_internal` takes
`skip_cascade` on its first iteration, `b` never enters the BFS queue, and `c`'s weak reference —
which hangs off `b`, not off `a` — is never collected.

Keeping the `b` assertion is deliberate. If a future change broke the weak-reference route while
fixing the graph route, a test that only checked `c` would stay green.

**Why it passes after the fix.** `a.txt` serializes to `b"Hello"` at finalization — the probe
confirms those are the exact bytes the store holds — so it carries `Version::from_bytes(b"Hello")`,
which `track_asset` registers. `expire_internal`'s version check now finds a concrete version,
`keyed_dependents[a]` is traversed, `b` enters the queue, its own dependents are collected, and the
invalidation reaches `c`.

The three queries were validated: `hello/a.txt`, `-R/a.txt/-/world/b.txt` and
`-R/b.txt/-/world/c.txt` all parse and plan, and the middle one resolves to
`GetAsset[a.txt] → Action{world} → Filename{b.txt}` — *not* `GetAsset[a.txt, world, b.txt]`, which
is the mistake `-R/` invites.

## Example: Scenario 2 — what makes the fix safe rather than merely effective

Scenario 1 shows invalidation happening. It does not show that the fix is *sound*, and the two
properties that make it sound are separately observable.

### The version is the hash of what the store holds

Not "a version was assigned" — the exact bytes.

```rust
let a = envref.evaluate("-R/a.txt").await?;
let state = a.get().await?;
let stored = envref.get_async_store().get_bytes(&parse_key("a.txt")?).await?;

assert_eq!(state.metadata.version(), Some(Version::from_bytes(&stored)));
```

This is the assertion the one-serialization decision buys. Had the design serialized twice — once
to hash, once to store — this test could still pass for a `Text` value and fail for any value whose
encoding is not byte-deterministic, and it would fail *in production*, not here.

### An unregistered dependency is unverified, not stale

Cold start, in one process: two environments over one store, the second never having seen `a.txt`.

```rust
// Environment 2, fresh DependencyManager. `b.txt` is asked for first — the ordinary order,
// since a dependent is what a caller asks for.
let b2 = envref2.evaluate("-R/b.txt").await?;
assert_eq!(b2.status().await, Status::Ready,
           "a dependent must not be expired merely because its dependency is not loaded yet");
```

At HEAD with real versions and no provisional rule this reads `Expired`: `b.txt`'s stored record
names `a.txt@v1`, the manager has never seen `a.txt`, `version_consistent` answers `false`, and
`add_dependency` expires `b.txt`. The provisional rule writes `a.txt@v1` into the map instead, and
the correction arrives when something loads `a.txt` — which I9 exercises by making it load with a
*different* version.

## Corner Cases

Ten, with the test that pins each. Most are shortcuts that compile.

| # | Corner case | Symptom if got wrong | Pinned by |
|---|---|---|---|
| P1 | Serializing at finalization for **every** asset, not just non-volatile keyed ones | Every query result is serialized; the commonest path in the system pays for a version nothing reads | I6, I7 |
| P2 | Serializing **twice** — once to hash, once to store | Silent for `Text`; a version that does not describe the stored bytes for anything non-deterministic | I1 |
| P3 | Assigning the version during persistence instead of at finalization | A parent that already read the child recorded `Version::unknown()`; the chain half-works and looks fine | I2 (fails intermittently rather than cleanly — see below) |
| P4 | Forgetting `lock.binary = None` when installing the value | Correct metadata describing stale bytes, no error anywhere (`EVALUATE-DOES-NOT-CLEAR-CACHED-BINARY`) | I3 |
| P5 | Leaving `serialize_to_binary` on `poll_state()` | An asset at a gated status serializes to `None` and `save_to_store` reports "Failed to obtain binary value" (`SERIALIZE-TO-BINARY-CONSULTS-THE-READ-GATE`) | I4, and the blocked sibling design's own tests |
| P6 | Applying the provisional rule to **command** keys too | A dependent survives the removal of the command that produced it | U2 |
| P7 | Making the delegating asset compute its own version | Two versions for one graph node; `add_dependency` expires a fresh parent (Phase 1's second failure mode) | I5 |
| P8 | Persisting a delegated evaluation | A stored record with a real version and an empty dependency list, which `try_fast_track` reads as "nothing to check" | I5 |
| P9 | Using a bare timestamp instead of `new_unique` for the fallback | Two assets finalized in the same clock tick share a version; on wasm the tick is a **millisecond** | U7 |
| P10 | "Fixing" `Version(0)` handling by deleting the `skip_cascade` branch | The future zero-version policy has no mechanism, and `expire_skips_version_zero_cascade` is deleted as obsolete | U6 |
| P11 | Reading `status()` on the result of `envref.evaluate(...)` without awaiting `get()` first | The asset is still `Processing`; `status()` lies and `expire()` returns `"Cannot expire asset in state Processing"`. **Hit while writing the probe for this phase**, so it is a real trap and not a hypothetical | every integration test's shape |
| P12 | "Tidying up" the disagreement between `version_consistent` (unregistered ⇒ `false`) and `add_dependency` (unregistered asset key ⇒ provisional) | The provisional rule is silently reverted and cold-start caching breaks again, with no test failing unless one pins the asymmetry | U9 |

### Two pitfalls no test can hold, and what is done instead

**P3 — the assignment-point ordering.** Whether a parent reads the child's metadata before or after
persistence depends on scheduling, so a version assigned during persistence makes I2 *flaky* rather
than red, and an implementer who "fixes" a flaky I2 with a sleep has hidden the defect. **This is a
Phase 4 precondition, not a suggestion:** the ordering constraint is stated as a code comment
immediately above the `assign_version` call, naming what breaks if the call moves. Phase 4 is not
complete without it, because it is the only artefact that will survive a refactor.

**P2 — double serialization.** I1 asserts `version == Version::from_bytes(stored_bytes)`, but the
test value is `Value::Text`, whose encoding is byte-deterministic — so **I1 would pass even if the
implementation serialized twice.** No test in `liquers-core` can catch this, because no value type
in `liquers-core` has a non-deterministic encoding. The control is the design (one serialization,
bytes cached and reused), not the test suite. Recorded here so nobody reads I1 as proof of
something it cannot prove, and so that whoever adds a non-deterministic value type knows there is a
test worth writing at that point.

## Test Plan

### Unit tests — `liquers-core/src/dependencies.rs`

They live here because `DependencyManager` is `pub(crate)`: only this module's `#[cfg(test)] mod
tests` can observe the `versions` map, and every assertion below is about that map. Integration
tests can observe only effects. The existing `TestEnv` alias and helpers are reused.

| Test | Assertion |
|---|---|
| `add_dependency_registers_unregistered_asset_dep_provisionally` | `add_dependency(&a, &b, v42)` with `-R/b` unregistered: `a` is **not** in `expired.keys`, the edge exists, and `get_version(&b) == Some(v42)`. **Replaces** `add_dependency_fails_unregistered_dep`, whose name recorded a behaviour nobody chose. |
| `add_dependency_expires_on_unregistered_command_dep` | Same shape with `DependencyKey::for_command_implementation(...)`: `a` **is** expired, and `get_version` stays `None`. |
| `provisional_version_matching_real_registration_does_not_cascade` | Provisional `v1`, then `register_version(&b, v1)` → `ExpiredDependents` is empty. |
| `provisional_version_differing_from_real_registration_cascades` | Provisional `v1`, then `register_version(&b, v2)` → `a` expired. This is the correction the approximation relies on; without it the provisional rule is just a hole. |
| `expire_cascades_through_versioned_keyed_chain` | `a→b→c` with concrete versions on all three: `expire(&a)` returns all three keys, **and specifically `c`** — the transitive step, which is the one HEAD cannot do. Asserting on `c` rather than on the length is what forces the root guard to be gone; a length assertion would pass on several wrong implementations. |
| `version_consistent_and_add_dependency_disagree_on_an_unregistered_asset_key` | Both in one test, because the point is the asymmetry: `version_consistent(&b, v42)` is `false` **and** `add_dependency(&a, &b, v42)` does not expire `a`. `version_consistent` is a question about what is registered; `add_dependency` is a decision about what to conclude from that, and after this change the two answers deliberately differ. Without this test the next reader "fixes" one to match the other. Phase 2 asked Phase 3 to record this; the test is the recording. |
| `expire_skips_version_zero_cascade` | **Unchanged assertions.** Its doc comment is rewritten: after this change no path registers `Version(0)` for a keyed asset by accident, so this test pins the declared-policy escape hatch rather than an accident. |

### Unit tests — `liquers-core/src/metadata.rs`

| Test | Assertion |
|---|---|
| `version_new_unique_is_distinct_within_one_clock_tick` | 1000 calls in a tight loop produce 1000 distinct values. A bare `Utc::now()` fails this on any platform; it is the counter's job. |
| `version_from_chrono_clock_is_never_unknown` | `new_unique()` and `from_time_now()` both return `!is_unknown()`. Guards the fallback's whole point — a fallback that produced `Version(0)` would be indistinguishable from having no fallback. |

`version_new_unique_produces_distinct_values` already exists at `dependencies.rs:742` and is kept;
it tests two calls, which passes even with a bare clock, which is why the tight-loop test is added
rather than substituted.

### Integration tests — new file `liquers-core/tests/keyed_version_cascade.rs`

Every integration test here awaits `get()` before reading `status()` — see P11; `evaluate()`
returns an asset that may still be `Processing`.

A new file rather than more of `expiration_integration.rs`, which is already 34 tests: this set is
about versions and cascade, shares one recipe-chain fixture, and reads better with that fixture at
the top. `expiration_integration.rs` keeps its 34 tests unchanged as R1.

| Test | Assertion |
|---|---|
| `computed_keyed_asset_version_is_the_hash_of_stored_bytes` (I1) | `state.metadata.version() == Some(Version::from_bytes(&stored_bytes))`. The probe confirms `a.txt` stores exactly `b"Hello"`, so the expected value is known rather than round-tripped. See P2 for what this test cannot prove. |
| `keyed_expiry_cascades_to_keyed_dependents` (I2) | Scenario 1 verbatim, three links. The `b` assertion is a control that passes at HEAD; **the `c` assertion is the regression test for the issue.** A two-link version of this test would pass today, which is why the repository has no failing test for a P1 defect. |
| `identical_content_yields_identical_version` (I3) | A counting command: first and second evaluation of unchanged content give the same version; changed content gives a different one. Also catches P4, since a stale cached binary makes the second version wrong. |
| `non_serializable_keyed_asset_takes_a_unique_fallback_version` (I4) | `n.bin` from a command returning `Value::I32`. Measured at HEAD: status `Ready`, version `None`, `persistence_status()` `NonSerializable`, and **the key is absent from the store** — which is the empirical confirmation of the durability decision's premise, since an asset that leaves no trace cannot be proved to reconstruct identically. After the fix: version is `Some` and `!is_unknown()`, two evaluations give **different** versions, the store still holds nothing, and the metadata log carries the warning that the fallback fired. The log assertion is what stops the net going silent (Phase 1's "a fallback that fires silently is a bug detector switched off"). |
| `query_asset_has_no_version` (I6) | `envref.evaluate("hello/world")` → `metadata.version()` is `None`. |
| `volatile_keyed_asset_has_no_version` (I7) | A `volatile: true` command behind a keyed recipe → `None`. |
| `cold_start_dependent_is_not_expired_by_an_unloaded_dependency` (I8) | Scenario 2, second half. |
| `cold_start_dependent_is_expired_when_the_dependency_reloads_changed` (I9) | Same setup, but `a.txt`'s stored bytes are overwritten before environment 2 starts, so loading `a.txt` registers a different version and `b.txt` is expired. The pair I8/I9 is what shows the approximation is *deferred* verification rather than *absent* verification. |

### Integration test — `liquers-core/tests/manager_parametric.rs`

| Test | Assertion |
|---|---|
| `scenario_keyed_delegation` (I5, **extended**) | Add two assertions to the existing scenario: `adhoc`'s version equals `owner`'s version, and `adhoc.persistence_status()` shows it did not write. The scaffolding is already there — `manager.apply((&key).into(), State::new(), None)` builds the delegating asset and the counter proves the branch was taken — so this is the shortest possible home for the delegation contract. |

### Fixture the cold-start tests need, and a correction

I8 and I9 need two environments over **one** store, and `AsyncMemoryStore` is neither `Clone` nor
shareable — `with_async_store(Box::new(store))` takes ownership.

The sibling design `stale-dependency-status-finalization` designed a `SharedMemoryStore` wrapper for
exactly this and recorded that "`AsyncStore` has only two required methods (so the shared-store
wrapper is two forwarding bodies)". **That is right about the trait and wrong about the wrapper.**
`AsyncStore` has 22 methods of which 2 are required, but the defaults are *not* forwarding defaults —
`set`'s default is `Err(key_not_supported)`. A wrapper that overrides only the two required methods
would compile and then fail every write. Phase 4 must forward every method the test path touches
(`get`, `set`, `set_metadata`, `contains`, `get_metadata`, and whatever the recipe provider uses to
find `recipes.yaml`) and determine that set by compiling and running, not by counting required
methods. Recorded here because that folder is picked up again after this one lands.

**No dependency runs the other way.** A reviewer asked whether I8/I9 are blocked if the sibling
design is delayed. They are not: that design is blocked on *this* one, its wrapper was designed and
never written, and this design writes its own inside its own test file. If both eventually want it,
promoting it to a shared fixture is the third occurrence that would justify the guide entry
discussed below.

### Commands and queries used

All default-namespace, all in `liquers-core`, no `liquers-lib` types — per the Phase 2 gate
decision that the shortest path is the existing `Value`:

| Command | Returns | Purpose |
|---|---|---|
| `hello` | `Value::Text("Hello")` | Root of the chain; serializable |
| `world` | `Value::Text(format!("{}, world!", …))` | Chain link |
| `count` | `Value::I32(n)` from an `AtomicUsize` | Non-serializable at `.bin`; also the changed-content source for I3 |

Queries, all validated with `liquers-validate --command hello --command world --command count`:
`hello/a.txt`, `-R/a.txt/-/world/b.txt`, `-R/b.txt/-/world/c.txt`, `-R/a.txt/-/count/n.bin`,
`-R/a.txt`, `-R/b.txt`, `-R/c.txt`, `hello/world`.

### Running

```bash
cargo test -p liquers-core --lib                                  # U1–U8
cargo test -p liquers-core --test keyed_version_cascade           # I1–I4, I6–I9
cargo test -p liquers-core --test manager_parametric              # I5
cargo test -p liquers-core --test expiration_integration          # R1 — must stay 34/34
cargo test -p liquers-core --test dependency_manager_integration --test dependency_scheduling
```

Baseline to beat, measured 2026-09-05 before any change: 793 lib, 34, 5, 4 — zero failures.

## Documentation and Learning Log

### Guide candidates

**None, and the Phase 1 decision stands.** The test material here answers "how do I assert that a
cascade happened", which is a question about this subsystem's tests rather than a repeatable task a
contributor performs. The one piece with reuse value beyond this design — the shared-store fixture
for two-environment cold-start tests — belongs to `guides/UNITTEST_GUIDE.md` if it recurs, and it
has now been wanted by two designs. **Recommendation: not now.** File it if a third design needs it;
noting the pattern twice is not yet evidence of a pattern.

### Learning to carry into Phase 5

1. **The defect was mis-stated in the issue, in Phase 1 and in Phase 2, and only measurement caught
   it.** All three said no keyed dependent is reached. In fact the first one always is, through the
   weak-reference route that runs outside the version guard; what never happens is the second step.
   Three documents and two reviewers repeated the claim because it was plausible and because the
   code reads that way. A ten-minute probe against HEAD disproved it. **The general lesson for this
   subsystem: assert against a running binary before writing the sentence down** — `assets.rs` is
   9,500 lines with two overlapping invalidation routes, and reading one of them is not knowing
   what happens.
2. **A two-asset test cannot see this bug.** That is why none of the 34 expiration tests fails.
   `test_dependent_expiration` reads as an end-to-end cascade test and is one — for a query asset,
   one level. Coverage that looks complete and is one link short is worse than absent coverage, and
   this shape hid a P1 defect through several designs. Worth stating in the reference.
3. **A test name can encode a bug.** `add_dependency_fails_unregistered_dep` asserted, and named as
   intended, behaviour nobody chose — absence read as change. Renaming it is part of the fix.
4. **`AsyncStore`'s required-method count does not tell you the size of a wrapper.** Corrected above;
   it is the kind of fact that is cheap to check and expensive to assume.
5. **Two constraints have no reliable test** — P3's ordering and P2's single serialization. The ordering constraint between version assignment and the
   `ValueProduced` notification can only be violated into flakiness, not into red. It is documented
   at the call site instead, which is the honest place for a constraint tests cannot hold.
