---
title: "Phase 2: Architecture — Versions for computed keyed assets"
kind: design
audience: internal
area: [core/assets]
---
# Phase 2: Solution & Architecture

## Overview

Four changes, all inside `liquers-core`, all decided by Phase 1's owner decisions:

1. **`evaluate` assigns a version.** After status finalization and before the `ValueProduced`
   notification, a non-volatile keyed asset serializes once, hashes those bytes into
   `MetadataRecord.version`, and keeps them for persistence to reuse. A *delegating* asset does
   not serialize: it takes the delegate's version verbatim.
2. **`add_dependency` stops reading absence as change.** An unregistered *asset* dependency key
   gets the dependent's recorded version written provisionally, so the real registration compares
   against it and cascades on a difference. Command keys keep today's behaviour, for a reason
   given below.
3. **`track_asset` is a net.** Anything reaching the graph with no version is given a time-based
   one, written back into the asset's metadata and recorded in its log.
4. **`expire_internal`'s guard is corrected, not deleted.** Its vacuous condition goes, its comment
   is made true, and the `skip_cascade` branch it guards becomes the mechanism a future
   zero-version policy will use.

**One correction to the problem statement, established by measurement after this phase was
drafted** (Phase 3, "What HEAD Actually Does"): the issue, Phase 1 and this document's first draft
all said no keyed dependent is reached today. In fact the *direct* one is, through the weak-
reference route `expire_internal` collects outside the guard; what never runs is the graph
traversal, which is the only route that enqueues a node and so the only one that can reach a second
level. Nothing in the architecture changes — the fix is the same and the guard is the same — but
the regression test must have three links, because a two-asset test passes at HEAD.

Two supporting corrections are unavoidable rather than optional: `serialize_to_binary` must read
through the ungated accessor (`SERIALIZE-TO-BINARY-CONSULTS-THE-READ-GATE`), and `evaluate` must
clear a stale cached binary (`EVALUATE-DOES-NOT-CLEAR-CACHED-BINARY`), because this design makes
`lock.binary` written on the evaluation path for the first time.

No public API changes, no new types outside two private helpers, no crate boundaries crossed.

## Known-Issue Preflight

Twenty-two `core/assets` issues are open. Those that touch this work:

| Issue | P/Cx | Status | Effect on this design | Blocking? |
|---|---|---|---|---|
| `SERIALIZE-TO-BINARY-CONSULTS-THE-READ-GATE` | P2/S | draft | **Must be fixed here.** `serialize_to_binary` polls through `poll_state()`, which returns `None` at `Expired`. This design calls it at finalization, where the stale-dependency rule can already have produced `Expired` once the blocked design lands, and where `save_to_store`'s existing comment says the gate must not apply. One-line change: `poll_state_any_status()`. | No — absorbed |
| `EVALUATE-DOES-NOT-CLEAR-CACHED-BINARY` | P2/S | draft | **Must be fixed here.** `evaluate` installs a value without clearing `lock.binary`. Latent today because nothing writes that cache on the evaluation path; this design does, so the invariant stops being luck. | No — absorbed |
| `KEYED-EXPIRY-DOES-NOT-CASCADE-TO-KEYED-DEPENDENTS` | P1/L | in_progress | The issue being fixed. | — |
| `DEPENDENCY-VERSIONS-NOT-LOADED-OR-VERIFIED-FROM-STORE` | P1/M | draft | Follow-up by owner decision. This design ships provisional registration as the approximation; that issue replaces it with real verification and closes the one case it cannot reach. | No |
| `ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY` / `stale-dependency-status-finalization` | P1/M | draft | That design is **blocked on this one** and asked for its C2 decision to be revisited once versions are real. It also owns an approved rename, `try_to_set_ready` → `finalize_status`, which this design deliberately does **not** take: the rename is theirs, and taking it would make their diff harder to read, not easier. | No — this design unblocks it |
| `SERIALIZED-BINARY-RETAINED-WITH-NO-DISPOSAL-POLICY` | P2/M | draft | Filed by this design. Scoped out by the owner; the retained set is unchanged. | No |
| `SAVE-TO-STORE-REPORTS-CANCELLED-WRITE-AS-PERSISTED` | P2/S | draft | Adjacent on the same path. Not touched: this design changes where bytes come from, not how a cancelled write is reported. Worth knowing that `PersistenceStatus::Persisted` is not evidence a version reached the store. | No |
| `EXPIRY-RECORDS-NO-REASON` | P2/S | draft | The tracking-time net's log entry is the same requirement from the other direction. This design records *its own* reason; it does not fix the four silent expiry routes. | No |
| `ASSET-REGISTRATION-OWNERSHIP-CONTRACT` | P2/L | draft | Relevant to delegation: ownership is approximated by keyedness, and `bound_owner_key` is the ownership-aware derivation `track_asset` uses. This design relies on that distinction (a delegating asset is keyed but not the owner) without changing it. | No |
| `ASSETS-FIX1`, `ASSETS-IMPROVEMENTS`, `CORE-ASSET-GC`, `EXTENDED-FAST-TRACK`, `COMBINED-EXPIRES`, `QUEUED-MANAGER-EVICTION-RACE`, `INLINE-DROP-REPAIR-STRANDS-EXISTING-WAITERS`, `ENVIRONMENT-MANAGER-REFERENCE-CYCLE`, `ERROR-WITH-KEY-SETS-QUERY-FIELD`, `RECIPE-PLAN-ANALYSIS-RUNS-OUTSIDE-PLAN-BUILDING`, `ASSET-FINISHED-PROGRESS-CONTRACT-UNDEFINED`, `CORE-TOKIO-REMOVAL`, `EXPIRATION-RECOVERY-WEB-API` | — | — | No interaction found. | No |

**No blocker remains, and none was found below P1.** The two absorbed issues are P2/S and are
absorbed because this design's correctness depends on them, not as opportunistic cleanup — Phase 5
closes both.

## Data Structures

### `ValueOrigin` — replaces `RecipeEvaluation::delegated`

`RecipeEvaluation` currently carries `delegated: bool`, read once to decide persistence. That flag
is exactly the discriminator for versioning too, and a `bool` cannot carry the delegate's version.
Replacing it with an enum makes both decisions read off one value and satisfies the project's
no-default-match-arm rule.

```rust
/// Where an evaluation's value came from. Decides both persistence and versioning.
enum ValueOrigin {
    /// Produced by applying this asset's own recipe. The asset owns the value, so it owns the
    /// version: serialize once, hash the bytes, keep them for the store write.
    Computed,
    /// Handed over by the key's registered owner in pure-key delegation. Both assets resolve to
    /// the same key and are therefore one dependency-graph node, so they must report the same
    /// version — carried here verbatim rather than recomputed, since two serializations of the
    /// same value are not guaranteed to agree byte for byte. `None` when the owner had none.
    Delegated { version: Option<Version> },
}

struct RecipeEvaluation<V: ValueInterface> {
    value: Arc<V>,
    dependencies: Vec<DependencyRecord>,
    origin: ValueOrigin,          // was: delegated: bool
}
```

