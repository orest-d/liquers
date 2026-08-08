# Phase 4: Implementation Plan - Expired-Safe Binary Reads

## Overview

**Feature:** Expired-safe binary reads (`ASSET-EXPIRED-CACHED-BINARY-READ`, P0)

**Architecture:** One classifier, `Status::read_exposure() -> ReadExposure`, becomes the single
decision point for what any read may expose. The state family and the binary family both derive
from it. Five `*_binary` methods are added, four are brought under the classifier, and
`get`/`get_binary` gain a pre-wait expiry check.

**Estimated complexity:** Medium. The change is small in lines but touches a contract with several
consumers, and one step (Step 4) can regress persistence if done carelessly.

**Estimated time:** 4–6 hours for an experienced Rust developer, including tests.

**Prerequisites:** Phases 1–3 approved. All open questions resolved — notably, `AssetRef::get`
**is** in scope (user decision, option A). No new dependencies.

**Ordering principle:** every step compiles and passes tests on its own. Steps 1–2 are additive and
change no behaviour; Step 3 is the first behavioural change; Step 4 must land *with* Step 3 to
avoid a persistence window. Steps are ordered so that a bisect landing between any two commits
finds a working tree.

---

## Implementation Steps

Eleven steps. Steps 1–7 are `liquers-core`, Step 8 is the `liquers-axum` consumer, Steps 9–11 are
decision, documentation and close-out.

### Step 1 — Add `ReadExposure` and `Status::read_exposure()`

**File:** `liquers-core/src/metadata.rs`

**Action:**
- Add the `pub enum ReadExposure` from Phase 2 §Data Structures, beside `Status`.
- Add `impl Status { pub fn read_exposure(&self) -> ReadExposure }` with an explicit arm for each
  of the fifteen variants, per the Phase 2 classification table.
- Do **not** touch `has_data()`, `is_finished()` or any existing predicate.
- Export `ReadExposure` wherever `Status` is exported.

**This step is purely additive** — nothing calls the new method yet, so behaviour cannot change.

**Validation:**
```bash
cargo check -p liquers-core
cargo test -p liquers-core --lib metadata
```

**Rollback:** `git checkout liquers-core/src/metadata.rs`

**Agent:** haiku · skills: `rust-best-practices` · knowledge: Phase 2 §Data Structures + classification
table, `metadata.rs` `Status` enum and its existing predicates.
**Rationale:** mechanical transcription of a table that Phase 2 already fixed; no judgement needed.

---

### Step 2 — Unit-test the classifier (U1–U3)

**File:** `liquers-core/src/metadata.rs` (existing `#[cfg(test)] mod tests`, `:2314`)

**Action:** add U1, U2, U3 from Phase 3 §Test Plan.
- U1 asserts all fifteen variants individually — not in a loop, so a failure names the variant.
- U2 is the exhaustiveness guard. **The guarantee comes from the `expected()` match, not from the
  array**; write the match out in full. `Status::all()` does not exist — do not call it.
- U3 pins the `has_data()` trap: true for `Expired` and `Partial`, where exposure is not `Value`.

**Validation:**
```bash
cargo test -p liquers-core --lib metadata
# Expected: U1-U3 pass. They test only the new classifier, which nothing consumes yet.
```

**Rollback:** `git checkout liquers-core/src/metadata.rs`

**Agent:** haiku · skills: `liquers-unittest`, `rust-best-practices` · knowledge: Phase 3 §Test Plan
(U1–U3 code is given), `metadata.rs` test module conventions.

---

### Step 3 — Rewrite the read methods over the classifier

**File:** `liquers-core/src/assets.rs`

**Action** — the behavioural core. Do all of it in one commit; the pieces are not independently
correct.

1. `AssetData::poll_state` (`:769`) — rewrite as a `match self.status.read_exposure()` with four
   arms, preserving today's behaviour exactly: `Value` → value state (with `type_identifier` /
   `type_name` set from `data`); `MetadataOnly` → metadata-only state (**note `Directory` sets
   `type_identifier("dir")` and `Error`/`Cancelled` do not** — keep that distinction inside the
   arm); `Expired` → `None`; `Pending` → `None`.
