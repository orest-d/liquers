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
| `test_dependent_expiration`, `test_dependent_expiration2` | `tests/expiration_integration.rs:282`, `:355` | Expected to keep passing. Their dependent is a *query* asset, invalidated through `dependent_assets` outside the version guard, so they never exercised the keyed→keyed path. | None. They are the reason the defect survived; Phase 3 adds the keyed→keyed sibling they lack. |

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

## Open Questions for the Gate

1. **Fallback clock** (Phase 1 open question 3). `Version::from_time_now` and `Version::new_unique`
   both call `std::time::SystemTime::now()`, which is not a supported clock on
   `wasm32-unknown-unknown`, while the rest of the codebase takes wall time from
   `chrono::Utc::now()` (which `metadata.rs` already uses for every timestamp — `:561`, `:1385`,
   `:2268` — and `expiration.rs` for every expiry comparison). `liquers-web` is wasm32-only, and
   that those chrono paths work there is the evidence: `chrono`'s default features include
   `wasmbind`, which routes `Utc::now()` through `js_sys::Date`. `Version::from_time_now` is
   currently reachable on wasm only through `register_command!`'s opt-in `version: now`, which is
   why nothing has tripped over it. Recommendation: **add a `chrono`-based constructor and use it
   on this path**, leaving the existing constructors alone rather than changing behaviour under
   other callers. Needs a decision because it adds a public constructor to `Version`. Not verified
   by compilation — the `wasm32-unknown-unknown` target is not installed in this environment, so
   Phase 4 should add it and check rather than trust this reasoning.
2. **Test surface** (see "Relevant Commands"): default-namespace commands inside `liquers-core`,
   or a `liquers-lib` rich value type for the non-serializable case?
3. **`expire_internal` root guard** (Phase 1 open question 4). Recommendation recorded above:
   delete the vacuous condition so the version is always consulted, and correct the comment. Under
   the zero-as-policy reading this is right — an asset that declares itself out of version-based
   invalidation stays out even when it is the root of an explicit expiry. Flagging it because it is
   the one place where "make the code match the comment" was also available and is being declined.
