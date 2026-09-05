---
title: "Phase 4: Implementation plan — Versions for computed keyed assets"
kind: design
audience: internal
area: [core/assets]
---
# Phase 4: Implementation Plan

## Overview

Eleven steps in three groups, all in `liquers-core`. The grouping is not cosmetic — it is what keeps
every intermediate commit green.

| Group | Steps | Why this group goes first |
|---|---|---|
| **A. Inert preparation** | 1–2 | Change nothing observable while versions are still unknown: a portable clock, and the ungated read the new serialization point needs. |
| **B. Dependency manager** | 3–5 | The provisional rule and the corrected guard must exist **before** real versions arrive. In the other order there is a commit where every recorded dependency version is concrete and `add_dependency` still reads absence as staleness — which empties the cross-process cache and breaks tests for a reason that has nothing to do with the commit that introduced it. |
| **C. Evaluation path** | 6–8 | The change itself. Once this lands the cascade is live. |
| **D. Tests and verification** | 9–11 | Integration tests, the wasm check, and the full matrix. |

Group B is inert on its own: with every computed version still `None`, `add_dependency` never
reaches the provisional branch (the recorded version is `Version(0)`, which short-circuits), and
`expire_internal`'s guard behaves identically. So B can be reviewed and merged as a no-op change to
behaviour, which is the safest way to land the part that is easiest to get wrong.

**Estimated diff:** ~120 lines of source across five files, ~450 lines of tests across four files.

---

## Implementation Steps

### Step 1 — `Version` gets a portable clock

**File:** `liquers-core/src/metadata.rs` (`Version`, `:41`–`:78`)

```rust
/// Creates a version from the current wall-clock time.
///
/// Uses `chrono::Utc::now()` rather than `std::time::SystemTime::now()`: the latter is not a
/// supported clock on `wasm32-unknown-unknown`, and `liquers-web` is wasm32-only. Every other
/// wall-clock read in this crate already goes through chrono for the same reason.
pub fn from_time_now() -> Self {
    Version(chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default().max(0) as u128)
}

/// A version that is unique within this process and, in practice, across processes.
///
/// The counter provides uniqueness; the clock only separates processes, so its resolution does not
/// matter — on wasm `Utc::now()` is millisecond-grained and the counter carries the rest. A bare
/// clock would be wrong here: two assets finalized in the same tick would share a version.
pub fn new_unique() -> Self {
    static UNIQUE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default().max(0) as u128;
    let counter = UNIQUE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed) as u128;
    Version(nanos.wrapping_shl(64) | counter)
}
```

`from_specific_time(SystemTime)` is **kept unchanged** — it takes an explicit instant rather than
reading a clock, so it is portable already and tests use it.

`unwrap_or_default()` rather than `unwrap()`: `timestamp_nanos_opt` returns `None` outside
1677–2262, and a panic in a version constructor is not an acceptable trade for a range nothing will
hit. `.max(0)` keeps the cast total.

Also in this step, the two existing non-serializable fallbacks move from `from_time_now()` to
`new_unique()` — `assets.rs:5245` and `:6364` — since uniqueness is what they need and a bare
timestamp can repeat within a tick.

**Add:** U7 `version_new_unique_is_distinct_within_one_clock_tick`, U8
`version_from_chrono_clock_is_never_unknown` (Phase 3).

**Validate:** `cargo test -p liquers-core --lib metadata::` · `grep -rn "SystemTime::now" liquers-core/src/` returns nothing.

**Agent:** haiku · rust-best-practices · knowledge: `metadata.rs` `Version` block, Phase 2 gate decision 1.

---

### Step 2 — `serialize_to_binary` reads through the ungated accessor

**File:** `liquers-core/src/assets.rs:2698`

One line: `self.poll_state().await` → `self.poll_state_any_status().await`, plus a comment matching
the one already above `binary_unchecked` at `:2619` — *persisting is not a read of the exposed
value*. Closes `SERIALIZE-TO-BINARY-CONSULTS-THE-READ-GATE`.

**Validate:** `cargo test -p liquers-core --lib --tests` — expect no behaviour change; nothing
persists at a gated status today, which is exactly why the issue was filed as latent.

**Agent:** haiku · rust-best-practices · knowledge: the issue, `assets.rs:2600–2710`.

