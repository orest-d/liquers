---
id: FOREIGN-VALUE-TYPE-REGISTRATION-PHASE3
kind: design
title: "Phase 3: Examples and tests — foreign and Python value types in the type registry"
status: in_review
phase: examples
area: [core/value, lib/value, web, py]
created: 2026-08-26
---
# Phase 3: Examples & Use-cases — Foreign Value Type Registration

## High-Level Introduction

Phase 1 asked for one thing: a value whose type identifier is known only to an integration should
be storable. The examples below show that from the outside in. **Scenario 1** is the whole point in
eight lines — an integration extends the base registry, passes it to the environment constructor,
and its foreign value stores. **Scenario 2** goes underneath it: how the same guarantee is proved
*natively*, without a wasm toolchain, and how `liquers-py` reaches the same place by a different
route because its foreign variant is statically describable. **Scenario 3** collects the four ways
to get this wrong, each of which has a test holding it shut.

The tests are the real deliverable of this phase. Two of them fail before the change and pass after;
one records a decision (hard refusal) that would otherwise erode; and one — the constant/instance
agreement check — *is* the guarantee the user chose in place of compile-time enforcement.

## Example Type

**Conceptual snippets for the scenarios; runnable code only as tests.**

No `examples/<feature>_demo.rs` is planned, and that is a deliberate departure from the template's
default. This change has no user-facing surface to demonstrate: nothing new appears in a query, no
command is added, and the visible difference is that an operation which used to return an error now
returns `Ok`. A demo binary would be a test wearing a costume. The guide will therefore link the
**integration test** `liquers-lib/tests/foreign_value_registration.rs` as its executable example,
which Phase 2's guide plan already anticipates.

## Overview Table

| # | Type | Name | What it demonstrates or checks |
|---|---|---|---|
| 1 | Example | Registering a foreign type in `liquers-web` | The primary workflow: extend, construct, store |
| 2 | Example | Proving it natively, and the Python route | Why a mock `ForeignValue` proves the same thing without wasm; why `liquers-py` needs no constructor |
| 3 | Example | Four ways to get it wrong | Empty base registry, registration in the wrong place, constant divergence, declaring a format a type cannot produce |
| 4 | Unit tests | `fvt1`–`fvt6` (6 files, 13 tests) | Registry extension, constructors, `type_info` derivation and routing, the constant/instance agreement, `liquers-py` descriptions |
| 5 | Integration tests | `fvt7` (1 new file, 5 tests) + `VALUE` repairs | End-to-end store of a foreign value, hard refusal, metadata-only persistence, and the stale wasm assertions |

## Example 1: Registering a foreign type in `liquers-web`

### Connection to the High-Level Design

This is Phase 1's purpose in its entirety: `js.Value` is known only at runtime to `liquers-lib`, but
it is known *statically* to `liquers-web`, which is the crate that owns the implementation. The
design's whole move is to let that crate say so at the one moment the registry accepts writes.

### Scenario

A JavaScript caller evaluates a query that produces a retained DOM node or a chart object, and the
result is cached as an asset. Before this change the cache write fails with
`Type identifier 'js.Value' is not registered in this build`. After it, the asset stores as metadata
only — the value has no byte form — and evaluation carries on.

### Sequence of Steps

1. `new_environment()` builds the base registry from the value type: `TypeRegistry::from_value_type::<Value>()`.
2. It registers the one type the value type cannot describe: `js_value_type_info()`.
3. It hands the finished registry to `WebEnvironment::new_with_type_registry`.
4. Built-in commands and the recipe provider are registered as they are today.
5. `to_ref()` shares the environment; the registry is frozen from here and needs no lock.
6. A later `AssetManager::set_state` carrying a `js.Value` passes `validate_metadata_hard` because
   the identifier is present, and passes the *format* check because the type declares no formats.

### Core Example Code