2. `AssetData::poll_state_any_status` (`:813`) — retarget onto `ReadExposure::Expired` instead of
   `Status::Expired`. Behaviour identical.
3. `AssetData::poll_binary` (`:841`) — **the bug fix.** Return the cached bytes only for
   `ReadExposure::Value`; `None` for the other three.
4. `AssetData::poll_binary_any_status` — NEW. As `poll_binary`, plus bytes for `Expired`.
5. `AssetData::binary_unchecked` — NEW, `pub(crate)`. Today's `poll_binary` body verbatim: no
   status consulted.
6. `AssetRef::poll_binary_any_status`, `AssetRef::get_binary_any_status` — NEW async wrappers,
   mirroring `poll_state_any_status` / `get_any_status` (`:2425`, `:2433`). Acquire the read lock,
   delegate, drop. **No lock held across an `.await`.**
7. `AssetRef::get_binary` (`:2387`) — check exposure *before* the `poll_binary` short-circuit:
   `Expired` → `Err`; `MetadataOnly` → `Err` (see Step 3a); `Value` → cached bytes, else serialize;
   `Pending` → `self.get().await?` then re-derive.
8. `AssetRef::get` (`:2325`) — pre-wait expiry check: if exposure is `Expired`, return `Err`
   immediately rather than subscribing and looping. **In scope by the user's option-A decision.**

**Step 3a — error construction.** Per Phase 2 §Error Handling. `Error` and `Cancelled` must be
**separate match arms** inside `get_binary` despite sharing a `ReadExposure`, because only `Error`
has a recorded failure to reuse:

```rust
// Error → reuse the asset's own recorded failure (preserves the traceback).
// Cancelled, Directory → construct one; neither records an error.
Error::general_error("Asset was cancelled; no binary representation".to_owned())
```

Use typed constructors only. **Never `Error::new`.**

**Code shape for the classifier match** (no default arm anywhere):
```rust
match self.status.read_exposure() {
    ReadExposure::Value => { /* … */ }
    ReadExposure::MetadataOnly => { /* … */ }
    ReadExposure::Expired => { /* … */ }
    ReadExposure::Pending => { /* … */ }
}
```

**Validation:**
```bash
cargo check -p liquers-core
cargo test -p liquers-core --lib
# Expected: existing tests still pass. Any state-read test that breaks means step 3.1
# changed behaviour it was supposed to preserve — investigate, do not adjust the test.
```

That last sentence is the point of the step: **`poll_state` is a refactor, not a change.** A
failing state test is a defect in the refactor.

**Rollback:** `git checkout liquers-core/src/assets.rs`

**Agent:** **sonnet** · skills: `rust-best-practices` · knowledge: Phase 2 §Function Signatures +
Behaviour Matrix + §Error Handling, `assets.rs:760-870` and `:2320-2470`, `CLAUDE.md` error rules.
**Rationale:** the only step needing judgement — preserving `poll_state` semantics exactly while
restructuring it, and getting `get_binary`'s four-way branch right. Not a haiku step.

---

### Step 4 — Point persistence at `binary_unchecked`

**File:** `liquers-core/src/assets.rs`

**Action:** in `AssetRef::save_to_store` (`:1944`), replace `self.poll_binary().await` with the
status-blind accessor. **This must land in the same commit as Step 3.** Between gating
`poll_binary` and repointing `save_to_store`, any asset persisted at a non-`Value` status would
fall through to `serialize_to_binary`, which consults `poll_state` and returns `None` — turning a
successful persist into `Err("Failed to obtain binary value for storing")`.

That window is reachable in production, not theoretical: `AssetRef::set_state` (`:2548`) persists
with whatever status the caller supplies, via `Context::set_state` (`context.rs:789`).

