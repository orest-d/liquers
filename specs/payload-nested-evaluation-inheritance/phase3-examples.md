# Phase 3: Examples & Use-cases - Payload Inheritance in Nested Evaluation

## Example Type

**Conceptual code.** Runnable prototypes are not possible here: the feature *is* a change to
`liquers-core` evaluation, so nothing can run until Phase 4 implements it. Every snippet below is
written to become a real test or a real call site in Phase 4 — they use only APIs that exist today
plus the ones Phase 2 specifies, so they can be pasted in and compiled once the implementation lands.

## Overview Table

| # | Type | Name | What it demonstrates / checks |
|---|---|---|---|
| E1 | Example | Nested UI evaluation | The motivating case: a payload-requiring command reached through `Context::evaluate` now receives the parent's payload |
| E2 | Example | Declaring the requirement | `payload: required` in `register_command!`, and the migration hazard for commands that omit it |
| E3 | Example | Boundaries and errors | Keyed recipe rejection, missing-payload error, and a payload-free child staying cached |
| U1 | Unit | `PayloadRequirement` semantics | `join` over all combinations, `is_required`/`is_none`, `Default` |
| U2 | Unit | Serialization compatibility | Absent field defaults; `None` is not emitted; round-trip for `CommandMetadata` and `Plan` |
| U3 | Unit | `PlanBuilder` local detection | Command requirement lands on the plan; link parameters propagate; `volatile` implied |
| U4 | Unit | Plan splitting | Both halves inherit `payload_required` (`plan.rs:1599,1607`) |
| U5 | Unit | Macro codegen | `payload: required` sets both `payload_required` and `volatile`; unknown ident is a compile error |
| I1 | Integration | Inheritance through `evaluate` | Rewrite of `test_payload_not_inherited_in_nested_evaluation` |
| I2 | Integration | Inheritance through `get_dependency_state` and `apply` | The other two nested entry points |
| I3 | Integration | Payload-free child stays shared | Child goes through the manager, is cached and reused |
| I4 | Integration | Missing payload is an error | Payload-required nested query with no payload in context |
| I5 | Integration | Keyed recipe rejection | A recipe whose plan requires payload fails at plan build |
| I6 | Integration | Cycle guard | `a → b → a` under payload fails with `dependency_cycle` |
| I7 | Integration | Dependency-graph invariants | Payload asset is not a registered dependency but does record its own |
| I8 | Integration | Inline manager parity | Same inheritance semantics on `ImmediateAssetManager` |
| C1-C5 | Corner cases | See below | Concurrency, payload cloning, deep nesting, immediacy, wasm |

## Test Infrastructure Gap (blocks I8)

**There is no inline environment with a payload.** `liquers-core` provides:

| Environment | Payload | Asset manager |
|---|---|---|
| `SimpleEnvironment<V>` (`context.rs:702`) | `()` | `DefaultAssetManager` (queued) |
| `ImmediateEnvironment<V>` (`context.rs:846`) | `()` | `ImmediateAssetManager` (inline) |
| `SimpleEnvironmentWithPayload<V, P>` (`context.rs:956`) | `P` | `DefaultAssetManager` (queued) |
| — missing — | `P` | `ImmediateAssetManager` |

`liquers-lib::DefaultEnvironment<V, P>` does not fill it either: its `SelectedAssetManager` is
**cfg-selected at compile time** (`environment.rs:20-22` — `DefaultAssetManager` on native,
`ImmediateAssetManager` on wasm), so a native test cannot choose the inline manager.

Phase 1's verification list requires "both queued and inline asset managers", so this must be
addressed. **Recommendation: add `ImmediateEnvironmentWithPayload<V, P>` to `liquers-core`**,
mirroring `SimpleEnvironmentWithPayload` with `type AssetManager = ImmediateAssetManager<Self>` and
no spawning in `init_with_envref`. It is a small, mechanical addition, generally useful beyond this
feature, and it is the only way to exercise the wasm-compatible path natively.

*Alternative if that is unwanted:* define a test-local environment inside the integration test file.
Cheaper to add, but it duplicates environment boilerplate and leaves real users without an inline
payload environment.

**This is an addition to the Phase 2 architecture, discovered here.** Phase 4 must include it.

## Example 1: Nested UI evaluation (the motivating case)

**Scenario:** A UI container command renders a child element by evaluating a nested query. The child
command needs the UI handle from the payload.

**Context:** Exactly the pattern `liquers-lib/src/ui/commands.rs` uses today, which currently fails
in nested position.

