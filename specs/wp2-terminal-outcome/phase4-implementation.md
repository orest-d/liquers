# Phase 4: Implementation Plan - Asset Terminal Outcome Contract (WP-2)

## Overview

**Feature:** Asset Terminal Outcome Contract (WP-2, F-2/F-3).

**Architecture:** Make `State` (backed by `Metadata`) the single faithful carrier of a terminal
asset outcome; single `get() -> Result<State, Error>` where `Err` = delivery failure only and
`Ok` = terminal `State` (value or error); value extraction is guarded (`value_state`/`value_error`);
`Cancelled`/`Error` are statuses, not errors, with `ErrorType::Cancelled` reserved for value
extraction; `Error`/`Cancelled`/`Expired` are re-evaluated at the manager request boundary.

**Estimated complexity:** Medium-High (single-crate core change with a compiler-driven downstream
migration across `liquers-lib`, `liquers-axum`, `liquers-py`).

**Estimated time:** ~1.5–2 days for an experienced Rust developer.

**Prerequisites:** Phases 1–3 approved. Open questions resolved to the recommended defaults below
(each is localized and can be revisited without reworking the core):

| Q | Decision taken in this plan | Revisit cost |
|---|---|---|
| py error-state policy | **Raise** on error/cancelled value extraction (matches guard) | Step 9, isolated |
| axum status codes | `Error` → 500; `ErrorType::Cancelled` → 499 (client-closed) | Step 8, one match |
| interim logging | `debug`-guarded `eprintln!` until WP-6 `tracing` | Steps 6–7 |
| `State.data` privatization | **Do it in this WP** (it is the safety net) | Steps 3, 8, 9 |
| re-eval boundary scope | **Always** re-eval `Error`/`Cancelled`/`Expired` (matches `Expired`) | Step 7, one branch |

> **Process note:** `rust-best-practices` is not installed and per environment guidance I do not
> spawn review sub-agents. The "Agent Specification" per step below is retained for fidelity to
> the designer workflow, but in practice these steps are executed inline. The inline critical
> review at the end stands in for the 4-haiku/1-opus fan-out.

## Implementation order (summary)

Red tests → foundation (`Error`) → `State` guards → `Metadata` confirm → `assets.rs` core →
manager re-eval → downstream migration (interpreter, UI, axum, py) → docs → full workspace gate.
Core changes keep `liquers-core` compiling at each step; the two API-breaking changes
(`ErrorType::Cancelled`, private `State.data`) are landed with their downstream fixes before the
workspace build runs.

---

## Implementation Steps

### Step 0: Add red tests (compile-red + behavioral-red)

**Files:** `liquers-core/tests/asset_failure_contract.rs` (new); unit test stubs appended to
`state.rs`, `metadata.rs`, `error.rs`, `assets.rs` `#[cfg(test)]` modules.

**Action:** Encode the Phase-3 test plan (`U1–U8`, `I1–I9`) with fixture commands (`boom`, `gate`,
`counter`, `flaky_store`). Tests referencing new APIs are `[red=compile]`; behavioral tests
(`I1`–`I5`) compile today and fail for the stated reason. Capture the red output for the PR.

**Validation:**
```bash
cargo test -p liquers-core --test asset_failure_contract 2>&1 | tee /tmp/wp2-red.txt   # expect failures/compile errors
```

**Rollback:** `git checkout liquers-core/tests/asset_failure_contract.rs` and revert unit stubs.

**Agent Specification:** Model sonnet · Skills liquers-unittest, rust-best-practices · Knowledge
Phase 3 doc, `expiration_integration.rs` (harness pattern), Phase 2 signatures · Rationale: test
authorship needs the harness conventions and the exact contract semantics.

---

### Step 1: `ErrorType::Cancelled` + constructors

**File:** `liquers-core/src/error.rs`

**Action:**
- Add `Cancelled` variant to `ErrorType` (after `DependencyCycle`).
- Add `Error::cancelled(message: impl Into<String>) -> Self` (sets `error_type = Cancelled`).
- Add `Error::is_cancelled(&self) -> bool`.

