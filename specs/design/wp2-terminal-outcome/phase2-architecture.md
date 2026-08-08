# Phase 2: Solution & Architecture - Asset Terminal Outcome Contract (WP-2)

## Overview

The change is a **contract tightening**, not a new subsystem: make `State` the single, faithful
carrier of an asset's terminal outcome (value **XOR** typed error+log), reserve `get()`'s `Err`
for *delivery* failures only, and give value extraction on `State` a fail-safe error guard.
Almost all code already exists — `Metadata` stores the typed error in `error_data`, `State`
already has `from_error`/`error_result`/`is_error`, and `poll_state()`'s `Error | Cancelled`
arm already returns a none-value + error-metadata `State`. The work is (a) unify the three
divergent failure-recording paths into one metadata-preserving `fail_asset`, (b) rewrite `get()`
to consult `poll_state()` (status) instead of notification content, (c) add
`State::value_state()` + error-checked extractors + private `State.data`, (d) a typed
`ErrorType::Cancelled`, (e) a post-finish message policy, and (f) migrate value-consuming
callers to check the `State`.

> **Note on process:** the designer workflow calls for `rust-best-practices` auto-invoke and a
> 2-haiku/1-sonnet review fan-out. `rust-best-practices` is **not installed** in this repo, and
> per environment guidance I am **not** spawning cold review sub-agents (they would re-derive
> context already built over four clarification rounds). The inline critical review below covers
> both reviewer concerns (Phase 1 conformity + codebase alignment). Say the word if you want the
> full multi-agent fan-out instead.

## Data Structures

### New Enums

#### `ErrorType::Cancelled` (new variant on existing enum) — `liquers-core/src/error.rs`

```rust
pub enum ErrorType {
    // ... existing variants ...
    DependencyCycle,
    Cancelled,          // NEW: the error TYPE returned when a *value* is requested from a
                        // cancelled state. NOT stored as the asset's computed error.
}
```

**Conceptual model (per clarification — this is the important correction).**
`Status::Cancelled` and `Status::Error` are *separate, legitimate terminal statuses*. Being in
either status is **not itself an error** — `poll_state()`/`get()` return such a state as `Ok`.
**Only asking such a state for a value is an error.** The two statuses differ in where the error
comes from at value-extraction time:

- `Status::Error` → the asset stored a *computed* `Error` in `Metadata.error_data`; value
  extraction returns that stored error.
- `Status::Cancelled` → **no error is stored** (`is_error` stays `false`, `error_data` stays
  `None`); value extraction *synthesizes* `Error::cancelled(...)` (type `ErrorType::Cancelled`).

Consequences: **cancellation does not go through `fail_asset`** and does **not** set
`is_error`/`error_data` — it only sets `Status::Cancelled` (as the current `Cancel` arm already
does, `:1222`). `ErrorType::Cancelled` is reserved strictly for the value-extraction error.
**No default match arm** exists on `ErrorType` today; every `match` over it must add an explicit
`Cancelled` arm (compiler-driven audit).

**No new structs, no new `ExtValue` variants, no `AssetOutcome` enum.** `AssetOutcome`,
`poll_outcome()`, and a separate `AssetData.error` field (all proposed by WP-2) are **dropped**
as redundant with `State` + `Metadata.error_data`.

## Trait Implementations

None. No trait added or modified. (`ValueInterface` untouched; the value-extraction guards live
on `State`, which is a concrete generic struct, not a trait.)

## Function Signatures

### `State<V>` — `liquers-core/src/state.rs`

```rust
// Field change: data becomes private so the error guard cannot be bypassed.
pub struct State<V: ValueInterface> {
    data: Arc<V>,             // was: pub data  (removes the existing `// TODO: remove pub`)
    pub metadata: Arc<Metadata>,
}

