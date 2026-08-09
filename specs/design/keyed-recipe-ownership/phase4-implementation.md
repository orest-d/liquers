# Phase 4: Implementation Plan - keyed-recipe-ownership

## Overview

Ten steps. Steps 1–3 are additive and cannot change behaviour; step 4 is the fix; steps 5–6 are the
invariant and the guard; steps 7–8 extend the guard to wasm and the browser; steps 9–10 are
bookkeeping and the full validation matrix.

Everything in steps 1–6 is in one file, `liquers-core/src/assets.rs`. Sequencing is chosen so that
each step ends with a green `cargo test -p liquers-core --lib --tests`, with one deliberate
exception — **step 4 adds code and tests together**, because its test cannot exist before its fix
(see §Sequencing constraint).

Total: ~200 lines of implementation, ~400 of tests, five `fixme` markers deleted.

---

## Implementation Steps

### Step 1 — Foundations

**File:** `liquers-core/src/assets.rs`

No import change: the `running_inline` field added in step 6 uses fully-qualified
`std::collections::HashSet`, matching its neighbours `assets` and `query_assets` (`:5415-5416`),
which are fully qualified even though `HashMap` is imported at `:199`. (Adding `HashSet` to that
import instead is equally fine — do one or the other, not a mix.)

Add beside `is_expired` (`:2392`):

```rust
/// Whether this asset is volatile: flagged before evaluation, or volatile as a final status.
///
/// Deliberately does not consult `Metadata::is_volatile()`, which is true for an `Override`
/// entry carrying the flag — the user-supplied override, which must stay reusable.
pub async fn is_volatile(&self) -> bool {
    let lock = self.data.read().await;
    lock.is_volatile || lock.status == Status::Volatile
}
```

**Validation:** `cargo check -p liquers-core`
**Risk:** none. Nothing calls it yet.

---

### Step 2 — `remove_key_asset_if`

**File:** `liquers-core/src/assets.rs`

Trait method with a default body next to `remove_key_asset` (`:3426`), plus an atomic override in
each manager (beside the existing `remove_key_asset` at `:4980` and `:5678`).

```rust
async fn remove_key_asset_if(&self, key: &Key, asset_id: u64) -> bool;   // default + 2 overrides
```

Signature and bodies as specified in Phase 2 §Trait Implementations.

**Tests:** T11 `remove_key_asset_if_respects_id` — insert A, replace with B, call with A's id,
assert `false` and that B survives.

**Validation:** `cargo test -p liquers-core --lib`
**Risk:** none. No existing call site changes.

---

### Step 3 — `owned_key_asset`

**File:** `liquers-core/src/assets.rs`

Trait method with a default body over `lookup_key_asset` + `is_volatile` + `remove_key_asset_if`,
per Phase 2. No override in either manager.

**Tests:** T7 registered owner returns `Some` with the same id · T8 `None` when unregistered ·
T9 a volatile entry yields `None` and is removed · **T10 the call does not evaluate** (a call
counter stays at 0 for a key that has a recipe).

T10 is the test that encodes the whole design; write it first and make it the one that would fail
loudest if someone later "optimizes" `owned_key_asset` back into `get`.

**Validation:** `cargo test -p liquers-core --lib`
**Risk:** none. Still nothing calls it.

---

### Step 4 — The fix: switch the ownership test

**Files:** `liquers-core/src/assets.rs`, `liquers-core/tests/manager_parametric.rs`,
`liquers-core/tests/payload_inheritance.rs`

In `evaluate_recipe` (`:1830-1836`), replace `manager.get(&key).await?` + the id `if` with the
three-arm match from Phase 2. **Move both existing bodies verbatim** — a diff that rewrites either
body is a diff that is doing something else.

Add to `manager_parametric.rs`:

- a `with_recipes` setup helper writing `recipes.yaml` into an `AsyncMemoryStore` and installing
  `DefaultRecipeProvider`, modelled on `payload_inheritance.rs:122-131`
- `scenario_keyed_eval`, instantiated as **T1** `keyed_eval_default` / `keyed_eval_immediate`
- **T2** `keyed_delegation_default` / `keyed_delegation_immediate`
- **T3** `keyed_eval_immediate_without_tokio_runtime`
- **T6** `volatile_keyed_recipe_evaluates_immediate`

In `payload_inheritance.rs`: **T4** invert `test_volatile_keyed_recipe_cycles_preexisting_defect`
(`:199`) into `test_volatile_keyed_recipe_evaluates`, deleting the panic branch that instructs a
reader to do exactly this; **T5** restore the `evaluate("-R/dash.txt")` assertion in
`test_keyed_recipe_requiring_payload_is_rejected` and trim the doc comment describing the detour.

**Validation, in this order:**