Private to `assets.rs`, `Debug + Clone + Copy`-free by intent (it holds an `Option<Version>`, which
is `Copy`, so `#[derive(Debug, Clone, Copy)]` is free and worth having for logging).

### `MetadataRecord.version` — unchanged

Already `Option<Version>` with `#[serde(default)]` (`metadata.rs:941`, documented at `:938`), already persisted in the
sidecar, already documented as "computed at save time as `Version::from_bytes(content)`" — the
sentence this design finally makes true. Records written before this deserialize to `None`, which
is what they mean. **No migration, no format change, no version bump.**

### `DependencyManager` — unchanged shape

No new field, no new map. Provisional registration writes into the existing `versions` map, which
is what makes it cheap: the correction is performed by the `register_version` comparison that
already exists.

## Trait Implementations

None added, none changed. Every change is an inherent method on `AssetRef<E>` / `AssetData<E>` or on
the crate-private `DependencyManager<E>`. `AssetManager` gains no method, so `liquers-py`,
`liquers-web` and `liquers-axum` see no signature change — which is what keeps this `M`-shaped work
inside an `L`-classified issue.

## Sync vs Async

| Operation | Choice | Rationale |
|---|---|---|
| `assign_version` | `async` | Takes the asset's `tokio::sync::RwLock`, and is called from `evaluate`. |
| Serialization itself (`State::as_bytes`) | **sync, outside the lock** | It is CPU-bound with no I/O. Following `serialize_to_binary`'s existing shape — read-lock, drop, serialize, write-lock, install — rather than holding the write lock across a potentially large encode. This is the established precedent on this exact operation, not a new pattern. |
| `version_for_tracking` | `async` | Same lock. |
| `add_dependency` provisional insert | `async`, no new await points | `scc`'s `entry_async` is already awaited there. The entry guard is dropped **before** any `expire(...)` call, as today. |
| Store I/O | unchanged | This design adds no store read or write. The tracking-time net deliberately does not re-persist (Phase 1 decision). |

No blocking I/O is introduced, and no lock is newly held across an `.await`.

## Function Signatures

### `liquers-core/src/assets.rs`

```rust
impl<E: Environment> AssetRef<E> {
    /// Assign this evaluation's version, and — when the value was computed here — the serialized
    /// bytes that produced it.
    ///
    /// Runs after [`Self::try_to_set_ready`] and **before** the `ValueProduced` notification.
    /// That is the earliest point at which the version is final: the value is installed, so its
    /// bytes can be computed, and no observer has read the asset yet. Assigning earlier would
    /// mean assigning provisionally, and a version is published once and never revised — a parent
    /// that recorded a provisional value would hold a version the child no longer has.
    ///
    /// A no-op for a non-keyed or volatile asset. Neither is a dependency-graph node, so neither
    /// needs a version, and serializing every query result would cost the commonest path in the
    /// system.
    ///
    /// Infallible by design: a value that cannot be serialized takes a time-based version and a
    /// log entry rather than failing an evaluation that has already produced a correct value.
    async fn assign_version(&self, origin: ValueOrigin);

    /// The version to register when this asset enters the dependency graph, assigning a
    /// time-based one first if it has none.
    ///
    /// The last-resort net under every route that can reach the graph without a version — a
    /// serialization that failed, a `Metadata::LegacyMetadata` record, a sidecar written before
    /// versions existed. Writes the assigned version back into the asset's metadata, so the asset
    /// and the manager can never disagree, and records that it fired.
    ///
    /// It does **not** re-persist: an asset that reaches this net left no durable trace, so it
    /// cannot be proved to reconstruct identically and its dependents *should* expire on restart.
    pub(crate) async fn version_for_tracking(&self) -> Version;

    /// (changed) Reads through the ungated accessor.
    /// `poll_state()` → `poll_state_any_status()`; fixes SERIALIZE-TO-BINARY-CONSULTS-THE-READ-GATE.
    async fn serialize_to_binary(&self) -> Result<Option<(Arc<Vec<u8>>, Arc<Metadata>)>, Error>;
}
```

`evaluate`'s changed sequence — the invariant order in its doc comment gains one step:

```rust
lock.data = Some(value);
lock.binary = None;              // NEW — EVALUATE-DOES-NOT-CLEAR-CACHED-BINARY
drop(lock);
self.try_to_set_ready().await;   // 5. status authority, unchanged
self.assign_version(origin).await; // 5b. NEW — version is final from here on
// 6. ValueProduced notification, unchanged — now fired with the version in place
...
if is_keyed && matches!(origin, ValueOrigin::Computed) {   // was: is_keyed && !delegated
    self.persist_with_status_tracking(save_in_background, cancelled).await;
}
```

### `liquers-core/src/dependencies.rs`

```rust
impl<E: Environment> DependencyManager<E> {
    /// (changed) An unregistered *asset* dependency is unverifiable, not stale.
    ///
    /// The dependent's recorded version is written provisionally, so the dependency's real
    /// registration compares against it through the existing `register_version` path and cascades
    /// if it differs. Deferred verification, not absent verification.
    ///
    /// Command keys keep the old behaviour — see "Integration Points".
    pub async fn add_dependency(
        &self,
        dependent: &DependencyKey,
        dependency: &DependencyKey,
        version: Version,
    ) -> Result<ExpiredDependents<E>, Error>;   // signature unchanged

    /// (changed) Uses `AssetRef::version_for_tracking()` instead of
    /// `mr.version.unwrap_or(Version::new(0))`.
    pub async fn track_asset(&self, asset: &crate::assets::AssetRef<E>) -> ExpiredDependents<E>;

    /// (changed) The vacuous `include_root || current != *key` condition is removed; the comment
    /// is corrected to describe the zero-version policy the branch now implements.
    async fn expire_internal(&self, key: &DependencyKey, include_root: bool) -> ExpiredDependents<E>;
}
```

`version_consistent` and `register_version` keep their signatures and their bodies. The provisional
decision lives in `add_dependency` because that is the only caller for which "unregistered" is a
question rather than an answer.

## Integration Points

### The provisional rule applies to asset keys, not command keys

This is the one non-obvious decision in the design, and skipping it would be a regression.

`AssetManager::start` loads **every** command's metadata and implementation version through
`load_command_versions_sync` (`assets.rs:3419`) before any asset is loaded. So for a command key the
manager's knowledge is *complete*: absence means the command no longer exists in this build, or its
declared version was withdrawn — which is exactly the case a dependent should be expired for.
Registering a removed command's version provisionally would keep a dependent alive across the
removal of the command that produced it.

For an asset key the manager's knowledge is *incremental* — a key enters `versions` only when
something evaluates or fast-tracks it — so absence carries no information.

