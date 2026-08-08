# Phase 3: Examples & Use-cases - Asset Terminal Outcome Contract (WP-2)

## Example Type

**User choice:** Conceptual code — illustrative snippets showing the contract semantics, not
compile-guaranteed. Phase 4 turns the test plan below into real red→green tests following the
`plan20260707.md` WP-2 discipline (unit tests inline; cross-module in
`liquers-core/tests/asset_failure_contract.rs`, using the
`SimpleEnvironment<Value>` + `register_command!` + `AsyncMemoryStore` + `DefaultRecipeProvider`
pattern from `expiration_integration.rs`).

> **Process note:** per environment guidance I drafted inline rather than fanning out the
> 5-haiku / sonnet-synthesizer drafting and 3-haiku review; `rust-best-practices` is not
> installed. The inline critical review at the end covers Phase 1/2 conformity, codebase, and
> query validation. Ask if you want the full agent fan-out.

## Overview Table

| # | Type | Name | Demonstrates / Checks |
|---|------|------|------------------------|
| E1 | Example | Failing command → `Ok(error_state)` | `get()` delivers the terminal state; the error surfaces only at value extraction; log/query preserved |
| E2 | Example | Cancellation | `Cancelled` is a status, not a stored error; `value_state()` → `ErrorType::Cancelled` synthesized |
| E3 | Example | Re-evaluation on re-request | `Error`/`Cancelled`/`Expired` = cache miss at the *manager boundary*; boundary-vs-await distinction |
| E4 | Example | Delivery failure | Store I/O failure → `get()` returns `Err` (no error-state), the second failure axis |
| E5 | Example | Dependency composition | dep error → parent error; dep cancelled (fresh/mid-flight) → parent cancelled; stale dep → re-eval |
| C1–C9 | Corner cases | see §Corner Cases | concurrency, notification overwrite, post-finish messages, metadata preservation, serialization, guard bypass, forwarding, re-eval-loop safety, none-value |
| U1–U8 | Unit tests | `state.rs`, `metadata.rs`, `error.rs`, `assets.rs` | `value_error`/`value_state` mapping, guarded extractors, `ErrorType::Cancelled`, `fail_asset` preserves metadata, `poll_state` arms |
| I1–I9 | Integration tests | `asset_failure_contract.rs` (+ expiration/dependency suites) | end-to-end failure/cancel/re-eval/dependency contract |

---

## Example 1 — Failing command: `get()` returns `Ok(error_state)`

**Scenario:** A command returns `Err`. Under the contract, the asset reaches `Status::Error`;
`get()` succeeds and returns the terminal error-`State`; the error surfaces when a *value* is
requested. The metadata audit trail (log entries, original query) is preserved.

**Context:** The most common failure path — any command that fails during evaluation.

```rust
// Command that logs a step then fails.
fn boom(state: &State<Value>, context: &Context<E>) -> Result<Value, Error> {
    context.info("step1");                       // recorded in metadata log
    Err(Error::general_error("boom".to_string()))
}

let asset = env.get_asset(&parse_query("boom")?).await?;   // manager submits & evaluates

// get() delivers the TERMINAL STATE (not an Err) — the asset finished, its outcome is an error.
let state = asset.get().await?;                  // Ok(error_state)   <-- Ok, not Err
assert_eq!(state.status(), Status::Error);

// The error surfaces at value extraction — "a State is always potentially an error".
assert!(state.value_state().is_err());
assert!(state.try_into_string().is_err());       // guarded extractor also errors
let e = state.value_state().unwrap_err();
assert!(e.message.contains("boom"));

// Audit trail preserved (fail_asset used metadata.with_error, NOT Metadata::from_error).
let md = asset.get_metadata().await?;
assert!(md.log().iter().any(|l| l.message.contains("step1")));
```

**Expected:** three repeated `get()` calls all yield the same `Ok(error_state)` whose
`value_state()` is `Err("…boom…")`; metadata still contains `step1` and the query.

---

## Example 2 — Cancellation: a status, not a stored error

**Scenario:** An in-flight asset is cancelled. It reaches `Status::Cancelled` with **no** error
stored (`is_error == false`, `error_data == None`). `get()` returns `Ok(cancelled_state)`;
requesting a value synthesizes a typed `ErrorType::Cancelled`.

