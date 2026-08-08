# Phase 4: Implementation Plan - liquers-web

## Overview

**Feature:** `liquers-web` — browser/JavaScript integration of Liquers (wasm).

**Architecture:** A `#[wasm_bindgen]` facade over existing machinery. `DefaultEnvironment` is already
generic over the value type and already cfg-selects `ImmediateAssetManager` on wasm, so a JS command
is an ordinary async command whose closure owns a `js_sys::Function`. The crate contributes a value
bridge, an object/eval/command surface, and a Promise bridge.

**Estimated complexity:** Medium. The architecture is settled and the foundation exists; the risk is
concentrated in two mechanical places (14 match arms; 82 conformance tests) — and Option Z made
the first of those compiler-enforced.

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

### Step 6: `ForeignValue` trait, ungated `ExtValue::Foreign` variant, 14 match arms

**File:** `liquers-lib/src/value/mod.rs`, new `liquers-lib/src/value/foreign.rs`
(+ `ui/web/html.rs:84`, `egui/mod.rs:72`)

```rust
// liquers-lib/src/value/foreign.rs — NO target_arch gate, NO feature gate.
pub trait ForeignValue: core::fmt::Debug
    + liquers_core::maybe_send::MaybeSend
    + liquers_core::maybe_send::MaybeSync + 'static
{
    fn origin(&self) -> &'static str;
    fn as_any(&self) -> &dyn core::any::Any;
    fn identifier(&self) -> Cow<'static, str>;
    fn type_name(&self) -> Cow<'static, str>;
    fn default_extension(&self) -> Cow<'static, str>;
    fn default_filename(&self) -> Cow<'static, str>;
    fn default_media_type(&self) -> Cow<'static, str>;
    fn try_into_string(&self) -> Result<String, Error>;
    fn try_into_json_value(&self) -> Result<serde_json::Value, Error>;
    fn as_bytes(&self, format: &str) -> Result<Vec<u8>, Error>;
}

// liquers-lib/src/value/mod.rs
Foreign { value: Arc<dyn ForeignValue> },
```

**`liquers-lib` gains no language-specific code.** `JsOpaque` lives in `liquers-web` (Step 9), not
here — that is what lets Starlark and Python implement the same trait later without touching this
crate. Every one of the 14 arms is a one-line delegation:
`ExtValue::Foreign { value } => value.identifier()`.

**The arms are unconditional**, which is the point of Option Z: a missing arm fails to compile in
*every* configuration, so the compiler enforces 13 of the 14 sites. **No `_ =>` arm may be
introduced** — the project rule this step is most likely to violate under time pressure.

**Expect the bound to cascade, and budget for it.** On `wasm32`, `dyn ForeignValue` is not
`Send`/`Sync`, so any trait that *stores* a `Value` under a hard `Send + Sync` bound stops
compiling. Two traits in `liquers-lib` are affected and must be relaxed to the same
`MaybeSend`/`MaybeSync` markers:

| Trait | Why |
|---|---|
| `ui::element::UIElement` (`element.rs:60`) | `AssetViewElement`, `StateViewElement`, `QueryConsoleElement` each hold a `Value` behind an `RwLock` |
| `ui::app_state::AppState` (`app_state.rs:155`) | stores `dyn UIElement` handles, so it cannot outrank them |

**Only the `wasm32` matrix configuration surfaces this** — every native configuration compiles
without it, so Step 7 is not optional before declaring Step 6 done.

**One site the compiler cannot guard.** `DefaultValueSerializer::as_bytes`
(`liquers-lib/src/value/mod.rs:190`) **already has a `_ =>` arm**, using `Error::new` — two
pre-existing violations. So `ExtValue::Foreign` falls silently into the catch-all there and nothing
fails. This site needs a *reviewer*, not a compiler.

Required at this site: delete the `_ =>` arm, write explicit arms for all six variants (`Image`,
`PolarsDataFrame`, `UiCommand`, `Widget`, `UIElement`, `Foreign`) with the cfg gates the *existing*
variants need, and replace `Error::new` with a typed constructor. Then a future variant breaks the
build as intended — which is what makes this the last time anyone has to think about it.

The adjacent `deserialize_from_bytes` (`:211`) also has a `_ =>` arm, but it matches on a `&str`
type identifier rather than the enum, so a catch-all there is correct and stays. Add an explicit
`"js"` arm for a clearer message.