```rust
// liquers-web/src/value.rs — the single construction site for the description.
pub const JS_VALUE_TYPE_IDENTIFIER: &str = "js.Value";

pub fn js_value_type_info() -> TypeInfo {
    TypeInfo::new(JS_VALUE_TYPE_IDENTIFIER)
        .with_type_name("JsValue")
        .with_defaults("json", "json", "application/json", "value.json")
    // No .with_data_formats: JsOpaque::as_bytes refuses.
}

// liquers-web/src/environment.rs
pub fn new_environment() -> Result<WebEnvironment, Error> {
    let mut types = TypeRegistry::from_value_type::<Value>();
    types.register(crate::value::js_value_type_info())?;

    let mut env = WebEnvironment::new_with_type_registry(types);
    crate::builtins::register_builtin_commands(&mut env)?;
    env.with_default_recipe_provider();
    Ok(env)
}
```

**What is deliberately absent:** any entry in `REGISTERED_SPECS`, any retained declaration, any
replay step. `rebuild_with` and `rebuild_without` both begin with `new_environment()`, so the
registration is reconstructed on every rebuild. Retaining it would add a second source of truth for
something that never varies at runtime.

### Guide and Executable Example

`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §VALUE will carry this snippet and link
`liquers-lib/tests/foreign_value_registration.rs` (Scenario 2's test) as the executable form, since
that one runs natively and a reader can execute it without a wasm toolchain.

**Expected output:** none — success is the absence of an error. The observable difference is that
`set_state` returns `Ok(())` where it previously returned
`[General] Type identifier 'js.Value' is not registered in this build`.

## Example 2: Proving it natively, and the Python route

### What this adds to Scenario 1

Scenario 1 cannot be run in the ordinary test loop: `liquers-web` is `wasm32`-only and its suites
need a separate target and harness. Scenario 2 shows the same guarantee proved with a **mock
`ForeignValue`** on native — the technique that verified the bug in the first place — and then shows
why `liquers-py`, which also holds foreign values, needs none of this machinery.

### The native mock

```rust
// liquers-lib/tests/foreign_value_registration.rs
const MOCK_TYPE_IDENTIFIER: &str = "mock.Value";

#[derive(Debug)]
struct MockForeign;

impl ForeignValue for MockForeign {
    fn origin(&self) -> &'static str { "mock" }
    fn as_any(&self) -> &dyn core::any::Any { self }
    fn identifier(&self) -> Cow<'static, str> { MOCK_TYPE_IDENTIFIER.into() }
    fn type_name(&self) -> Cow<'static, str> { "MockObject".into() }
    fn default_extension(&self) -> Cow<'static, str> { "json".into() }
    fn default_filename(&self) -> Cow<'static, str> { "value.json".into() }
    fn default_media_type(&self) -> Cow<'static, str> { "application/json".into() }
    // type_info() is not implemented: the default derives it from the six methods above.
}

fn environment_knowing_the_mock() -> DefaultEnvironment<Value> {
    let mut types = TypeRegistry::from_value_type::<Value>();
    types.register(MockForeign.type_info()).expect("a fresh identifier");
    let mut env = DefaultEnvironment::new_with_type_registry(types);
    env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
    env
}
```

`MOCK_TYPE_IDENTIFIER` is `mock.Value`, not `js.Value`: the test must not depend on a name another
crate owns, and a provider prefix keeps it inside the naming rule.

### The Python route, and why it is different

`liquers-py` holds Python objects in `Value::Py`, which is a variant of a value type **it owns**. So
its identifier is statically knowable and belongs in `type_descriptions()` — no constructor, no
registration call, no runtime step:

```rust
// liquers-py/src/value.rs
pub const PY_OBJECT_TYPE_IDENTIFIER: &str = "py.Object";