**Validation:**
```bash
cargo check -p liquers-core            # expect: errors ONLY at non-exhaustive `match ErrorType`
```
Record every surfaced match site (esp. `liquers-axum`) — this is the compiler-driven audit; fix in
Step 8. `liquers-core`-internal matches (if any) fixed here.

**Rollback:** `git checkout liquers-core/src/error.rs`.

**Agent Specification:** Model haiku · Skills rust-best-practices · Knowledge `error.rs`, CLAUDE.md
error rules · Rationale: mechanical, well-specified.

---

### Step 2: `Status` helper for value-bearing check (support for Step 3)

**File:** `liquers-core/src/metadata.rs`

**Action:** Confirm/keep `Status::has_data()` as the value-bearing predicate (Ready/Source/Override/
Volatile/Partial/Directory → true; Error/Cancelled/… → false). No change expected; if
`Directory`/`Partial` need adjustment for `value_state`, decide here (Partial is Open Question 2 —
default: treat `Partial` as value-bearing, unchanged).

**Validation:** `cargo check -p liquers-core`.

**Rollback:** n/a (no change) or `git checkout liquers-core/src/metadata.rs`.

**Agent Specification:** Model haiku · Skills rust-best-practices · Knowledge `metadata.rs` Status
impl · Rationale: verification-only.

---

### Step 3: `State` — value guard + private `data`

**File:** `liquers-core/src/state.rs`

**Action:**
- Change field `pub data: Arc<V>` → `data: Arc<V>` (remove `// TODO: remove pub`).
- Add `value_error(&self) -> Option<Error>`: `None` if `status().has_data()`; else
  `Status::Error` → `self.metadata.error_result().unwrap_err()`; `Status::Cancelled` →
  `Error::cancelled(self.message())`; other non-data terminal → `Error::general_error("no value: {status}")`.
- Add `value_state(self) -> Result<Self, Error>` = `match self.value_error() { Some(e)=>Err(e), None=>Ok(self) }`.
- Add `value(&self) -> Result<Arc<V>, Error>` = `value_error` guard then `Ok(self.data.clone())`.
- Add `data_unchecked(&self) -> &Arc<V>`.
- Change `try_into_string`/`as_bytes` to run the `value_error` guard first.

**Validation:**
```bash
cargo check -p liquers-core            # expect: errors at internal `state.data` field uses (fix them here)
cargo test -p liquers-core --lib state 2>&1   # U2–U5 should start passing
```

**Rollback:** `git checkout liquers-core/src/state.rs`.

**Agent Specification:** Model sonnet · Skills rust-best-practices · Knowledge `state.rs`, Phase 2
`State` signatures, `Status::has_data`, `Metadata::error_result` · Rationale: touches an invariant
and the public surface; needs judgment on the `value_error` mapping.

---

### Step 4: `fail_asset` — the one metadata-preserving failure routine

**File:** `liquers-core/src/assets.rs`

**Action:**
- Add `pub(crate) async fn fail_asset(&self, e: Error) -> Result<(), Error>` on `AssetRef<E>`:
  write lock → `data=None`, `binary=None`, `status=Error`, `metadata.with_error(e)` (NOT
  `Metadata::from_error`), single notification; idempotent if already `Error`.
- Reroute the three sites to `fail_asset`:
  - `finish_run_with_result` result=`Err` branch (`:1354–1359`) — replace `Metadata::from_error`.
  - `AssetRef::set_error` (`:2206–2210`) — reimplement via `fail_asset` (keep the one
    `ErrorOccurred` send for subscribers, or drop if redundant with the notify inside `fail_asset`).
  - `process_service_messages` `ErrorOccurred` arm (`:1277–1288`) — call `fail_asset` (its body
    already does `with_error`; unify).
- **Do NOT** route the `Cancel` arm through `fail_asset` — it stays `set_status(Cancelled)` with no
  stored error (`:1221–1232`), only ensuring `is_error`/`error_data` remain unset.

**Validation:**
```bash
cargo check -p liquers-core
cargo test -p liquers-core --lib assets::tests::test_fail_asset 2>&1   # U8
```

**Rollback:** `git checkout liquers-core/src/assets.rs`.