```rust
type CommandEnvironment = SimpleEnvironmentWithPayload<Value, TestPayload>;

// Child: needs the payload.
fn child_cmd(_state: &State<Value>, window_id: WindowId) -> Result<Value, Error> {
    Ok(Value::from(format!("window:{}", window_id.0)))
}

// Parent: also needs the payload, and evaluates the child as a nested query.
async fn parent_cmd(
    _state: State<Value>,
    user_id: UserId,
    context: Context<CommandEnvironment>,
) -> Result<Value, Error> {
    let nested = parse_query("/-/child_cmd")?;
    let child = context.evaluate(&nested).await?;
    let child_text = child.get().await?.value_state()?.try_into_string()?;
    Ok(Value::from(format!("parent:{}|child:{}", user_id.0, child_text)))
}

register_command!(cr, async fn parent_cmd(state, user_id: UserId injected, context) -> result
    payload: required
)?;
register_command!(cr, fn child_cmd(state, window_id: WindowId injected) -> result
    payload: required
)?;

let asset = envref.evaluate_immediately("/-/parent_cmd", TestPayload::new("laura", 777)).await?;
assert_eq!(asset.get().await?.try_into_string()?, "parent:laura|child:window:777");
```

**Before this change:** `"parent:laura|child:None"` — the child fails because the payload is not
inherited (the current `test_payload_not_inherited_in_nested_evaluation` asserts exactly this).

**After:** `"parent:laura|child:window:777"`.

## Example 2: Declaring the requirement, and the migration hazard

**Scenario:** What an author writes, and what happens when they forget.

```rust
// Correct: declared.
register_command!(cr, fn get_user_id(state, user_id: UserId injected) -> result
    payload: required
)?;

// Also needs it — payload read through `context`, which no argument reveals.
// This is the case an argument-level keyword could never have caught (Phase 1 D2).
register_command!(cr, fn get_context_data(state, context) -> result
    payload: required
)?;
```

**The hazard, stated precisely.** A payload-reading command *without* the declaration:

| Position | Behavior |
|---|---|
| Top level via `evaluate_immediately` | **Works** — payload is installed on the ad-hoc asset directly |
| Nested via `Context::evaluate` | **Silently payload-free** — routed to the manager path, injection fails at runtime |

This asymmetry is why the migration audit below is not optional: the failure is invisible in the
common case and only appears when the command is reused as a dependency.

**Interaction with `volatile`:** the declaration also sets `volatile` (Phase 1 D7), so an author who
writes `payload: required` does not additionally write `volatile: true`. Writing both is harmless and
idempotent.

## Example 3: Boundaries and errors

```rust
// (a) Keyed recipes are a payload boundary — this recipe is rejected at plan build.
//     Stored at key "recipes/dash.yaml": query "-/get_user_id"
//     -> Error: keyed recipe requires a payload
let result = envref.evaluate("dash").await;
assert!(result.is_err());

// (b) Payload-required query, no payload in context -> error, not silent failure.
let result = envref.evaluate("/-/get_user_id").await;   // evaluate(), not evaluate_immediately()
assert!(result.is_err());

// (c) A payload-free child of a payload-requiring parent stays a normal cached asset.
fn pure_child(state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(format!("{}->pure", state.try_into_string()?)))
}
register_command!(cr, fn pure_child(state) -> result)?;   // no payload: required
// Evaluated through the manager, cached, shared, and a registered dependency.
```

**Why (a) matters:** keys are global, payloads are per-evaluation. Rejecting at plan build makes the
limitation explicit at authoring time rather than producing a confusing runtime injection failure.

## Corner Cases

### C1. Concurrency — two payloads, same query

Two tasks evaluate `/-/get_user_id` concurrently with different payloads. Because
`payload ⟹ volatile`, each resolves through `get_volatile_query_asset`, which builds a **fresh,
unshared** `AssetRef` and never inserts it into `query_assets` (`assets.rs:3774-3788`).

**Expected:** each task sees its own payload; no cross-talk; no cache entry created.
**Test:** `tokio::join!` two evaluations with distinct `user_id`s, assert each result matches its own.

### C2. Payload cloning cost

`Context` clones the payload per action (`context.rs:518,579,674`). `PayloadType` requires `Clone`,
and the guide directs large data behind `Arc`.

**Expected:** cloning is `Arc`-cheap for well-formed payloads. **Test:** a payload holding an
`Arc<Mutex<Vec<String>>>`; assert all actions in a chain observe the same underlying allocation
(push from several commands, count once at the end).

### C3. Deep nesting

Three levels, each requiring payload. **Expected:** payload reaches level 3 unchanged.
**Test:** `/-/l1` → evaluates `/-/l2` → evaluates `/-/l3`, assert the leaf sees the original payload.