impl ValueInterface for Value {
    fn type_descriptions() -> Vec<TypeInfo> {
        vec![
            // … one entry per variant, identifiers matching liquers-core's …
            TypeInfo::new(PY_OBJECT_TYPE_IDENTIFIER)
                .with_type_name("python_object")
                .with_defaults("json", "json", "application/json", "value.json"),
        ]
    }
}
```

**The distinction worth remembering:** the constructor route is for a type whose implementation
lives in a *different crate* from its value type. That is `js.Value` and nothing else today.
Reaching for the constructor when `type_descriptions()` would do adds a runtime step for a
compile-time fact.

## Example 3: Four ways to get it wrong

### 3.1 Starting from an empty registry

```rust
let mut types = TypeRegistry::new();          // WRONG
types.register(js_value_type_info())?;
```

**Symptom:** every write fails except `js.Value` — including error states, with
`Type identifier 'error' is not registered in this build`.
**Cause:** `TypeRegistry::new()` is empty; `from_value_type::<V>()` is what adds `V`'s descriptions
*and* the `error` pseudo-type.
**Correct:** always extend `TypeRegistry::from_value_type::<V>()`.
**Test:** `fvt7.5`.

### 3.2 Registering in the replay list instead of the constructor

**Symptom:** the foreign type works until the first command is registered after evaluation begins,
then stops — a rebuild produced an environment without it.
**Cause:** `REGISTERED_SPECS` and `STORE_CONFIG` retain runtime-varying declarations, and a rebuild
replays those; anything registered *outside* `new_environment()` and outside that list is lost.
**Correct:** register inside `new_environment()`, which every rebuild path calls.
**Test:** `fvt8.2` asserts the registry still contains the type after a rebuild.

### 3.3 Letting the constant and the instance drift

```rust
impl ForeignValue for JsOpaque {
    fn identifier(&self) -> Cow<'static, str> { "js.Object".into() }   // WRONG: not the constant
}
```

**Symptom:** exactly the bug this design fixes — `Type identifier 'js.Object' is not registered` —
but now caused by a typo rather than by a structural gap, and therefore much harder to find.
**Cause:** two spellings of one fact. The type system cannot close this (a default trait body is
checked with `Self: ?Sized` and cannot call an associated function), which is why the guarantee is a
constant plus a test.
**Correct:** both spellings read `JS_VALUE_TYPE_IDENTIFIER`.
**Test:** `fvt5.1` — the check the user chose in place of compile-time enforcement.

### 3.4 Declaring a data format a type cannot produce

```rust
TypeInfo::new(JS_VALUE_TYPE_IDENTIFIER)
    .with_data_formats(["json"])            // WRONG while as_bytes refuses
```

**Symptom:** `set_binary` accepts bytes for a `js.Value` asset that can never be materialized; the
failure moves from write time to read time, which is worse.
**Cause:** `supported_data_formats` declares what `as_bytes` **accepts**, not what would be nice.
**Correct:** declare nothing while `as_bytes` refuses; the write path exempts a formatless type from
the format check exactly as it exempts a UI element.
**Test:** `fvt3.1` asserts the default declares no formats; `fvt3.2` asserts an override that *can*
serialize is honoured.

## Corner Cases

### 1. Memory

The registry holds a few tens of `TypeInfo` values, each a handful of `Cow<'static, str>`. A
`liquers-web` rebuild clones and rebuilds it; that cost is noise beside the asset cache the rebuild
already discards. **No test needed**, recorded so nobody optimises it.

Worth stating because it is easy to fear: registering `js.Value` pins **nothing**. A `TypeInfo` is
strings. It is the *value* — `Arc<dyn ForeignValue>` holding a `JsValue` — that pins a DOM subtree
for the asset's lifetime, and that behaviour is unchanged by this design.

### 2. Concurrency

The registry is immutable once the environment is constructed, so `get_type_registry(&self)` hands
out a shared reference with no lock from any thread. This design **narrows** rather than widens the
concurrency surface: it was already true, and choosing the constructor over a post-construction
registration point is what kept it true.

On `wasm32` the borrow rule applies — no `RefCell` borrow across an `await` or a call into
JavaScript. Registration happens synchronously inside `new_environment()`, before the environment
reaches `PENDING_ENV`, so no borrow is held across anything. **Test:** the existing `ENVIRON` suite
covers the borrow discipline; no new test.

### 3. Errors