---

### Step 3 — `add_dependency`: provisional registration, with the command-key exception

**File:** `liquers-core/src/dependencies.rs:236`–`:262`

```rust
if !version.is_unknown() {
    let stored = self.versions.get_async(dependency).await.map(|e| { let v = *e.get(); drop(e); v });
    match stored {
        Some(stored) if !stored.matches(&version) => return Ok(self.expire(dependent).await),
        Some(_) => {}
        None if dependency.is_command_metadata() || dependency.is_command_implementation() => {
            // The manager's knowledge of commands is COMPLETE: `AssetManager::start` registers
            // every command's versions before any asset loads. So absence here is evidence —
            // the command was removed, or its declared version withdrawn — and the dependent
            // must not survive the disappearance of the command that produced it.
            return Ok(self.expire(dependent).await);
        }
        None => {
            // An asset key enters this map only when something evaluates or fast-tracks it, so
            // absence carries no information: this is "not verified yet", not "changed".
            // Record what the dependent expects; the dependency's real registration compares
            // against it through `register_version` and cascades if it differs. Deferred
            // verification — see DEPENDENCY-VERSIONS-NOT-LOADED-OR-VERIFIED-FROM-STORE for the
            // mechanism that replaces this approximation with a store lookup.
            //
            // Placed before the cycle check deliberately: a rejected cyclic edge leaves behind
            // the version the dependency would have registered anyway.
            let _ = self.versions.insert_async(dependency.clone(), version).await;
        }
    }
}
```

The `scc` guard is dropped before any `.await` on `expire`, matching the existing code.
`version_consistent` is **not** changed — it answers "is this registered and matching", and
`add_dependency` now decides separately what to conclude from `None`.

Also correct `load_from_records`'s doc comment (`:657`): it claims to ignore
`DependencyVersionMismatch` errors, which `add_dependency` never returns. It ignores
`Err(dependency_cycle)`; a mismatch arrives as `Ok(expired)` and is applied.

**Rewrite** `add_dependency_fails_unregistered_dep` → U1, **add** U2, U3, U4 and U9 (Phase 3). U3 and U4 are the pair that makes the approximation defensible — that a provisional entry equal to the later real registration does not cascade, and that a differing one does. Without them the provisional rule is only tested as "does not expire", which is half of it.

**Validate:** `cargo test -p liquers-core --lib dependencies::`

**Agent:** sonnet · rust-best-practices · knowledge: `dependencies.rs` in full, Phase 2 "Integration Points", Phase 3 U1/U2/U9. Sonnet rather than haiku: this is the step with the subtle branch, and the one a reviewer already caught a missing test for.

---

### Step 4 — `expire_internal`: delete the vacuous condition, correct the comment

**File:** `liquers-core/src/dependencies.rs:588`–`:599`

```rust
// A key registered at `Version(0)` does not propagate invalidation: the key itself is expired,
// but its dependents are not reached. After `keyed-expiry-cascade-fix` no path registers a zero
// for a keyed asset by accident — every asset that enters the graph carries a concrete version —
// so this branch is reserved for a *declared* policy that an asset opts out of version-based
// invalidation. It is not dead code; it is the mechanism that policy will use.
//
// The condition that used to guard this check (`include_root || current != *key`) was removed:
// it was true on both call paths, and the "(except for the root key)" exemption its comment
// claimed was never implemented. An asset that has opted out stays opted out even when it is the
// root of an explicit expiry.
let mut skip_cascade = false;
if let Some(entry) = self.versions.get_async(&current).await {
    let ver = *entry.get();
    drop(entry);
    if ver.is_unknown() { skip_cascade = true; }
}
```

**Add** U5. **Re-comment** `expire_skips_version_zero_cascade` (U6) — assertions unchanged.

**Validate:** `cargo test -p liquers-core --lib dependencies::` · `--test dependency_manager_integration --test dependency_scheduling`

**Agent:** haiku · rust-best-practices · knowledge: `expire_internal`, Phase 2 gate decision 3, Phase 3 U5/U6.

---

### Step 5 — `AssetRef::version_for_tracking`, and `track_asset` uses it

**Files:** `liquers-core/src/assets.rs` (new method), `dependencies.rs:302`