```rust
// in add_dependency, when `versions` has no entry for `dependency`
if dependency.is_command_metadata() || dependency.is_command_implementation() {
    return Ok(self.expire(dependent).await);   // absence is evidence: the command is gone
}
// asset key: absence is not evidence
entry.insert_entry(version);
```

`DependencyKey::is_command_metadata()` and `is_command_implementation()` already exist
(`metadata.rs:137`, `:141`).

An unversioned command is never registered at all (`load_command_versions_sync` skips
`is_unknown()`), and `register_plan_dependencies` only records an edge when `get_version` returns
`Some` — so a dependent never carries a concrete version for a command that has none, and this
branch cannot fire on that path.

### The provisional rule also closes an in-process race

A parent that reads a freshly-Ready child records the child's metadata version (available from
step 5b) and calls `add_dependency` — but the child registers with the manager only at step 8,
`track_asset`. Between those, the manager has no entry for the child's key. Today that window is
harmless because the recorded version is unknown; with real versions it would expire the parent.

The provisional rule removes the window without reordering anything: the parent's `add_dependency`
inserts the child's version provisionally, and the child's own `register_version` then finds an
equal entry and does not cascade. **This is why `track_asset` does not need to move earlier**,
which was the open half of Phase 1's assignment-point decision.

### Delegation

`evaluate_recipe_outcome`'s delegation branch already holds the delegate's `State`
(`assets.rs:2410`, from `wait_for_dependency`). It returns
`ValueOrigin::Delegated { version: state.metadata.version() }`. The hand-off continues to transfer
no dependency records; the version is the one field that crosses it.

The `is_keyed && !delegated` persistence guard becomes `is_keyed && matches!(origin, Computed)` and
keeps its meaning. It gains a second justification, which the code comment should state: the
hand-off carries no dependency records, so a persisted delegating asset would store a record
claiming a real version with an empty dependency list, which `try_fast_track` would later read as
"nothing to check".

### What is *not* touched

- `try_fast_track` — no change. Its recorded-dependency check becomes non-vacuous, which is
  intended, and its `if let Some(version) = self.metadata.version()` registration is already
  correct: a stored record with no version is one the net will handle when the asset is tracked.
- `set_binary` / `set_state` on both managers — already version their inputs. This design makes
  the evaluate path match them rather than changing them.
- `register_plan_dependencies`, `refresh_command_versions*`, `load_command_versions_sync` —
  unchanged.
- Every crate above `liquers-core`.

## Relevant Commands

**No new commands, and no command signature changes.** This is runtime asset behaviour, below the
command layer entirely. `specs/command_registry.yaml` does not change and
`cargo test -p liquers-lib --test registry_export` is unaffected.

Existing namespaces are relevant only as *test material*: Phase 3 needs a keyed chain of computed
assets, which is built from ordinary recipes over the default namespace (`hello`/`world`-style
commands as in `expiration_integration.rs`) plus a deliberately non-serializable value for the
fallback path. No `pl`, `img`, `lui` or `egui` command is involved. **Question for the user, per
the workflow:** confirm that the default namespace is the right test surface here, rather than
exercising the fallback through a `liquers-lib` rich value type — the latter would move the tests
out of `liquers-core`, where the change lives.

## Documentation Architecture

| Path | Kind | Audience | Change | Links |
|---|---|---|---|---|
| `specs/reference/DEPENDENCIES_STATUS.md` | reference | internal | **Extend.** Its "Current contract" states `Version::unknown()` semantics; add that every non-volatile keyed asset now carries a concrete version, that a zero is reserved for a future policy rather than produced by any path, and the provisional rule for an unregistered dependency (with the command-key exception). Correct Flow A step 3 and Flow B step 4, which describe a computed dependency's version as unknown. `## History` row + `reviewed:` bump in the same commit. | → this design folder while `built`; → the issue |
| `specs/reference/ASSETS.md` | reference | internal | **Review, update only if it states the evaluation sequence.** It does not mention `version` today, so this may be a confirmed no-op recorded in Phase 5. | — |
| `specs/reference/ASSET_LIFECYCLE.md` | reference | internal | Same treatment as `ASSETS.md`. | — |
| `specs/issues/KEYED-EXPIRY-DOES-NOT-CASCADE-TO-KEYED-DEPENDENTS.md` | issue | — | `status: closed` + resolution note with test evidence. | → design |
| `specs/issues/SERIALIZE-TO-BINARY-CONSULTS-THE-READ-GATE.md` | issue | — | `status: closed` — absorbed. | → design |
| `specs/issues/EVALUATE-DOES-NOT-CLEAR-CACHED-BINARY.md` | issue | — | `status: closed` — absorbed. | → design |
| `specs/design/stale-dependency-status-finalization/DESIGN.md` | design | — | Record whether the blocker is discharged and whether C2 is now revisitable. | → this design |
| `specs/README.md` | map | — | Capability line moves `designing` → `built`, then → `documented` if the reference work makes that the entry point. | — |

**Proposed authoritative `affects_docs`:** `[DEPENDENCIES_STATUS, ASSETS, ASSET_LIFECYCLE]` — as
already set in `DESIGN.md`. `DOC_03_ASSETS_EXECUTION_LIFECYCLE` was considered and excluded: it
describes execution lifecycle rather than the dependency contract, and Phase 5 will confirm by
reading it rather than by assumption.

No new reference and no guide, per Phase 1. Reconsider the guide decision only if Phase 3's cascade
assertions turn out to need a reusable test recipe.

## Error Handling

Every path uses `liquers_core::error::Error` with typed constructors; no new error type, no
`Error::new`.

| Failure | Handling | Why |
|---|---|---|
| `State::as_bytes` fails (non-serializable value) | Not an error. Time-based version, `LogEntry::warning` naming the underlying `Error`, `lock.binary` left `None`. | The evaluation produced a correct value; refusing it because it cannot be hashed would turn a versioning concern into a data-loss one. `set_state` already takes this branch (`assets.rs:5245`, and `:6364` on the immediate manager). |
| `Metadata::set_version` fails (`LegacyMetadata` that is not `Null`) | `LogEntry::warning`, continue. | Matches `try_to_set_ready`, which logs and continues when `set_status`/`set_expiration_time_from` fail. The asset then reaches the tracking-time net, which is where a legacy record is expected to be caught. |
| `poll_state_any_status()` returns `None` at finalization | `LogEntry::warning`, no version. | Should be unreachable — the value was just installed — so it is recorded rather than asserted, and the net catches the asset downstream. |
| Dependency cycle in `add_dependency` | Unchanged: `Err(Error::dependency_cycle(dependent))`. | The provisional insert happens *before* the cycle check today and must stay ordered as it is, so a rejected edge does not leave a version behind for a key nothing depends on. **Phase 4 must order these deliberately** — see below. |
| `expire(...)` inside `add_dependency` | Unchanged: `Ok(ExpiredDependents)`, never an `Err`. | `load_from_records`'s doc comment claims it ignores `DependencyVersionMismatch` *errors*; no such error is produced. The comment is corrected in the same change. |