| Scenario | Expected | Test |
|---|---|---|
| The same identifier registered twice | `Error::general_error` naming the identifier and the existing type; `new_environment()` propagates it as a construction failure | `fvt1.2` |
| An unregistered foreign type is stored | `Error::general_error`, "is not registered in this build" — **still refused**, per the Phase 1 decision | `fvt7.1` |
| The base registry was not used | `error` is missing, so even an errored asset cannot be stored | `fvt7.5` |
| A `liquers-py` conversion has no meaning for a variant | `Error::conversion_error` | `fvt6.4` |

### 4. Serialization

Nothing new is serialized, so there is no round-trip to test. The one serialization-adjacent claim
that *does* need a test is the one Phase 1 made and nobody has verified: that a registered
formatless value **persists as metadata only** rather than failing or storing empty bytes.
`fvt7.3` reads the asset back and checks what is actually there.

### 5. Integration (cross-crate)

| Interaction | Expected | Test |
|---|---|---|
| Store | Metadata-only persistence through `AsyncMemoryStore` | `fvt7.3` |
| Commands | Unchanged — no command is added, so `specs/command_registry.yaml` must not move | `cargo test -p liquers-lib --test registry_export` |
| Assets | `set_state` accepts a registered foreign value | `fvt7.2` |
| Web/API | No endpoint change | — |
| Feature matrix | `ExtValue::type_info` matches over variants gated three ways | `scripts/check-build-matrix.sh` |
| Python | The crate compiles with `value` and `context` declared | `cargo check -p liquers-py --lib` |

## Test Plan

### Unit tests

| ID | File | Test | Checks |
|---|---|---|---|
| fvt1.1 | `liquers-core/src/type_system.rs` | `a_base_registry_can_be_extended` | `from_value_type` + `register` of a `provider.Local` type; `contains` is true and the base types survive |
| fvt1.2 | `liquers-core/src/type_system.rs` | `a_duplicate_foreign_registration_is_refused` | Second `register` of the same identifier errors and the message names it |
| fvt2.1 | `liquers-core/src/context.rs` | `new_matches_new_with_the_default_registry` | `SimpleEnvironment::new()` and `new_with_type_registry(from_value_type::<V>())` describe the same types — proves the delegation |
| fvt2.2 | `liquers-core/src/context.rs` | `a_supplied_registry_is_what_the_environment_reports` | An extra type passed at construction is visible through `get_type_registry` |
| fvt3.1 | `liquers-lib/src/value/foreign.rs` | `the_default_type_info_derives_from_the_value` | Mock's `type_info()` identifier, `type_name` and four defaults match its methods; `supported_data_formats` is **empty** |
| fvt3.2 | `liquers-lib/src/value/foreign.rs` | `an_implementation_that_serializes_can_declare_formats` | A mock overriding `type_info` with `with_data_formats(["json"])` reports `supports_data_format("json")` |
| fvt4.1 | `liquers-lib/src/value/extended.rs` | `type_info_still_finds_a_described_type` | `Image` resolves to its declared `TypeInfo` — the routing changes nothing for existing types |
| fvt4.2 | `liquers-lib/src/value/extended.rs` | `type_info_delegates_to_the_foreign_value` | A `Foreign` value reports the foreign value's own description, not the generic fallback |
| fvt5.1 | `liquers-web/src/value.rs` | `the_constant_and_the_instance_agree` | `js_value_type_info().type_identifier`, `JsOpaque::new(..).identifier()` and `JS_VALUE_TYPE_IDENTIFIER` are all equal — **the guarantee chosen in place of compile-time enforcement** |
| fvt6.1 | `liquers-py/src/value.rs` | `type_descriptions_match_identifier` | One description per variant, no more and no less; identifiers and defaults agree — mirrors `liquers-core`'s `vts7.1` |
| fvt6.2 | `liquers-py/src/value.rs` | `identifiers_follow_the_naming_rule` | Every identifier is alphanumeric with at most one dot and a lowercase provider — the check `python_value` fails today |
| fvt6.3 | `liquers-py/src/value.rs` | `shared_variants_match_the_core_identifiers` | `Text`→`Text`, `Bytes`→`Bytes`, … agree with `liquers_core::value::Value`, so a store written from Python is readable from Rust |
| fvt6.4 | `liquers-py/src/value.rs` | `repaired_conversions_error_rather_than_panic` | `try_into_bytes`/`try_into_key`/`try_into_command_metadata` return `conversion_error` on the wrong variant; no `todo!()` remains |

