# Phase 4: Implementation Plan - liquers-web

## Overview

**Feature:** `liquers-web` — browser/JavaScript integration of Liquers (wasm).

**Architecture:** A `#[wasm_bindgen]` facade over existing machinery. `DefaultEnvironment` is already
generic over the value type and already cfg-selects `ImmediateAssetManager` on wasm, so a JS command
is an ordinary async command whose closure owns a `js_sys::Function`. The crate contributes a value
bridge, an object/eval/command surface, and a Promise bridge.

**Estimated complexity:** Medium. The architecture is settled and the foundation exists; the risk is
concentrated in two mechanical places (14 cfg'd match arms; 82 conformance tests).

**Estimated time:** 5-7 days for an experienced Rust developer, distributed as M1 ≈ 0.5 d,
M2 ≈ 1 d, M3 ≈ 1.5 d, M4 ≈ 1 d, M5 ≈ 1.5 d, M6 ≈ 1 d.

**Prerequisites:** Phases 1-3 approved · all open questions resolved · Rust 1.75+ (AFIT, already
required) · `wasm-bindgen-cli` 0.2.126 and `trunk`, both already used by
`liquers-lib/examples-web/`.

## Milestones and dependency order

The order is forced by two hard dependencies: nothing in `liquers-web` compiles until
`ValueExtension` is relaxed (M1), and no *conformance* test runs until the surface exists (M4).

| # | Milestone | Gate (must be green before the next starts) |
|---|---|---|
| **M1** | `liquers-core` + `liquers-lib` groundwork | native suite green; build matrix green |
| **M2** | Crate skeleton, value bridge | `wasm-pack test` runs; `VALUE*` pass |
| **M3** | Object surface, errors, environment | `OBJECT*`, `ERROR*`, `ENVIRON*` pass |
| **M4** | Commands and evaluation | `COMMAND*`, `EVAL*`, `ASYNCQ*`, `ASYNCCMD*`, `RUNTIME*` pass |
| **M5** | Delivery: trunk, quick start, stubs | `PACKAGE*`, `STUBS*` pass |
| **M6** | Extensibility proof, benchmark, docs | second value type compiles and passes; benchmark recorded |

**M1 is the only milestone that touches existing crates.** If it cannot be made green, the design
is wrong and later milestones must not be started on top of it.

---

## Implementation Steps

26 steps across six milestones. Each names its files, its validation command, its rollback and the
agent that should execute it.

### M1 — Groundwork in `liquers-core` and `liquers-lib`

### Step 1: Relax `ValueExtension`

**File:** `liquers-lib/src/value/extended.rs:12`

```rust
// Before:
// pub trait ValueExtension:
//     core::fmt::Debug + Clone + Sized + DefaultValueSerializer + Send + Sync + 'static
// After:
pub trait ValueExtension:
    core::fmt::Debug
    + Clone
    + Sized
    + DefaultValueSerializer
    + liquers_core::maybe_send::MaybeSend
    + liquers_core::maybe_send::MaybeSync
    + 'static
```

**Validation:**
```bash
cargo check -p liquers-lib
cargo test -p liquers-lib --lib --tests     # native behaviour must be unchanged
```

**Rollback:** `git checkout liquers-lib/src/value/extended.rs`

**Agent:** haiku · skills `rust-best-practices` · knowledge: this step, `liquers-core/src/maybe_send.rs`,
Phase 1 decision 1. *Rationale:* a one-line supertrait edit with a verified blast radius; no judgement
required.

---

### Step 2: `RUNTIME01` — prove the relaxation did not weaken native

**File:** `liquers-lib/src/value/extended.rs` (test module)

```rust
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    /// RUNTIME01: on native, MaybeSend/MaybeSync must still resolve to Send/Sync.
    /// If the relaxation ever weakens the native build, this stops compiling.
    #[test]
    fn runtime01_native_adapter_satisfies_thread_bounds() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<crate::value::ExtValue>();
        assert_send_sync::<crate::value::Value>();
    }
}
```

**Why it belongs here, not in `liquers-web`:** the risk being guarded is that Step 1 silently weakens
the *native* build, which `liquers-web` cannot observe at all.

**Validation:** `cargo test -p liquers-lib --lib runtime01`

**Agent:** haiku · skills `rust-best-practices`, `liquers-unittest` · knowledge: Phase 3 inventory
row `RUNTIME01`.

---

### Step 3: `CommandMetadataRegistry::remove_command`

**File:** `liquers-core/src/command_metadata.rs` (beside `add_command`, ~line 1072)

```rust
/// Removes a command's metadata. Returns the removed metadata, or `None` if absent.
pub fn remove_command<K>(&mut self, key: K) -> Option<CommandMetadata>
where
    K: Into<CommandKey>,
{
    // find by key in `self.commands`, `Vec::remove`, return it
}
```

**Validation:** `cargo check -p liquers-core`

**Agent:** haiku · skills `rust-best-practices` · knowledge: `add_command` (`:1058-1072`) for the
key-matching idiom. *Rationale:* mirrors an adjacent method.

---

### Step 4: `CommandRegistry::unregister`

**File:** `liquers-core/src/commands.rs` (in `impl<E: Environment> CommandRegistry<E>`, after
`register_async_command`, ~line 526)

```rust
/// Removes a command's sync executor, async executor and metadata.
///
/// Returns `true` if anything was removed. Idempotent: unregistering an absent
/// command is `false`, not an error.
///
/// All three stores are cleared together by design — planning consults metadata
/// while execution consults the executors, so a partial removal would leave a
/// command that plans successfully and then fails at execution.
pub fn unregister<K>(&mut self, key: K) -> bool
where
    K: Into<CommandKey>,
{
    let key = key.into();
    let a = self.executors.remove(&key).is_some();
    let b = self.async_executors.remove(&key).is_some();
    let c = self.command_metadata_registry.remove_command(key).is_some();
    a || b || c
}
```

**Validation:** `cargo check -p liquers-core && cargo check -p liquers-py -p liquers-axum`
(additive inherent method — no implementor should be affected; this proves it).

**Agent:** sonnet · skills `rust-best-practices` · knowledge: `CommandRegistry` fields (`:470-474`),
Phase 2 "Unregistration". *Rationale:* the three-store coupling is the correctness point of the
whole step and must not be simplified away.

---

### Step 5: Tier-1 unregister tests

**File:** `liquers-core/src/commands.rs` (test module) and
`liquers-core/tests/unregister_COMMAND.rs`

Four tests, per Phase 3: `unregister01_removes_metadata_and_executors` (asserts the query fails to
**plan** with `ActionNotRegistered` — a failure at execution with `unknown_command_executor` means
metadata survived and **must fail the test**), `unregister02_absent_is_false_not_error`,
`unregister03_reregister_resets_impl_version`, `unregister04_async_and_sync_both_removed`.

**Validation:** `cargo test -p liquers-core unregister`

**Agent:** sonnet · skills `liquers-unittest`, `rust-best-practices` · knowledge: Phase 3 "How the
three hardest tests actually assert", `liquers-core/tests/async_hellow_world.rs` for environment
setup. *Rationale:* `unregister01`'s assertion is the subtle one — a naive version passes with the
bug present.

---

### Step 6: `ExtValue::Js` variant and its 14 match arms

**File:** `liquers-lib/src/value/mod.rs` (+ `ui/web/html.rs:84`, `egui/mod.rs:72`)

```rust
#[cfg(all(target_arch = "wasm32", feature = "webui"))]
Js { value: crate::value::js::JsOpaque },
```

Add one cfg'd arm to each of the 14 sites enumerated in Phase 2. **No `_ =>` arm may be introduced**
— that is the project rule this step is most likely to violate under time pressure.

**One site is not like the other 13, and it is the dangerous one.**
`DefaultValueSerializer::as_bytes` (`liquers-lib/src/value/mod.rs:190`) **already has a `_ =>` arm**,
using `Error::new` — two pre-existing violations of the project rules. Consequences:

- Adding `ExtValue::Js` there does **not** fail to compile. The new variant silently falls into the
  catch-all, so this is the one site where forgetting the arm produces no error at all.
- **The build matrix cannot catch it.** Step 7 guards the other 13 sites, which fail to compile when
  an arm is missing. Here the code compiles either way — so this site needs a *reviewer*, not a
  compiler, and is called out for that reason.

Required at this site: delete the `_ =>` arm, write explicit arms for all six variants (`Image`,
`PolarsDataFrame`, `UiCommand`, `Widget`, `UIElement`, `Js`) with their cfg gates, and replace
`Error::new` with the typed constructor. Then a future variant breaks the build as intended.

The adjacent `deserialize_from_bytes` (`:211`) also has a `_ =>` arm, but it matches on a `&str`
type identifier rather than the enum, so a catch-all there is correct and stays. Add an explicit
`"js"` arm to it for a clearer message; the fallback would otherwise produce a serviceable but
vaguer error.

`JsOpaque` itself lives in a new `liquers-lib/src/value/js.rs`, gated the same way:

```rust
#[derive(Clone)]
pub struct JsOpaque { value: wasm_bindgen::JsValue, type_tag: std::sync::Arc<str> }
// hand-written Debug printing `Js(<type_tag>)` — never delegating to JsValue's Debug,
// which can invoke JS and is unsuitable inside error paths.
```

**Validation — the build matrix, all six configurations:**
```bash
cargo check -p liquers-lib --no-default-features
cargo check -p liquers-lib --no-default-features --features egui
cargo check -p liquers-lib --no-default-features --features polars
cargo check -p liquers-lib --no-default-features --features webui
cargo check -p liquers-lib                       # default
cargo check -p liquers-lib --target wasm32-unknown-unknown --no-default-features --features webui
```

**Rollback:** `git checkout liquers-lib/src/value/ liquers-lib/src/ui/web/html.rs liquers-lib/src/egui/mod.rs`

**Agent:** sonnet · skills `rust-best-practices` · knowledge: Phase 2's 14-site table, the existing
`#[cfg(feature = "polars")]` arms as the pattern to copy. *Rationale:* mechanical but wide, and the
cfg matrix is where this kind of change fails; haiku would likely reach for `_ =>`.

### Step 7: Build-matrix check script

**File:** `scripts/check-build-matrix.sh` (new)

**This repository has no CI.** There is no `.github/` directory at all, so the earlier draft's "CI
job" assumed infrastructure that does not exist. The matrix ships as a **script** instead — runnable
by a developer, and by CI later if the project adopts it:

```bash
#!/usr/bin/env bash
set -euo pipefail
# Guards the cfg-gated ExtValue::Js variant across every build configuration.
# A forgotten cfg arm breaks exactly one of these and nothing else catches it.
for args in \
  "--no-default-features" \
  "--no-default-features --features egui" \
  "--no-default-features --features polars" \
  "--no-default-features --features webui" \
  "" \
  "--target wasm32-unknown-unknown --no-default-features --features webui"
do
  echo "==> cargo check -p liquers-lib $args"
  cargo check -p liquers-lib $args
done
```

Adding a GitHub Actions workflow is a **separate decision for the project owner**, not something
this feature should introduce unilaterally. Recorded as such rather than assumed. The script is the
deliverable; wiring it into CI is optional and additive.

**Validation:** `bash scripts/check-build-matrix.sh`

**Agent:** haiku · knowledge: `liquers-lib/Cargo.toml` feature list, `CLAUDE.md` disk constraints
(the wasm target build should follow a `cargo clean` if disk is tight).

**M1 gate:** `cargo test -p liquers-lib --lib --tests` green, `cargo test -p liquers-core` green,
all six matrix configurations green.

---

### M2 — Crate skeleton and value bridge

### Step 8: Create the crate

**Files:** `liquers-web/Cargo.toml`, `liquers-web/src/lib.rs`, workspace `Cargo.toml`

```toml
members = [..., "liquers-web"]
default-members = ["liquers-core", "liquers-macro", "liquers-store", "liquers-lib",
                   "liquers-axum", "liquers-py"]   # liquers-web excluded — wasm-only
```

`liquers-web` depends on `liquers-lib` with `default-features = false, features = ["webui"]`.
The crate body is `#![cfg(target_arch = "wasm32")]`-gated with a documented "wasm32 only" notice on
native, so `cargo check --workspace` stays green without pretending the crate works natively.

**Validation:** `cargo check --workspace && cargo check -p liquers-web --target wasm32-unknown-unknown`

**Agent:** sonnet · knowledge: `liquers-axum/Cargo.toml` as a sibling-crate template, `CLAUDE.md`
build constraints. *Rationale:* the `default-members` and target-gating decisions have consequences
for every later build command.

---

### Step 9: `JsValueBridge` and the conversion layer

**Files:** `liquers-web/src/value.rs`, `liquers-web/src/convert.rs`

The trait from Phase 2, plus `js_to_value` / `value_to_js` implementing the full conversion table,
**and the `opaque()` opt-in itself**:

```rust
// liquers-web/src/value.rs — exported; decision 2's opt-in. Must exist before Step 10,
// because VALUE05 and VALUE11 both exercise it.
#[wasm_bindgen]
pub fn opaque(value: JsValue) -> LiquersValue;
```

**Hard rule for this step:** no item in `convert.rs` may name `liquers_lib::value::Value`
concretely — that is what keeps the Tier-2 extension path (Step 24) possible. A reviewer should
grep for it.

**Validation:**
```bash
cargo check -p liquers-web --target wasm32-unknown-unknown
grep -n "liquers_lib::value::Value" liquers-web/src/convert.rs   # must be empty
```

**Agent:** sonnet · skills `rust-best-practices` · knowledge: Phase 2 conversion table, Phase 1
decision 2, `liquers-core/src/value.rs`. *Rationale:* the numeric/bytes/cycle edge cases are where
silent data corruption lives.

### Step 10: `VALUE*` tests (13)

**File:** `liquers-web/tests/value_bridge_VALUE.rs` (`wasm-bindgen-test`)

All 13 `VALUE` rows from the Phase 3 inventory, including the three that were nearly excused:
`VALUE08` (roundtrip through `ExtValue::Js`), `VALUE11` (bare function → `ConversionError`;
`opaque(fn)` → retained), and `VALUE03`'s `BigInt`/2^53 boundaries.

**Validation:** `wasm-pack test --headless --chrome liquers-web`

**Agent:** sonnet · skills `liquers-unittest` · knowledge: Phase 3 inventory, corner cases C4-C7.

**M2 gate:** all `VALUE*` green in headless Chromium.

---

### M3 — Object surface, errors, environment

### Step 11: Object wrappers

**File:** `liquers-web/src/objects.rs` — `LiquersQuery`, `LiquersKey`, `LiquersMetadata`,
`LiquersState`, `LiquersAsset`, per Phase 2's signature list. Enums cross as lowercase strings,
reusing the existing serde renames; every mapping is an exhaustive match with no `_ =>` arm.

**Agent:** haiku · knowledge: Phase 2 "Function Signatures", `liquers-py/src/` for wrapper
conventions worth mirroring. *Rationale:* follows a fully specified list.

### Step 12: `LiquersError` and the error bridge

**File:** `liquers-web/src/error.rs` — `js_error_to_liquers`, `liquers_error_to_js`, all **22**
`ErrorType` variants mapped exhaustively. Conversion failures are `ConversionError`, not
`ExecutionError`. `console_error_panic_hook` installed by `init()`.

**Agent:** sonnet · skills `rust-best-practices` · knowledge: `liquers-core/src/error.rs:13-39`,
Phase 2 "Error Handling". *Rationale:* the ConversionError/ExecutionError distinction is a contract,
not a detail.

### Step 13: Environment, singleton and explicit instances

**File:** `liquers-web/src/environment.rs`

`thread_local! { static GLOBAL_ENV: RefCell<Option<EnvRef<WebEnvironment>>> }`,
`LiquersEnvironment::new()` / `global()` / instance methods.

**The three layers, enumerated — they share names and must not be confused:**

| Layer | Items | Implemented in |
|---|---|---|
| **Module-level** (singleton path, the documented default) | `init()`, `evaluate(q)`, `registerCommand(spec)`, `unregisterCommand(name)`, `opaque(v)`, `version()` | this step, except `opaque` (Step 9) and the `registerCommand`/`unregisterCommand` bodies (Steps 16-17) — the module-level forms are thin delegates to the singleton |
| **`LiquersEnvironment` methods** (explicit-instance path) | `evaluate`, `evaluateQuery`, `getAsset`, `registerCommand`, `describeCommand`, `commandNames`, `unregisterCommand` | this step, delegating to the generic internals |
| **Generic internals** (Tier-2 reuse; never name a concrete value type) | `js_to_value`, `value_to_js`, `register_js_command`, `evaluate_to_promise` | Steps 9, 16, 18 |

Both public layers delegate to the same generic internals — that is what makes Step 24's second
value type possible.

**`version()`** belongs here and is required before Step 23: it reports the `liquers-web` crate
version and the linked `liquers-core` version, which is what `PACKAGE04` compares against
`Cargo.toml`. Source the values from `env!("CARGO_PKG_VERSION")` and a re-exported core constant
rather than a hand-maintained string, or `PACKAGE04` becomes a test of someone's diligence.

**The rule that makes reentrancy safe** (Phase 2): every accessor clones the `EnvRef` out and drops
the `RefCell` borrow **before** any `await` or any call into JS. No borrow may be held across either.
This is the single most important invariant in the crate and belongs in a module-level comment.

**Agent:** sonnet · skills `rust-best-practices` · knowledge: Phase 2 "Reentrancy", `EnvRef`
(`context.rs:225`), `DefaultEnvironment` (`liquers-lib/src/environment.rs`). *Rationale:* the borrow
discipline is subtle and a violation deadlocks rather than failing loudly.

### Step 14: `OBJECT*` (8), `ERROR*` (5), `ENVIRON*` (6) tests

**Files:** `liquers-web/tests/objects_OBJECT.rs`, `errors_ERROR.rs`, `environment_ENVIRON.rs`

Includes `OBJECT07` (unknown enum *string from JS* → `ConversionError` naming it) and the
`web_evaluate_before_init` corner case.

**Agent:** sonnet · skills `liquers-unittest` · knowledge: Phase 3 inventory.

**M3 gate:** `OBJECT*`, `ERROR*`, `ENVIRON*` green.

---

### M4 — Commands and evaluation

### Step 15: Command spec parsing, inference, namespaces

**File:** `liquers-web/src/command/spec.rs`

`JsCommandSpec`, `StateMode`, `IsAsync`; the argument rule from Phase 2 — explicit `arguments` wins;
otherwise infer only when **every token is a plain identifier** (`^[A-Za-z_$][A-Za-z0-9_$]*$`) **and**
`tokens.length == fn.length`; refuse with `ParameterError` naming the offending parameter otherwise.
Namespace policy: root default, any explicit namespace, `web` reserved and rejected.

**Agent:** sonnet · skills `rust-best-practices` · knowledge: Phase 2 "Argument declaration",
Phase 3's 13-row inference sub-suite. *Rationale:* the `fn.length`-with-defaults trap makes a naive
implementation silently misbind.

### Step 16: The command adapter

**File:** `liquers-web/src/command/adapter.rs` — `register_js_command`, `call_js_command`.

Registers through `CommandRegistry::register_async_command` (and additionally `register_command` for
sync commands). The closure clones the `js_sys::Function`; the returned future is `'static` and
borrows nothing. **No `RefCell` borrow or manager guard may be held across the call into JS.**

**Agent:** sonnet · skills `rust-best-practices` · knowledge: `commands.rs:485-526` (registration
bounds), `:433-470` (wasm closure aliases), Phase 2 "No `CommandExecutor` implementation".
*Rationale:* the `'static`/non-`Send` closure construction is the crate's most delicate piece of
lifetime work.

### Step 17: Replacement warnings and `unregisterCommand`

**File:** `liquers-web/src/command/registry.rs`

`console.warn` on **every** replacement, with the two distinct messages. `unregisterCommand` →
`CommandRegistry::unregister`, returning `false` for unknown rather than throwing.

**Agent:** haiku · knowledge: Phase 2 "Namespace policy". *Rationale:* fully specified.

### Step 18: Promise bridge and evaluation

**File:** `liquers-web/src/eval.rs` — `evaluate_to_promise` via
`wasm_bindgen_futures::future_to_promise`; the future owns an `EnvRef` clone and an owned `Query`.
Rejection carries a `LiquersError`, never a string. `LiquersAsset.cancel()` is the cancellation
surface; the Promise then rejects with `ErrorType::Cancelled`.

**Generic over `E`** — see Step 9's hard rule.

**Agent:** sonnet · skills `rust-best-practices` · knowledge: Phase 2 "The Promise bridge",
`assets.rs` `get_asset`/`AssetRef::get`.

### Step 19: `COMMAND*` (11 + 2 sub-suites), `EVAL*` (6), `ASYNCQ*` (8), `ASYNCCMD*` (6), `RUNTIME*` (5 wasm)

**Files:** `liquers-web/tests/commands_COMMAND.rs`, `eval_EVAL.rs`, `async_ASYNCQ.rs`,
`async_commands_ASYNCCMD.rs`, `runtime_RUNTIME.rs`

Includes the three specified-mechanism tests from Phase 3: `RUNTIME04` (three cases + 2 s timeout),
`RUNTIME05` (deterministic handle-count assertion), and the two sub-suites — **13 inference cases and
5 namespace cases**, one per row of Phase 3's tables, not a representative sample. Requires the
`console.warn` spy and the `debug-handles` feature.

**Agent:** sonnet · skills `liquers-unittest`, `rust-best-practices` · knowledge: Phase 3 "How the
three hardest tests actually assert" **in full**, the fixture-command table. *Rationale:* three of
these assertions are unfalsifiable if written naively; that section exists precisely to prevent it.

**M4 gate:** all of the above green. `COMMAND01` is the guide's mandatory end-to-end test.

---

### M5 — Delivery

### Step 20: `encodeParam`

**File:** `liquers-web/src/encode.rs`

**Decision required, and recommended answer.** `PARAMETER-ESCAPING-INCOMPLETE` (`specs/ISSUES.md`)
makes `encode_token` unsuitable to mirror — it emits unparseable text for `:` and all non-ASCII.
The options are (a) fix the core encoder first, which needs the entity redesign and blocks this
crate on a grammar change, or (b) implement `encodeParam` against the *current* entity table.

**Recommend (b)**, so `liquers-web` is not blocked on a grammar change, with the limitation stated
in the API docs: values containing a lone colon or non-ASCII characters cannot be encoded and raise
a typed error rather than producing a broken query. **Raising an error is the requirement** — silent
production of unparseable text is the defect being avoided. When the entity design lands,
`encodeParam` delegates to the fixed `encode_token` and the limitation disappears.

**Agent:** sonnet · knowledge: `specs/ISSUES.md` `PARAMETER-ESCAPING-INCOMPLETE`, `parse.rs:386`
entity table, Phase 3 `web_encode_param_roundtrip`.

### Step 21: trunk quick-start example

**Files:** `liquers-web/examples/quickstart/index.html`, `main.rs`, `Trunk.toml`

Example 1 from Phase 3, loaded by a plain `<script type="module">` — no bundler.

**Agent:** haiku · knowledge: `liquers-lib/examples-web/ui_spec_demo/` as the working template.

### Step 22: `.d.ts` review, `LiquersCommandSpec`, freshness check

**Files:** `liquers-web/js/liquers-web.d.ts` (hand-written interface),
`liquers-web/scripts/check-stubs.sh`

`tsc --noEmit` over a usage sample (`STUBS02`), a deliberately-wrong sample that must fail
(`STUBS06`), and a regeneration diff check. Packaged as a **script**, for the same reason as Step 7
— there is no CI in this repository to attach it to.

**Agent:** sonnet · knowledge: Phase 2 `STUBS` section. *Rationale:* deciding what degrades to
`any` versus what gets a real declared type is a judgement call.

### Step 23: `STUBS*` (7) and `PACKAGE*` (6) tests

**Files:** the Step 7 / Step 22 scripts + `liquers-web/tests/e2e/` Playwright specs

**Agent:** sonnet · skills `liquers-unittest` · knowledge: Phase 3 inventory,
`liquers-lib/examples-web/playwright.config.ts`.

**M5 gate:** quick-start page evaluates a query end to end with **zero console errors**
(`PACKAGE03`).

---

### M6 — Extensibility, benchmark, documentation

### Step 24: Second value type (Tier-2 proof)

**File:** `liquers-web/tests/second_value_type.rs`

A minimal `TestValue` with a `JsValueBridge` impl, instantiating **every** generic function, plus a
reduced conversion suite (`VALUE01`, `VALUE04`, `VALUE09`) so the generic path is shown to *behave*,
not merely type-check. If Step 9's hard rule was violated, this step fails to compile — which is the
point.

**Agent:** sonnet · skills `rust-best-practices` · knowledge: Phase 2 "Extensibility", Phase 3
Example 4.

### Step 25: Boundary benchmark

**File:** `liquers-web/benches/boundary.rs` (or a `wasm-bindgen-test` reporting timings)

Objects of 10 / 10² / 10³ / 10⁴ properties and a 1 MB `Uint8Array`, structural vs `opaque()`,
`performance.now()`, 100 iterations. **Reported, not asserted** — no CI threshold.

**Feed the result back into Phase 1 decision 2.** If structural conversion is cheap at realistic
sizes, the docs should stop implying `opaque()` is the performance answer; identity pass-through
remains its own justification either way.

**Agent:** haiku · knowledge: Phase 3 "Benchmark".

### Step 26: Documentation

**Files:** `specs/PROJECT_OVERVIEW.md` (add `liquers-web` to the crate structure), `CLAUDE.md` (build
and test commands for the new crate), `liquers-web/README.md`, `specs/liquers-web/DESIGN.md` (final
status), `specs/LANGUAGE-INTEGRATION_GUIDE.md` (the feature matrix for this integration, per §7
item 1).

**Agent:** sonnet · knowledge: all phase documents.

---

## Testing Plan

**There is no CI in this repository** (no `.github/` directory). Every gate below is therefore a
command a developer runs, and the two multi-step gates ship as scripts (Steps 7 and 22). Adopting
CI is a separate decision for the project owner; nothing here depends on it.

| When | Command | Gates |
|---|---|---|
| Every step in M1 | `cargo check -p <crate>` | compiles |
| M1 exit | `cargo test -p liquers-core`; `cargo test -p liquers-lib --lib --tests`; `bash scripts/check-build-matrix.sh` | no regression in existing crates |
| Every step in M2-M6 | `cargo check -p liquers-web --target wasm32-unknown-unknown` | compiles |
| Milestone exits | `cargo clean && wasm-pack test --headless --chrome liquers-web` | that milestone's conformance IDs |
| M5 exit | `trunk build && npx playwright test` | `PACKAGE*`, `STUBS07` |
| Final | all of the above, plus `cargo check --workspace` | 82 required tests pass |

**Build isolation is mandatory, not advisory.** Per `CLAUDE.md`, browser tests build a different
target and their own crate; run them after `cargo clean`, never interleaved with the native loop.
The 30 GB allowance does not fit both. A step that fails with "No space left on device" is a
disk-allowance symptom, not a code error — `cargo clean` and retry before investigating.

## Rollback Plan

| Scope | Action |
|---|---|
| A single step | `git checkout <file>` — every step lists its files |
| M1 (touches existing crates) | `git revert` the M1 commits. This is the **only** milestone that can break existing crates, which is why it is first and separately gated |
| M2-M6 | `liquers-web` is a new crate excluded from `default-members`; deleting the directory and its workspace entry restores the previous state exactly |
| The `ExtValue::Js` variant | Removing the variant and its 14 arms is mechanical; the build matrix proves the removal is complete |

## Risk Register

| Risk | Likelihood | Mitigation |
|---|---|---|
| A forgotten cfg arm breaks a downstream build config | **High** — 14 sites × 6 configs | Step 7's matrix script; it is the cheapest test in the plan. With no CI to enforce it, running it is part of M1's exit gate rather than automatic — a stated weakness |
| `'static` / non-`Send` closure construction fights the borrow checker | Medium | Step 16 assigned to sonnet with the exact alias definitions as knowledge |
| A `RefCell` borrow held across `await` deadlocks | Medium | Single documented invariant (Step 13); `RUNTIME04` has a timeout so it fails rather than hangs |
| Conformance tests written to pass rather than to catch | Medium | Phase 3 specifies the mechanism for the three unfalsifiable ones; `unregister01` in particular must fail on execution-time failure |
| Disk exhaustion during browser tests | Medium | `cargo clean` between native and browser loops |
| `encodeParam` limitation surprises users | Low | Typed error, never silent breakage; documented; disappears when the entity design lands |

## Review record

Both prescribed reviewers found real defects.

**Feasibility reviewer (verified against source).** One blocking finding, and it is the most useful
result of this phase: `DefaultValueSerializer::as_bytes` (`value/mod.rs:190`) **already has a
`_ =>` arm**, so it is the single site among the 14 where adding `ExtValue::Js` compiles silently and
the build matrix provides no protection. Step 6 now calls it out specifically and requires the
catch-all be removed rather than merely not introduced. It also confirmed every other load-bearing
claim: the `maybe_send` paths, the three `CommandRegistry` fields, the 14 sites, the absence of
`default-members` in the workspace, `register_async_command`'s bounds, and the six feature
configurations.

**Conformity reviewer.** `opaque()` and `version()` appeared in Phase 2's signature list but were
assigned to no step — `opaque()` is needed before Step 10's `VALUE05`/`VALUE11`, and `version()`
before Step 23's `PACKAGE04`, so both were test-before-implementation gaps. Step 13's "module-level
delegates" was also underspecified; the three layers (module-level singleton, instance methods,
generic internals) share method names and are now enumerated in a table with the file that owns each.

