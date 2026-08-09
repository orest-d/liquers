# Phase 3: Examples & Use-cases - keyed-recipe-ownership

## Example Type

**Runnable.** Everything below is a test that lands in the tree. This is a defect fix, so the
"examples" are the scenarios that stop failing, and each is written as the test that proves it —
the issue's own closing line is that no regression guard exists and that absence is why the defect
survived.

## Overview Table

| # | Name | Where | Kind | What it proves |
|---|---|---|---|---|
| **E1** | keyed evaluation in the browser | `liquers-web/tests/e2e/store.spec.ts` | e2e | `-R/` works end to end through a real store in a real browser |
| **E2** | volatile keyed recipe produces a value | `liquers-core/tests/payload_inheritance.rs` | integration | closes `VOLATILE-KEYED-RECIPE-SELF-DELEGATION` |
| **E3** | a runtime-volatile result is recomputed | `liquers-core/src/assets.rs` | unit | volatile is used once, never reused |
| T1 | `keyed_eval_{default,immediate}` | `tests/manager_parametric.rs` | integration | the fix, on both managers — the missing coverage |
| T2 | `keyed_delegation_{default,immediate}` | `tests/manager_parametric.rs` | integration | the delegation arm still fires when another asset owns the key |
| T3 | `keyed_eval_immediate_without_tokio_runtime` | `tests/manager_parametric.rs` | integration | keyed evaluation (incl. persistence) is spawn-free |
| T4 | `test_volatile_keyed_recipe_evaluates` | `tests/payload_inheritance.rs` | integration | E2 — inverts the test that asserts the defect |
| T5 | `test_keyed_recipe_requiring_payload_is_rejected` | `tests/payload_inheritance.rs` | integration | re-enables the `evaluate()` path the defect blocked |
| T6 | `volatile_keyed_recipe_evaluates_immediate` | `tests/manager_parametric.rs` | integration | E2 on the inline manager |
| T7 | `owned_key_asset_returns_registered_owner` | `assets.rs` unit | unit | `Some(self)` for a registered key |
| T8 | `owned_key_asset_none_when_unregistered` | `assets.rs` unit | unit | `None` is an answer, not an error |
| T9 | `owned_key_asset_evicts_volatile_entry` | `assets.rs` unit | unit | a volatile entry is not an owner, and is dropped |
| T10 | `owned_key_asset_does_not_evaluate` | `assets.rs` unit | unit | **the core property** — a call counter stays at 0 |
| T11 | `remove_key_asset_if_respects_id` | `assets.rs` unit | unit | a slow caller cannot evict a replacement |
| T12 | `runtime_volatile_asset_is_recomputed` | `assets.rs` unit | unit | E3 — second `get` re-runs the command |
| T13 | `runtime_volatile_value_is_returned_once` | `assets.rs` unit | unit | eviction is on entry; the computing caller still gets its value |
| T14 | `runtime_volatile_query_asset_is_recomputed` | `assets.rs` unit | unit | same rule on the query map |
| T15 | `try_enter_inline_refuses_second_entry` | `assets.rs` unit | unit | the guard refuses re-entry and releases on drop |
| T16 | `inline_guard_releases_on_error` | `assets.rs` unit | unit | an early return clears the id |
| T17 | `EVAL07 keyed evaluation resolves` | `liquers-web/tests/eval_EVAL.rs` | wasm | `-R/` under the real wasm target, Node loop |
| T18–T22 | five `test.fixme` markers removed | `liquers-web/tests/e2e/store.spec.ts` | e2e | E1 — the guard the issue asked for, already written |

Twenty-two checks; five of them (T18–T22) already exist and only need un-disabling.

---

## Example 1: keyed evaluation in the browser (E1)

The scenario the issue opens with. Today `env.evaluate('-R/…')` exhausts the wasm stack, the
instance dies, and the `Promise` never settles.

```ts
// liquers-web/tests/e2e/store.spec.ts — existing test, `fixme` removed
test('STORE07 a fetched resource evaluates end to end', async ({ page }) => {
  const text = await page.evaluate(async () => {
    const env = /* configured with a fetch store over fixtures/ */;
    return await env.evaluate('-R/data/input.txt/-/to_text');
  });
  expect(text).toBe('hello from the fixture');
});
```