### Integration tests

**New file:** `liquers-lib/tests/foreign_value_registration.rs` (native, `#[tokio::test]`).

| ID | Test | Checks | Before the change |
|---|---|---|---|
| fvt7.1 | `an_unregistered_foreign_value_is_refused` | Environment built with plain `new()`; `set_state` errors, message names `mock.Value` | Passes — this **records the hard-refusal decision** rather than proving the fix |
| fvt7.2 | `a_registered_foreign_value_can_be_stored` | Environment built with `new_with_type_registry`; `set_state` returns `Ok` | **Does not compile** — the constructor does not exist |
| fvt7.3 | `a_registered_foreign_value_persists_as_metadata_only` | Read the key back: metadata present with `mock.Value`; asking for the value degrades rather than returning wrong data | **Does not compile** |
| fvt7.4 | `the_refusal_names_the_identifier` | The `fvt7.1` message contains `mock.Value` and "not registered" | Passes |
| fvt7.5 | `an_empty_base_registry_loses_the_error_type` | `TypeRegistry::new()` + the mock only; storing an **error** state fails naming `error` | **Does not compile** |

**`liquers-web` conformance** (wasm32, Node):

| ID | Test | Checks |
|---|---|---|
| fvt8.1 | `ENVIRON` — `the_environment_knows_the_javascript_type` | `build_environment()?.get_type_registry().contains("js.Value")` |
| fvt8.2 | `ENVIRON` — `a_rebuild_keeps_the_javascript_type` | Register a command after evaluation has begun, forcing a rebuild; the registry still contains `js.Value` — the pitfall in 3.2 |
| VALUE04 | *repair* | `value_bridge_VALUE.rs:156` and `second_value_type.rs:324`, `:336` |
| VALUE13 | *repair* | `value_bridge_VALUE.rs:343` |

### A finding: `WEB-VALUE04` is understated

The issue reports **one** failing assertion. Reading the suites turned up **four**, all the same
class — a test asserting an identifier spelling that `value-type-system` changed:

| Location | Asserts | Should be | Why |
|---|---|---|---|
| `second_value_type.rs:324` | `"bytes"` | `"Bytes"` | The one the issue names |
| `second_value_type.rs:336` | `assert_ne!(…, "bytes")` | `"Bytes"` | Passes vacuously today — it compares against a string nothing produces, so it would keep passing even if text *did* become bytes |
| `value_bridge_VALUE.rs:156` | `"bytes"` | `"Bytes"` | `SimpleValue::Bytes.identifier()` is `"Bytes"` (`simple.rs:170`) |
| `value_bridge_VALUE.rs:343` | `"js"` | `"js.Value"` | `JsOpaque::identifier()` is `"js.Value"`; a **bare** `js` would violate the naming rule outright, since bare names are reserved for core and lib |

**Derived from reading, not from a run** — the same standing the original issue had, and for the
same reason: `wasm32-unknown-unknown` is not installed in this environment. The first Phase 4 step
is to install it, run the suite, and confirm the count. `:336` is the one worth dwelling on: a
vacuous assertion is worse than a failing one, because nothing announces it.

## Documentation and Learning Log

### Guide candidate workflows and examples

- **How do I type an integrated value?** Scenario 1 end to end, plus the `liquers-py` contrast from
  Scenario 2 — the constructor route versus `type_descriptions()`, and how to tell which you need.
- **What is the typical workflow?** Extend the base registry → construct → freeze. The three-word
  version belongs in `TYPE_SYSTEM_GUIDE.md`; the worked version in `LANGUAGE-INTEGRATION_GUIDE.md`.