**A plan-level correction of my own:** Step 7 originally specified a CI job. **This repository has no
CI** — no `.github/` directory exists — so the matrix ships as a script instead, and adopting CI is
flagged as a separate decision for the project owner rather than something this feature introduces
unilaterally.

## Agent Assignment Summary

| Model | Steps | Why |
|---|---|---|
| haiku | 1, 2, 3, 7, 11, 17, 21, 25 | Fully specified, pattern-following, or mechanical |
| sonnet | 4, 5, 6, 8, 9, 10, 12, 13, 14, 15, 16, 18, 19, 20, 22, 23, 24, 26 | Judgement, subtle invariants, or wide mechanical change where `_ =>` is a temptation |
| opus | — | No step requires it; the architecture is settled and Phase 2 removed the open questions |

Every agent gets `rust-best-practices`; test steps also get `liquers-unittest`. **Every agent must be
told the two project rules most at risk here:** no `_ =>` arm on a Liquers enum, and no `unwrap()`/
`expect()` outside tests.

## Definition of Done

1. All six build configurations green, native suites unregressed.
2. **82 of 83 prescribed conformance tests pass**; `PACKAGE06` remains `NA` with its reversing
   condition recorded.
3. The quick-start page evaluates a query from a plain `<script type="module">` with zero console
   errors.
4. The second value type compiles and passes its reduced suite — the Tier-2 extension path proven.
5. The benchmark result is recorded back into Phase 1 decision 2.
6. `specs/liquers-web/DESIGN.md` carries a feature matrix with level, status, limitations and test
   evidence, per the guide's §7 item 1.