Note `/-/`: without it the whole tail is consumed as a key and the query fetches a file *named*
`to_text`. Both forms parse, so this is checked by reading, not by the validator. All queries in
this document were run through `liquers-validate` (4/4 ok).

**What changes:** `evaluate_recipe` asks the manager who owns `data/input.txt` with a map read
instead of an evaluation, finds itself, and evaluates its own recipe once.

## Example 2: a volatile keyed recipe produces a value (E2)

```rust
register_command!(cr, fn vol_cmd() -> result volatile: true)?;
// recipes.yaml: { query: "vol_cmd/dash.txt" }

let asset = envref.evaluate("-R/dash.txt").await?;
assert_eq!(asset.get().await?.try_into_string()?, "vol");
```

Today this fails with `Dependency cycle detected involving '-R/dash.txt'`: the manager mints a
fresh asset for a volatile key on every call, so the id comparison never matches and the asset
delegates to itself. With `owned_key_asset`, a volatile key has no registered owner, `None` means
"evaluate it here", and the delegation never happens.

`liquers-core/tests/payload_inheritance.rs:199` currently asserts the *broken* behaviour and
panics with instructions to invert it. T4 is that inversion; T5 re-enables the `evaluate()` path in
the sibling test that had to route around it.

## Example 3: a runtime-volatile result is recomputed (E3)

A key whose recipe is not volatile, evaluated by a command that marks its own result volatile
through metadata expiry:

```rust
let m = envref.get_asset_manager();
let a1 = m.get(&key).await?;                     // computes; ends Status::Volatile
assert_eq!(a1.get().await?.try_into_string()?, "tick-1");
assert_eq!(CALLS.load(Ordering::SeqCst), 1);     // and the value IS returned

let a2 = m.get(&key).await?;                     // entry evicted, recomputed
assert_eq!(a2.get().await?.try_into_string()?, "tick-2");
assert_eq!(CALLS.load(Ordering::SeqCst), 2);
assert_ne!(a1.id(), a2.id());
```

Today the second `get` returns `a1` from the map — `Status::Volatile.is_finished()` is `true` and
the expiry re-check is gated on `Status::Ready`, so nothing evicts it, ever. This is the reuse hole
Phase 2 identified; registration cannot predict it because the expiry is set during evaluation.

---

## Corner Cases

### 1. Memory

`running_inline` holds one `u64` per asset whose inline run is in progress — bounded by evaluation
depth, not by the number of assets. `InlineRunGuard::drop` removes the id on every exit path
including unwind, so a failed or cancelled run cannot leak an entry (T16). No `Arc` is retained:
the guard borrows the set, so it cannot keep the manager alive.

`owned_key_asset` clones an `AssetRef` (an `Arc` pair) on the hit path and nothing on the miss
path. The call it replaces built and possibly registered a whole asset, so this strictly reduces
allocation.

### 2. Concurrency

- **Two callers resolve the same key at once.** Unchanged: both managers insert under one atomic
  operation (`entry_async().or_insert_with()` on `scc`, double-checked insert under the `Mutex`),
  so exactly one asset is registered and both ownership tests agree on it. Covered by the existing
  `immediate_concurrent_same_query_runs_once`; T1 adds the keyed counterpart.
- **A slow caller evicting a replacement.** `owned_key_asset` reads the map, releases the lock,
  awaits the volatility check, then removes. Between those, another thread may register a new asset
  for the key. `remove_key_asset_if(key, id)` is why that is safe (T11) — it is exactly the
  compare-before-remove the `get` loops already inline.
- **Lock ordering.** `is_volatile()` takes `data.read()`, possibly on the calling asset.
  `tokio::sync::RwLock` is write-preferring, so a re-entrant read behind a queued writer deadlocks.
  At the ownership test no lock is held (`initial_state_and_recipe` releases inside its own block),
  and the call being replaced reached the same lock, so this is a preserved property rather than a
  new risk — recorded because moving the test earlier would break it.
