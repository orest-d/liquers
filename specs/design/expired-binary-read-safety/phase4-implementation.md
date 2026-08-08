# Phase 4: Implementation Plan - Expired-Safe Binary Reads

## Overview

**Feature:** Expired-safe binary reads (`ASSET-EXPIRED-CACHED-BINARY-READ`, P0)

**Architecture:** One classifier, `Status::read_exposure() -> ReadExposure`, becomes the single
decision point for what any read may expose. The state family and the binary family both derive
from it. Five `*_binary` methods are added, four are brought under the classifier, and
`get`/`get_binary` gain a pre-wait expiry check.

**Estimated complexity:** Medium. The change is small in lines but touches a contract with several
consumers, and one step (Step 3) can regress persistence if its two parts are split.

**Estimated time:** 4–6 hours for an experienced Rust developer, including tests.

**Prerequisites:** Phases 1–3 approved. All open questions resolved — notably, `AssetRef::get`
**is** in scope (user decision, option A). No new dependencies.

**Ordering principle:** every step compiles and passes the test suite on its own. Steps 1–2 are
additive and change no behaviour; Step 3 is the first behavioural change, and its two parts are one
commit.

**One honest caveat about "working tree":** between Step 3 and Step 7, the build is green and
`liquers-core` is correct, but `liquers-axum` is *behaviourally* degraded — its handlers treat the
now-gated `Expired` as "still processing" and spin to the 30-second timeout instead of returning
stale bytes. Compiling and passing tests is not the same as behaving correctly here, because the
handler gap is exactly what no test covers (Step 8). **Do not ship a release cut between Step 3 and
Step 7.** Land them in one PR.

---

## Implementation Steps

Ten steps. Steps 1–6 are `liquers-core`, Step 7 is the `liquers-axum` consumer, Steps 8–10 are
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

### Step 3 — Read methods + persistence  ⚠ ONE COMMIT

> **Parts A and B below are a single commit.** They are numbered as one step, not two, because
> splitting them opens a real persistence window *that Part A's own tests would not catch* — I1,
> the regression test, does not exist until Step 4. A green `cargo test` between A and B is not
> evidence of a working tree.

#### Part A — rewrite the read methods over the classifier

**File:** `liquers-core/src/assets.rs`

**Action** — the behavioural core. Do all of it in one commit; the pieces are not independently
correct.

1. `AssetData::poll_state` (`:769`) — rewrite as a `match self.status.read_exposure()` with four
   arms, preserving today's behaviour exactly: `Value` → value state (with `type_identifier` /
   `type_name` set from `data`); `MetadataOnly` → metadata-only state; `Expired` → `None`;
   `Pending` → `None`.
   **The `MetadataOnly` arm needs an inner `match` on `Status`**: `Directory` sets
   `type_identifier("dir")` (`:772-778`) and `Error`/`Cancelled` do not (`:785-790`). A flat
   restructure loses that distinction — which is a behaviour change, and this step must not make
   one. The inner match names all three statuses explicitly; no default arm.
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
   Use the same message as `get_binary`'s expired case (Phase 2 §Error Handling), so the two report
   the same condition identically — that is the symmetry, stated at the level a caller sees.

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

#### Part B — point persistence at `binary_unchecked`

In `AssetRef::save_to_store` (`:1944`), replace `self.poll_binary().await` with the status-blind
accessor. Between Part A and Part B, any asset persisted at a non-`Value` status falls through to
`serialize_to_binary`, which consults `poll_state`, gets `None`, and turns a successful persist into
`Err("Failed to obtain binary value for storing")`.

That window is reachable in production, not theoretical: `AssetRef::set_state` (`:2548`) persists
with whatever status the caller supplies, via `Context::set_state` (`context.rs:789`).

**Validation (whole step):**
```bash
cargo check -p liquers-core
cargo test -p liquers-core --lib
# Expected: existing tests still pass. Any state-read test that breaks means Part A.1
# changed behaviour it was supposed to preserve — investigate, do not adjust the test.
```

That expectation is the point of the step: **`poll_state` is a refactor, not a change.** A failing
state test is a defect in the refactor.

**Rollback:** revert the whole commit (see §Rollback Plan — `git checkout` is wrong here).

**Agent:** **sonnet** · skills: `rust-best-practices` · knowledge: Phase 2 §Function Signatures +
Behaviour Matrix + §Error Handling + §"Why it is needed", `assets.rs:760-870`, `:1930-1990` and
`:2320-2470`, `CLAUDE.md` error rules.
**Rationale:** the only step needing judgement — preserving `poll_state` semantics exactly while
restructuring it, and getting `get_binary`'s four-way branch right. Not a haiku step.

---

### Step 4 — `AssetManager::get_binary_any_status`

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

### Step 5 — Core tests (Examples 1–3, U4–U10, I1)

**File:** `liquers-core/src/assets.rs` (`mod tests`, `:5472`, `use super::*` at `:5480`)