**Agent Specification:** Model sonnet · Skills rust-best-practices · Knowledge Phase 2 Audit A,
`assets.rs:1180–1400,2205–2220`, `Metadata::with_error` · Rationale: unifies concurrency-sensitive
paths; must preserve the no-default-match-arm rule and idempotency.

---

### Step 5: Rewrite `get()` to consult status, not notification content

**File:** `liquers-core/src/assets.rs` (`AssetRef::get`, `:1990–2049`)

**Action:**
- Loop: `if let Some(s) = poll_state().await { return Ok(s); }` then subscribe and, on each
  `rx.changed()`, re-poll. Any notification is a wake-up only.
- **Delete** `AssetNotificationMessage::ErrorOccurred(e) => return Err(e)` (`:2020`) — a failed
  asset now returns `Ok(error_state)` via `poll_state`.
- Keep the finished-but-no-state case as a delivery `Err` (now unreachable on the normal failure
  path); keep `Expired => Err` for WP-3 (documented). Channel-closed → delivery `Err`.
- Remove the debug `println!` at `:2009` (or gate it) — no behavior change.

**Validation:**
```bash
cargo test -p liquers-core --test asset_failure_contract -- I1 I2 I4 I6   # error/cancel/concurrent
cargo test -p liquers-core
```

**Rollback:** `git checkout liquers-core/src/assets.rs`.

**Agent Specification:** Model sonnet · Skills rust-best-practices · Knowledge Phase 2 `get()`
shape, `poll_state:596/2084`, notification enum · Rationale: the central behavioral change; the
concurrency/notification-overwrite guarantees ride on it.

---

### Step 6: Post-finish message policy

**File:** `liquers-core/src/assets.rs` (`process_service_messages`, `:1180–1293`)

**Action:**
- Add an explicit finishing phase (bool entered on `JobFinishing`, or reuse `is_finished()` — keep
  behavior identical to the matrix). Extend the existing `should_ignore` set (`:1194–1202`) to
  include `LogMessage` so late log messages are **dropped**, not applied (`:1213–1220`).
- Late drops: `debug`-guarded `eprintln!` (interim, until WP-6 `tracing`).
- Keep the `JobFinishing` arm not sending a premature `JobFinished` (commented-out send stays
  deleted — resolves the "meaningless send" FIXME). Retain explicit arms (no default).

**Validation:**
```bash
cargo test -p liquers-core --test asset_failure_contract -- I3 I5   # metadata preserved / late msgs ignored
```

**Rollback:** `git checkout liquers-core/src/assets.rs`.

**Agent Specification:** Model sonnet · Skills rust-best-practices · Knowledge Phase 2 policy
matrix, `process_service_messages` body · Rationale: message-lifecycle correctness under races.

---

### Step 7: Manager re-evaluation policy

**File:** `liquers-core/src/assets.rs` (`DefaultAssetManager::get`, `:3255–3288`)

**Action:** Extend the stale-terminal branch (`:3260`, currently `status == Status::Expired`) to
`matches!(status, Status::Expired | Status::Error | Status::Cancelled)`: remove from the map if the
id matches, `continue` to rebuild fresh. Keep `status.is_finished()` returning the ref for
value-bearing terminals (Ready/Source/Override/Directory/Volatile). Confirm the rebuilt asset is
fresh (non-finished) so no intra-request loop. Note the recipe requirement (source assets without a
recipe cannot rebuild — leave their state; add a guarded branch if needed).

**Validation:**
```bash
cargo test -p liquers-core --test asset_failure_contract -- I7   # re-eval on re-request
cargo test -p liquers-core
cargo check -p liquers-py                                        # core public types changed
```

**Rollback:** `git checkout liquers-core/src/assets.rs`.

**Agent Specification:** Model sonnet · Skills rust-best-practices · Knowledge Phase 2 re-eval
section, `get(key)` loop, fast-track interaction · Rationale: subtle boundary-vs-await placement;
loop-safety reasoning required.

---

### Step 8: Downstream migration — `liquers-core` interpreter + `liquers-axum`

**Files:** `liquers-core/src/interpreter.rs`; `liquers-axum/src/assets/handlers.rs`; the axum
`ErrorType` match surfaced in Step 1.