- **The guard is per-manager, not per-stack.** On native, immediate mode may be driven from more
  than one task. Refusing a second inline run of an asset already running elsewhere is correct
  independently of re-entrancy: it is the execute-once property `run_with_future_inline`'s
  `is_finished()`-only check does not provide.

### 3. Errors

- A refused claim returns `Error::dependency_cycle(&DependencyKey::from(key))` —
  `ErrorType::DependencyCycle`, carrying the key. The point is that a diagnosable typed error
  replaces `RuntimeError: memory access out of bounds`, after which the wasm instance is dead and
  the caller's `Promise` never settles.
- A keyed asset whose key has **no** recipe is unaffected: `recipe.key()` still returns the key,
  `owned_key_asset` still finds the registered asset, and the recipe-provider lookup fails exactly
  as before. This fix does not touch `LIB-RECIPE-PROVIDER-PANIC`, which sits two lines further on.
- Neither new method returns `Result`, so no error path is added to `evaluate_recipe`. One is
  removed: the `?` on `manager.get`.

### 4. Serialization

Nothing new is serialized. `running_inline` is runtime-only. Stored metadata is untouched, so no
store written by an older build becomes unreadable and no format version moves.

The one adjacent behaviour deliberately left alone: a volatile asset still persists with
`Status::Volatile`, and `try_fast_track` still accepts only `Ready | Source | Override`, so the
stored copy is an override opportunity and never a value that gets read back (Phase 1, decision on
persistence).

### 5. Integration (cross-crate)

- `liquers-py`: no signature moves; both new trait methods are defaulted. Nothing to update.
- `liquers-lib`: `specs/command_registry.yaml` unchanged — no command is added, so
  `cargo test -p liquers-lib --test registry_export` should stay green untouched.
- `liquers-axum`: unaffected; the manager is reached through the same trait.
- `liquers-web`: no source change. Five e2e `fixme` markers and one new wasm test.

---

## Test Plan

### Unit Tests — `liquers-core/src/assets.rs` `#[cfg(test)] mod tests`

Ten tests, all `#[tokio::test]`, using the fixtures already in that module.

**T7–T11 — `owned_key_asset` and `remove_key_asset_if`**

```rust
/// The core property, and the one the defect violated: asking who owns a key must not
/// start an evaluation. A call counter is the only honest way to assert "nothing ran".
#[tokio::test]
async fn owned_key_asset_does_not_evaluate() -> Result<(), Error> {
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    // env with a recipe at `dash.txt` whose command bumps CALLS
    let m = envref.get_asset_manager();
    let owner = m.owned_key_asset(&key).await;
    assert!(owner.is_none(), "nothing is registered before the first get");
    assert_eq!(CALLS.load(Ordering::SeqCst), 0, "ownership query must not evaluate");
    Ok(())
}
```

T7 registers the key first (`m.get(&key)`) and asserts `Some` with the same id. T8 asserts `None`
for a key never requested. T9 puts a volatile asset in the map and asserts `None` *and* that the
entry is gone. T11 inserts asset A, calls `remove_key_asset_if(key, A.id())` after replacing the
entry with B, and asserts `false` and that B survives.

**T12–T14 — volatile is used once, never reused**

T12 is E3 above. T13 asserts the same first request returns the value with `CALLS == 1` — the
regression that a careless fix would introduce is evicting *before* returning, which would make
volatile keys produce nothing. T14 repeats T12 for a query asset, covering the three query-map
sites Phase 2 added the variant to.

**T15–T16 — the re-entrancy guard**

```rust
#[tokio::test]
async fn try_enter_inline_refuses_second_entry() {
    let m = ImmediateAssetManager::<TestEnv>::new();
    let first = m.try_enter_inline(7);
    assert!(first.is_some());
    assert!(m.try_enter_inline(7).is_none(), "re-entry must be refused");
    assert!(m.try_enter_inline(8).is_some(), "a different asset is unaffected");
    drop(first);
    assert!(m.try_enter_inline(7).is_some(), "the id is released on drop");
}
```

T16 drives a `get` whose `run_inline` returns `Err` and asserts a later `get` for the same key is
not refused — i.e. the guard released on the error path.

### Integration Tests — `liquers-core/tests/manager_parametric.rs`