**Validation — the build matrix, all six configurations.** With ungated arms the compiler already
covers 13 sites in every configuration, so this now guards feature *interactions* and the `as_bytes`
site rather than being the sole defence:
```bash
bash scripts/check-build-matrix.sh
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
# Feature-interaction check across every build configuration. With Option Z the
# ExtValue::Foreign arms are unconditional, so the compiler guards them directly;
# this catches interactions and the as_bytes site the compiler cannot see.
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

`JsOpaque` (the `ForeignValue` implementation for JavaScript) lives **here**, not in `liquers-lib`:

```rust
// liquers-web/src/value.rs
#[derive(Clone)]
pub struct JsOpaque { value: JsValue, type_tag: Arc<str> }

impl ForeignValue for JsOpaque {
    fn origin(&self) -> &'static str { "javascript" }
    fn as_any(&self) -> &dyn core::any::Any { self }
    // … per Phase 2's JsOpaque table
}
```

Hand-written `Debug`, printing `Js(<type_tag>)` — never delegating to `JsValue`'s `Debug`, which can
invoke JS and is unsuitable inside error paths.

Recovering a JS value from an `ExtValue::Foreign` is a **checked downcast**
(`value.as_any().downcast_ref::<JsOpaque>()`); `None` means the value came from another language
runtime and yields a `ConversionError` naming `value.origin()`.

Then the trait from Phase 2, plus `js_to_value` / `value_to_js` implementing the full conversion
table, **and the `opaque()` opt-in itself**:

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
`VALUE08` (roundtrip through `ExtValue::Foreign` plus a downcast to `JsOpaque`), `VALUE11` (bare function → `ConversionError`;
`opaque(fn)` → retained), and `VALUE03`'s `BigInt`/2^53 boundaries.

**Validation:** `wasm-pack test --headless --chrome liquers-web`

**Agent:** sonnet · skills `liquers-unittest` · knowledge: Phase 3 inventory, corner cases C4-C7.

**M2 gate:** all `VALUE*` green in headless Chromium.

#### Browser-test tooling — prerequisites the plan under-specified

Running `wasm-bindgen-test` needs three things beyond `cargo`, none of which were in the
prerequisites list and all of which cost a cycle to discover:

1. **A cargo runner for the target.** A `wasm32` test binary is not natively executable; without a
   runner, `cargo test --target wasm32-unknown-unknown` fails with `Exec format error (os error 8)`
   as cargo tries to exec the `.wasm`. Fixed by `.cargo/config.toml`:

   ```toml
   [target.wasm32-unknown-unknown]
   runner = "wasm-bindgen-test-runner"
   ```

2. **A `wasm-bindgen-cli` whose version matches the `wasm-bindgen` crate exactly.** A mismatch fails
   at bindgen time with a schema-version error, not at compile time. Check `Cargo.lock` for the
   resolved version and `cargo install -f wasm-bindgen-cli --version <that>`.

3. **A WebDriver and a browser** — *if* you use the WebDriver harness at all.

**Finding: the WebDriver harness is unavailable in this environment, and it does not matter.**
`chromedriver` here is 147 and the bundled Chromium is 141; ChromeDriver enforces a major-version
match and refuses the session outright (`session not created: This version of ChromeDriver only
supports Chrome version 147`). Fetching either a matching driver or a matching browser is blocked
by the environment's network policy — `googlechromelabs.github.io` is denied, and the `chromedriver`
npm package installs from the (permitted) registry but downloads its binary from a host that is not.

Two paths cover everything anyway, which is why this is a note rather than a blocker:

| Harness | Provides | Covers |
|---|---|---|
| **Node** (`wasm-bindgen-test`, no driver) | full event loop, Promises, timers, ECMAScript | `VALUE` `OBJECT` `ERROR` `ENVIRON` `COMMAND` `EVAL` `ASYNCQ` `ASYNCCMD` `RUNTIME` |
| **Playwright** (CDP, not WebDriver — already installed, already used by this repo) | a real page and DOM | `PACKAGE02/03/07`, `STUBS07` |

The insight worth carrying: **`RUNTIME` and `ASYNCQ` do not need a *browser*, they need an *event
loop*, and Node has one.** Only tests asserting something about a delivered artifact in a real page
need the browser, and Playwright reaches those without WebDriver. `run_in_browser` should be used
only where a DOM is genuinely required.

**Feature forwarding is required, and its absence is silent.** `liquers-web` matches on `ExtValue`,
whose `PolarsDataFrame` and `UiCommand`/`Widget` variants are gated on **`liquers-lib`'s** features.
A `#[cfg(feature = "polars")]` written in `liquers-web` is evaluated against *`liquers-web`'s* own
features, so without forwarding it is always false — the arm compiles out, and a feature-unified
build that does have the variant would fail to match. `liquers-web/Cargo.toml` therefore forwards
`polars` and `egui` to `liquers-lib`. The symptom before forwarding is only an
`unexpected_cfg_condition_value` warning, which is easy to scroll past.

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
| The `ExtValue::Foreign` variant | Removing the variant and its 14 arms is mechanical, and because the arms are unconditional the compiler proves the removal is complete |