**Context:** User/control cancels a running evaluation (gate command blocks until released).

```rust
let asset = env.get_asset(&parse_query("gate")?).await?; // gate blocks on a shared oneshot
asset.cancel().await?;                                    // sets Status::Cancelled

let state = asset.get().await?;                          // Ok(cancelled_state)
assert_eq!(state.status(), Status::Cancelled);

// No stored error — error_result() is Ok for a cancelled state (this is the subtlety).
assert!(state.error_result().is_ok());
assert!(!state.is_error()?);

// But asking for a VALUE errors, with the reserved type.
let e = state.value_state().unwrap_err();
assert_eq!(e.error_type, ErrorType::Cancelled);         // synthesized, not stored
```

**Expected:** cancelled state carries no `error_data`; `value_state()`/`value()`/`try_into_string()`
all return `Err(ErrorType::Cancelled)`. Contrast with E1 where the error is the stored computed one.

---

## Example 3 — Re-evaluation on re-request (cache miss at the manager boundary)

**Scenario:** After an asset finishes `Error` (or `Cancelled`, or `Expired`), requesting it
*again from the manager* re-evaluates it; awaiting the *same ref* does not.

**Context:** A transient failure (hardware/volatile). The determinism best-practice says a value
shouldn't change across evaluations, but a failure might be transient, so re-request re-runs.

```rust
// A command that fails the first time, succeeds the second (static AtomicUsize).
let asset1 = env.get_asset(&q).await?;
assert_eq!(asset1.get().await?.status(), Status::Error);      // 1st eval fails

// Awaiting the SAME ref again = same terminal outcome (no re-eval).
assert_eq!(asset1.get().await?.status(), Status::Error);      // still Error, N times

// RE-REQUESTING from the manager = cache miss → re-evaluate (get(key) :3260 branch).
let asset2 = env.get_asset(&q).await?;                        // fresh asset, re-evaluated
let state2 = asset2.get().await?;
assert_eq!(state2.status(), Status::Ready);                  // 2nd eval succeeds
assert_eq!(state2.value()?.try_into_string()?, "ok");
```

**Expected:** the boundary-vs-await distinction holds — `asset1.get()` is stable (WP-2 "N times"
contract); a new `get_asset` triggers re-evaluation. No infinite loop (the rebuilt asset is fresh).

---

## Example 4 — Delivery failure: `get()` returns `Err` (the second axis)

**Scenario:** The store errors while fast-track loads a persisted asset. No faithful `State`
exists, so `get()` returns `Err` — distinct from a computed error-state.

**Context:** Store/network/hardware failure during retrieval, not a failure of the computation.

```rust
// Store::get is wired to fail (I/O error) for this key.
let asset_ref = env.get_asset(&parse_query("-R/stored.txt")?).await?;
let result = asset_ref.get().await;                 // delivery failure path

// Delivery failure surfaces as Err — there is no error-STATE to return.
let err = result.unwrap_err();
assert!(err.message.contains("store"));             // delivery error, not a computed "boom"
```

**Expected:** contrast with E1 — a *computed* failure is `Ok(error_state)`; a *delivery* failure
is `Err`. `try_fast_track`'s `store.get().await?` (`:471`) is the concrete source.

---

## Example 5 — Dependency composition

**Scenario:** A parent depends on a child. The child's terminal kind drives the parent:

```rust
// (a) Child computes an error → parent fails with dependency error.
//     parent.get() -> Ok(error_state); value_state() -> Err mentioning the child/dependency.

// (b) Child cancelled mid-flight while parent waits (Status::Dependencies)
//     → parent cascade-cancelled: parent.get() -> Ok(cancelled_state).

// (c) Child is a STALE Cancelled/Error from a prior lifecycle when the parent requests it
//     → child re-evaluated (cache miss); parent proceeds on the fresh outcome.
```

**Expected:** error propagates as failure, cancellation cascades, staleness re-evaluates — the
three are genuinely distinct. (WP-1 overlap; this WP defines the contract WP-1 consumes.)

---

## Corner Cases

### C1 — Concurrency: N getters see the same outcome
8 tasks call `get()` on a gated asset that fails after the gate opens. All 8 must receive the
same `Ok(error_state)` whose `value_state()` is `Err("boom")`. Guaranteed because the decision is
read from status/metadata, **not** the lossy `watch` payload. *(WP-2 flaky-today test.)*