**Action:** land Examples 1–3 and U4–U10 + I1 from Phase 3. **Read Phase 3 §Verified Setup Facts
first** — it is the binding list, and the drafting pass invented APIs repeatedly. The essentials,
restated here so this step is self-contained:

- **The setup route is two moves, not one.** `try_fast_track` (declared at **`:634`**; it assigns
  `binary` and `data` at `:679`) refuses any stored status outside `Ready | Source | Override`
  (`:650-653`) — so it **cannot load an `Expired` entry**. The route is: store the bytes as
  `Ready` → `try_fast_track()` → **then** `expire()`. Fast-tracking `Ready` lands exactly where
  expiry is legal, since `expire()` accepts only `Ready`/`Override`.
- I1 must be **in-file**, not in `tests/` — `set_state` is `pub(crate)`.
- `AssetRef::set_binary` and `State::new_with_value` **do not exist**. `try_poll_binary` **already
  exists** (`:2456`) — it is gated, not added. `Status::all()` **does not exist**.
- `EnvRef::evaluate` *does* exist (`context.rs:278`) but returns after *submission*, not
  evaluation — with a queued manager you must still `get()` to wait.

Example 2 contributes **two** tests: `test_binary_recovery_across_layers` and
`test_manager_binary_recovery_skips_deserialization`. The second is the one that proves Step 4's
efficiency claim by storing a type this build cannot deserialize; do not drop it as redundant.

U9 (`get` on `Expired`) **must** wrap the call in `tokio::time::timeout` (Phase 3 uses 2 s), so a
regression fails the suite instead of hanging it.

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

### Step 6 — Integration tests (I2–I4)

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

### Step 7 — `liquers-axum` consumer fix

**File:** `liquers-axum/src/query/handlers.rs`

**Action:**
1. Replace the `_ =>` catch-all status arms — `:109` (GET) and `:216` (POST, an empty `_ => {}`) —
   with explicit arms for all fifteen statuses, per the no-default-match-arm rule.
2. `Expired` returns an **error response**. Do not re-request from the manager: re-evaluation
   belongs at the request boundary (`get_asset`/`get`), and a handler holding an `AssetRef` is past
   it. (Phase 1 §"Expiry is an error".)
3. `Error`/`Cancelled` keep their current handling. `Pending` statuses keep looping.
4. Search the file for any *other* status dispatch (`grep -n "match.*status" `) and confirm none
   retains a catch-all. The compiler enforces exhaustiveness for `match`, but an `if`/`else` chain
   on status would slip through silently.

Without this step the fix converts stale bytes into a 30-second timeout, because the catch-all
currently swallows `Expired` as "still processing".

**Validation:**
```bash
cargo check -p liquers-axum
cargo test -p liquers-axum
# NOTE: this proves the crate compiles and existing tests pass. It does NOT exercise the handler
# fix — liquers-axum has no handler test scaffolding. That gap is Step 8's decision, not something
# this command covers. Do not read a green result here as "the handler fix is verified".
```

**Rollback:** `git checkout liquers-axum/src/query/handlers.rs`

**Agent:** sonnet · skills: `rust-best-practices` · knowledge: Phase 2 §Integration Points, Phase 1
§"Expiry is an error", both handler loops.

---

### Step 8 — The axum test-coverage decision

**Carried from Phase 3 as an explicit choice, not a default.** `liquers-axum` has no handler test
scaffolding, and Step 7 sits over the layer where the bug actually bites.

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

### Step 9 — Documentation

**Files:** `liquers-core/src/assets.rs` module docs (`:100-116`), `specs/reference/ASSETS.md`

**Action:**
1. The module-level read-contract table currently documents the bug as intended behaviour — "A
   cached binary may be returned" for both `get_binary` and `poll_binary`. Replace with the Phase 2
   Behaviour Matrix, adding rows for the new methods.
2. `ASSETS.md` has **no** "Status and reads" section today (verified) — its nearest neighbours are
   `## Status Enum` (`:105`) and `### Status Properties` (`:145`). **Create** a new subsection
   under `## Status Enum` carrying the Behaviour Matrix and the `ReadExposure` classification.
   `### Status Properties` also tabulates per-status predicates and should gain a `read_exposure`
   column so the two do not disagree.
3. `ASSETS.md` requires a `## History` row (the table exists at `:751`) and a `reviewed:` bump
   **in the same commit** (`DOCS_STRUCTURE_GUIDE.md` §9.2).

**Validation:**
```bash
python3 scripts/docs_index.py --check
```

**Agent:** haiku · skills: none · knowledge: Phase 2 Behaviour Matrix, `DOCS_STRUCTURE_GUIDE.md` §9.2.

---