## Risk Register

| Risk | Likelihood | Mitigation |
|---|---|---|
| A forgotten match arm breaks a build config | **Low** — was High under the superseded Option Y | Option Z made the arms **unconditional**, so 13 of the 14 sites fail to compile everywhere if missed. The residual is the single `as_bytes` site whose pre-existing `_ =>` arm hides the omission — mitigated by calling it out in Step 6 and by review, not by tooling |
| `'static` / non-`Send` closure construction fights the borrow checker | Medium | Step 16 assigned to sonnet with the exact alias definitions as knowledge |
| A `RefCell` borrow held across `await` deadlocks | Medium | Single documented invariant (Step 13); `RUNTIME04` has a timeout so it fails rather than hangs |
| Conformance tests written to pass rather than to catch | Medium | Phase 3 specifies the mechanism for the three unfalsifiable ones; `unregister01` in particular must fail on execution-time failure |
| Disk exhaustion during browser tests | Medium | `cargo clean` between native and browser loops |
| `encodeParam` limitation surprises users | Low | Typed error, never silent breakage; documented; disappears when the entity design lands |

## M1 execution record — COMPLETE ✅

Steps 1-7 executed and green. Deviations from the plan, and what they cost:

| Step | Outcome |
|---|---|
| 1 `ValueExtension` relaxed | as planned |
| 2 `RUNTIME01` | as planned, **plus** an assertion that `Arc<dyn ForeignValue>` is `Send + Sync` on native — the property that lets the variant be ungated |
| 3 `remove_command` | as planned |
| 4 `unregister` | as planned |
| 5 unregister tests | 4 tests, all pass. `unregister01` asserts the query fails to **plan**, not at execution |
| 6 `ForeignValue` + `ExtValue::Foreign` + arms | **the compiler found the missing arms, exactly as Option Z predicted** — `egui/mod.rs:72` on the default build, `ui/web/html.rs:84` under `webui`. Both were compile errors, not silent |
| 7 matrix script | `scripts/check-build-matrix.sh`; all six configurations green |

**One design finding, and the matrix is what caught it.** The wasm32 configuration — the one the
native loop never builds — failed with 24 errors: `dyn ForeignValue` is not `Send`/`Sync` there, and
two further traits in `liquers-lib` transitively required it. Phase 1 decision 1 had costed the
blast radius as "one implementor, bounds local to `extended.rs`", which was true of `ValueExtension`
alone but missed that making `Value` non-`Send` on wasm propagates to anything storing a `Value`
under a hard bound:

- `ui::element::UIElement` (`element.rs:60`) — `AssetViewElement`, `StateViewElement` and
  `QueryConsoleElement` each hold a `Value` behind an `RwLock`;
- `ui::app_state::AppState` (`app_state.rs:155`) — stores `dyn UIElement` handles, so it cannot be
  more strongly bounded than they are.

Both relaxed to the same markers. The chain is `ValueExtension → UIElement → AppState` and stops
there — established by relaxing `UIElement` alone, watching `AppState` fail, then confirming the
full matrix green. **Native builds never showed any of this**, because the markers still mean
`Send + Sync` there.

**Gate results:** `liquers-core` 431 unit + 12 integration suites, 0 failed · `liquers-lib` 296 unit
+ 14 integration suites, 0 failed · `liquers-axum` and `liquers-py` check clean · all 6 build
configurations green.

## M2/M3 execution record