**One ordering point Phase 4 must not get wrong.** In `add_dependency` the version check runs
before the cycle check. If the provisional insert is placed in the version branch, a subsequently
rejected (cyclic) edge leaves a provisional version for a dependency that gained no dependent. That
is harmless — the entry is what the dependency would register anyway — but it should be a decision
rather than an accident, and the alternative (insert after the cycle check passes) costs a second
map access. Recommendation: keep it in the version branch and say so in a comment.

## Rust Best Practices Review

Applied to the signatures above.

- **No `unwrap`/`expect`.** `assign_version` and `version_for_tracking` are infallible by
  construction, not by unwrapping: every fallible step has a named fallback.
- **Typed error constructors only.** No error is constructed on the new paths; failures are logged.
- **No default match arm.** `match origin { ValueOrigin::Computed => …, ValueOrigin::Delegated { .. } => … }`
  is exhaustive over a two-variant enum, and adding a third origin becomes a compile error — which
  is the reason for the enum over the `bool`.
- **Async default, no blocking I/O.** No store access is added. The one CPU-bound step is performed
  outside the lock, matching `serialize_to_binary`.
- **Ownership.** Bytes are `Arc<Vec<u8>>`, as `AssetData::binary` already is; `ValueOrigin` is
  `Copy` and passed by value. Nothing large is cloned.
- **Minimal bounds.** No new generic parameter or bound anywhere.
- **Crate flow.** `dependencies.rs` calling `AssetRef::version_for_tracking` is `liquers-core`
  calling `liquers-core`; `track_asset` already takes an `&AssetRef<E>`. The *mutation* lives in
  `assets.rs` deliberately, so the dependency graph does not become a thing that edits assets.
- **Advisory:** `assign_version` takes `origin` by value. It is `Copy`, so this is free and avoids a
  borrow that fights the write lock; noted so nobody "optimizes" it into a reference.

## Existing Tests This Changes

Found by the Phase 2 codebase-alignment review; the architecture is unchanged by it, but the change
set is not complete without these.

| Test | Where | What happens | Action |
|---|---|---|---|
| `add_dependency_fails_unregistered_dep` | `dependencies.rs:835` | **Breaks.** It registers `-R/a`, leaves `-R/b` unregistered, calls `add_dependency(&a, &b, Version::new(42))` and asserts `expired.keys.contains(&a)`. Those are *asset* keys, so the provisional rule applies and `a` must no longer expire. | Rewrite in place: assert the edge is recorded, `a` is **not** expired, and `get_version(&b) == Some(Version::new(42))` — the provisional entry. Rename to `add_dependency_registers_unregistered_asset_dep_provisionally`. Its old name records a behaviour that was never intentional. |
| — (new) | `dependencies.rs` | No test covers the command-key exception. | Add `add_dependency_expires_on_unregistered_command_dep`, using `DependencyKey::for_command_implementation(...)`, asserting the dependent *is* expired. This is the pair that keeps the two branches from being collapsed later. |
| `add_dependency_fails_stale_version` | `dependencies.rs:824` | Unaffected — the dependency **is** registered, at a different version. | None. |
| `add_dependency_version_zero_skips_check` | `dependencies.rs:846` | Unaffected — `Version(0)` short-circuits before the provisional branch. | None. |
| `version_consistent_unregistered_returns_false` | `dependencies.rs:792` | Unaffected — `version_consistent` keeps its behaviour; only `add_dependency`'s *use* of the unregistered case changes. | None, but Phase 3 should note the two now deliberately disagree, and why. |
| `expire_skips_version_zero_cascade` | `dependencies.rs:934` | Unaffected. Removing the vacuous `include_root \|\| current != *key` condition changes no outcome, because the condition was already true on both call paths; the test registers `Version(0)` by hand, which is exactly the policy case the branch now exists for. | None — it becomes the regression test for the zero-version policy, and its comment should say so. |
| `test_dependent_expiration`, `test_dependent_expiration2` | `tests/expiration_integration.rs:282`, `:355` | Expected to keep passing. Their dependent is invalidated through `dependent_assets` outside the version guard, and there is only one level of it. | None. **They are the reason the defect survived** — measurement (Phase 3) shows a *direct* keyed dependent is invalidated at HEAD too, so no two-asset test of any shape fails. Phase 3 adds the three-link chain that does. |

Baseline before any change (2026-09-05, `CARGO_INCREMENTAL=0`): `liquers-core` 793 lib, 34
`expiration_integration`, 5 `dependency_manager_integration`, 4 `dependency_scheduling` — 0 failures.

## Documentation Corrections Inside the Code

Two doc comments state things that are not true and are corrected in this change, because the
change makes them load-bearing rather than merely wrong:

1. `DependencyManager::load_from_records` (`dependencies.rs:657`) — "Ignores
   `DependencyVersionMismatch` errors (the loaded dependency version may have advanced since the
   record was written)." `add_dependency` never returns that error; a version mismatch comes back
   as `Ok(expired)` and the expiry is applied. Only `Err(dependency_cycle)` is swallowed. The
   corrected comment must say which of the two it drops, since after this change the `Ok(expired)`
   path is the ordinary one.
2. `DependencyManager::expire_internal` (`dependencies.rs:589`) — "we don't cascade to its
   dependents (except for the root key)". No such exemption exists. The corrected comment states
   the rule the branch actually implements: a key registered at `Version(0)` does not propagate
   invalidation, root or not, and after this change no path produces such a registration by
   accident — it is reserved for a declared policy.

## Gate Decisions (2026-09-05)

All three questions answered by the project owner; nothing is left open at this gate.

### 1. Clock: chrono, and the purpose is uniqueness rather than time

> "Use chrono — do I understand that it works crossplatform? It is not the purpose to provide
> system time, it should just generate reasonably unique versions."

**Yes, cross-platform.** `chrono`'s default features include `wasmbind`, which routes
`Utc::now()` through `js_sys::Date` on `wasm32-unknown-unknown`; on native it reads the OS clock.
The evidence is in this repository rather than in the changelog: every metadata timestamp
(`metadata.rs:561`, `:1385`, `:2268`) and every expiry comparison (`expiration.rs:823`, `:853`)
already takes wall time from `chrono::Utc::now()`, and those paths run inside `liquers-web`, which
is wasm32-only. `std::time::SystemTime::now()` is the one that is not supported there.

**The reframing changes what to build.** If the goal is uniqueness rather than a timestamp, the
clock is doing exactly one job: separating *processes*. Uniqueness *within* a process comes from a
counter, and a counter alone would be wrong — it restarts at zero, so a second process could
re-issue a version a first process already handed out, and a dependent recording the old one would
match and be served warm. That is precisely the case the durability decision requires to expire. So
a coarse clock is sufficient and a clock is necessary.

`Version::new_unique()` (`metadata.rs:69`) already has exactly this shape — nanoseconds shifted
left 64 bits, OR'd with an atomic counter. Only its clock source is wrong. So:

- **Reimplement `Version::new_unique()` on `chrono::Utc::now()`**, keeping the counter. No new
  constructor, and nothing depends on its current value — its only use today is one test
  (`dependencies.rs:742`).
- **The fallback path calls `new_unique()`, not `from_time_now()`.** Uniqueness is the requirement;
  a bare timestamp can repeat within a clock tick.
- Resolution differs by platform and that is fine by construction: `js_sys::Date::now()` is
  milliseconds, so on wasm the counter carries the uniqueness and the clock only separates
  processes — which is all it is being asked to do.

**Scope addition, small and same-defect:** switch `Version::from_time_now()` to chrono as well, and
point the two existing non-serializable fallbacks (`set_state` at `assets.rs:5245` and the
immediate manager's at `:6364`) at `new_unique()`. That removes `std::time::SystemTime::now()` from
`liquers-core` entirely and closes a *reachable* wasm hazard — `set_state` with a non-serializable
value, callable from `liquers-web` today. Three lines. Split it out if the gate would rather keep
this design to the evaluate path; leaving it would mean fixing the new path and leaving the old one
to panic on the platform the new one was made safe for.

**Not verified by compilation.** `wasm32-unknown-unknown` is not installed in this environment, so
the claim that `SystemTime::now()` panics there — and that the chrono replacement does not — is
reasoning, not evidence. **Phase 4 adds the target and checks**; it is a `rustup target add` plus
the existing `scripts/check-build-matrix.sh` wasm rows.

### 2. Test surface: no new value type, no new code

> "Whatever is the easiest/shortest code. A non-serializable Value type — or serializable string,
> but non-serializable int should also be implemented for the purpose of the test."

The second option already exists and needs **zero new code**. `Value::as_bytes` refuses an integer
for the `bytes`/`b`/`bin` data format and accepts a string (`value.rs:965-975`):

```rust
"bytes" | "b" | "bin" => match self {
    Value::Bytes(x) => Ok(x.clone()),
    Value::Text(x) => Ok(x.as_bytes().to_vec()),
    _ => Err(Error::new(ErrorType::SerializationError, …)),   // I32/I64/F64/None land here
},
```

and `data_format` is seeded from the key's extension (`metadata.rs:777`, `:1203`). So:

| Recipe target | Command returns | `as_bytes` | Version assigned |
|---|---|---|---|
| `count.bin` | `Value::I32` | `Err(SerializationError)` | time-based fallback, logged |
| `greeting.txt` | `Value::Text` | `Ok(bytes)` | `Version::from_bytes` |

Both are ordinary default-namespace commands, both stay in `liquers-core` where the change lives,
and the pair differs in one character of a filename — which also makes the test readable. Phase 3
builds the keyed→keyed chain from these.

### 3. `expire_internal` root guard: delete the vacuous condition — **agreed**

Confirmed by the owner. `include_root || current != *key` is removed so the version is always
consulted, and the comment is corrected to describe the rule the branch implements: a key
registered at `Version(0)` does not propagate invalidation, root or not. The claimed
"(except for the root key)" exemption is deleted rather than implemented, because under the
zero-as-policy reading an asset that has opted out of version-based invalidation should stay out
even when it is the root of an explicit expiry.


---

# Revision 2 (2026-09-05) — the version authority

**Status of this section: it supersedes parts of everything above.** The Phase 4 review established
that persisted `DependencyRecord.version` is always zero (`DESIGN.md`, B1), which made the
cross-process half of Revision 1 rest on a false premise. The owner chose **Option B — fix the
record here** — on the grounds that there is more context to reason about correctness with while
these mechanisms are in hand, and proposed the architecture below.

> "We probably need some authoritative way to obtain a version — perhaps a `version(key)` method on
> the asset manager? If the asset is known, it is just returned from the asset. If it is in store
> only, then metadata are loaded and the version is extracted. This may eventually be passed as a
> closure to dependency manager, e.g. to verify dependencies on a specific asset. Limited time use
> would be desirable to prevent dependency manager to create yet another cyclic arc leak."

## What this replaces, and what it simplifies away

| Revision 1 | Revision 2 |
|---|---|
| Provisional registration in `add_dependency` | **Deleted.** With an authority that can answer "what version does this key have", absence is no longer something to guess about. This removes U1's rewrite premise, U3, U4, U9, P12 and I8/I9's approximation framing. |
| The command-key exception | **Deleted as a special case.** It existed because absence meant different things for the two key kinds. The authority answers for asset keys and the manager already holds command versions, so one rule covers both. |
| `DEPENDENCY-VERSIONS-NOT-LOADED-OR-VERIFIED-FROM-STORE` as follow-up | **Absorbed.** The store lookup *is* the authority's second branch. That issue closes with this design. |
| `assign_version` after `try_to_set_ready` | **Merged into it.** See C5. |

Revision 2 is a larger change than Revision 1 but a *smaller* set of concepts: one authority
replaces one approximation plus one exception plus one deferred issue.

## C1 — `AssetManager::version(key)`, the single authority

```rust
/// The authoritative version of a keyed asset, without evaluating it.
///
/// Three sources, in order:
/// 1. a live asset registered for `key`, **if it has a version yet** — an asset that is
///    mid-evaluation does not, and must not shadow the durable answer below;
/// 2. otherwise, the store's metadata for `key`, if it holds any;
/// 3. otherwise `None` — the key has no durable version, which is not the same as
///    `Version::unknown()` and must not be conflated with it.
///
/// **This never evaluates and never submits.** It is a map read and at most one metadata read,
/// for the same reason `owned_key_asset` is (`specs/design/keyed-recipe-ownership/`): asking a
/// question about an asset must not be able to run it, or an inline manager recurses until the
/// stack is gone.
async fn version(&self, key: &Key) -> Result<Option<Version>, Error>;
```

Added to the `AssetManager` trait with a **default implementation** so `liquers-py`,
`liquers-web` and any out-of-tree implementor keep compiling — the project rule is to extend traits
with defaulted methods rather than change them.

`Ok(None)` and `Err` are different answers and stay different: a store that fails to read is not a
key without a version, and collapsing them would silently expire dependents on a transient store
error.

## C2 — `VersionResolver`, borrowed and never stored

```rust
/// Resolves a dependency key's authoritative version for the dependency manager.
///
/// **Passed as `&dyn VersionResolver` for the duration of one call and never retained.** The
/// dependency manager is a field of the asset manager, so storing an `Arc` to the manager here
/// would close a third reference cycle on top of the two `ENVIRONMENT-MANAGER-REFERENCE-CYCLE`
/// already records. A borrow cannot leak; that is the whole reason for the shape.
#[async_trait]
pub(crate) trait VersionResolver: Send + Sync {
    async fn resolve_version(&self, key: &DependencyKey) -> Option<Version>;
}
```

`DefaultAssetManager` and `ImmediateAssetManager` implement it by delegating to C1 for asset keys
and returning `None` for command keys, which the manager's own `versions` map already holds
authoritatively from startup.

Call sites take it as a parameter:

```rust
pub async fn add_dependency(&self, dependent: &DependencyKey, dependency: &DependencyKey,
                            version: Version, resolver: &dyn VersionResolver)
    -> Result<ExpiredDependents<E>, Error>;
pub async fn load_from_records(&self, dependent: &DependencyKey, records: &[DependencyRecord],
                               resolver: &dyn VersionResolver) -> ExpiredDependents<E>;
pub async fn track_asset(&self, asset: &AssetRef<E>, resolver: &dyn VersionResolver)
    -> ExpiredDependents<E>;
```

Every caller is inside the asset manager, which passes `self` — two immutable borrows of the same
value, no refcount, no lifetime beyond the call. **No `DependencyManager` field is added.**

## C3 — `add_dependency` asks the authority instead of guessing

```rust
if !version.is_unknown() {
    let known = match self.versions.get_async(dependency).await {
        Some(entry) => { let v = *entry.get(); drop(entry); Some(v) }
        None => resolver.resolve_version(dependency).await,   // live asset, else the store
    };
    match known {
        Some(known) if !known.matches(&version) => return Ok(self.expire(dependent).await),
        Some(_) => {}
        // No durable version anywhere: the dependency cannot be shown to reconstruct
        // identically, so the dependent is not entitled to be served. This is the owner's
        // durability rule, and it is the row `DEPENDENCY-VERSIONS-NOT-LOADED-OR-VERIFIED-FROM-STORE`
        // was filed for.
        None => return Ok(self.expire(dependent).await),
    }
}
```

Three outcomes, matching the table that issue defined: verified fresh, verified stale, not durable.
No provisional entry, no ordering dependence, no command-key branch.

## C4 — the record carries the version the dependency actually had

Two writes, both currently wrong (`DEPENDENCY-RECORD-VERSION-CAPTURED-BEFORE-DEPENDENCY-EVALUATES`,
`PLAN-DEPENDENCY-RECORDS-HARDCODE-VERSION-ZERO`):

- **`Context::wait_for_dependency` (`context.rs:657`) upserts after the wait.** This is the single
  funnel where a dependency's `State` — and therefore its settled version — becomes available to
  the dependent's context. `Context::add_dependency` already preserves a known version over a later
  unknown one, so a second call upgrades the schedule-time zero and nothing else has to change.
  The schedule-time capture at `context.rs:553` stays: it is what feeds the *cycle check*, which
  must happen before the dependency runs.
- **`finalize_plan` (`interpreter.rs:71`) stops hard-coding `Version::new(0)`** and uses the
  version `register_plan_dependencies` looks up eleven lines later.

**Known and accepted gap:** a command that calls `Context::evaluate` and then awaits
`asset.get()` directly — permitted by `DEPENDENCIES_STATUS.md` Flow B step 5 — bypasses
`wait_for_dependency`, so its record stays unknown. Unknown is *compatible*, so this under-detects
staleness rather than inventing it. Recorded rather than fixed; closing it means routing every
consumption through one place, which is `CORE-EVALUATE-PATH-CONSOLIDATION`'s business.

**The upgrade transition is gentle, and this is worth checking rather than hoping.** Every record
persisted before this change is zero, and zero matches anything — so the first run against an
existing store invalidates nothing. Only assets persisted *after* the change carry real versions
and become subject to verification. There is no migration and no invalidation storm.

## C5 — assignment is atomic with the status transition (supersedes the Revision 1 placement)

The Phase 4 review showed the Revision 1 placement does not close the window it was chosen to
close: `try_to_set_ready` sets `Ready` while `data` is already present, so `poll_state` returns
`Some` the moment its write lock drops, and both `AssetRef::get` (`assets.rs:3043`) and
`AssetManager::wait_for_dependency` (`:4801`) re-poll at the top of their loops on *any* wake-up —
including the log and progress messages the service loop is sending concurrently. A delegate
observed inside that window yields `Delegated { version: None }`, which is the failure mode Phase 1
named.

So: **serialize first, then install bytes, version and status in one write transaction.**

```rust
// evaluate(), after the value is installed and the binary cleared
let prepared = self.prepare_version(origin).await;   // serializes OUTSIDE any lock; no status change
self.finalize_status_with_version(prepared).await;   // one write lock: binary + version + status
```

`prepare_version` reads the value through an ungated accessor — it runs *before* the status is
final, so it cannot depend on the read gate. This is the same rule `binary_unchecked` and
(after Step 2) `serialize_to_binary` already follow.

This makes the ordering an invariant of the code rather than of a comment. Phase 3's P3 stops being
"a constraint no test can hold"; the comment stays, but it now documents a structure rather than
substituting for one.

## C6 — the fallback's log entry is written under the asset's own write lock

`AssetServiceMessage::LogMessage` handling calls `save_metadata_to_store` (`assets.rs:2060`). A
fallback warning routed through the service channel would therefore **persist** the fallback
version, contradicting the "the net does not re-persist" decision and creating the metadata-only
store entry that I4 asserts does not exist. It must be `lock.metadata.add_log_entry(...)` under the
write lock the assignment already holds.

## C7 — two pre-existing facts that Phase 5 must not publish as contract

- **`track_asset` is not the single funnel.** There are five `register_version` call sites, four
  outside it (`assets.rs:1138` in `try_fast_track`, `:5165`, `:5295`, `:6331`/`:6390`), and
  `try_fast_track` never calls `track_asset` at all. The tracking-time net therefore does not cover
  a fast-tracked asset. Benign — an *absent* entry does not set `skip_cascade`, only a *zero* one
  does — but the "single funnel" wording in Phase 1 is wrong and must not reach the reference.
- **`versions` retains an entry for a key that later becomes volatile.** `DependencyManager::remove`
  has no production caller. Pre-existing; it matters now only because Phase 5 was about to publish
  the volatile exclusion as a contract.

## Consequences for Phase 3 and Phase 4

| Item | Change |
|---|---|
| U1, U3, U4, U9, P6, P12 | **Removed** — they test the provisional rule, which no longer exists. |
| U2 (`add_dependency_expires_on_unregistered_command_dep`) | **Replaced** by a resolver-based trio: verified-fresh, verified-stale, no-durable-version. |
| I8, I9 | **Kept, and now meaningful.** They become the real cross-process tests rather than tests of an approximation, and I9 can pass. |
| New | A test that a record carries the dependency's post-evaluation version (C4), and one that an old zero record still matches (the gentle-transition property). |
| New | `AssetManager::version` unit tests: live asset, store-only, absent, store error. |
| Phase 4 Step 3 | Rewritten around C3. Group B grows: the resolver, the trait method, and the two record fixes. |
| Phase 4 ordering | The B-before-C argument becomes **real** rather than hypothetical: with C4 in place, records do carry concrete versions, so C3 must exist before them. |

## Gate decision: `version` goes on the trait, defaulted (owner, 2026-09-05)

Confirmed. The default body needs nothing the trait does not already require — `lookup_key_asset`
(a required sync map read, `:3781`) and `get_envref` (required, `:3826`) — so no implementor has to
do anything, and `liquers-py`, `liquers-web` and any out-of-tree manager keep compiling unchanged:

```rust
async fn version(&self, key: &Key) -> Result<Option<Version>, Error> {
    // 1. A live asset registered for this key, if it has a version yet.
    if let Some(asset) = self.lookup_key_asset(key) {
        if let Some(v) = asset.get_metadata().await?.version() {
            return Ok(Some(v));
        }
        // Deliberately falls through rather than returning None: an asset that is mid-evaluation
        // carries no version yet, while the store still holds the last durable one. Returning
        // None here would make C3 read "no durable version" and expire a dependent whose
        // dependency is merely being recomputed.
    }
    // 2. The store's metadata. `contains` first, so an absent key is `Ok(None)` and a failing
    //    store is `Err` — see below.
    let store = self.get_envref().get_async_store();
    if !store.contains(key).await? {
        return Ok(None);
    }
    Ok(store.get_metadata(key).await?.version())
}
```

`lookup_key_asset`, not `owned_key_asset`: the map entry for the key *is* the authority here, and
the ownership question is a different one. Both are map reads, neither evaluates.

**`contains` before `get_metadata` is load-bearing.** `AsyncStore::get_metadata` returns `Err` for a
missing key, and mapping that error to `Ok(None)` would make a store outage indistinguishable from
"this key has no version" — which under C3 expires every dependent. Asking `contains` first keeps
the two answers apart, at the cost of one extra store round-trip on the cold path.

A directory key yields synthesized metadata with no version, hence `None`, which is correct: a
directory is not a versioned asset.


---

# Revision 2.1 (2026-09-05) — corrections from the re-gate review

Two compile-time blockers, both verified independently before acceptance. Revision 2's C1, C3–C7
stand; **C2 is replaced**.

## D1 — the resolver could not reach three of its four callers (replaces C2)

Revision 2 said "every caller is inside the asset manager, which passes `self`". That is false for
the three call sites that carry the load. They are generic code holding
`Arc<E::AssetManager>`, not a concrete manager:

| Caller | Site |
|---|---|
| `AssetRef::evaluate` → `track_asset` | `assets.rs:2557–2560` |
| `AssetData::try_fast_track` → `load_from_records` | `assets.rs:1116–1143` |
| `AssetRef::record_dependency_on_asset` → `add_dependency` | `assets.rs:1608–1610` |

`Environment::AssetManager` is bound only `AssetManager<Self>` (`context.rs:159`), so there is no
path from `Arc<E::AssetManager>` to `&dyn VersionResolver` and the code as specified does not
compile. The same applies inside `AssetManager`'s own default methods
(`register_plan_dependencies`, `cascade_expire_dependents`, `assets.rs:3897–3936`), where `Self` is
only known to be an `AssetManager<E>`.

**The fix is cheaper than it looks, because the cost is already paid.** `AssetManager` is *already*
sealed by a `pub(crate)` supertrait:

```rust
pub(crate) trait DependencyManagerAccess<E: Environment> { … }   // :3446

#[allow(private_bounds)]
pub trait AssetManager<E: Environment>:
    crate::maybe_send::MaybeSend + crate::maybe_send::MaybeSync
    + DependencyManagerAccess<E>                                  // :3470–3472
```

So "no crate outside `liquers-core` can implement `AssetManager`" is the **status quo**, not a new
cost — and the review's stated objection to the supertrait route does not apply here. Adding
`VersionResolver` alongside `DependencyManagerAccess<E>` follows an established precedent in the
same declaration, and every generic call site then gets `&*manager as &dyn VersionResolver` for
free, because `E::AssetManager: AssetManager<E>` implies it.

The alternative — a blanket `impl<T: AssetManager<E>> VersionResolver for T` — is rejected: it
needs `E` to be inferable at the impl, which it is not for an object-safe `E`-free trait, and it
would collide with any future hand-written impl.

## D2 — `VersionResolver` must follow the `maybe_send` convention

Revision 2 wrote `#[async_trait] pub(crate) trait VersionResolver: Send + Sync`. That breaks
`wasm32`, which is the platform `liquers-web` is. `maybe_send.rs`'s own module documentation states
the rule, and every async trait in this crate follows it (`AssetManager` at `:3467`, `AsyncStore`,
the recipe providers, the command traits):

```rust
/// Resolves a dependency key's authoritative version for the dependency manager.
///
/// **Passed as `&dyn VersionResolver` for the duration of one call and never retained.** Storing
/// it — in a `DependencyManager` field, or anywhere reachable from one — would close a third
/// reference cycle on top of the two `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` records. A borrow
/// cannot leak; that is the whole reason for the shape, and no test can catch its violation.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub(crate) trait VersionResolver:
    crate::maybe_send::MaybeSend + crate::maybe_send::MaybeSync
{
    async fn resolve_version(&self, key: &DependencyKey) -> Option<Version>;
}
```

`MaybeSend + MaybeSync`, not `Send + Sync`: on `wasm32` the markers are vacuous so an
`ImmediateAssetManager` holding `!Send` browser data still implements it, which is exactly what
`liquers-web` needs and what a hard `Send + Sync` would have broken. Two hand-written impls, one
per concrete manager, each delegating to C1's `version()` for asset keys and answering `None` for
command keys (which the manager's own `versions` map already holds authoritatively).

## D3 — C4 corrections

- **A4, naming.** The hard-coded `Version::new(0)` is in **`finalize_plan_expanded`**
  (`interpreter.rs:71`); `finalize_plan` (`:114`) merely calls it. Fix the function that contains
  the line.
- **Q2, the key must be passed, not re-derived.** `Context::wait_for_dependency` receives only
  `&AssetRef<E>`, and deriving a `DependencyKey` from the asset would risk producing a *different*
  key from the one `schedule_dependency_asset` wrote at `context.rs:535` — which, since
  `Context::add_dependency` upserts by key equality (`context.rs:945`), would silently create a
  **second** record rather than upgrading the first. So `wait_for_dependency` takes the
  `DependencyKey` as a parameter from `get_dependency_state`, which already has the query. No
  derivation, no mismatch possible.
- **Q1, the two writers converge by design.** A `PlanDependency` may name an asset key, not only a
  command key. That is not a problem: both writers go through `Context::add_dependency`, which
  matches on key equality and prefers a concrete version over an unknown one, so a plan record for
  an asset key is *upgraded* by the wait-time upsert rather than duplicated. The plan-time value
  remains a schedule-time snapshot, and for command keys — which nothing waits on — it is the only
  writer and is correct there.

## D4 — A3: C3's safety rests on an unnamed convention, so name it

The re-gate traced every spurious-expiry candidate (volatile, non-keyed, command, non-serializable,
concurrently evaluating) and found all of them safe — but safe *because upstream callers pass
`Version::unknown()` for exactly those cases*, and C3's resolver consultation is gated on
`!version.is_unknown()`. That is load-bearing and was nowhere written down.

**Stated as an invariant, and it belongs in `DEPENDENCIES_STATUS.md`:** a concrete version reaches
`add_dependency` only for a dependency that had one, and `Version::unknown()` is the answer for a
volatile, non-keyed, or not-yet-versioned dependency. A future change that made
`record_dependency_on_asset` always produce a concrete version would reactivate the
spurious-expiry risk C3 appears to have designed away.

The re-gate also supplied the sentence that replaces the provisional rule in the reference: the
in-process race Revision 1 needed provisional registration for is now closed by C1's live-asset
check, not by anything in the dependency manager.

## Confirmed clean by the re-gate

C1's default body (both required methods genuinely available to a defaulted method; `get_metadata`
callable), the `contains`-before-`get_metadata` ordering and why it is load-bearing, and
`Context::add_dependency`'s preserve-known-over-unknown upsert. One overclaim corrected: "keeps
every implementor compiling" is trivially true — `DefaultAssetManager` and `ImmediateAssetManager`
are the only implementors in the workspace, and `liquers-py` names the former as its associated
type rather than implementing the trait.


---

# Revision 2.2 (2026-09-05) — record vs. verify, and where an audit would live

Prompted by the owner's question: *does anything like `trigger_dependency_audit(query)` /
`trigger_dependency_audit_all_registered()` exist in this design?*

**No. It does not, and the design as written cannot express the second use case at all.** That is
worth fixing structurally now, because the fix is a simplification.

## Async: already true, but it has a consequence worth naming

`AssetManager::version` and `VersionResolver::resolve_version` are `async` in Revision 2, and
`add_dependency`, `load_from_records` and `track_asset` were already `async`. So nothing changes
mechanically.

What *does* change is a property the codebase had: **the dependency graph performed no I/O.**
Revision 2's C3 puts a store read inside `add_dependency` — a function also reached from
schedule-time edge registration and cycle checking. That is an inversion, not a detail, and it is
the same place the policy question lands.

## The conflation

Revision 2 makes `add_dependency` do two different jobs:

| Job | Cost | Frequency | Policy-dependent? |
|---|---|---|---|
| **Record** that `dependent` depends on `dependency` at an observed version | in-memory | every edge, constantly | no |
| **Verify** that the recorded version still holds | possibly a store read | only meaningful on load or on demand | **yes** |

Fusing them means verification happens wherever an edge is registered, which is the hot path, and
gives no place to stand for a policy. It hard-codes the strict service, use case (a), and makes use
case (b) unreachable.

## The two use cases, and what each needs

**(a) Strict service.** Every guarantee available that a served value is valid. Verify on load, and
possibly on every `get`.

**(b) Long calculation with large intermediates.** The user deletes intermediate files by hand;
the end result stays technically valid and is worth building on. Two sub-cases, and the design
already serves the first one *by accident*:

- **Metadata kept, data deleted.** `AssetManager::version(key)` reads *metadata only* — it never
  touches the value. So the authority can still answer "`a.txt` was version `v1`", and every
  dependent verifies clean, even though the data is gone. **This works today under Revision 2 with
  no policy at all**, and it is a genuinely useful property that fell out of the design rather than
  being aimed at. Worth stating in the reference so nobody "optimizes" `version()` into reading the
  value.
- **Metadata deleted too.** `version()` returns `None`, and Revision 2's C3 reads that as "not
  durable" and expires the dependent. Under a policy where nothing audits, nobody ever asks, and
  the result stays valid. **This is the case that needs the split.**

## The correction: `add_dependency` records; verification becomes an entry point

The decisive observation is that **the keyed→keyed cascade — the defect this design exists to fix —
does not need `add_dependency` to verify anything.** Propagation is driven by `register_version`
and `expire_internal`; `add_dependency`'s check is about detecting a *stale dependent on load*,
which is a different question that happens to share a function.

And removing it costs nothing today: the probe established that a concrete version never reaches
`add_dependency` in production, so its expire branch has **never fired**. Taking it out is removing
a branch that has never run; leaving it in and letting C4 make records concrete would switch it on
by side effect, which is exactly the decision the owner is asking to be able to make deliberately.

So:

```rust
// dependencies.rs — records, never verifies, no I/O, no resolver.
pub async fn add_dependency(&self, dependent: &DependencyKey, dependency: &DependencyKey,
                            version: Version) -> Result<ExpiredDependents<E>, Error>;

/// Verify one key's recorded dependency versions against the authority, and expire it if any
/// no longer hold. `depth` bounds transitive descent; `Depth::Shallow` checks only this key's
/// own records.
pub(crate) async fn audit(&self, key: &DependencyKey, resolver: &dyn VersionResolver,
                          depth: AuditDepth) -> ExpiredDependents<E>;
```

with the manager-level entry points the owner named:

```rust
async fn trigger_dependency_audit(&self, query: &Query) -> Result<AuditReport, Error>;
async fn trigger_dependency_audit_all_registered(&self) -> Result<AuditReport, Error>;
```

**Policy is then simply: who calls these, and when.** On startup; on every `get`; on an explicit
user action; never. This design does **not** build that vocabulary — it builds the seam. The
default is `never`, which is precisely today's behaviour, so this design ships the cascade fix and
no change in when staleness is detected.

## What this does to the rest of the design

| Item | Effect |
|---|---|
| C3 | **Reduced to recording.** The resolver is not consulted on the hot path. |
| C2 / D1 / D2 | **Still needed, and now better placed:** the resolver is passed to `audit`, which is called from the manager, so the `&dyn` borrow is natural and the supertrait may not even be required. Phase 4 determines which. |
| C4 | **Unchanged and still essential** — an audit is worthless if the records it audits carry zeros. |
| C1 | Unchanged. |
| I8 / I9 | Become **audit** tests: I8 asserts a dependent reloaded before its dependency is served (nothing audits), I9 asserts that an explicit `trigger_dependency_audit` expires it. Sharper than before: they now test a named operation rather than an emergent behaviour. |
| The `add_dependency` resolver tests | Move to `audit`. |
| `DEPENDENCY-VERSIONS-NOT-LOADED-OR-VERIFIED-FROM-STORE` | Still absorbed — the authority exists and the audit uses it. |
| Scope | **Net smaller.** One function gains a job it can do without I/O; one new function has the job that needs it. |

## The question this leaves for the owner

The seam costs little and is worth having. **Should this design ship the two `trigger_*` entry
points, or only the internal `audit` plus the `add_dependency` simplification?** Shipping the entry
points means a public API addition with no caller yet, which normally argues against — but here it
is what makes the policy question answerable later without reopening the dependency manager.
Recommendation: **ship `audit` and the entry points, default policy "never", and file the policy
vocabulary as a separate feature** — the entry points are the thing that is expensive to add later,
and they are three lines each over a mechanism this design already builds.