```bash
cargo test -p liquers-core --test manager_parametric keyed_eval_default   # queued first
cargo test -p liquers-core --test manager_parametric                      # then inline
cargo test -p liquers-core --test payload_inheritance
cargo test -p liquers-core --lib --tests
```

Queued first is not superstition: if the fix is wrong, the inline variant aborts the test binary
and tells you nothing, while the queued variant fails with an assertion that says what went wrong.

**Risk: medium — this is the step that can change behaviour.** The delegation arm now fires only
when an asset is registered *and* is not the caller, where before it fired whenever `get` returned
a different id. The behaviour change is intended and is what closes
`VOLATILE-KEYED-RECIPE-SELF-DELEGATION`; T2 is what proves the arm still fires when it should.

---

### Step 5 — Volatile is never served from a map

**File:** `liquers-core/src/assets.rs`

Add `Status::Volatile` to the stale-terminal `matches!` at all five sites: `:4467`, `:4154`,
`:4203`, `:5621`, `:5534` (Phase 2 §Volatile entries in the key map has the table). Extend the
adjacent comments — they enumerate the states in prose and would otherwise go stale.

**Tests:** T12 runtime-volatile asset recomputed on the next `get` · T13 the computing caller still
receives its value · T14 the same for a query asset.

**Validation:**

```bash
cargo test -p liquers-core --test volatility_integration
cargo test -p liquers-core --test expiration_integration
cargo test -p liquers-core --lib --tests
```

**Watch list.** These two suites are where a wrong reading of the invariant shows up:
`volatility_integration.rs::test_asset_manager_volatile_no_cache` (`:106`) should still pass and
arguably gets stronger; `expiration_integration.rs::test_immediately_expiring_command_is_volatile`
(`:61`) exercises precisely the runtime-volatility route this step changes and is the most likely
place to need its expectations updated. If it does, read carefully before editing it — a test that
asserted caching of a volatile value was asserting the defect.

**Risk: medium.** Behaviour changes for any asset that ends `Status::Volatile` while registered.

---

### Step 6 — Re-entrancy guard — **implemented, then reverted**

> **Outcome:** this step was completed as written and then rolled back. A manager-global id set
> cannot distinguish re-entrancy on one stack from two tasks legitimately awaiting the same asset;
> `liquers-web/tests/async_ASYNCQ.rs` fails with the guard in place, because a JavaScript `async`
> command yields and the second caller was refused. The rollback plan below anticipated this. The
> evidence and the correct fix live in `INLINE-PATH-LACKS-EXECUTE-ONCE`. What follows is the step
> as planned, kept because the reasoning is what the issue now builds on.


**File:** `liquers-core/src/assets.rs`

Add `running_inline: std::sync::Mutex<HashSet<u64>>` to `ImmediateAssetManager` (`:5410`) and to
`new()` (`:5432`). Add `InlineRunGuard<'a>` with its `Drop`, and the inherent
`try_enter_inline(&self, asset_id) -> Option<InlineRunGuard<'_>>`. Apply at `:5647` (`get`) and
`:5567` (`get_asset`), returning `Error::dependency_cycle(&DependencyKey::from(…))` on refusal.

The guard must **not** hold the `MutexGuard` — see Phase 2 §Data Structures. If the enclosing
future stops being `Send` on native, that is the mistake.

**Tests:** T15 `try_enter_inline_refuses_second_entry` · T16 `inline_guard_releases_on_error`.

**Validation:** `cargo test -p liquers-core --lib --tests`
**Risk: low.** On a correct step 4 the guard never fires in normal operation.

---

### Step 7 — wasm regression test

**File:** `liquers-web/tests/eval_EVAL.rs`

Add **T17** `eval07_keyed_evaluation_resolves`, per Phase 3. It needs a store, which this suite does
not currently configure; bring `configure_store_on` / `js_store_config` over from
`store_js_STORE.rs:493`.

**Validation:**

```bash
cargo clean
cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
```

`cargo clean` first: a different target, and the native `target/` is not shared usefully.
**Risk: low**, but this is the first execution on the real target — a wasm-only compile problem
(a stray `Send` bound, a `tokio::spawn`) surfaces here rather than earlier.

---

### Step 8 — Enable the browser tests

**File:** `liquers-web/tests/e2e/store.spec.ts`

Remove `fixme` from the five tests at `:87`, `:105`, `:189`, `:220`, `:247` (**T18–T22**), and
delete the block comment at `:7-16` that explains why they were disabled. Leave the surrounding
prose about why these live in Playwright rather than `wasm-bindgen-test` — that is still true.

**Validation:**

```bash
./liquers-web/examples-web/quickstart/build.sh
cd liquers-web/tests/e2e && npm install && npx playwright test
```

**Risk: low for the fix, moderate for the environment** — this is the only step needing a built
page and a browser. A failure here that is not a `-R/` failure is a harness problem, not a
regression; check that a non-`fixme` neighbour such as `STORE02 a missing URL is key_not_found`
(`:129`) still passes before concluding anything.