**M2 COMPLETE ✅** — value bridge, 13/13 `VALUE*` green. Details in the commit; the substantive
outcome is that writing the tests alongside the code found three real bridge bugs (text encoded as
bytes, `BigInt` never readable, nested integers emitted as floats).

**M3 COMPLETE ✅** — object wrappers, error bridge, environment. **32 tests green**
(`VALUE*` 13, `OBJECT*` 8, `ERROR*` 5, `ENVIRON*` 6).

Additions beyond the plan, each with a reason:

- **`liquers_core::VERSION`** — a crate-version constant sourced from Cargo. `version()` needs to
  report the linked core version and there was nothing to report; a hand-maintained string would
  drift from the manifest exactly when `PACKAGE04` cared. Additive.
- **`LiquersError::from_thrown`** — the plan had `js_error_to_liquers` returning a plain `Error`,
  which left `jsClass`/`jsStack` permanently null. `ERROR03` would have passed on the message text
  alone while the fields it names stayed empty — hollow conformance. The structured constructor
  populates them.

**One test was knowingly partial.** `ENVIRON01`'s contract is "evaluates a built-in command", and
M3 had no evaluation surface — that is M4. It asserted the half that existed (the environment
builds, its registry is reachable) and carried a comment saying it must be extended when M4 landed.
**M4 extended it**: it now evaluates end to end.

**A bug the tests caught in the error bridge:** `JsString::from(JsValue)` is an unchecked cast, not
a string coercion, so `throw 42` produced a generic "a non-Error value was thrown" with the value
discarded. Scalars are now stringified explicitly with a debug fallback.

## M4 execution record — COMPLETE ✅

Steps 15-19 executed. **79 wasm tests green** (`VALUE*` 13, `OBJECT*`+`ERROR*` 13, `ENVIRON*` 8,
`COMMAND*` 17, `EVAL*` 7, `ASYNCQ*` 8, `ASYNCCMD*` 7, `RUNTIME*` 6), plus 4 native `unregister`
tests. Every prescribed ID for these groups is present; `ASYNCQ07` remains `NA` with its reversing
condition recorded.

Additions beyond the plan:

- **`liquers-web/src/asset.rs`** — `Asset` and `State` wrappers, plus `getAsset`. Step 18 named
  `LiquersAsset.cancel()` as the cancellation surface but no step created the type. Without it
  `ASYNCQ04`, `EVAL03` and `EVAL06` had nothing to test against.