- **Executable link:** `liquers-lib/tests/foreign_value_registration.rs`, not a demo binary.

### Usage, meaning, and connections

- The registry is a property of the **build**, and this design makes that literal: an integration
  contributes to it exactly once, at construction.
- The one-identifier-per-variant rule is what makes "which variant is this?" answerable from a
  stored string, and it is why `type_name` exists to carry everything that varies per instance.
- A formatless type is a first-class case, not a degradation: UI elements, egui widgets and foreign
  handles all live there, and the write path has an explicit exemption for them.

### Repeatable development guidance

- Verifying a wasm-only claim natively with a mock implementation of the trait — the technique that
  settled this issue's "not verified" caveat in minutes rather than after a toolchain install.
- `scripts/check-build-matrix.sh` after touching any `match` over `ExtValue`, because a gated arm
  compiles fine under default features and breaks the minimal or wasm build.

### Corrections and unexpected learning

- **Corrected in Phase 2:** `liquers-py` needs no constructor; six constructors became five.
- **Corrected in Phase 2:** `liquers-py`'s `from_asset_info` is `todo!()` against a `Vec` signature,
  forcing an `AssetInfo` variant the earlier phases had not anticipated.
- **Corrected here:** `WEB-VALUE04` is one of four stale assertions, and one of the four is vacuous.
- **Useful dead end, recorded so it is not retried:** `ForeignValue::type_info` as a
  `where Self: Sized` associated function. Object-safe, but no useful default and unreachable
  through the trait object.
- The accumulated material is still well inside what a Phase 5 summary plus two guide extensions can
  carry, so Phase 1's "no new reference, no new guide" decision stands.

## Manual Validation

```bash
cargo test -p liquers-lib --lib --tests                      # the native loop, incl. fvt3–fvt4, fvt7
cargo test -p liquers-core --lib                             # fvt1, fvt2
cargo check -p liquers-py --lib                              # the repaired module compiles
cargo test -p liquers-lib --test registry_export             # no command changed
bash scripts/check-build-matrix.sh                           # gated match arms, 11 configurations

rustup target add wasm32-unknown-unknown                     # not installed here
cargo clean                                                  # the wasm loop needs the disk
cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
```

The wasm loop is expected to go from **four failing assertions to zero**, and to gain `fvt8.1`
and `fvt8.2`.

## Review record

The workflow specifies five drafting agents and three reviewers. This host does not spawn subagents
for this session, so the passes were run sequentially against the codebase, as the skill's
host-compatibility section provides for.

**Reviewer 1 — Phase 1 conformity.** No drift. Every test traces to a Phase 1 decision: `fvt7.1` to
hard refusal, `fvt5.1` to the constant-plus-test guarantee, `fvt6.3` to one-identifier-per-variant,
`fvt7.5` to extend-a-base. No example introduces behaviour Phase 1 did not sanction.

**Reviewer 2 — Phase 2 conformity.** Signatures in the examples match Phase 2 exactly:
`new_with_type_registry`, `ForeignValue::type_info(&self)` with a default, the free
`js_value_type_info()`, `PY_OBJECT_TYPE_IDENTIFIER`. One gap found and closed: Phase 2 asserted
metadata-only persistence for a formatless type without proposing a test, so `fvt7.3` was added to
verify rather than assume it.

**Reviewer 3 — Codebase and query validation.** No queries appear anywhere in this design — no
`-R/`, no action chain, no recipe — so `liquers-validate` has nothing to check, and no store needs
to exist for any example. Three code-level findings, all folded in above: `SimpleValue::Bytes`
reports `"Bytes"` (`simple.rs:170`) so `value_bridge_VALUE.rs:156` is stale too; `JsOpaque` reports
`"js.Value"` so `:343` is stale and its expected value would break the naming rule; and
`second_value_type.rs:336` is an `assert_ne!` against a string nothing produces, so it passes
vacuously.