### C2 — Notification overwrite
`ErrorOccurred` is overwritten in the `watch` channel by a later `JobFinished` before a late
subscriber reads it. The late `get()` must still return the error-state (via `poll_state()` off
status), never a generic "finished but no data". *(WP-2 `:1825`/`:2034` regression.)*

### C3 — Post-finish messages ignored
After `Ready`, send `UpdatePrimaryProgress` + `LogMessage` via the retained `service_sender()`.
Metadata must be unchanged except possibly a debug log line; status stays `Ready`. The key
regression: **late `LogMessage` currently still mutates metadata** (`:1213-1220`) — must be dropped.

### C4 — Metadata preservation on failure
A command logs `step1`, sets a filename, then fails. After completion, metadata must still contain
`step1`, the filename/type info, and the original query — proving `with_error` (mutate) replaced
`Metadata::from_error` (destroy) at `:1359`.

### C5 — Serialization round-trip of the outcome
An `Error` state persisted and reloaded retains its typed `error_data` (serde field). A
`Cancelled` state persists with **no** error payload; on reload, value extraction still
synthesizes `ErrorType::Cancelled` from status. Fast-track rejects both (loads only
Ready/Source/Override) → re-evaluate.

### C6 — Guard bypass safety
A caller that skips `value_state()` and calls `try_into_string()` / `as_bytes()` / `value()`
directly on an error- or cancelled-state still gets `Err` (the value-access guard). Only
`data_unchecked()` bypasses — used deliberately by forwarding/rendering.

### C7 — Forwarding an error-state
Delegation copies a child's terminal error-`State` to the parent via `data_unchecked()` /
state clone; this is correct (the `State` still carries the error) and must **not** be flagged by
the guard, because no value extraction happens.

### C8 — Re-eval loop safety
A deterministically failing asset, requested repeatedly from the manager, re-evaluates each
*top-level request* but never loops within a single request (the rebuilt asset is fresh /
non-finished, so it is not re-matched by the stale-terminal branch).

### C9 — Legitimate none-value ≠ error
A command that returns `Value::none()` successfully reaches `Status::Ready`; `value_state()` is
`Ok` (none is a valid value). Distinguishes "no value because success-with-none" from "no value
because error/cancelled".

---

## Test Plan (conceptual — named tests, red-before / green-after)

### Unit tests

| ID | File | Test | Red before | Green after |
|---|---|---|---|---|
| U1 | `error.rs` | `test_error_cancelled_constructor_and_type` `[red=compile]` | No `ErrorType::Cancelled` | `Error::cancelled(..).error_type == Cancelled`; `is_cancelled()` true |
| U2 | `state.rs` | `test_value_error_maps_by_status` | `value_error` absent | `None` for Ready; stored error for Error; `Cancelled` for Cancelled |
| U3 | `state.rs` | `test_value_state_and_guarded_extractors_error_on_error_and_cancelled` | extractors ignore status | `value_state`/`value`/`try_into_string`/`as_bytes` all `Err` on Error & Cancelled |
| U4 | `state.rs` | `test_cancelled_state_error_result_is_ok_but_value_state_errs` | conflated | `error_result()==Ok`, `value_state().is_err()` — pins the subtlety |
| U5 | `state.rs` | `test_data_unchecked_bypasses_guard` | no bypass API | `data_unchecked()` returns the (none) value without error |
| U6 | `metadata.rs` | `test_with_error_preserves_log_and_query` | `from_error` destroys | log/query retained, `error_data` set, status Error |
| U7 | `assets.rs` | `test_poll_state_error_and_cancelled_return_state` | pins current arm | `Some(none-value + status)` for Error/Cancelled; `None` for non-terminal |
| U8 | `assets.rs` | `test_fail_asset_idempotent_and_metadata_merged` `[red=compile]` | no `fail_asset` | data/binary None, `with_error`, single notify, idempotent |

### Integration tests (`liquers-core/tests/asset_failure_contract.rs` + dependency/expiration suites)