### C4. Immediacy is preserved

A payload-evaluated parent depending on a payload-free child must not stall on a queue slot. The
existing claim-based machinery covers this — `wait_for_dependency` direct-claims a runnable child
with no slot consumed (`assets.rs:1768`), and `drain_dependencies` inline-runs the local queue
(`assets.rs:2692-2698`).

**Test:** with `DefaultAssetManager` at its default capacity of four (`assets.rs:3342`), saturate the
queue, then run a payload evaluation whose child is payload-free; assert it completes rather than
deadlocking.

### C5. Serialization round-trip

Covered by U2. The invariant that matters operationally: **metadata written before this change must
still load**, and re-serializing it must not introduce a `payload_required` key.

## Test Plan

### Unit tests

**U1 — `PayloadRequirement` semantics** (`command_metadata.rs`, inline `mod tests`)

```rust
#[test]
fn test_payload_requirement_join() {
    use PayloadRequirement::{None, Required};
    assert_eq!(None.join(None), None);
    assert_eq!(None.join(Required), Required);
    assert_eq!(Required.join(None), Required);
    assert_eq!(Required.join(Required), Required);
}

#[test]
fn test_payload_requirement_default_is_none() {
    assert_eq!(PayloadRequirement::default(), PayloadRequirement::None);
    assert!(PayloadRequirement::default().is_none());
    assert!(!PayloadRequirement::default().is_required());
}
```

**U2 — Serialization compatibility** (`command_metadata.rs`, `plan.rs`)

```rust
#[test]
fn test_command_metadata_without_payload_field_deserializes() -> Result<(), Box<dyn std::error::Error>> {
    // JSON captured before this change: no `payload_required` key.
    let json = r#"{"realm":"","namespace":"","name":"test","cache":true,"volatile":false}"#;
    let cm: CommandMetadata = serde_json::from_str(json)?;
    assert_eq!(cm.payload_required, PayloadRequirement::None);
    Ok(())
}

#[test]
fn test_none_requirement_is_not_serialized() -> Result<(), Box<dyn std::error::Error>> {
    let cm = CommandMetadata::new("test");
    assert!(!serde_json::to_string(&cm)?.contains("payload_required"));
    Ok(())
}

#[test]
fn test_required_round_trips() -> Result<(), Box<dyn std::error::Error>> { /* Required survives */ }

#[test]
fn test_plan_without_payload_field_deserializes() -> Result<(), Box<dyn std::error::Error>> {
    // Guards the deliberate serde(default) deviation from `is_volatile`.
}
```

**U3 — `PlanBuilder` local detection** (`plan.rs`)

| Test | Asserts |
|---|---|
| `test_plan_payload_required_from_command` | a `payload: required` command sets `plan.payload_required` |
| `test_plan_payload_none_by_default` | a plain command leaves it `None` |
| `test_payload_required_implies_volatile` | the same plan also has `is_volatile == true` |
| `test_payload_link_parameter_propagates` | a link parameter to a payload-requiring query propagates (mirrors `check_parameter_for_volatile_links`) |
| `test_keyed_step_does_not_propagate_payload` | `Step::GetAsset` is a boundary — the plan stays `None` |
| `test_plan_records_reason` | an explanatory `Step::Info` is added, mirroring `mark_volatile` |

**U4 — Plan splitting** (`plan.rs`) — the site Phase 1 D8 flagged as easiest to miss:

```rust
#[test]
fn test_plan_split_preserves_payload_required() -> Result<(), Box<dyn std::error::Error>> {
    // Build a payload-requiring plan, split it, assert BOTH halves carry payload_required
    // (mirrors the is_volatile copies at plan.rs:1599,1607).
}
```

**U5 — Macro codegen** (`liquers-macro`)

| Test | Asserts |
|---|---|
| `test_payload_required_sets_metadata` | `cm.payload_required == Required` |
| `test_payload_required_sets_volatile` | `cm.volatile == true` (the D7 shortcut) |
| `test_payload_none_is_default` | omitting the statement leaves `None` |
| `test_unknown_payload_ident_fails` | `payload: bogus` is a compile error — `trybuild` if available, otherwise documented as manual |

### Integration tests (`liquers-core/tests/injection.rs`)

**I1 — the headline rewrite.** `test_payload_not_inherited_in_nested_evaluation` is **replaced** by
`test_payload_inherited_in_nested_evaluation`, asserting `"parent:laura|child:window:777"` instead of
`"parent:laura|child:None"`. Per Phase 1, the doc/rustdoc/test must change together.