- **`tests/common/mod.rs`** — the shared fixture module Phase 3 required (infrastructure #1 and
  #2): the four fixture commands, the `console.warn` spy, and a `with_timeout` helper so
  `RUNTIME04`'s deadlock guard fails by name rather than hanging.
- **`arguments_were_inferred` / `argumentsInferred`** — Phase 2 says inference must never be
  invisible, and `describeCommand` did not report it. Kept beside the registry rather than added to
  `CommandMetadata`, which has no business carrying a JavaScript-only concern.
- **`debug-handles` feature** — `live_handle_count()`, the deterministic mechanism Phase 3
  specified for `RUNTIME05`.

**Deviation from Phase 2's signature list.** `Asset.status()` and `Asset.cancel()` were written as
synchronous; `AssetRef::status` and `AssetRef::cancel` are `async` in `liquers-core`, so exposing
them synchronously would require blocking — the one thing this crate refuses to do on an event
loop. Both are `Promise`s.

### Three bugs the tests caught

1. **JSON objects crossed to JavaScript as `Map`, not objects.** `serde_wasm_bindgen::to_value`
   routes `serialize_map` — which every `serde_json::Value::Object` goes through — to a JavaScript
   `Map`. A page reading `result.a`, `Object.keys(result)` or `JSON.stringify(result)` saw nothing.
   Rust *structs* serialize as objects either way, which is why it survived M2: the affected values
   were exactly the ones that came *from* JavaScript as objects. Every conversion now goes through
   one `serialize_to_js` with `serialize_maps_as_objects(true)`, and `VALUE02` — which only tested
   the inbound direction, and is named "roundtrip" — now closes the loop.
2. **`describeCommand` omitted `arguments` entirely for a zero-argument command**, because
   `CommandMetadata` carries `skip_serializing_if = "Vec::is_empty"`. Right for a config file,
   wrong for an API: `describeCommand(n).arguments.map(...)` worked for every command except the
   ones with no arguments. The shape is now normalized.
3. **A stale `EnvRef` after registration.** `eval06` shared the environment before registering a
   command; registering afterwards rebuilds, so the held handle no longer had the command. Correct
   behaviour, documented on `register_command_on` — but it is a trap worth its test.

### One finding filed as an issue

**`WEB-CANCELLATION-INERT`.** `ImmediateAssetManager` evaluates during `get_asset`, so an asset is
terminal before a caller can hold it and `cancel()` can never do anything. This follows from Phase
1 decision 5 and is not a defect, but it was not stated, and `cancel()` resolving successfully makes
it look like a working feature.

**How it was found matters more than the finding.** The first version of all three cancellation
tests matched on two outcomes — "either it was cancelled or it had already finished" — and passed.
That assertion passes regardless of what the implementation does, and would have kept passing if
`cancel()` began throwing or hanging. Probing which branch actually ran is what surfaced it. This is
a live instance of the risk Phase 3 named — *conformance tests written to pass rather than to
catch* — arriving in exactly the tests that section did **not** single out as hard. The three it
did single out (`RUNTIME04`, `RUNTIME05`, `unregister01`) were fine, because they had specified
mechanisms. The lesson generalizes: a two-branch `match` in a conformance test is a smell, and the
fix is to determine which branch is real and assert it.

All three are now deterministic assertions of the inert behaviour, so they fail the day a deferred
asset manager lands.

## M5 execution record — COMPLETE ✅

Steps 20-23 executed. **84 wasm tests** under Node, **5 Playwright tests** in Chromium, and
`check-stubs.sh` covering the rest. Every `STUBS*` and `PACKAGE*` ID is satisfied except
`PACKAGE06`, which stays `NA`.

**M5 gate met:** the quick-start page evaluates eight queries end to end in a real browser with
**zero console errors**, under both delivery paths.

| Step | Outcome |
|---|---|
| 20 `encodeParam` | Recommendation (b) taken — written against the *parser's* accepted set, not mirrored from `encode_token`, and refusing what it cannot represent |
| 21 trunk quick start | `examples-web/quickstart/`, plus a `build.sh` that does the same job without trunk |
| 22 `.d.ts` | **Deviation, see below** — generated with real types rather than hand-written |
| 23 `STUBS*`/`PACKAGE*` | `scripts/check-stubs.sh` (no browser) + `tests/e2e/package.spec.ts` (Playwright) |

### Deviation: the declarations are generated, not hand-written

Step 22 specified a hand-written `js/liquers-web.d.ts` plus a freshness check that diffs it against
the generated file. That is a worse design than it looks: it creates a second source of truth whose
only defence against drift is a check someone has to run, and a stale declaration file is *worse*
than none — a type checker confidently accepts code that fails at runtime.

wasm-bindgen already generates `liquers_web.d.ts` from the exported surface, and it cannot drift.
What it could not do is see inside a `JsValue`, so `registerCommand(spec: JsValue)` generated as
`spec: any` — which type-checks a declaration with a misspelled field or a missing `run`, making
`STUBS02`/`STUBS06` untestable.

`src/typescript.rs` fixes that at the source: a `typescript_custom_section` carrying
`LiquersCommandSpec`, `LiquersArgument`, `LiquersArgumentType`, `LiquersStateMode` and
`LiquersCommandInfo`, plus `typescript_type`-annotated extern types so the exported signatures
reference them. The generated file now reads:

```typescript
export function registerCommand(spec: LiquersCommandSpec): void;
export function describeCommand(name: string): LiquersCommandInfo | null;
export function getAsset(query: string): Promise<Asset>;
```

One generated file, real types, no freshness check needed — the drift the check existed to detect
is now impossible. `STUBS06` asserts an exact **error count** (6), so a declaration change that
legalizes any single mistake fails, not merely one that legalizes them all.

### Additions beyond the plan

- **The built-in Rust command set is registered** (`src/builtins.rs`). It was not, and `ENVIRON01`'s
  contract is literally "evaluates a built-in command" — the M3 record had deferred that to M4, and
  M4 satisfied it with a JavaScript command instead. `register_core_commands!` gives `to_text`,
  `to_metadata`, `commands_doc` and the `dep` introspection commands. This also makes the argument
  for structural conversion demonstrable rather than asserted: `hello/to_text` composes a
  JavaScript command with a Rust one in a single query, which an opaque value could not do. Its
  own module because the macro expands into the calling crate and names `futures`, `Context` and
  `liquers_macro` as if they were the caller's imports.
- **`WebValue`** in `default_value.rs` — the module claimed to be the only one naming a concrete
  value type but never named it. `PACKAGE05` now has something to assert.
- **`build.sh`** beside `Trunk.toml`. Both were run and both verified in a browser.

### PACKAGE04 moved from tier C to tier P

Phase 3 assigned it to the build-step tier, checking the version "against `Cargo.toml`". Written
that way it greps the artifact for a version string, which proves the bytes are present — not that
`version()` returns them, and it would pass for a hard-coded literal that had drifted from the
manifest. It now runs in Playwright, calls `version()`, and compares against both manifests
exactly. Tier is a harness question, not an applicability one.

### Two bugs found by running it

1. **The trunk and `build.sh` delivery paths load differently.** Trunk emits a *content-hashed*
   filename, injects its own loader, and publishes the module as `window.wasmBindings`; it does
   **not** rewrite imports. The page's `import "./liquers_web.js"` 404s under trunk. The page now
   detects which loader is present. Found by building with trunk and opening the result — the
   original comment in the page ("Trunk rewrites this to the hashed build artifact") was simply
   wrong, and no test would have caught it because the tests run against `build.sh`'s output.
2. **A self-referential feature detection.** The trunk check searched every module script for the
   string `TrunkApplicationStarted` — including the script doing the searching, whose own source
   contains it. Always true, so the `build.sh` page waited forever for an event nobody would
   dispatch. Fixed by excluding the script by `id`, which is now a load-bearing comment.

## M6 execution record — COMPLETE ✅

Steps 24-26 executed. **89 wasm tests**, 5 Playwright tests, `check-stubs.sh`, 4 native tests.

### Step 24 found that Phase 2's Tier-2 promise did not compile

This is what the step was for, and it earned its place. Phase 2's Example 4 sketched:

```rust
impl JsValueBridge for MyValue { /* MyValue = CombinedValue<SimpleValue, MyExt> */ }
```

That is `error[E0117]` from a downstream crate. `JsValueBridge` belongs to `liquers-web` and
`CombinedValue` to `liquers-lib`, so both are foreign there; `CombinedValue` is not
`#[fundamental]`, so instantiating it with a local `MyExt` does not make the self type local.
**`liquers-web` never noticed** because its own trait is local to it — the promise was
first-class for the crate making it and impossible for anyone else.

Fixed by moving the extension point to a type the downstream crate owns:

- `JsExtensionBridge` — the same four hooks at the *value extension* level. `impl
  JsExtensionBridge for MyExt` is a foreign trait on a local type, always allowed.
- A blanket `impl<B, Ext: JsExtensionBridge> JsValueBridge for CombinedValue<B, Ext>` carries it
  up to the whole value type.
- `default_value.rs` now implements `JsExtensionBridge for ExtValue` rather than `JsValueBridge for
  Value`, so **this crate takes the same route it documents**. A path the author does not use is a
  path that breaks unnoticed.

`tests/second_value_type.rs` then does what the step asked: a `MatrixExt` carrying a third-party
type, `TestValue = CombinedValue<SimpleValue, MatrixExt>`, a never-called function naming every
generic entry point at that type (so a regression is a *compile* error), and the reduced
`VALUE01`/`VALUE04`/`VALUE09` suite plus a full end-to-end evaluation on a downstream environment.

A smaller wart, found the same way and **since fixed**: `liquers_lib::value` glob-imported
`CombinedValue`, `SimpleValue` and `ValueExtension` privately, so they were not re-exported at
that level and a downstream crate had to reach into `value::extended` and `value::simple`. They
are now `pub use`d explicitly from `value/mod.rs`, which is where anyone writing their own value
type looks first. Additive — three names appear at a path that previously had none — and
`liquers-lib`'s native suite is unchanged.

### Step 25: the measurement Phase 1 deferred

Median round trip, `--release`, under Node:

| Input | Structural | Opaque | Ratio |
|---|---|---|---|
| object, 10 properties | 0.078 ms | 0.006 ms | 13× |
| object, 100 properties | 0.502 ms | 0.005 ms | 92× |
| object, 1 000 properties | 5.23 ms | 0.005 ms | 1 013× |
| object, 10 000 properties | 58.5 ms | 0.008 ms | 7 564× |
| `Uint8Array`, 1 MB | 0.868 ms | 0.006 ms | 140× |

The shape is as Phase 1 predicted — opaque flat, structural linear — but the reading is not. The
ratio grows without bound and is the wrong number to quote; the useful one is the absolute cost,
and **78 µs for a 10-property object is invisible**. It reaches a dropped frame only at ten
thousand properties. So the docs stop implying `opaque()` is the performance answer: its
justification is *identity*. Fed back into Phase 1 decision 2 in `DESIGN.md`.

### Step 26: documentation

`README.md` (new), `CLAUDE.md` (the three test loops and why there are three),
`specs/PROJECT_OVERVIEW.md` (crate structure), `DESIGN.md` (final status and the measurement), and
`LANGUAGE-INTEGRATION_GUIDE.md` — which gained the two findings that generalize beyond JavaScript:
the orphan-rule trap in checklist item 3, and "conformance tests that pass whatever the code does"
in §3.

## Code review record — PR #19

An automated reviewer raised seven findings after M6. **All seven were valid**, which is the useful
fact: none were false positives, and six were things the conformance suite should have caught.

| # | Finding | Disposition |
|---|---|---|
| P1 | `Environment` class had no methods — instances were constructible and unusable | Fixed: `evaluate`, `evaluateQuery`, `getAsset`, `describeCommand`, `commandNames`. `registerCommand` on an instance is **refused with a typed error** rather than faked |
| P2 | `Environment.global()` threw between `init()` and the first evaluation | Fixed: it shares the pending environment, which is the step the first evaluation would have taken anyway |
| P1 | A declared `volatile: true` was never read | Fixed — one line, and the planner was treating every JavaScript command as cacheable |
| P1 | A command returning `liquers.opaque(x)` failed outright | Fixed: the conversion layer recognises a `Value` wrapper by a duck-typed marker, so it works for a downstream crate's wrapper too |
| P2 | `state: "state"` passed only the value | Fixed: a plain object with `value`, `metadata`, `status`, `log`. Plain rather than the exported class because the adapter is generic over the value type |
| P2 | `jsClass`/`jsStack` are null on the real evaluation path | **Partially fixed.** The duplicated first line of the message is fixed; the structured fields need somewhere to live in `liquers_core::Error` — filed as `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT` |
| P2 | `JsValue::clone` retained the caller's declaration object, not a copy | Fixed: registration snapshots the declaration and its `arguments` |

### Why the suite missed six of them

Every one is an instance of the pattern §3 of the guide names, and they are worth listing because
the tests were *green* and *wrong* in four distinguishable ways:

- **`COMMAND08` tested only the negative case.** It asserted that an un-opted-in `Date` is refused
  and never that an opted-in value is retained — so the documented opt-in had never once run.
- **`COMMAND10` claimed more than it checked.** "Complete declaration preserves every supported
  metadata field" checked `doc`, `label` and namespace behaviour, and not `volatile`.
- **`ERROR03` tested the bridge at the wrong entry point.** `LiquersError::from_thrown` populates
  `jsClass`/`jsStack`, and the test called it directly; production traffic goes through
  `js_error_to_liquers` into a `liquers_core::Error`, which has no such fields.
- **`ENVIRON05` asserted a Rust fact about a JavaScript contract.** It checked that two instances
  hold distinct `Arc`s — true, and silent about the class having no methods at all.

The generalizable rule, now in the guide: *what implementation change would make this assertion
fail?* A negative-only test, a test that names more than it checks, and a test that calls a
convenience constructor instead of the shipped path all answer "none that anyone would make".

**Two of the new tests were wrong before the code was**, which is worth recording as the healthy
case: the `state: "state"` test asserted status `ready` when a mid-plan input state is `recipe`,
and the instance test assumed a bare `to_text` fails to plan when it does not. Both were corrected
to assert what is true rather than what was assumed.

## Review record

Both prescribed reviewers found real defects.

**Feasibility reviewer (verified against source).** One blocking finding, and it is the most useful
result of this phase: `DefaultValueSerializer::as_bytes` (`value/mod.rs:190`) **already has a
`_ =>` arm**, so it is the single site among the 14 where adding the new variant compiles silently and
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