---

### Step 9 — Documentation and issue bookkeeping

| File | Change |
|---|---|
| `specs/issues/CORE-IMMEDIATE-MANAGER-KEYED-RECURSION.md` | `status: draft` → `closed`; set `design: keyed-recipe-ownership` |
| `specs/issues/VOLATILE-KEYED-RECIPE-SELF-DELEGATION.md` | same |
| `specs/reference/ASSETS.md` | document the ownership rule and the volatile-never-owned invariant; add a `## History` row and bump `reviewed:` in the same commit (§9.2) |
| `specs/README.md` | add the design folder to the capability map, next to `design/liquers-web-store/` (`:149`) |
| `specs/design/keyed-recipe-ownership/DESIGN.md` | `status: complete`, drop `phase:` |
| `specs/index.csv` | regenerate: `python3 scripts/docs_index.py` |

`closed` is the right terminal status: both issues are local (no `github:` number), so §4.3's
`draft → closed` transition applies and no GitHub issue is opened.

**Two new issues to file** (Phase 1 §Noted, not fixed here — CLAUDE.md requires filing, not
mentioning):

1. `CONTEXT-APPLY-BARE-KEY-ILL-DEFINED` — `Context::apply(&pure_key_query, state)` discards the
   input state and persists under the key with status `Ready`, which `try_fast_track` later
   accepts. P2, complexity S, area `[core/assets]`.
2. `INLINE-PATH-LACKS-EXECUTE-ONCE` — `RunClaim` gives the queued path execute-once;
   `run_with_future_inline` has only an `is_finished()` check, and `RunClaim` is wasm-gated and
   `JobQueue`-bound. Generalizing it to a queue-less form would subsume this design's re-entrancy
   guard. P2, complexity M, area `[core/assets, web]`.

Search `specs/index.csv` first, per §4.8.

**Validation:** `python3 scripts/docs_index.py` runs clean and the diff shows only expected rows.

---

### Step 10 — Full validation matrix

```bash
# native
cargo test -p liquers-core --lib --tests
cargo test -p liquers-lib  --lib --tests          # nothing changed here; proves nothing broke
cargo test -p liquers-lib  --test registry_export  # no commands changed — must stay green untouched

# wasm, after cargo clean
cargo clean
cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
./liquers-web/examples-web/quickstart/build.sh
./liquers-web/scripts/check-stubs.sh

# browser
cd liquers-web/tests/e2e && npx playwright test
```

`liquers-lib` is not modified but sits downstream of the changed trait; if the two defaulted
methods somehow broke an implementor, this is where it shows.

---

## Sequencing constraint

**A Rust stack overflow aborts the process — it is not a catchable failure.** `keyed_eval_immediate`
therefore cannot land before step 4: it would take the whole test binary down and `cargo test`
would report a signal, not an assertion. Code and test go in together.

This is the opposite of T4, which asserts the *broken* behaviour and is safe in the tree today.
The asymmetry is worth stating because "write the failing test first" is the normal instinct and
here it breaks the suite.

To confirm the new test genuinely reproduces the defect, run it once against unfixed code as a
throwaway (expect a crash, not a failure). Do not commit that state.

---

## Testing Plan

| When | Command | Gate |
|---|---|---|
| after each of steps 1–3 | `cargo check -p liquers-core`, then `cargo test -p liquers-core --lib` | compiles, unit tests green |
| step 4 | `--test manager_parametric keyed_eval_default`, then the whole suite | queued before inline, deliberately |
| step 5 | `--test volatility_integration`, `--test expiration_integration` | the watch list above |
| step 6 | `cargo test -p liquers-core --lib --tests` | guard unit tests green, nothing else moved |
| step 7 | wasm suite after `cargo clean` | first run on the real target |
| step 8 | Playwright | five previously-disabled tests pass |
| step 10 | the full matrix | everything, including untouched crates |

Coverage by layer: 10 unit, 6 integration, 1 wasm, 5 e2e — the inventory is Phase 3's overview
table, and each test there names the step that introduces it.

**Disk.** `cargo clean` between the native and wasm loops is not optional here (CLAUDE.md
§Building and testing): the two targets together exceed the session allowance. Budget ~3 minutes
for a cold `liquers-core` build.

---

## Agent Assignment

The whole change is ~200 lines in one file plus tests, so a single implementer working the steps in
order is the realistic execution. The table is what to use if the work is split.