```rust
// assets.rs
pub(crate) async fn version_for_tracking(&self) -> Version { … }
```

Returns the metadata version when there is one. Otherwise, for a **keyed non-volatile** asset,
assigns `Version::new_unique()`, writes it into the asset's metadata so asset and manager cannot
disagree, adds `LogEntry::warning` naming this as the last-resort net, and returns it. For anything
else, returns `Version::unknown()`.

**It does not re-persist.** An asset reaching this net left no durable trace, so its dependents
*should* expire on restart (Phase 1, "Non-durable means expired on restart").

```rust
// dependencies.rs, in track_asset. The snapshot at :295 still supplies `deps`; only the version
// binding changes, and the call goes AFTER the existing `drop(lock)` at :297.
let (deps, _) = match &metadata { … };            // unchanged, minus the version
if let Some(key) = key_opt {
    let version = asset.version_for_tracking().await;   // may take a WRITE lock — hence after drop
    …
}
```

**Two placement rules, both verified against the source rather than assumed:**

- **After `drop(lock)` at `:297`.** `track_asset` holds a read lock on the same asset from `:294`
  and drops it at `:297`; `version_for_tracking` takes a *write* lock, so calling it while that
  read guard lived would deadlock on `tokio::sync::RwLock`. The existing drop is what makes this
  safe, so a future edit that moves the drop later breaks this step.
- **Inside the `if let Some(key)` branch.** Only a keyed asset needs a version, and calling it
  unconditionally would take a write lock on every query asset evaluated — the commonest path in
  the system — to compute a value that branch discards.

The version is taken from the **return value**, not re-read from the `metadata` snapshot cloned at
`:295`: that snapshot predates any write `version_for_tracking` makes. `deps` still comes from the
snapshot, which is correct — dependency records are not touched.

**Validate:** `cargo test -p liquers-core --lib --tests`

**Agent:** sonnet · rust-best-practices · knowledge: `track_asset`, `AssetData` fields, Phase 2 signatures. The lock discipline (write lock in `assets.rs`, called from `dependencies.rs`) is the part to get right.

---

### Step 6 — `ValueOrigin` replaces `delegated: bool`

**File:** `liquers-core/src/assets.rs:1402`, `:2410`, `:2515`, `:2550`

```rust
#[derive(Debug, Clone, Copy)]
enum ValueOrigin {
    Computed,
    Delegated { version: Option<Version> },
}
```

- `RecipeEvaluation.delegated: bool` → `origin: ValueOrigin`.
- Delegation branch (`:2410`) returns `ValueOrigin::Delegated { version: state.metadata.version() }`
  — the delegate's `State` is already in hand.
- Persistence guard (`:2550`) becomes `if is_keyed && matches!(origin, ValueOrigin::Computed)`,
  with a comment giving the *second* reason: the hand-off carries no dependency records, so a
  persisted delegating asset would store a real version with an empty dependency list, which
  `try_fast_track` later reads as "nothing to check".

**Validate:** `cargo check -p liquers-core` · `cargo test -p liquers-core --test manager_parametric`

**Agent:** haiku · rust-best-practices · knowledge: `RecipeEvaluation`, `evaluate_recipe_outcome`'s delegation branch, Phase 1 "Delegation carries the version".

---

### Step 7 — `AssetRef::assign_version`, and the call in `evaluate`

**File:** `liquers-core/src/assets.rs`

```rust
async fn assign_version(&self, origin: ValueOrigin) { … }
```

Infallible. No-op unless keyed and non-volatile. For `Delegated { version }`, installs that version
verbatim. For `Computed`, calls `serialize_to_binary()` — which caches the bytes in `lock.binary` —
and sets `Version::from_bytes(&bytes)`; on `Err`, sets `Version::new_unique()` and logs a warning
naming the serialization error. Follows `serialize_to_binary`'s existing shape: the encode happens
outside the write lock.

In `evaluate`:

```rust
lock.data = Some(value);
lock.binary = None;   // EVALUATE-DOES-NOT-CLEAR-CACHED-BINARY: every other value-installing path
                      // clears it; `assign_version` is about to write it, so this must be exact.
drop(lock);

self.try_to_set_ready().await;

// ORDERING IS LOAD-BEARING — do not move this below the notification or into persistence.
// `record_dependency_on_asset` reads this version out of the child's *live metadata*, and a
// parent can read the child as soon as `ValueProduced` is sent. Assigning later means parents
// record `Version::unknown()`; assigning earlier means assigning provisionally, and a version is
// published once and never revised. No test can hold this constraint — violating it makes the
// cascade tests flaky rather than red — so this comment is the guard.
// See specs/design/keyed-expiry-cascade-fix/ Phase 1, "Owner decisions".
self.assign_version(origin).await;
```

**That comment is a completion criterion for this step, not a nicety** (Phase 3, P3).

**Validate:** `cargo test -p liquers-core --lib --tests`

**Agent:** sonnet · rust-best-practices · knowledge: `evaluate`, `try_to_set_ready`, `serialize_to_binary`, `save_to_store`, Phase 1 owner decisions, Phase 3 pitfalls P1–P5.

---

### Step 8 — Integration tests: `keyed_version_cascade.rs`

**File:** new, `liquers-core/tests/keyed_version_cascade.rs`

I1–I4, I6–I9 from Phase 3, over the three-link fixture. Await `get()` before reading `status()`
(P11). The `SharedMemoryStore` wrapper for I8/I9 forwards **every method the test path touches** —
determined by compiling and running, not by counting the trait's two required methods, whose
defaults are errors rather than forwards.

**Validate:** `cargo test -p liquers-core --test keyed_version_cascade`

**Agent:** sonnet · liquers-unittest + rust-best-practices · knowledge: Phase 3 in full, `expiration_integration.rs` as the style model, the probe output in Phase 3.

---

### Step 9 — Delegation assertions (I5)

**File:** `liquers-core/tests/manager_parametric.rs:170` (`scenario_keyed_delegation`)

Two assertions added to the existing scenario: the delegating asset's version equals the owner's,
and it did not persist. Runs under both manager parameterisations for free.

**Validate:** `cargo test -p liquers-core --test manager_parametric`

**Agent:** haiku · liquers-unittest · knowledge: that scenario, Phase 3 I5.

---

### Step 10 — wasm verification

```bash
rustup target add wasm32-unknown-unknown
bash scripts/check-build-matrix.sh
```

The Phase 2 gate recorded the wasm reasoning as **unverified**. This step is where it stops being
reasoning. If `chrono`'s wasm path does not resolve as expected, Step 1 is where the fix goes and
the gate decision needs revisiting — say so rather than working around it.

**Agent:** haiku · knowledge: `CLAUDE.md` "Feature matrix", `scripts/check-build-matrix.sh`.

---

### Step 11 — Full verification

Below. Includes `cargo clean` first if `target/` has seen several profiles.

---

## Testing Plan

| When | Command | Expectation |
|---|---|---|
| After every step | `cargo test -p liquers-core --lib` | 793 + new, 0 failures |
| After steps 3–5 | `cargo test -p liquers-core --lib dependencies::` | U1–U6, U9 pass; **B is a behaviour no-op**, so `expiration_integration` must be untouched at 34/34 |
| After step 7 | `cargo test -p liquers-core --test keyed_version_cascade` | I2's `c` assertion flips from fail to pass — the moment the defect is fixed |
| After step 9 | `cargo test -p liquers-core --test manager_parametric --test dependency_scheduling --test dependency_manager_integration` | green |
| After step 10 | `bash scripts/check-build-matrix.sh` | every row, including wasm32 |
| Final | `cargo test -p liquers-core --test expiration_integration` | **34/34, unchanged** — R1. Stated in the final row as well as the mid-plan one: it is the regression guard for the whole change, not only for group B |
| Final | `cargo test -p liquers-lib --lib --tests` | the default loop; `liquers-lib` builds on core |
| Final | `cargo test -p liquers-core --test registry_export`-equivalent in `liquers-lib` | unchanged — no command signature changed |

**The single most informative check:** run `keyed_version_cascade.rs` *before* Group C. I2's `b`
assertion must already pass and its `c` assertion must fail. If `b` fails, the fixture is wrong; if
`c` passes, the test is not testing what it claims.