**Action:**
- `interpreter.rs` (`:660,711,724,758,774`): append `.value_state()?` where a value is required
  (many are auto-covered because they already chain `.try_into_string()`, now guarded).
- axum handlers (`:52,219`): map a computed error-`State` to an HTTP error via `value_state()` —
  `match get().await { Ok(s) => match s.value_state() { Ok(v)=>serve, Err(e)=>error_detail }, Err(e)=>error_detail }`.
- axum `ErrorType` match: add explicit `Cancelled => 499` arm; `Error`/general → 500.

**Validation:**
```bash
cargo check -p liquers-core && cargo check -p liquers-axum
cargo test -p liquers-axum
```

**Rollback:** `git checkout liquers-core/src/interpreter.rs liquers-axum/src/`.

**Agent Specification:** Model sonnet · Skills rust-best-practices · Knowledge Phase 2 Audit B,
handler code, axum error mapping · Rationale: per-caller judgment (serve vs. error vs. render).

---

### Step 9: Downstream migration — `liquers-lib` UI + `liquers-py`

**Files:** `liquers-lib/src/ui/element.rs`; `liquers-py/src/state.rs`, `liquers-py/src/commands.rs`.

**Action:**
- UI `AssetViewElement::from_asset_ref` (`:302,327,329,338–345`): read the error from
  `state.error_result()` / `value_error()` (terminal state), **not** the transient `ErrorOccurred`
  notification; use `data_unchecked()` for display of value states.
- py `state.rs` (`:29,37,42,48`), `commands.rs` (`:134,141`): replace `state.data` with
  `value()?` (raise on error/cancelled — chosen policy) or `data_unchecked()` for display/`__repr__`.
  Decide `__value__`/value getter: **raise** `Err(ErrorType::Cancelled/…)` mapped to a Python
  exception.

**Validation:**
```bash
cargo check -p liquers-lib && cargo test -p liquers-lib
cargo check -p liquers-py
```

**Rollback:** `git checkout liquers-lib/src/ui/element.rs liquers-py/src/`.

**Agent Specification:** Model sonnet · Skills rust-best-practices · Knowledge Phase 2 Audit B, UI
element flow, py bindings · Rationale: cross-crate; py exception mapping is a real decision.

---

### Step 10: Documentation

**Files:** `specs/ASSETS.md`, `specs/ISSUES.md`, `specs/wp2-terminal-outcome/DESIGN.md`.

**Action:** Add "Terminal outcome contract" section to `ASSETS.md` (value-XOR-error, single `get()`,
cancelled-is-a-status, re-eval policy, dependency composition). Update
`ASSET-MESSAGE-LIFECYCLE-ROBUSTNESS` in `ISSUES.md` with the implemented policy matrix and mark it
resolved. Cross-reference WP-1 (dependency) and WP-3 (expired) overlaps.

**Validation:** `python3 .claude/skills/liquers-designer/scripts/validate_phase.py wp2-terminal-outcome 4` (docs consistency, manual).

**Rollback:** `git checkout specs/`.

**Agent Specification:** Model haiku · Skills (none) · Knowledge Phases 1–3, `ISSUES.md` · Rationale:
prose from settled design.

---

### Step 11: Full workspace validation gate

**Validation:**
```bash
cargo test -p liquers-core --test asset_failure_contract     # all I1–I9 green
cargo test -p liquers-core                                   # incl. U1–U8
cargo test --workspace --exclude liquers-py
cargo clippy --workspace --exclude liquers-py --all-targets
cargo check -p liquers-py
```
Also re-run `expiration_integration.rs` (WP-3 overlap) and any dependency suite (WP-1 overlap) to
confirm no regression. Attach before/after red→green output to the PR.

**Agent Specification:** Model sonnet · Skills rust-best-practices, liquers-unittest · Knowledge all
phases · Rationale: final green-gate + triage of any fallout.

---

## Agent Assignment