**I2 — other nested entry points**

```rust
#[tokio::test]
async fn test_payload_inherited_via_get_dependency_state() -> Result<(), Box<dyn std::error::Error>>;
#[tokio::test]
async fn test_payload_inherited_via_apply() -> Result<(), Box<dyn std::error::Error>>;
```

**I3-I8**

| Test | Asserts |
|---|---|
| `test_payload_free_child_is_cached_and_shared` | a payload-free child resolves through the manager; a second request reuses the same asset id |
| `test_missing_payload_is_error` | a payload-required nested query with no payload in context returns an error naming the query |
| `test_keyed_recipe_requiring_payload_is_rejected` | a stored recipe whose plan requires payload fails at plan build; needs a `MemoryStore` + `RecipeProvider` |
| `test_payload_cycle_is_detected` | `/-/a` evaluating `/-/b` evaluating `/-/a`, all payload-required, yields `ErrorType::DependencyCycle` via the `Context` active-query guard |
| `test_payload_asset_is_not_a_registered_dependency` | after a nested payload evaluation, the dependency manager holds no edge naming the payload query |
| `test_payload_asset_records_its_own_dependencies` | the payload asset's metadata *does* list what it depended on (action #4 kept) |
| `test_inline_manager_payload_inheritance` | I1's assertion under `ImmediateEnvironmentWithPayload` — **blocked on the infrastructure gap** |
| `test_deep_nesting_payload_propagation` | C3 |
| `test_concurrent_payloads_do_not_share` | C1 |

**Query hygiene:** every query above is a single token with no spaces or newlines
(`/-/parent_cmd`, `/-/a`, `dash`). Only `test_keyed_recipe_requiring_payload_is_rejected` uses a
resource part, and it defines a `MemoryStore`-backed environment accordingly. Every command used is
registered in its test.

## Migration Audit (required deliverable, not optional)

Every payload-reading command must gain `payload: required`. Sites to audit:

| File | Sites | Notes |
|---|---|---|
| `liquers-lib/src/ui/commands.rs` | ~12 `E::Payload: UIPayload` bounds | The only production consumers |
| `liquers-core/tests/injection.rs` | all `injected` payload commands | Also the I1 rewrite |
| `specs/PAYLOAD_GUIDE.md` | every example | Must show `payload: required` |

**Detection aid:** a command whose function body calls `get_payload_clone()`, or whose injected
parameter type implements `ExtractFromPayload`, needs the declaration. Neither is visible to the
compiler, so this is a grep-and-read pass — `grep -rn "get_payload_clone\|ExtractFromPayload"`.

**Suggested regression guard:** `test_unannotated_payload_command_is_payload_free_when_nested` —
a deliberately un-annotated payload command asserted to fail in nested position. It documents the
hazard as designed behavior rather than leaving it as a latent surprise.

## Documentation Updates (Phase 1 requirement)

| File | Change |
|---|---|
| `specs/PAYLOAD_GUIDE.md` | "Inheritance" bullet becomes true; add `payload: required`; correct "Not available: background/async" to the new error semantics |
| `specs/PROJECT_OVERVIEW.md` | lines 271, 390 — inheritance now real, note the keyed boundary |
| `liquers_core::context` rustdoc | `:76-80` currently states nested evaluation does **not** inherit; `:450`, `:459-460`, `:469-470` say the same per-method |
| `specs/ISSUES.md` | close the issue; record the `Optional` deferral and the keyed limitation |

## Review Checklist

- [x] Overview table present, covering every example and test
- [x] 3 examples spanning primary, declaration/migration, and boundary/error cases
- [x] Corner cases: concurrency, memory/cloning, deep nesting, immediacy, serialization
- [x] Unit and integration tests planned, both happy and error paths
- [x] Both asset managers covered — with the infrastructure gap blocking inline called out
- [x] Queries contain no spaces/newlines; store defined where a resource part is used
- [x] All commands in examples are registered in their snippets
- [x] No `unwrap()`/`expect()` outside tests; typed error constructors only
- [x] Test signatures return `Result<(), Box<dyn std::error::Error>>`
- [x] `type CommandEnvironment` alias present before `register_command!`

## Open Items for Phase 4

1. **`ImmediateEnvironmentWithPayload<V, P>`** must be added to `liquers-core` (or a test-local
   equivalent accepted) before I8 can be written. This is a Phase 2 architecture addition discovered
   here.
2. **Compile-fail testing** for `payload: bogus` depends on whether `trybuild` is an acceptable
   dev-dependency; otherwise U5's last case is documented-manual.