This file is where the defect should have been caught: it runs the same scenarios against both
managers, and every scenario in it is non-keyed. Its own no-runtime test even says so — *"(Non-keyed
query ⇒ no persistence.)"* Adding a keyed scenario closes the structural gap, not just the bug.

```rust
/// Keyed evaluation through a recipe, run against BOTH managers.
///
/// Under `ImmediateAssetManager` this is the `CORE-IMMEDIATE-MANAGER-KEYED-RECURSION`
/// reproducer: before the fix it recurses until the stack is exhausted.
async fn scenario_keyed_eval<E>(envref: EnvRef<E>) -> Result<(), Error>
where E: Environment<Value = Value>
{
    let asset = envref.get_asset_manager().get(&parse_key("dash.txt")?).await?;
    let state = asset.get().await?;
    assert_eq!(state.try_into_string()?, "hello");
    Ok(())
}
```

Instantiated as `keyed_eval_default` and `keyed_eval_immediate` (T1). The existing helpers need one
addition — a `with_recipes` setup that writes `recipes.yaml` into an `AsyncMemoryStore` and
installs `DefaultRecipeProvider`, following `payload_inheritance.rs:122-131`.

T2 (`keyed_delegation_*`) covers the arm the fix must **not** break: build an untracked asset from
the same key recipe via `create_asset(key.into())` after the key is registered, run it, and assert
it produced the registered owner's value and recorded a dependency rather than evaluating twice
(`CALLS == 1`).

T3 extends `immediate_runs_without_tokio_runtime` to a keyed query under
`futures::executor::block_on`. Keyed evaluation persists, and persistence on the inline path is
synchronous by design (`persist_with_status_tracking`, `:1433`); a reintroduced `tokio::spawn`
anywhere on that path panics with "no reactor running". The existing test cannot catch that because
its query is non-keyed.

T6 puts the volatile keyed recipe through the inline manager, so both P1 issues are covered on both
managers.

### Integration Tests — `liquers-core/tests/payload_inheritance.rs`

T4 inverts `test_volatile_keyed_recipe_cycles_preexisting_defect` (`:199`) into
`test_volatile_keyed_recipe_evaluates`, asserting the value and deleting the panic branch that
tells a future reader to do exactly this. T5 restores the `evaluate("-R/dash.txt")` assertion in
`test_keyed_recipe_requiring_payload_is_rejected` and trims the doc comment that explains the
detour. Both are prescribed by `VOLATILE-KEYED-RECIPE-SELF-DELEGATION` §Verification.

### wasm Tests — `liquers-web/tests/eval_EVAL.rs`

T17, `EVAL07`, runs in the Node loop (no browser, no chromedriver):

```rust
#[wasm_bindgen_test]
async fn eval07_keyed_evaluation_resolves() {
    fresh();
    register_fixture_commands();
    configure_store_on(js_store_config("mem", "fixtures")).expect("configure");
    // write greeting.txt into that store, then:
    let envref = shared_env().expect("env");
    let query = parse_query("-R/greeting.txt/-/to_text").expect("parse");
    let asset = get_asset_for(envref, query).await.expect("keyed evaluation");
    assert_eq!(asset./* state */.try_into_string().expect("string"), "hello");
    reset_global();
}
```

The helpers are the ones already used in this crate: `fresh` / `register_fixture_commands` from
`tests/common/mod.rs` (`:94`, `:121`), `get_asset_for` from `liquers_web::asset`, `shared_env` /
`reset_global` from `liquers_web::environment`, and the in-memory store configuration from
`store_js_STORE.rs:493`. `eval_EVAL.rs` currently registers commands but configures no store, so
the store setup crosses over from the STORE suite — the one new thing this test needs.

This is the cheapest guard that runs on the real target, and the only one in the routine loop —
the e2e suite needs a built page and a browser.

### End-to-end — `liquers-web/tests/e2e/store.spec.ts`

T18–T22: remove `fixme` from `STORE07 a fetched resource evaluates end to end` (`:87`),
`STORE07 a nested fetched key resolves` (`:105`), `STORE07 a localStorage resource survives a
reload` (`:189`), `STORE07 a JavaScript store evaluates end to end` (`:220`), and
`STORE11 fetch and localStorage coexist in one configuration` (`:247`). The block comment at
`:7-16` explaining why they are disabled goes with them.