**Validation:**
```bash
cargo test -p liquers-core --lib
cargo test -p liquers-core --test expiration_integration
```

**Rollback:** revert Steps 3 and 4 together.

**Agent:** haiku · skills: `rust-best-practices` · knowledge: Phase 2 §"Why it is needed",
`save_to_store` and `persist_with_status_tracking`.

---

### Step 5 — `AssetManager::get_binary_any_status`

**File:** `liquers-core/src/assets.rs`

**Action:** add the trait method with a **default body** (Phase 2 §Trait Implementations), modelled
on `get_any_status` (`:3281`):
1. `lookup_key_asset(key)` → if present, return `asset_ref.get_binary_any_status().await`.
2. Otherwise `store.contains(key)` → `Ok(None)` if absent.
3. `store.get(key)` → `(binary, metadata)`; `Ok(None)` if `!metadata.status().has_data()`.
4. Return `Ok(Some((Arc::new(binary), Arc::new(metadata))))` — **stop here.** Do *not* call
   `deserialize_stored_value`; skipping it is the point, and it is what lets recovery work for a
   type this build cannot deserialize.

Note step 3 uses `has_data()` deliberately — this is the store fallback, where "is there a value in
there" *is* the right question. That is not a contradiction of Step 1's rule that `has_data()` is
unsuitable as a *read gate*.

Neither implementor (`DefaultAssetManager`, `ImmediateAssetManager`) needs code.

**Validation:**
```bash
cargo check -p liquers-core
cargo check -p liquers-py   # trait change: confirm no downstream implementor breaks
```

**Rollback:** `git checkout liquers-core/src/assets.rs`

**Agent:** haiku · skills: `rust-best-practices` · knowledge: Phase 2 §Trait Implementations,
`get_any_status` at `:3281`, `deserialize_stored_value` at `:461`.

---

### Step 6 — Core tests (Examples 1–3, U4–U10, I1)

**File:** `liquers-core/src/assets.rs` (`mod tests`, `:5472`, `use super::*` at `:5480`)

**Action:** land the tests from Phase 3. Use the **verified** setup facts:
- `try_fast_track` (`:679`) is the clean way to reach `Expired` while holding cached bytes.
- `expire()` accepts only `Ready`/`Override`.
- I1 must be **in-file**, not in `tests/` — `set_state` is `pub(crate)`.
- Do not use `AssetRef::set_binary` or `State::new_with_value`; neither exists.

U9 (`get` on `Expired`) **must** use `tokio::time::timeout`, so that a regression fails the suite
instead of hanging it.

**Validation:**
```bash
cargo test -p liquers-core --lib
```

**Rollback:** `git checkout liquers-core/src/assets.rs`

**Agent:** sonnet · skills: `liquers-unittest`, `rust-best-practices` · knowledge: Phase 3 in full,
especially §Verified Setup Facts and §Access.
**Rationale:** the setup is the hard part and the drafting pass got it wrong repeatedly; needs a
model that will check APIs rather than assume them.

---

### Step 7 — Integration tests (I2–I4)

**File:** `liquers-core/tests/expiration_integration.rs`

**Action:** I2 end-to-end expiry; I3 manager re-request still rebuilds (call-counting command); I4
fast-track after expiry does not resurrect stale bytes. I3 is the important one — it proves manager
routing is unchanged and that the fix has not converted "expired → recompute" into "expired →
error" at the request boundary.

**Validation:**
```bash
cargo test -p liquers-core --test expiration_integration
cargo test -p liquers-core --test manager_parametric
```

**Rollback:** `git checkout liquers-core/tests/expiration_integration.rs`

**Agent:** sonnet · skills: `liquers-unittest` · knowledge: Phase 3 §Integration Tests, existing
`expiration_integration.rs` conventions.

---

### Step 8 — `liquers-axum` consumer fix

**File:** `liquers-axum/src/query/handlers.rs`