| Step | Model | Skills | Why |
|---|---|---|---|
| 0 tests | sonnet | liquers-unittest, rust-best-practices | harness + contract semantics |
| 1 Error | haiku | rust-best-practices | mechanical |
| 2 Status | haiku | rust-best-practices | verification-only |
| 3 State | sonnet | rust-best-practices | invariant + public surface |
| 4 fail_asset | sonnet | rust-best-practices | concurrency-sensitive unify |
| 5 get() | sonnet | rust-best-practices | central behavioral change |
| 6 post-finish | sonnet | rust-best-practices | message-lifecycle races |
| 7 re-eval | sonnet | rust-best-practices | boundary placement + loop-safety |
| 8 interpreter/axum | sonnet | rust-best-practices | per-caller judgment |
| 9 UI/py | sonnet | rust-best-practices | cross-crate + py exceptions |
| 10 docs | haiku | — | settled-design prose |
| 11 gate | sonnet | rust-best-practices, liquers-unittest | final green-gate triage |

In practice these steps execute inline (no sub-agent fan-out per environment guidance); the table
records the intended model/skill profile per the designer workflow.

## Testing Plan

- **Unit (run after Steps 1,3,4):** `U1` (error type), `U2–U5` (state guards), `U6` (metadata
  preserve), `U7` (poll_state arms), `U8` (fail_asset).
- **Integration (run after Steps 5–7):** `I1–I6` (failure/cancel/concurrency), `I7` (re-eval),
  `I8` (delivery), `I9` (dependency; may partially defer to WP-1 — mark clearly).
- **Regression nets:** existing `expiration_integration.rs`, dependency suites, axum/py builds.
- **Red→green evidence:** capture Step 0 red output and Step 11 green output for the PR.

## Rollback Plan (overall)

Each step is a single-file (or small) change with its own `git checkout`. The two risky,
wide-blast changes are isolated: `ErrorType::Cancelled` (Step 1, revert unblocks all matches) and
private `State.data` (Step 3, revert restores field access). If Step 7 (re-eval) proves too broad,
it can be reverted independently — the rest of the contract stands without it (re-eval is additive
policy). Branch-level rollback: `git reset --hard <pre-WP2-commit>`.

## Documentation Updates

- `specs/ASSETS.md`: new "Terminal outcome contract" section (Step 10).
- `specs/ISSUES.md`: `ASSET-MESSAGE-LIFECYCLE-ROBUSTNESS` → resolved with policy matrix.
- `CLAUDE.md`: no change required (no new error type/struct; one enum variant).
- `specs/PROJECT_OVERVIEW.md`: touch only if the layer-2/3 (`State`/`Asset`) description needs the
  value-XOR-error invariant spelled out — check during Step 10.

## Inline Critical Review (stands in for 4-haiku/1-opus)

- **Phase 1 conformity:** ✔ single `get()`, value guard + private `data`, cancelled-is-a-status,
  re-eval policy, always-check principle, both audits — all have steps. No `AssetOutcome`.
- **Phase 2 conformity:** ✔ every signature/decision maps to a step (Error S1, State S3,
  fail_asset S4, get S5, post-finish S6, re-eval S7, integration points S8–S10).
- **Phase 3 conformity:** ✔ `U1–U8`/`I1–I9` and fixtures scheduled in Step 0 and re-run at each
  relevant step; validation commands match Phase 3.
- **Codebase compatibility:** ✔ line numbers verified against current `assets.rs`/`state.rs`/
  `metadata.rs`/`error.rs`/handlers; the two breaking changes are landed with their downstream
  fixes before the workspace gate; `cargo check -p liquers-py` gates the public-type change.
- **Residual risks:** (1) `I9` dependency behavior overlaps WP-1 — if WP-1 is unlanded, implement
  the dependency-cancel/propagate/re-eval in `wait_for_dependency` here and mark it; (2) `Expired`
  get()-arm stays `Err` pending WP-3 — documented, not a regression; (3) re-eval of deterministic
  failures re-runs per request (accepted; gate is Open Question 5 if cost matters).

## Execution options (after approval)

1. **Execute now** — implement Steps 0–11 on `claude/wp2-state-design-6hub1b`, red→green, committing per step.
2. **Create task list** — defer; land as a tracked checklist.
3. **Revise plan** — return to Phase 4.
4. **Exit** — you implement manually from this plan.