### Step 10 — Close out

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
| 3 (A+B) | **sonnet** | rust-best-practices | The only step needing judgement: preserve `poll_state` exactly while restructuring it, and get `get_binary`'s four-way branch right. Part B is one line but must ship in the same commit |
| 4 | haiku | rust-best-practices | Mirrors `get_any_status` minus one call |
| 5 | **sonnet** | liquers-unittest, rust-best-practices | Setup is the hard part; the drafting pass got it wrong repeatedly, so this needs a model that verifies APIs rather than assuming them |
| 6 | sonnet | liquers-unittest | Integration setup with recipes and managers |
| 7 | sonnet | rust-best-practices | Fifteen explicit arms plus a behaviour decision per status |
| 8 | sonnet if (a), none if (b) | — | Filing an issue needs no agent |
| 9 | haiku | — | Mechanical, against a matrix that already exists |
| 10 | haiku | — | Follows `DOCS_STRUCTURE_GUIDE.md` §4.8/§5.5/§8.1 |

Every agent needs `CLAUDE.md`'s hard rules in context: no `unwrap`/`expect` in library code, no
`_ =>` on Liquers enums, typed error constructors only, `eprintln!` never `println!`.

## Testing Plan

| When | Command | Gate |
|---|---|---|
| After Steps 1, 2 | `cargo test -p liquers-core --lib metadata` | classifier correct in isolation |
| After Step 3 | `cargo test -p liquers-core --lib` | **no existing state-read test regresses** |
| After Step 4 | `cargo check -p liquers-py` | trait default breaks no downstream implementor |
| After Step 5 | `cargo test -p liquers-core --lib` | new contract holds |
| After Step 6 | `cargo test -p liquers-core --lib --test expiration_integration --test manager_parametric` | routing + persistence unchanged, unit tests still green |
| After Step 7 | `cargo check -p liquers-axum` + `cargo test -p liquers-axum` | consumer compiles; handler behaviour **not** covered (Step 8) |
| **Final** | **`cargo test -p liquers-core`** | **every test this design adds** |
| Final, project | `cargo test -p liquers-lib --lib --tests` | the project's default loop (per `CLAUDE.md`) — a regression check on dependents, *not* on this feature |

**The final gate is `-p liquers-core`, not `-p liquers-lib`.** `CLAUDE.md` names the `liquers-lib`
loop as the project default, and it is — but Cargo does not run a *dependency's* tests, and every
test this design adds lives in `liquers-core`. Running only the default loop at the end would
verify nothing about this change. Run both.

Do **not** run `cargo test --workspace` — `CLAUDE.md` warns it exhausts the 30 GB disk allowance.

**The single most informative signal** is the Step 3 gate: `poll_state` is being restructured but
must not change. If a pre-existing state test fails there, the refactor is wrong — fix the code,
not the test.

## Rollback Plan

**`git checkout <file>` is only safe for a step whose file no other landed step has touched.**
Steps 3, 4 and 5 all modify `liquers-core/src/assets.rs`, so once two of them are in,
`git checkout liquers-core/src/assets.rs` discards both. Use `git revert <commit>` to undo one
step selectively.

| Scope | Action |
|---|---|
| A step whose file is untouched by later landed steps (1, 2, 6, 7) | `git checkout <file>` |
| Steps 3, 4, 5 (all in `assets.rs`) | `git revert <commit>` for the specific step — **not** `git checkout` |
| Step 3 | Revert as one commit. Parts A and B cannot be separated; splitting them opens the persistence window described in Part B. |
| Whole feature | Revert to before Step 1. Steps 1–2 are additive and safe to leave in place if only the behavioural change needs backing out — the classifier is dead code without Step 3. |

The design is **not** feature-gated. A read-contract fix behind a flag would mean shipping two
contracts, which is worse than shipping the fix.

## Risk Register

| Risk | Likelihood | Mitigation |
|---|---|---|
| `poll_state` refactor silently changes state reads | Medium | U4 asserts behaviour class for all 15 statuses; Step 3 gate treats a failing existing test as a defect |
| Persistence breaks for non-`Value` statuses | Medium | Step 3's parts land as one commit; I1 is the regression test. **Note Part A's own validation would pass while this is broken** — I1 does not exist until Step 5, which is why the parts are not separate steps |
| Axum handlers hang instead of erroring | High if Step 7 skipped | Step 7 is mandatory, not optional; and no test catches it, so review is the only gate. Do not cut a release between Steps 3 and 7 |
| `Cancelled` error wording diverges from `Error` | Low | U8 asserts error *identity*, not just failure |
| A future `Status` variant lands in the wrong bucket | Low | U2's exhaustive match makes it a compile error |

## Open Items Carried Forward

1. **Step 8's decision** — recommendation (b): accept review-only verification and file
   `AXUM-HANDLER-TEST-COVERAGE`. Stated as a recommendation because it trades coverage for scope;
   confirm it at execution time rather than treating it as settled.
2. `EXPIRATION-RECOVERY-WEB-API` may want to grow to cover `get_binary_any_status`. Affects that
   issue's scope, not this design's code.
3. Phase 3 named two things as untestable — `try_poll_binary` lock contention and the exact
   interleaving of expiry with an in-flight read. Both are pre-existing and unchanged by this work;
   recorded so nobody mistakes their absence for an oversight.