**Action:**
1. Replace the `_ =>` catch-all status arms — `:109` (GET) and `:216` (POST, an empty `_ => {}`) —
   with explicit arms for all fifteen statuses, per the no-default-match-arm rule.
2. `Expired` returns an **error response**. Do not re-request from the manager: re-evaluation
   belongs at the request boundary (`get_asset`/`get`), and a handler holding an `AssetRef` is past
   it. (Phase 1 §"Expiry is an error".)
3. `Error`/`Cancelled` keep their current handling. `Pending` statuses keep looping.

Without this step the fix converts stale bytes into a 30-second timeout, because the catch-all
currently swallows `Expired` as "still processing".

**Validation:**
```bash
cargo check -p liquers-axum
cargo test -p liquers-axum
```

**Rollback:** `git checkout liquers-axum/src/query/handlers.rs`

**Agent:** sonnet · skills: `rust-best-practices` · knowledge: Phase 2 §Integration Points, Phase 1
§"Expiry is an error", both handler loops.

---

### Step 9 — The axum test-coverage decision

**Carried from Phase 3 as an explicit choice, not a default.** `liquers-axum` has no handler test
scaffolding, and Step 8 sits over the layer where the bug actually bites.

Pick one and record it in `DESIGN.md`:
- **(a)** Build the scaffolding — a test `Router` plus a request helper, ~80 lines, reusable by
  every future handler test — and test that an expired asset yields an error response promptly.
- **(b)** Accept review-plus-manual-verification **and file `specs/issues/AXUM-HANDLER-TEST-COVERAGE.md`**
  (`status: draft`, `priority`/`complexity`/`area` filled) per `CLAUDE.md`. Shipping (b) without
  the issue is not an option.

**Recommendation: (b).** The scaffolding is worth building, but it is a different piece of work
from a P0 read-contract fix, and bundling it widens this change's blast radius for no correctness
gain.

**Agent:** sonnet if (a); no agent if (b) — filing the issue is a five-minute task.

---

### Step 10 — Documentation

**Files:** `liquers-core/src/assets.rs` module docs (`:100-116`), `specs/reference/ASSETS.md`

**Action:**
1. The module-level read-contract table currently documents the bug as intended behaviour — "A
   cached binary may be returned" for both `get_binary` and `poll_binary`. Replace with the Phase 2
   Behaviour Matrix, adding rows for the new methods.
2. `ASSETS.md` §"Status and reads": same correction, plus the `ReadExposure` classification.
3. `ASSETS.md` requires a `## History` row and a `reviewed:` bump **in the same commit**
   (`DOCS_STRUCTURE_GUIDE.md` §9.2).

**Validation:**
```bash
python3 scripts/docs_index.py --check
```

**Agent:** haiku · skills: none · knowledge: Phase 2 Behaviour Matrix, `DOCS_STRUCTURE_GUIDE.md` §9.2.

---

### Step 11 — Close out

**Action:**
1. `specs/issues/ASSET-EXPIRED-CACHED-BINARY-READ.md` → `status: complete`, noting that the
   "needs verification against PR #11" caveat was resolved (the bug *was* still live) and that the
   fix went wider than the issue described — the gate covers all non-`Value` statuses, not only
   `Expired`.
2. `DESIGN.md`: set `gh_pr`, and **remove any derived `status:`** — once `gh_pr` is set, status is
   GitHub's to determine (`DOCS_STRUCTURE_GUIDE.md` §5.5).
3. Update the capability map in `specs/README.md`.
4. Run `python3 scripts/docs_index.py --sync`.

**Agent:** haiku · knowledge: `DOCS_STRUCTURE_GUIDE.md` §4.8, §5.5, §8.1.

---

## Agent Assignment

Summary of the per-step assignments above. Model choice tracks how much judgement the step needs,
not how long it is.