| ID | Test | Red before | Green after |
|---|---|---|---|
| I1 | `test_failed_asset_get_returns_ok_error_state_thrice` | callers treat `Ok(none-state)` as success | `get()` `Ok(error_state)` ×3; `value_state()` `Err("boom")` |
| I2 | `test_error_message_survives_notification_overwrite` | generic "finished but no data" | `value_state()` `Err("…boom…")` |
| I3 | `test_failure_preserves_metadata_log_and_query` | metadata replaced | `step1` + query + error present |
| I4 | `test_concurrent_getters_all_see_same_error` | flaky (some see none-state) | all 8 `Err("boom")` via `join_all` |
| I5 | `test_late_progress_and_log_after_finish_ignored` | late `LogMessage` mutates metadata | metadata unchanged; status `Ready` |
| I6 | `test_cancelled_asset_value_state_is_cancelled_type` | `Cancelled` yields `Ok(none-state)` | `get()` `Ok`; `value_state()` `Err(ErrorType::Cancelled)`; no stored error |
| I7 | `test_error_asset_reevaluated_on_manager_rerequest` | `Error` served stale (`:3270` returns as-is) | re-request re-evaluates; fresh outcome |
| I8 | `test_delivery_failure_returns_err_not_error_state` | conflated | store I/O failure → `get()` `Err` |
| I9 | `test_dependency_error_propagates_and_cancel_cascades` | path-dependent | dep error → parent error; dep cancel → parent cancelled; stale dep → re-eval (WP-1 overlap) |

### Test fixtures / commands (registered via `register_command!`)
- `boom` — always `Err(general_error("boom"))`, logs `step1` first.
- `gate` — blocks on a shared `oneshot`/`Semaphore` for deterministic mid-flight cancel.
- `counter` — static `AtomicUsize`; fails first call, succeeds second (E3/I7 re-eval).
- `flaky_store` — memory store wrapper whose `get` errors for a chosen key (E4/I8).

### Validation commands (Phase 4)
```
cargo test -p liquers-core --test asset_failure_contract
cargo test -p liquers-core
cargo test --workspace --exclude liquers-py
cargo clippy --workspace --exclude liquers-py --all-targets
cargo check -p liquers-py
```

## Query Validation

Queries used are simple and registration-checked: `boom`, `gate`, `counter` (action queries, no
args, no spaces/newlines/special chars); `-R/stored.txt` and `-R/...` resource queries require a
store — the tests use `AsyncMemoryStore` pre-populated per the `expiration_integration.rs`
pattern. No new query-language commands are introduced by WP-2; the fixture commands above are
test-local registrations.

## Inline Critical Review (stands in for 3-haiku/1-sonnet)

- **Phase 1 conformity:** ✔ every contract element has an example/test — value-XOR-error (E1/U7),
  always-check principle (C6/U3), single `get()` + `value_state` (E1/U3), cancelled-is-a-status
  (E2/U4/I6), re-eval policy (E3/I7), delivery axis (E4/I8), dependency composition (E5/I9),
  post-finish policy (C3/I5), metadata preservation (C4/I3).
- **Phase 2 conformity:** ✔ signatures used match (`value_error`, `value_state`, `value`,
  `data_unchecked`, `fail_asset`, `Error::cancelled`, `ErrorType::Cancelled`); `error_result()`
  correctly shown as Error-only (U4).
- **Codebase + query validation:** ✔ line refs match real code (`:1359`, `:1213-1220`, `:2034`,
  `:3260/:3270`, `:471`); queries are space/newline-free; resource queries have a store; fixture
  commands are registered.
- **Open items unchanged** (carried at recommended defaults into Phase 4): py raise-vs-None,
  axum status codes, interim logging, `State.data` privatization timing, re-eval boundary scope.
  None block the test plan; each maps to a Phase 4 decision point.

## Corner-case coverage checklist
- [x] Concurrency (C1), notification races (C2)
- [x] Post-finish message lifecycle (C3)
- [x] Metadata/audit preservation (C4)
- [x] Serialization round-trip incl. cancelled-has-no-error (C5)
- [x] Guard bypass + deliberate forwarding (C6, C7)
- [x] Re-eval loop safety (C8), none-value vs error (C9)
- [x] Delivery vs computed axis (E4/I8)
- [x] Cross-crate: axum/py/UI callers exercised via the caller-migration tests (Phase 4)