**Baseline** (2026-09-05, `CARGO_INCREMENTAL=0`): 793 lib · 34 expiration_integration · 5
dependency_manager_integration · 4 dependency_scheduling · 0 failures.

---

## Agent Assignment

| Step | Model | Skills | Why |
|---|---|---|---|
| 1 | haiku | rust-best-practices | Mechanical, well-specified. |
| 2 | haiku | rust-best-practices | One line plus a comment. |
| 3 | **sonnet** | rust-best-practices | The subtle branch. A reviewer already caught a missing test here; the command/asset split is the one thing that must not be simplified away. |
| 4 | haiku | rust-best-practices | Deletion plus a comment, with the test already written. |
| 5 | **sonnet** | rust-best-practices | Cross-module lock discipline: the write lives in `assets.rs`, the caller in `dependencies.rs`. |
| 6 | haiku | rust-best-practices | Mechanical enum substitution; the compiler finds every site. |
| 7 | **sonnet** | rust-best-practices | The heart of the change, and the ordering comment is a deliverable. |
| 8 | **sonnet** | liquers-unittest, rust-best-practices | Most of the new code; the shared-store fixture is discovery work. |
| 9 | haiku | liquers-unittest | Two assertions in an existing scenario. |
| 10 | haiku | — | Run and report. Do not work around a wasm failure. |
| 11 | **opus** | all | Final review of the whole diff against all four phase documents. |

Every agent gets: `CLAUDE.md`, `specs/reference/DEPENDENCIES_STATUS.md`, and this design folder's
Phases 1–3. The Phase 1 owner decisions are the part most likely to be re-litigated by an agent
that only reads code.

---

## Rollback Plan

Each group is a separate commit, so rollback is per-group and the boundaries are chosen so no
partial state is incoherent:

| Group | Revert cost | Leaves behind |
|---|---|---|
| A (1–2) | `git revert` — self-contained | Nothing. Two independent bug fixes. |
| B (3–5) | `git revert` | Nothing. B is a behaviour no-op without C. |
| C (6–8) | `git revert` | Nothing — but **reverting C without B is fine, and reverting B without C is not.** B is what makes C safe. If both must go, revert C first. |
| D (9–11) | tests only | — |

**The one irreversible-shaped thing is not code.** Once C ships, keyed chains start recomputing
where they previously served stale values. That is the fix working, and it will look like a
performance regression to anyone who measures cache hit rate without knowing why. Phase 5 says so
in the reference. There is no data migration and no stored-format change to undo: `version` is
`Option` and was already persisted.

---

## Phase 5 Entry Criteria

1. Steps 1–11 complete; full verification green including the wasm matrix.
2. I2's `c` assertion passes, and was demonstrated failing before Group C.
3. The ordering comment at the `assign_version` call site exists (Step 7's completion criterion).
4. All review comments answered.
5. Then Phase 5: the summary; `DEPENDENCIES_STATUS.md` extended with the version contract, the
   provisional rule and its command-key exception, plus a `## History` row and a `reviewed:` bump;
   `ASSETS.md` and `ASSET_LIFECYCLE.md` reviewed and updated or recorded as no-ops;
   `KEYED-EXPIRY-DOES-NOT-CASCADE-TO-KEYED-DEPENDENTS`,
   `SERIALIZE-TO-BINARY-CONSULTS-THE-READ-GATE` and `EVALUATE-DOES-NOT-CLEAR-CACHED-BINARY` closed
   with evidence; `stale-dependency-status-finalization` told whether its blocker is discharged and
   whether C2 is revisitable; `specs/README.md` stage line advanced.

## Open Risks

1. **The wasm claim is unverified until Step 10.** Ranked first because it is the only item that
   could send a decision back to the Phase 2 gate.
2. **The shared-store fixture is discovery work.** The method set is found by compiling. If it
   proves larger than a test file should carry, I8/I9 move behind
   `DEPENDENCY-VERSIONS-NOT-LOADED-OR-VERIFIED-FROM-STORE` and this design ships without them —
   they pin the approximation, not the fix.
3. **Cost, not correctness, is the live-fire unknown.** Nothing measures how much recomputation the
   cascade adds on a real workload, because nothing in the repository measures that today
   (`BENCHMARK-SUITE` is open). Phase 5 states the exposure rather than pretending it was measured.