| Step | Model | Skills | Why |
|---|---|---|---|
| 1 | haiku | rust-best-practices | Transcribes a table Phase 2 already fixed |
| 2 | haiku | liquers-unittest, rust-best-practices | Test code given in Phase 3 |
| 3 | **sonnet** | rust-best-practices | The only step needing judgement: preserve `poll_state` exactly while restructuring it, and get `get_binary`'s four-way branch right |
| 4 | haiku | rust-best-practices | One-line change, but must ship with Step 3 |
| 5 | haiku | rust-best-practices | Mirrors `get_any_status` minus one call |
| 6 | **sonnet** | liquers-unittest, rust-best-practices | Setup is the hard part; the drafting pass got it wrong repeatedly, so this needs a model that verifies APIs rather than assuming them |
| 7 | sonnet | liquers-unittest | Integration setup with recipes and managers |
| 8 | sonnet | rust-best-practices | Fifteen explicit arms plus a behaviour decision per status |
| 9 | sonnet if (a), none if (b) | — | Filing an issue needs no agent |
| 10 | haiku | — | Mechanical, against a matrix that already exists |
| 11 | haiku | — | Follows `DOCS_STRUCTURE_GUIDE.md` §4.8/§5.5/§8.1 |

Every agent needs `CLAUDE.md`'s hard rules in context: no `unwrap`/`expect` in library code, no
`_ =>` on Liquers enums, typed error constructors only, `eprintln!` never `println!`.

## Testing Plan

| When | Command | Gate |
|---|---|---|
| After Steps 1, 2 | `cargo test -p liquers-core --lib metadata` | classifier correct in isolation |
| After Steps 3, 4 | `cargo test -p liquers-core --lib` | **no existing state-read test regresses** |
| After Step 5 | `cargo check -p liquers-py` | trait default breaks no downstream implementor |
| After Step 6 | `cargo test -p liquers-core --lib` | new contract holds |
| After Step 7 | `cargo test -p liquers-core --test expiration_integration --test manager_parametric` | routing + persistence unchanged |
| After Step 8 | `cargo test -p liquers-axum` | consumer compiles and passes |
| Final | `cargo test -p liquers-lib --lib --tests` | the project's default loop (per `CLAUDE.md`) |

Do **not** run `cargo test --workspace` — `CLAUDE.md` warns it exhausts the 30 GB disk allowance.

**The single most informative signal** is the Steps 3–4 gate: `poll_state` is being restructured
but must not change. If a pre-existing state test fails there, the refactor is wrong — fix the
code, not the test.

## Rollback Plan

| Scope | Action |
|---|---|
| Any single step | `git checkout <file>` — each step is one file, except 3+4 |
| Steps 3+4 | Revert together; separating them opens the persistence window described in Step 4 |
| Whole feature | Revert to before Step 1. Steps 1–2 are additive and safe to leave in place if only the behavioural change needs backing out — the classifier is dead code without Step 3. |

The design is **not** feature-gated. A read-contract fix behind a flag would mean shipping two
contracts, which is worse than shipping the fix.

## Risk Register

| Risk | Likelihood | Mitigation |
|---|---|---|
| `poll_state` refactor silently changes state reads | Medium | U4 asserts behaviour class for all 15 statuses; Step 3 gate treats a failing existing test as a defect |
| Persistence breaks for non-`Value` statuses | Medium | Steps 3+4 land together; I1 is the regression test |
| Axum handlers hang instead of erroring | High if Step 8 skipped | Step 8 is mandatory, not optional |
| `Cancelled` error wording diverges from `Error` | Low | U8 asserts error *identity*, not just failure |
| A future `Status` variant lands in the wrong bucket | Low | U2's exhaustive match makes it a compile error |

## Open Items Carried Forward

1. **Step 9's decision** — recommendation (b), needs confirmation at execution time.
2. `EXPIRATION-RECOVERY-WEB-API` may want to grow to cover `get_binary_any_status`. Affects that
   issue's scope, not this design's code.
3. Phase 3 named two things as untestable — `try_poll_binary` lock contention and the exact
   interleaving of expiry with an in-flight read. Both are pre-existing and unchanged by this work;
   recorded so nobody mistakes their absence for an oversight.