impl<V: ValueInterface> State<V> {
    /// The single "can I take a value from this state?" gate. Returns:
    ///   - None            if the state carries an extractable value (status().has_data()),
    ///   - Some(stored)    for Status::Error (the computed Metadata.error_data),
    ///   - Some(cancelled) for Status::Cancelled (synthesized Error::cancelled, ErrorType::Cancelled),
    ///   - Some(no-value)  for any other non-data status (defensive; get() only yields terminals).
    /// NOTE: this is NOT error_result() — error_result() is Error-only; a Cancelled state's
    /// error_result() is Ok, which is exactly why value extraction must consult status.
    pub fn value_error(&self) -> Option<Error>;

    /// Validating projection. Ok(self) if this is a value-state; Err otherwise (see value_error).
    /// The ergonomic path is `asset.get().await?.value_state()?`.
    pub fn value_state(self) -> Result<Self, Error>;      // { if let Some(e)=self.value_error(){Err(e)}else{Ok(self)} }

    /// Error-checked value accessors (the safety net). Each returns value_error() BEFORE touching
    /// the (none) value, so a forgotten `value_state()` fails at value access — for Cancelled too.
    pub fn value(&self) -> Result<Arc<V>, Error>;         // checked replacement for `.data`
    pub fn try_into_string(&self) -> Result<String, Error>;   // now: value_error()? then extract
    pub fn as_bytes(&self) -> Result<Vec<u8>, Error>;         // now: value_error()? then extract

    /// Raw, UNCHECKED access for the few callers that legitimately forward/inspect an error- or
    /// cancelled-state (delegation copy, UI rendering). Explicit & greppable bypass.
    pub fn data_unchecked(&self) -> &Arc<V>;

    // Unchanged / already present: status(), error_result() (Error-only), is_error(),
    // from_error(), with_data(), with_metadata(), get_asset_info(), extension(), ...
}
```

**Why `value_error()` and not `error_result()`:** a `Status::Cancelled` state has `is_error =
false`, so `error_result()` returns `Ok` — a caller relying on it would wrongly treat a cancelled
state as value-bearing. `value_error()` keys on `status().has_data()` so both `Error` and
`Cancelled` (and any future non-data terminal) are gated, each mapped to the right typed error.

**Ownership rationale:** `value_state(self)` consumes and returns `Self` (no clone) — it is a
type-level tag that the state was checked. `value()` returns `Arc<V>` (cheap clone of the Arc)
so callers get owned shared access without exposing the field. `data_unchecked()` returns a
borrow.

**`with_data` invariant note (WP-4 overlap):** `with_data`/`with_metadata` keep syncing type
identifiers; unchanged here. Setting a value does not clear an error flag — but by construction
a state is built either from a value or from an error, never mutated across the boundary.

### `Error` — `liquers-core/src/error.rs`

```rust
impl Error {
    /// Typed cancellation constructor.
    pub fn cancelled(message: impl Into<String>) -> Self;   // ErrorType::Cancelled
    /// Convenience predicate used by the post-finish policy and callers.
    pub fn is_cancelled(&self) -> bool;                     // error_type == Cancelled
}
```

### `AssetRef<E>` / `AssetData<E>` — `liquers-core/src/assets.rs`

```rust
// (1) UNIFIED FAILURE ROUTINE — replaces the 3 divergent error-recording sites.
impl<E: Environment> AssetRef<E> {
    /// Put the asset into a terminal ERROR state (Status::Error), preserving the metadata audit
    /// trail: data=None, binary=None, metadata.with_error(e) (NOT Metadata::from_error), notify
    /// once. Idempotent if already terminal-failed. Used ONLY for computed errors — cancellation
    /// does NOT use this (it sets Status::Cancelled without storing an error; see cancel path).
    pub(crate) async fn fail_asset(&self, e: Error) -> Result<(), Error>;
}

// (2) get() — consults status via poll_state(), not notification CONTENT.
impl<E: Environment> AssetRef<E> {
    /// Ok(state) for ANY obtained terminal outcome (value OR error-state).
    /// Err ONLY for delivery failure (closed channel, finished-but-no-state anomaly,
    /// expired-while-waiting until WP-3). `watch` is a pure wake-up signal.
    pub async fn get(&self) -> Result<State<E::Value>, Error>;
}