### Manual Validation

```bash
# native loop — core only; liquers-lib is not touched but shares the target dir
cargo test -p liquers-core --lib --tests

# the two suites that matter most, by name
cargo test -p liquers-core --test manager_parametric
cargo test -p liquers-core --test payload_inheritance

# wasm, after cargo clean (different target)
cargo clean
cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles

# the delivery form
./liquers-web/examples-web/quickstart/build.sh
cd liquers-web/tests/e2e && npm install && npx playwright test
```

### Sequencing constraint — read before writing T1

**A Rust stack overflow aborts the process; it is not a catchable failure.** So
`keyed_eval_immediate` cannot be landed before the fix — it would take the whole test binary down
rather than fail one case, and `cargo test` would report a signal, not an assertion. It must go in
the same commit as the fix.

This is the opposite of T4, which asserts the *broken* behaviour and is therefore safe to have in
the tree today. The asymmetry is worth stating because "write the failing test first" is the normal
instinct and here it breaks the suite.

To confirm the new test genuinely reproduces the defect, run it once against unfixed code as a
one-off (expect a crash, not a failure) and discard the result. Do not commit that state.

---

## Review Checklist

- [x] Overview table present, every example and test named with what it proves
- [x] Examples are runnable and cover the primary (E1), secondary (E2) and edge (E3) scenarios
- [x] Corner cases: memory, concurrency, errors, serialization, cross-crate
- [x] Unit + integration + wasm + e2e coverage; error paths included (T16, §Errors)
- [x] All queries validated — `liquers-validate`, 4/4 ok, no spaces or special characters
- [x] `-R/` queries have a store: `AsyncMemoryStore` natively, the configured web store on wasm
- [x] Commands used are registered in the test environment; no `liquers-lib` namespace needed
- [x] Signatures match Phase 2 (`owned_key_asset`, `remove_key_asset_if`, `is_volatile`,
      `try_enter_inline`)

## Review Findings

Three passes were run inline — Phase 1 conformity, Phase 2 conformity, codebase and query
validation.

**Phase 1 conformity.** Every decision has a test: self-evaluation when unregistered (T8 + T1),
volatile never owned (T9, T12–T14), persistence untouched (§Serialization, no test needed because
nothing changes), the re-entrancy guard (T15–T16). The regression guard Phase 1 promised — the five
`fixme` cases plus a wasm test — is T17–T22.

**Phase 2 conformity.** Signatures match; `owned_key_asset` is asserted to return `Option` and never
to evaluate (T10), which is the property the whole design turns on. The three-arm match is covered
arm by arm: `Some(self)` T7/T1, `Some(other)` T2, `None` T8/E2. All five stale-terminal sites are
reached (T12 key map, T14 query map).

**Codebase and queries.** `manager_parametric.rs` and `payload_inheritance.rs` exist with the
fixtures needed; the only new helper is a `with_recipes` store setup, modelled on
`payload_inheritance.rs:122-131`. `ImmediateEnvironment` exposes `with_async_store` and
`with_recipe_provider` (`context.rs:1020`, `:1025`), so the inline manager is fully testable on
native — which is what makes T1 possible at all. Queries: 4/4 validated.

One gap accepted: **T2's delegation scenario has no pre-existing example in the tree**, so its
setup is the least certain part of this plan. If `create_asset` + `run` turns out not to reach
`evaluate_recipe`'s delegation arm, fall back to asserting the arm through `Context::apply` with a
bare key — noting that Phase 1 already records that path as ill-defined and to-be-filed.

## References

- Phase 1: `./phase1-high-level-design.md` · Phase 2: `./phase2-architecture.md`
- `liquers-core/tests/manager_parametric.rs` — the parametric harness, and the gap
- `liquers-core/tests/payload_inheritance.rs:188-247` — the test that asserts the defect
- `liquers-web/tests/e2e/store.spec.ts:1-28` — why five tests are disabled
- `specs/guides/UNITTEST_GUIDE.md`, `.claude/skills/liquers-unittest/references/test-patterns.md`