| Step | Model | Skills | Knowledge it must load |
|---|---|---|---|
| 1–3 | haiku | `rust-best-practices` | Phase 2 §Trait Implementations, §Function Signatures; `assets.rs:2380-2400`, `:3420-3435`, `:4970-4990`, `:5660-5685` |
| 4 | **sonnet** | `rust-best-practices`, `liquers-unittest` | all of Phase 2 and Phase 3; `assets.rs:1826-1885`; `manager_parametric.rs` entire; `payload_inheritance.rs:88-247` |
| 5 | sonnet | `rust-best-practices` | Phase 2 §Volatile entries in the key map; the five sites; both watch-list suites |
| 6 | sonnet | `rust-best-practices` | Phase 2 §Data Structures and the `RunClaim` rationale; `assets.rs:5028-5140`, `:5410-5445` |
| 7 | sonnet | `liquers-unittest` | Phase 3 §wasm Tests; `liquers-web/tests/eval_EVAL.rs`, `tests/common/mod.rs`, `store_js_STORE.rs:490-500`; `liquers-web/README.md` for the loops |
| 8 | haiku | — | `store.spec.ts:1-30` and the five sites |
| 9 | sonnet | — | `specs/DOCS_STRUCTURE_GUIDE.md` §4.3, §4.8, §5.1, §9.2; `specs/README.md:140-152` |
| 10 | haiku | — | CLAUDE.md §Building and testing |

Step 4 is the one to give the strongest model: it is the only step where a plausible-looking diff
can be wrong in a way the type system will not catch, and the failure mode on the inline path is a
process abort rather than a message.

---

## Rollback Plan

Each step is one commit, and steps 1–3 are additive — reverting any single step leaves a compiling,
passing tree.

| Step | If it goes wrong | Rollback |
|---|---|---|
| 1–3 | compile error | revert the commit; nothing depends on it |
| **4** | keyed evaluation breaks, or the inline test aborts | `git revert` restores the `manager.get` ownership test. The tests added in the same commit go with it — that is why they are in the same commit. Partial rollback is not available and should not be attempted: the tests do not compile against the old predicate. |
| 5 | a volatile test fails and the expectation is genuinely wanted | revert this commit alone; step 4 does not depend on it. `owned_key_asset` keeps its own volatility check, so the ownership fix stays correct — only the map-eviction half is lost. |
| 6 | the guard misfires, or the future stops being `Send` | revert this commit alone. Nothing depends on it; the recursion is already fixed by step 4. This is the cheapest thing in the change to abandon. |
| 7–8 | test-only | revert or re-`fixme`. A failure here after a green step 4 points at the test harness, not the fix. |
| 9 | doc bookkeeping | revert and re-run `scripts/docs_index.py` |

**The one-way part:** step 4 changes a behaviour two issues describe as broken. If it turns out
some caller depends on the old delegation-on-every-volatile-key behaviour, that is a design
question, not a rollback — reverting reinstates a P1 wasm crash.

---

## Review Findings

Four conformity passes were run inline against Phases 1–3 and the codebase.

**Phase 1.** Scope matches exactly: ownership test, both managers, volatile case, the invariant,
the guard, regression tests on both targets. The two "noted, not fixed" items are filed as issues
in step 9 rather than dropped, as CLAUDE.md requires. `LIB-RECIPE-PROVIDER-PANIC` stays untouched.

**Phase 2.** Every signature in §Function Signatures has a step: `is_volatile` step 1,
`remove_key_asset_if` step 2, `owned_key_asset` step 3, the `evaluate_recipe` match step 4,
`try_enter_inline` + `InlineRunGuard` step 6. All five stale-terminal sites are in step 5. Nothing
in Phase 2 is unimplemented, and no step adds anything Phase 2 does not specify.

**Phase 3.** All 22 checks are placed: T7–T11 steps 2–3, T1–T6 step 4, T12–T14 step 5, T15–T16
step 6, T17 step 7, T18–T22 step 8. The sequencing constraint is carried into the plan rather than
left in Phase 3.

**Codebase.** Line references re-checked against HEAD. `specs/command_registry.yaml` needs no
regeneration (no command changes), and `registry_export` is in the step-10 matrix to prove it.
`scripts/docs_index.py` is the generator for `index.csv` (§3), so step 9 regenerates rather than
edits it. Both issues lack a `github:` number, so `draft → closed` is available; neither has a
`status` that GitHub owns.

**One residual uncertainty, unchanged from Phase 3.** T2's delegation setup has no precedent in the
tree. If `create_asset` + `run` does not reach the delegation arm, the fallback is `Context::apply`
with a bare key — the path step 9 files as ill-defined. This is the only place where the plan may
need to adapt during execution, and it affects a test, not the fix.

## References

- Phase 1: `./phase1-high-level-design.md` · Phase 2: `./phase2-architecture.md` ·
  Phase 3: `./phase3-examples.md`
- `specs/DOCS_STRUCTURE_GUIDE.md` §4.3 (issue status), §4.8 (filing), §5.1 (design status),
  §9.2 (reference-document history)
- `CLAUDE.md` §Building and testing — the disk budget and the per-crate loops