// (3) poll_state() — UNCHANGED behavior for Error|Cancelled (already returns the error-state);
//     stays Option<State>, None iff not finished. No poll_outcome() is added.
pub fn poll_state(&self) -> Option<State<E::Value>>;   // on AssetData (sync, :596)
pub async fn poll_state(&self) -> Option<State<E::Value>>;  // on AssetRef (:2084)
```

**`get()` new loop (shape only):**
```
if let Some(s) = poll_state().await { return Ok(s); }   // includes terminal error-states
subscribe; loop {
    if let Some(s) = poll_state().await { return Ok(s); }
    match rx.changed().await {
        Ok(())  => continue,                             // any notification = re-poll
        Err(_)  => return Err(Error::unexpected_error("asset channel closed before terminal state")),
    }
}
```
The `AssetNotificationMessage::ErrorOccurred(e) => return Err(e)` arm (`:2020`) and the
`JobFinished => … "Asset finished but no data available"` `Err` (`:2034`) are **removed**: after
`fail_asset`, `poll_state()` returns the error-state, so `get()` returns `Ok(error_state)`. A
genuine finished-but-no-state remains a delivery `Err` but is now unreachable on the normal
failure path.

## Audit A — `Err`-vs-error-`State` classification (mandatory Phase-1 deliverable)

Every `Err`/error-recording site in the get/evaluate/finish/fast-track paths, classified as
**Computed** (asset's own failure → `fail_asset`, surfaced as `Ok(error_state)`) or **Delivery**
(framework could not produce/obtain a state → stays `Err`).

| Site (`assets.rs`) | Today | Class | Action |
|---|---|---|---|
| `finish_run_with_result` result=Err (`:1354–1359`) | `Metadata::from_error` (destroys log/query) | **Computed** | Route through `fail_asset` → `with_error` (preserve) |
| `AssetRef::set_error` (`:2206–2209`) | `Metadata::from_error` + sends `ErrorOccurred` | **Computed** | Reimplement as `fail_asset`; keep single notify |
| `process_service_messages` `ErrorOccurred` arm (`:1277–1288`) | `metadata.with_error` (already preserves) ✓ | **Computed** | Fold into `fail_asset` for one code path |
| `process_service_messages` `Cancel` arm (`:1221–1232`) | sets `Status::Cancelled`, no error stored ✓ | **Cancellation (not error)** | Keep as-is: set `Status::Cancelled`, do **not** store an error / set `is_error`. Value extraction later synthesizes `Error::cancelled`. Ensure `is_error`/`error_data` remain unset. |
| `get()` `ErrorOccurred => Err(e)` (`:2020`) | `Err` | **Computed** | Delete; `poll_state` returns error-state |
| `get()` `JobFinished`→"no data" `Err` (`:2034`) | `Err(unexpected)` | **Delivery** (anomaly) | Keep as delivery `Err`; now off the normal failure path |
| `get()` `Expired => Err` (`:2039`) | `Err` | **Delivery** (WP-3 will re-evaluate) | Leave for WP-3; keep `Err` for now, documented |
| `try_fast_track` `store.contains/get().await?` (`:468/:471`) | `Err` propagates | **Delivery** | Stays `Err` (store I/O) |
| `try_fast_track` deserialize failure (`:491–502`) | `Ok(false)` → re-evaluate | **Neither** (cache miss) | Unchanged |
| `process_service_messages` `LogMessage`/`save_metadata` `?` (`:1217–1218`) | `Err` propagates to `finish_run_with_result` | **Delivery** | Unchanged (join handled at `:1336`) |

## Audit B — `get()` caller migration (mandatory Phase-1 deliverable)

`get()` now returns `Ok(error_state)` on computed failure, so value-wanting callers must project.
The `State` value-extraction guard is the backstop; these are the explicit fixes.

| Caller | File:line | Needs value? | Migration |
|---|---|---|---|
| `get_binary` | `assets.rs:2057` | Yes | `self.get().await?.value_state()?;` before polling binary |
| `wait_for_dependency` | `assets.rs:2293` | Depends | **WP-1 overlap.** Parent must treat a computed-failed dependency as failure: `dependency.get().await?.value_state()?` (or forward the error-state deliberately — see §"forwarding" below). Fix here if WP-1 has not. |
| recipe-delegation copy path | `assets.rs` (~`:1310`, WP-1) | Forwards | Allowed to copy an error-`State` (still carries error); must not extract a value unchecked |
| axum data handlers ×2 | `liquers-axum/src/assets/handlers.rs:52,219` | Yes | `match asset_ref.get().await { Ok(s) => match s.value_state() { Ok(v)=>…, Err(e)=>error_detail }, Err(e)=>error_detail }` — a computed error must not 200 with a none-value |
| `interpreter.rs` | `:660,711,724,758,774` | Yes | append `.value_state()?` (already `?`-chained with `try_into_string`, which becomes error-checked — so many are auto-covered by the guard) |
| `liquers-py` `State.data`/value | `liquers-py/src/state.rs:29,37,42,48`; `commands.rs:134,141` | Yes | replace `state.data` with `state.value()?` / `data_unchecked()`; decide py policy: **raise** on error-state (recommended) vs. return None → **Open Question py** |
| UI `AssetViewElement::from_asset_ref` | `liquers-lib/src/ui/element.rs:302,327,329` | Renders | switch error source from transient `ErrorOccurred` notification to `state.error_result()`; use `data_unchecked()` for display |

**Forwarding rule:** "check, don't silently treat as a value" ≠ "always convert to `Err`". A site
that copies/forwards an error-`State` (delegation) is correct because the `State` still carries
the error; only *value extraction* must be guarded.

## Re-evaluation policy — `Error` / `Cancelled` / `Expired` are a cache miss at the *manager request boundary*

**Best-practice assumption (documented, not enforced):** an asset's value should not change from
one evaluation to the next *unless it expired*; evaluation is expected to be effectively
deterministic. Under that assumption a logic `Error` tends to recur and a `Cancelled` asset tends
to succeed on retry — **but neither is guaranteed** (errors can come from hardware or volatile
logic). Therefore a *usable value* cannot be assumed for a stored `Error` or `Cancelled` state,
so **both, like `Expired`, must be re-evaluated when the asset is requested from the manager.**

**Placement (this is what reconciles it with the get() contract).** Re-evaluation is triggered at
the **manager request boundary** — `DefaultAssetManager::get(key)` and the query/dependency
request paths — **not** inside `AssetRef::get()`'s wait loop. The manager already does exactly
this for `Expired` (`assets.rs:3260`: remove the stale asset from the map, `continue`, rebuild
fresh). The change: extend that stale-terminal branch from `{Expired}` to
`{Expired, Error, Cancelled}`.

```
// DefaultAssetManager::get(key), :3255 — conceptual
let status = asset_ref.status().await;
if matches!(status, Status::Expired | Status::Error | Status::Cancelled) {
    // stale terminal → drop from map and rebuild a fresh asset from its recipe
    remove_from_map_if_same_id(...); continue;
}
if status.is_finished() { return Ok(asset_ref); }   // Ready/Source/Override/... served as-is
```

- **Boundary vs. await.** `AssetRef::get()` still returns the *current* evaluation's terminal
  outcome as `Ok(state)` (incl. an error- or cancelled-state). It does **not** re-evaluate. So
  `get()` called N times on one completed evaluation yields the same error N times (the WP-2
  contract); re-evaluation only happens when the asset is *re-requested from the manager*.
- **No re-eval storm / no loop.** Removing the stale terminal and looping creates a *fresh*
  (non-finished) asset that is submitted, not re-matched by the branch. Deterministic errors will
  re-run on each fresh top-level request (accepted cost); within one request there is no loop.
- **Requires a recipe.** Re-evaluation needs a recipe; a source/stored asset with no recipe that
  is `Error`/`Cancelled` cannot be rebuilt — its terminal state stands. (Edge; Phase 3 corner case.)

### Dependency composition (WP-1 overlap — contract this WP defines, WP-1 consumes)

Dependencies are requested through the manager, so the same rule applies, plus an in-flight rule:

| Dependency situation | Behavior |
|---|---|
| Requested and currently `Error`/`Cancelled`/`Expired` (stale terminal from a prior lifecycle) | **Re-evaluate** (cache miss, per above) — do **not** propagate the stale error/cancellation |
| Fresh evaluation of the dependency reaches `Error` | Parent fails: propagate as a **dependency error** (`fail_asset` with dependency context) |
| Fresh evaluation of the dependency reaches `Cancelled` | **Cascade-cancel** the parent (`Status::Cancelled`) |
| Dependency `Cancelled` **mid-flight** while the parent waits in `Status::Dependencies` | **Cascade-cancel** the parent |
| Dependency `Ready` | Use its value (parent continues) |

This makes cancellation and error genuinely different at the dependency level: a *stale* error or
cancellation is a cache miss (re-evaluate), an *error outcome* propagates as failure, and a
*cancellation outcome* (fresh or mid-flight) cascades cancellation. `wait_for_dependency`
(`:2287`) and the WP-1 dependency-readiness checks must implement this; if WP-1 has not landed it,
it is fixed here.

## Post-finish message policy (resolves ASSET-MESSAGE-LIFECYCLE-ROBUSTNESS)

`process_service_messages` already drops most late messages when `is_finished()` (`:1193–1211`),
but **not `LogMessage`**, and it keys off `is_finished()` rather than an explicit phase. Change:
introduce a `finishing` phase entered on `JobFinishing` and treat the policy as a matrix.

| Message kind | Before finish | After finish/finishing |
|---|---|---|
| `LogMessage` | append to metadata log + notify | **debug-log & drop** (was: still mutated metadata) |
| `UpdatePrimaryProgress` / `UpdateSecondaryProgress` | update + notify | debug-log & drop (already dropped) |
| `JobSubmitted` / `JobStarted` | status transition | debug-log & drop (already dropped) |
| `Cancel` | set `Status::Cancelled` (no stored error, `is_error` stays false) | **no-op** + debug-log (already dropped) |
| `ErrorOccurred(e)` | `fail_asset(e)` (Status::Error, stores error) | debug-log & drop (already dropped) |
| `JobFinishing` | enter finishing phase, return | idempotent |
| `JobFinished` | return | idempotent |

`JobFinishing` arm (`:1272–1276`) keeps **not** sending a premature `JobFinished` notification
(the commented-out send stays deleted — resolves the "meaningless send" FIXME). Late drops use
`tracing::debug!` (WP-6 introduces `tracing`; until then, drop silently or via a local
`debug`-guarded `eprintln!` — **Open Question logging**).

## Sync vs Async Decisions

| Function | Async? | Rationale |
|---|---|---|
| `State::value_state` / `value` / `try_into_string` / `as_bytes` | Sync | Pure, in-memory; also used from py sync bindings |
| `Error::cancelled` / `is_cancelled` | Sync | Pure |
| `AssetRef::fail_asset` | Async | Takes the `data` write lock, notifies, persists metadata |
| `AssetRef::get` | Async | Waits on `watch` + polls |
| `AssetData::poll_state` | Sync | Pure read of already-locked data (unchanged) |

## Error Handling

- Use typed constructors only: `Error::cancelled(...)`, `Error::general_error(...)`,
  `Error::from_error(ErrorType::…, e)`. No `Error::new`, no new error *types* beyond the one
  `ErrorType::Cancelled` variant (CLAUDE.md permits variants on the existing enum; it forbids new
  error *structs/types* outside `liquers_core::error`).
- `fail_asset` is the one place that maps a computed `Error` into asset state; it must be
  idempotent and must prefer an already-recorded computed error over a later delivery hiccup
  (e.g. a metadata-persist failure while recording the error is logged, not overwriting the
  error).
- Every `match` on `ErrorType` and on `Status`/message enums stays default-arm-free (CLAUDE.md);
  adding `ErrorType::Cancelled` will surface all such matches at compile time.

## Serialization Strategy

No new serialized types. `Metadata.error_data: Option<Error>` and `is_error: bool` are already
serde fields, so a persisted error-`State` round-trips with its typed error. `ErrorType`
(de)serializes by variant name — adding `Cancelled` is backward-compatible for writing; reading
an unknown future variant is out of scope. `Status::Cancelled` stores no error (`is_error` false,
`error_data` none), so a persisted cancelled state carries no error payload; the cancellation
error is synthesized at value-extraction time. Fast-track loads only `Ready/Source/Override`
(`:473`) **and** the manager treats a stored/in-memory `Error`/`Cancelled`/`Expired` as a cache
miss (re-evaluate) — this is now **decided policy** (see Re-evaluation policy), superseding
Phase-1 Open Question 4.

## Concurrency Considerations

- `fail_asset` takes the single `data` write lock, mirroring existing mutation sites; no new
  lock or shared state. It must not hold the lock across `.await` on external I/O beyond the
  existing `save_metadata_to_store()` pattern.
- `get()` holds no lock while awaiting `rx.changed()`; it re-polls under the read lock each wake.
  The value-XOR-error invariant makes concurrent getters observe the same terminal `State`
  (the WP-2 "8 concurrent getters" guarantee) because the decision is read from status/metadata,
  not from a lossy `watch` payload.

## Integration Points

| Crate | File | Change |
|---|---|---|
| liquers-core | `src/error.rs` | Add `ErrorType::Cancelled`, `Error::cancelled`, `Error::is_cancelled` |
| liquers-core | `src/state.rs` | Private `data`; add `value_state`, `value`, `data_unchecked`; error-check `try_into_string`/`as_bytes` |
| liquers-core | `src/assets.rs` | Add `fail_asset`; rewrite `get()` loop; reroute `finish_run_with_result`/`set_error`/psm error arms (`Cancel` arm stays status-only); post-finish message phase; **extend manager `get(key)` stale-terminal branch (`:3260`) from `{Expired}` to `{Expired, Error, Cancelled}`** (re-eval policy) |
| liquers-core | `src/interpreter.rs` | Add `.value_state()?` where a value is required |
| liquers-lib | `src/ui/element.rs` | Error source → `state.error_result()`; `data_unchecked()` for display |
| liquers-axum | `src/assets/handlers.rs` | Map computed error-`State` to HTTP error via `value_state()`; add `Cancelled` status mapping |
| liquers-py | `src/state.rs`, `src/commands.rs` | Replace `state.data` with `value()?`/`data_unchecked()`; decide raise-vs-None policy |
| specs | `ASSETS.md`, `ISSUES.md` | Add "Terminal outcome contract" section; update ASSET-MESSAGE-LIFECYCLE-ROBUSTNESS with the policy matrix |

Dependency flow preserved: all semantic changes originate in `liquers-core`; `lib`/`axum`/`py`
only adapt to the new signatures. No new external dependencies.

## Relevant Commands

### New Commands
**None.** WP-2 is an internal lifecycle/contract change; it introduces no query-language commands.

### Relevant Existing Namespaces
**None functionally affected.** Commands produce values/errors via `Context` and `State` as
before; the only command-facing change is that a command returning `Err` still results in a
terminal error-`State` (unchanged from the command author's view). No namespace needs review.

*(Phase 2 normally asks the user to confirm command namespaces; for WP-2 there are none — the
equivalent decision is the caller-migration/py-policy questions below.)*

## Web Endpoints

No new routes. Behavior change only: the two asset data handlers
(`liquers-axum/src/assets/handlers.rs:52,219`) must map a computed error-`State` to an HTTP error
(via `value_state()`), not a 200 with a none-value. HTTP status mapping for `ErrorType::Cancelled`
should be chosen (e.g. 499/503) when the axum error match is updated — **Open Question axum-status**.

## Compilation Validation

- Adding `ErrorType::Cancelled` → compile errors at every non-exhaustive `match ErrorType`
  (intended; drives the audit). Locate & fix (axum error mapping, any py conversion).
- Privatizing `State.data` → compile errors at `liquers-py` (`state.rs:29,37,42,48`,
  `commands.rs:134,141`) and `liquers-lib` UI (`element.rs:329`); fixed via `value()?` /
  `data_unchecked()`. `cargo check -p liquers-py` is the gate (CLAUDE.md).
- Signatures above are concrete and compilable modulo bodies (Phase 4).

## References to liquers-patterns.md

- [x] Crate dependency flow respected (core changes; lib/axum/py adapt downstream).
- [x] No new `ExtValue` variants; no new error struct/type (one enum variant only).
- [x] Async default; sync only for pure `State`/`Error` helpers used by py.
- [x] Typed error constructors; no `Error::new`.
- [x] No default match arms (the new `ErrorType::Cancelled` enforces this at compile time).
- [x] Commands unchanged (register_command! not involved).

## Inline Critical Review (stands in for the 2-haiku/1-sonnet fan-out)

**Phase 1 conformity:** ✔ Scope unchanged — single `get()`, `value_state()`, value guard +
private `data`, typed cancellation, `fail_asset`, post-finish policy, both audits. No `AssetOutcome`.
No scope creep (WP-1 delegation and WP-3 expiration are referenced as overlaps, not absorbed).

**Codebase alignment:** ✔ Signatures matched to real code (`poll_state:596/2084`,
`finish_run_with_result:1359`, `set_error:2206`, `process_service_messages:1180`,
`try_fast_track:457`). Reuse maximized: `Metadata.error_data`/`error_result` and the existing
`poll_state` error arm are kept, not reinvented. Risk noted: privatizing `State.data` has the
widest blast radius (py + UI) — mitigated by `data_unchecked()` for legitimate raw access.

**Corrections folded in from clarification (no longer open):** `Cancelled`/`Error` are legitimate
terminal *statuses*, not errors in themselves — only value extraction errors; `ErrorType::Cancelled`
is reserved for the value-extraction error and is **not** stored; cancellation does not use
`fail_asset`. `Error`/`Cancelled`/`Expired` are re-evaluated at the manager request boundary
(supersedes old Open Question 4). Dependency error propagates, dependency cancellation cascades,
stale dependency re-evaluates.

**Open questions for you (genuine decisions, not resolvable from code):**
1. **py error-state policy:** should `liquers-py` `State.value()`/`__value__` **raise** on an
   error- or cancelled-state (recommended, matches the "always check" principle) or return `None`?
2. **axum status codes:** HTTP code for a computed `Error` (500?) vs. `ErrorType::Cancelled`
   (499 client-closed vs 503 vs a 200 with error body for render-style endpoints)?
3. **Interim logging:** WP-6 adds `tracing`; until then, log post-finish drops via a
   `debug`-guarded `eprintln!` or drop silently?
4. **`State.data` privatization now vs. deferred:** enforce the guard fully in this WP (recommended;
   it *is* the safety net) or land `value_state()` first and privatize as a fast follow?
5. **Re-eval boundary scope:** re-evaluate `Error`/`Cancelled` on *every* manager request
   (recommended, matches `Expired`), or gate it (e.g. only for volatile/expirable assets) to avoid
   re-running expensive deterministic failures? The determinism best-practice suggests "always",
   but there is a cost tradeoff worth your call.
