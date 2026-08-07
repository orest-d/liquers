# Phase 3: Examples & Use-cases - liquers-web

## Example Type

**Conceptual code, with verified queries.** Runnable prototypes are not possible: `liquers-web`
does not exist until Phase 4. The examples below therefore show intended usage rather than compiled
code — but nothing hand-wavy is smuggled in under that licence:

- **Every query is real** and was checked with `liquers-validate`, including its *resolved plan*,
  not merely its parse status. See "Query validation" below.
- **Every claim about JavaScript semantics** was measured with `node` (argument introspection in
  Phase 2; the conversion edge cases here).
- The one genuinely runnable artefact of this phase is the **test inventory**, which is the real
  deliverable — Phase 4 implements against it.

## Overview Table

| # | Type | Name | Demonstrates / checks |
|---|---|---|---|
| 1 | Example | Quick start from a plain page | `init` → `registerCommand` → `evaluate` with no bundler; the `PACKAGE03` path |
| 2 | Example | Async command fetching from a server | `ASYNCCMD` as the motivating case; Promise `run`, structured rejection |
| 3 | Example | Opaque value lifecycle | Decision 2 end to end: opt-in, pass-through, serialization failure, asset pinning |
| 4 | Example | Custom value type (Tier 2) | The extensibility promise from Phase 2 actually exercised |
| 5 | Unit tests | `liquers-core` unregister | `CommandRegistry::unregister`, `remove_command` — native, fast feedback |
| 6 | Unit tests | Conversion + metadata | `VALUE*`, `OBJECT*`, `ERROR*` under `wasm-bindgen-test` |
| 7 | Integration | Command + evaluation | `COMMAND*`, `EVAL*`, `ASYNCQ*`, `ASYNCCMD*`, `RUNTIME*` in headless Chromium |
| 8 | Integration | Delivery | `PACKAGE*`, `STUBS*` — Playwright against the built artefact |
| 9 | Corner cases | Memory, reentrancy, concurrency, serialization | The failure modes Phases 1-2 identified |
| 10 | Benchmark | Boundary cost | Phase 1 decision 2's unmeasured hypothesis |

## Query validation

Every query used below, checked with `liquers-validate` against the real registry plus an overlay
declaring the fixture commands (`--command hello --command repeat --command shout --command number`):

| Query | Status | Resolved meaning (read from the plan, not assumed) |
|---|---|---|
| `hello` | Ok | one action, `hello()`, no parameters |
| `hello/repeat-3` | Ok | `hello()` then `repeat("3")` — parameter is the string `"3"` |
| `number-42` | Ok | `number("42")` — a *source* command parameterised |
| `hello/to_text` | Ok | `hello()` then the existing root `to_text()` |
| `hello/ns-myapp/shout` | Ok | `hello()`, then `ns("myapp")`, then `shout()` — `ns-myapp` is an action named `ns` taking the namespace as its parameter |
| `hello/shout` | Ok | `hello()` then `shout()` |
| `load_table/row_count` | Ok | `load_table()` then `row_count()` |
| `fetch_json-~Hapi.example.com~/data/to_text` | Ok | parameter decodes to exactly `https://api.example.com/data`, then `to_text()` |
| `fetch_json-https%3A%2F%2F…` | **Error** | percent-encoding is not in the grammar — `%` fails at offset 16. Recorded because the first draft of Example 2 assumed it worked |

**Validation caught a real defect**, not just an example typo — see Example 2 and
`PARAMETER-ESCAPING-INCOMPLETE` in [`specs/ISSUES.md`](../ISSUES.md).

**Why these shapes.** Per the guide's Appendix A, *every query segment is a command* — there is no
literal-value segment. Test input therefore comes from a **source command** (`hello`, `number-42`),
never from a bare literal. `hello/repeat-3` is the canonical transform-with-parameter shape.

---

## Example 1: Quick start from a plain page

**Scenario.** A developer adds Liquers to an existing page with no build step — the delivery form
decision 7 made the requirement.

```html
<script type="module">
  import init, * as liquers from "./liquers_web.js";

  await init();                       // ENVIRON: Promise, never a blocking initializer

  liquers.registerCommand({
    name: "shout",
    run: (text) => text.toUpperCase(),   // arguments inferred: none beyond state
    state: "text",
  });

  liquers.registerCommand({
    name: "repeat",
    run: (text, count) => text.repeat(count),
    arguments: [{ name: "count", type: "int", default: 2 }],   // explicit: reliable path
    state: "text",
  });

  console.log(await liquers.evaluate("hello/shout"));    // "HELLO, WORLD!"
  console.log(await liquers.evaluate("hello/repeat-3"));
</script>
```

**What it demonstrates.** The minimal declaration is one line plus `run` (`COMMAND09`); the singleton
is the default path (decision 4); `evaluate` returns a Promise (`ASYNCQ01`); no bundler is involved
(`PACKAGE03`). `shout` takes no declared `arguments` and needs none — inference over the safe subset
sees `(text)`, drops the state token, and yields zero arguments.

**Corner it exposes.** `repeat` declares `arguments` explicitly *because* its JS default `count = 2`
would be refused by inference (Phase 2's identifier-only rule). This is the ergonomic/reliable split
in one screen, and the example is written to show the split rather than hide it.

---

## Example 2: Async command fetching from a server

**Scenario.** The motivating case for promoting `ASYNCCMD` into the initial phase (decision 6): a
browser command that fetches data.

```javascript
liquers.registerCommand({
  name: "fetch_json",
  arguments: [{ name: "url", type: "string" }],
  doc: "Fetch JSON from a URL.",
  run: async (url) => {                       // no state → a source command
    const response = await fetch(url);
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    return await response.json();             // structurally converted
  },
});

// `~H` is the `https://` entity and `~/` the slash entity — verified to decode to
// exactly "https://api.example.com/data".
const data = await liquers.evaluate("fetch_json-~Hapi.example.com~/data/to_text");
```

**What it demonstrates.** `run` returning a Promise is awaited by the executor (`ASYNCCMD01`); the
result is structurally converted so *Rust* commands like `to_text` can consume it — the reason the
default is structural rather than opaque. A thrown `Error` becomes a Liquers `ExecutionError`
carrying class and stack (`ASYNCCMD02`, `ERROR03`).

**Corner it exposes — and a `liquers-core` defect found while validating it.** A URL argument
contains `/` and `:`, which are query syntax. Liquers has a purpose-built escape scheme (tilde
entities), and `~H` is a dedicated `https://` entity, so the query above is expressible and was
verified to decode correctly. **Percent-encoding does not work** — `%` is not in the grammar at all,
which is what the first draft of this example wrongly assumed.

But the Rust encoder does not produce this form. `encode_token` (`query.rs:503`) escapes only `~`,
space, `/` and `-`; it passes `:` through unchanged, while the parser's unescaped set is
alphanumerics plus `_`, `+`, `.`. Measured:

| Input to a parameter | Parses? |
|---|---|
| `f-a~Pb` (`://` entity) | Ok |
| `f-a~.b`, `f-a~_b`, `f-a~~b`, `f-a+b`, `f-a.b` | Ok |
| `f-a:b` — what `encode_token("a:b")` emits | **ParseError** |
| `fetch_json-https:~/~/…` — what `encode_token` emits for a URL | **ParseError** |

So `encode_token` is **not round-trip safe** for any string containing a colon, and separately there
is **no lone-colon entity** in the grammar — `~P` covers `://` only, so a value like `12:30` cannot
be encoded by any encoder. Filed as `PARAMETER-ESCAPING-INCOMPLETE` in [`specs/ISSUES.md`](../ISSUES.md).

**Consequence for this design:** the planned JS helper must *not* be a port of `encode_token`, which
would inherit the defect. Phase 4 either fixes `encode_token` first and mirrors it, or implements
`liquers.encodeParam` against the entity table directly. A test asserts round-tripping of a URL, a
lone colon, a space and a leading minus.

---

## Example 3: Opaque value lifecycle

**Scenario.** A third-party JS object passes through a Liquers pipeline — Phase 1 decision 2 end to
end.

```javascript
liquers.registerCommand({
  name: "load_table",
  run: () => liquers.opaque(new ArrowTable(buffer)),   // explicit opt-in
});

liquers.registerCommand({
  name: "row_count",
  state: "value",
  run: (table) => table.numRows,        // receives the original object, by identity
});

await liquers.evaluate("load_table/row_count");    // 1000
```

**What it demonstrates.** Opt-in opacity (never accidental); the object arrives at the second
command as the *same* JS object, so no O(size) conversion happens on a value merely passing through
(the performance argument); `identifier()` is `"js"` and `type_name()` is the captured
`constructor.name`, so metadata stays debuggable. Under the hood the value is
`ExtValue::Foreign(JsOpaque)`, and `row_count` recovers it by downcasting — a value from a different
language runtime would fail that downcast and produce a `ConversionError` naming its `origin()`.

**Corners it exposes**, each with a test:

1. `await liquers.evaluate("load_table/to_text")` → `ConversionError` naming `ArrowTable`. Opaque
   values do not coerce to text.
2. The asset holding the opaque value persists **metadata only** — `as_bytes` fails, and the core
   already tolerates that (`assets.rs:3016`). The asset's version is time-based
   (`assets.rs:2994`), so it looks freshly changed to dependency tracking.
3. **The pinning consequence:** while that asset is cached, the `ArrowTable` and its buffer stay
   alive. This is the cost the explicit opt-in exists to make visible.
4. Mutating `table` after caching retroactively changes the asset's value — accepted by discipline
   (decision 2), and asserted as *documented behaviour* so a future change to it is deliberate.

---

## Example 4: Custom value type (Tier 2 extensibility)

**Scenario.** A downstream crate needs Rust commands to operate structurally on a third-party type,
so the opaque path is not enough. This example exists to prove Phase 2's extensibility promise
rather than assert it.

```rust
// my-app-web/src/lib.rs — a downstream crate depending on liquers-web
pub enum MyExt { Matrix(Arc<Matrix>) }

impl ValueExtension for MyExt { /* identifier, type_name, … */ }
pub type MyValue = CombinedValue<SimpleValue, MyExt>;

impl JsValueBridge for MyValue {
    fn from_js_custom(js: &JsValue) -> Result<Option<Self>, Error> { /* recognise a Matrix */ }
    fn to_js_custom(&self) -> Result<Option<JsValue>, Error> { /* … */ }
    fn from_js_opaque(js: JsValue, tag: Arc<str>) -> Result<Self, Error> { /* … */ }
}

#[wasm_bindgen]
pub struct MyEnvironment { envref: EnvRef<DefaultEnvironment<MyValue, ()>> }

#[wasm_bindgen]
impl MyEnvironment {
    pub fn evaluate(&self, q: &str) -> js_sys::Promise {
        liquers_web::evaluate_to_promise(self.envref.clone(), parse_query(q)?)   // reused
    }
}
```

**What it demonstrates.** The downstream crate writes its value type, its `JsValueBridge` impl and
its `#[wasm_bindgen]` wrapper — and reuses the conversion layer, command adapter and Promise bridge
unchanged. This is why Phase 2 forbids those modules from naming `liquers_lib::value::Value`
concretely.

**Corner it exposes.** Two independently compiled wasm modules do **not** share an environment —
a documented limitation, not a bug (`POLYGLOT` is `NA`).

---

## Corner Cases

| # | Corner case | Risk | Handling | Test |
|---|---|---|---|---|
| C1 | Re-entrant `evaluate` from a sync JS command | Deadlock on the single-threaded loop | **Cannot arise:** JS cannot block on a Promise, so the command either *returns* it (`IsAsync::Auto` awaits it — effectively async) or ignores it (nested evaluation resolves independently). Both asserted | `RUNTIME04` |
| C2 | Re-entrant `evaluate` from an **async** JS command | Should work | Yields to the event loop; nested evaluation is a separate task | `RUNTIME04`, `ASYNCCMD04` |
| C3 | `RefCell` borrow held across `await` | `already borrowed` panic | Rule: clone the `EnvRef` out, drop the borrow before any `await` or JS call | `RUNTIME04` |
| C4 | `i64` beyond 2^53 | Silent precision loss | Checked conversion; `BigInt` is the lossless path; out-of-range is `ConversionError` | `VALUE03` |
| C5 | `Uint8Array` treated as text | Corrupt data | Bytes map to `Value::Bytes`, never `Text` | `VALUE04` |
| C6 | Cyclic JS object | Infinite recursion / stack overflow | Depth-limited; cycle raises `ConversionError` naming the path | `VALUE07` |
| C7 | `null` vs `undefined` | Conflation | Both → `None`; documented as lossy and asserted | `VALUE01` |
| C8 | JS throws a non-`Error` (`throw 42`) | Panic or opaque failure | Coerced to string, `ExecutionError` | `ERROR04` |
| C9 | Rust panic reaching JS | Aborts the wasm instance | `console_error_panic_hook`; entry points return `Result` | `ERROR05`, `RUNTIME06` |
| C10 | Command registered then unregistered mid-flight | Executor gone while a query plans | Unregister clears metadata *and* both executor maps together, so it fails at plan time | `COMMAND06` |
| C11 | Command closure retains a DOM node | Leak for the environment's lifetime | `unregister` drops the `Arc`, releasing the `Function` | `RUNTIME05` |
| C12 | Replacing a Rust command | Built-in destroyed irrecoverably | Permitted, warned; documented limitation | `COMMAND06` |
| C13 | Opaque value reaches the store | Silent data loss | `as_bytes` fails → metadata-only persistence; core already tolerates | `VALUE06` |
| C14 | Minified inferred argument names | Wrong labels | Arity holds, binding is positional; heuristic `console.warn` | `COMMAND05` |
| C15 | Two concurrent `evaluate` calls | Interleaving corruption | Independent assets; both must progress | `ASYNCQ03` |
| C16 | `init()` called twice | Duplicate environments | Idempotent; resolves with the existing singleton | `ENVIRON03` |
| C17 | Failed `init()` then retry | Unrecoverable module | Singleton left unset on failure | `ENVIRON04` |
| C18 | `.free()` then use | Use-after-free | wasm-bindgen throws; documented, not reimplemented | `OBJECT05` |

---

## Test Plan

Four tiers, chosen by what each test actually needs rather than by convenience:

| Tier | Harness | Command | Contains |
|---|---|---|---|
| **N** | native Rust | `cargo test -p liquers-core` / `-p liquers-lib` | `unregister01-04`, `RUNTIME01` — the only tests that need no browser, and the fastest loop in the project |
| **W** | `wasm-bindgen-test`, headless Chromium | `wasm-pack test --headless --chrome` | The bulk: everything touching `JsValue`, Promises or the registry |
| **C** | CI build step | `cargo check` matrix, `tsc --noEmit`, artefact inspection | `STUBS01-06`, `PACKAGE01/04/05`, `web_build_matrix` |
| **P** | Playwright | `npx playwright test` | `PACKAGE02/03/07`, `STUBS07` — the built artefact in a real page |

**Why most tests are W and not N.** `JsValue` exists on native (wasm-bindgen compiles there) but
every operation on it panics at runtime, so a "native conversion test" would test nothing. Only
tests that touch no `JsValue` — the `liquers-core` registry work, and `RUNTIME01`'s static
assertion — can run natively, and those are placed there deliberately because they are the fast
feedback loop.

**Ordering.** N and C tiers gate first (seconds, no browser). W and P run after `cargo clean`,
separately from the native loop, per `CLAUDE.md`'s disk-allowance constraint.

## Test Inventory

Per the guide, every prescribed test is marked **required** or **`NA` with a reason**. Naming
follows the guide: Rust `fn value01_primitive_roundtrip()`, browser
`test("ASYNCQ02 promise rejects with structured error", …)`.

### Tier 1 — `liquers-core` unit tests (native, `cargo test -p liquers-core`)

The only tests that run without a browser, and the fastest feedback loop in the project.

| Test | Checks |
|---|---|
| `unregister01_removes_metadata_and_executors` | After `unregister`, the query fails to **plan** — proves all three stores cleared together |
| `unregister02_absent_is_false_not_error` | Idempotent |
| `unregister03_reregister_resets_impl_version` | The documented divergence from replace |
| `unregister04_async_and_sync_both_removed` | Both executor maps |

### Tier 2 — the complete conformance inventory

**All 83 prescribed tests for the 11 selected features are enumerated below, named, and assigned a
tier. 82 are required; one is `NA`.** The guide demands a disposition for every prescribed test;
this table *is* that disposition, and Phase 4 implements it row by row.

**Naming scheme**, per the guide §3 — the logical ID leads the test-specific part of the name:

| Harness | Form |
|---|---|
| Rust (`wasm-bindgen-test` and native) | `fn value01_primitive_roundtrip()` |
| Playwright / TypeScript | `test("PACKAGE03 quickstart evaluates a query end to end", …)` |
| File/module names | carry the feature ID: `tests/value_bridge_VALUE.rs`, `tests/commands_COMMAND.rs` |

Tiers: **N** native Rust · **W** `wasm-bindgen-test` in headless Chromium · **C** CI build step ·
**P** Playwright against the built artefact.

| ID | Test name | Tier | Note |
|---|---|---|---|
| OBJECT01 | `object01_query_parse_encode_roundtrip` | W | |
| OBJECT02 | `object02_key_equality_and_hash` | W | `equals()`, not `===` |
| OBJECT03 | `object03_command_metadata_roundtrip` | W | |
| OBJECT04 | `object04_invalid_parse_produces_error` | W | |
| OBJECT05 | `object05_wrapper_valid_for_documented_lifetime` | W | incl. use-after-`.free()` (C18) |
| OBJECT06 | `object06_every_enum_variant_roundtrips` | W | all 22 `ErrorType`, all `ArgumentType` |
| OBJECT07 | `object07_unknown_enum_variant_follows_policy` | W | unknown *string from JS* → `ConversionError` naming it — the forward-compatibility policy |
| OBJECT08 | `object08_wrappers_follow_naming_and_ownership_conventions` | W | |
| ERROR01 | `error01_every_error_type_maps` | W | all **22** variants |
| ERROR02 | `error02_fields_survive_rust_language_rust` | W | |
| ERROR03 | `error03_language_exception_includes_class_and_stack` | W | |
| ERROR04 | `error04_non_error_throw_has_safe_fallback` | W | `throw 42`, `throw undefined` |
| ERROR05 | `error05_no_panic_crosses_the_boundary` | W | |
| RUNTIME01 | `runtime01_native_adapter_satisfies_thread_bounds` | **N** | **Not `NA`.** Reinterpreted for this integration: a static assertion in `liquers-lib` that `ExtValue: Send + Sync` still holds *on native* after decision 1's relaxation. This is the test that catches the relaxation accidentally weakening the native build — the one real risk it carries |
| RUNTIME02 | `runtime02_wasm_accepts_non_send_callback` | W | |
| RUNTIME03 | `runtime03_stored_callback_outlives_registration_scope` | W | |
| RUNTIME04 | `runtime04_nested_evaluation_does_not_deadlock` | W | three cases + timeout; see "How the three hardest tests assert" |
| RUNTIME05 | `runtime05_cancellation_and_shutdown_release_handles` | W | handle-count assertion; see same section |
| RUNTIME06 | `runtime06_panic_and_exception_containment` | W | |
| VALUE01 | `value01_primitive_roundtrip` | W | incl. `null`/`undefined` collapse (C7) |
| VALUE02 | `value02_nested_array_object_roundtrip` | W | |
| VALUE03 | `value03_integer_boundaries` | W | 2^53, `BigInt`, out-of-range → `ConversionError` |
| VALUE04 | `value04_bytes_are_not_confused_with_text` | W | |
| VALUE05 | `value05_unknown_object_uses_opaque_value` | W | policy = refuse unless `opaque()` |
| VALUE06 | `value06_opaque_serialization_fails_or_uses_its_codec` | W | |
| VALUE07 | `value07_cycles_follow_policy` | W | |
| VALUE08 | `value08_representative_extvalue_roundtrip` | W | **Corrected.** An earlier draft called this nearly-`NA` for want of a wasm-viable rich variant; Appendix A uses `Query` and `Key` as representatives, and both are available on wasm. Tests `Query`, `Key` **and** `ExtValue::Foreign`, which is a stronger test than the original disposition |
| VALUE09 | `value09_checked_upcast_and_downcast` | W | |
| VALUE10 | `value10_language_only_object_retains_documented_identity` | W | asserts identity holds *and* that it is documented as incidental |
| VALUE11 | `value11_callable_retention_or_rejection_follows_policy` | W | **Not `NA`.** The policy is testable: a bare function → `ConversionError`; `opaque(fn)` → retained |
| VALUE12 | `value12_scalar_operators_produce_documented_result` | W | |
| VALUE13 | `value13_state_operations_preserve_or_discard_metadata` | W | |
| ENVIRON01 | `environ01_default_environment_evaluates_builtin` | W | |
| ENVIRON02 | `environ02_custom_services_are_the_ones_returned` | W | |
| ENVIRON03 | `environ03_repeated_initialization_follows_policy` | W | idempotent `init()` (C16) |
| ENVIRON04 | `environ04_failed_initialization_is_recoverable` | W | (C17) |
| ENVIRON05 | `environ05_isolated_test_environments_do_not_leak_registration` | W | explicit instances |
| ENVIRON06 | `environ06_shutdown_is_idempotent` | W | |
| EVAL01 | `eval01_evaluate_builtin_query` | W | |
| EVAL02 | `eval02_string_and_wrapped_query_agree` | W | |
| EVAL03 | `eval03_metadata_and_logs_available` | W | |
| EVAL04 | `eval04_invalid_query_maps_through_error` | W | |
| EVAL05 | `eval05_payload_and_context_reach_a_command` | W | **Not `NA`.** Scoped to what exists: `Context` reaches the command and is usable; the payload half asserts the documented `Payload = ()` behaviour. Widens when `UIUSE` introduces a real payload |
| EVAL06 | `eval06_cancellation_has_defined_terminal_result` | W | |
| COMMAND01 | `command01_register_and_execute_first_command` | W | **the guide's mandatory end-to-end test** |
| COMMAND02 | `command02_transform_receives_state_and_parameter` | W | `hello/repeat-3` |
| COMMAND03 | `command03_exception_crosses_command_boundary` | W | |
| COMMAND04 | `command04_defaults_enums_and_variadics_bind` | W | |
| COMMAND05 | `command05_metadata_matches_the_declaration` | W | + the inference sub-suite below |
| COMMAND06 | `command06_duplicate_and_unregister_policy` | W | + the namespace sub-suite below |
| COMMAND07 | `command07_context_injection` | W | |
| COMMAND08 | `command08_returned_opaque_value_follows_value_rules` | W | |
| COMMAND09 | `command09_minimal_declaration_has_useful_metadata_defaults` | W | |
| COMMAND10 | `command10_complete_declaration_preserves_every_field` | W | every field in Phase 2's spec object |
| COMMAND11 | `command11_closure_captures_retained_per_runtime_rules` | W | |
| ASYNCQ01 | `asyncq01_await_successful_evaluation` | W | |
| ASYNCQ02 | `asyncq02_failure_rejects_with_structured_error` | W | rejects with `LiquersError`, never a string |
| ASYNCQ03 | `asyncq03_two_evaluations_make_progress` | W | (C15) |
| ASYNCQ04 | `asyncq04_cancellation_propagates` | W | via `LiquersAsset.cancel()` |
| ASYNCQ05 | `asyncq05_dropping_host_handle_follows_policy` | W | incl. Promise pending when the environment is freed |
| ASYNCQ06 | `asyncq06_no_event_loop_blocking` | W | |
| ASYNCQ07 | — | — | **`NA`: JavaScript has a native async model.** Appendix A scopes this test to *languages with no async model*; an earlier draft missed that scoping and reinterpreted it. The useful part of the reinterpretation is kept as `web_no_blocking_api` below, which is not a conformance test |
| ASYNCQ08 | `asyncq08_nested_event_loop_use_is_rejected_or_safe` | W | |
| ASYNCCMD01 | `asynccmd01_async_command_result` | W | |
| ASYNCCMD02 | `asynccmd02_async_exception` | W | |
| ASYNCCMD03 | `asynccmd03_cancellation_in_both_directions` | W | |
| ASYNCCMD04 | `asynccmd04_nested_async_evaluation` | W | (C2) |
| ASYNCCMD05 | `asynccmd05_concurrent_calls_do_not_corrupt_state` | W | |
| ASYNCCMD06 | `asynccmd06_sync_and_async_metadata_differ` | W | `IsAsync` visible in metadata |
| STUBS01 | `STUBS01 declarations exist for every exposed module` | C | |
| STUBS02 | `STUBS02 type checker accepts representative sample` | C | **Not `NA`.** `tsc --noEmit` over a usage sample |
| STUBS03 | `STUBS03 declared names match runtime surface` | C | |
| STUBS04 | `STUBS04 command declaration preserves signature` | C | **Not `NA`, and passes by construction:** `registerCommand` neither wraps nor returns the user's function, so unlike a decorator it *cannot* erase the signature. The test pins that property so a future wrapping API does not silently break it |
| STUBS05 | `STUBS05 async entry points declared awaitable` | C | `evaluate`, `init` declared `Promise<…>` |
| STUBS06 | `STUBS06 type checker rejects incorrect usage` | C | **Not `NA`.** Deliberately wrong spec object must fail `tsc` |
| STUBS07 | `STUBS07 declarations ship in the artifact` | P | |
| PACKAGE01 | `PACKAGE01 clean build produces artifact` | C | |
| PACKAGE02 | `PACKAGE02 install into clean environment loads` | P | fresh browser context, no bundler |
| PACKAGE03 | `PACKAGE03 quickstart evaluates a query end to end` | P | Example 1, zero console errors |
| PACKAGE04 | `PACKAGE04 version metadata matches linked core` | C | **Not `NA`.** `version()` reports crate + linked core version; the check compares against `Cargo.toml` |
| PACKAGE05 | `PACKAGE05 default feature set produces documented value type` | C | |
| PACKAGE06 | — | — | **`NA`: no optional extras exist in this phase.** `liquers-web` exposes no installable extra Cargo feature — the build is one artefact with one feature set. Becomes required the moment any feature is exposed as an extra (npm packaging, `UIUSE`) |
| PACKAGE07 | `PACKAGE07 artifact carries declarations license and metadata` | P | `.d.ts` + LICENSE beside the wasm |

**Count:** 83 prescribed · **81 required** · 2 `NA`, each with the condition that reverses it: `PACKAGE06` (no optional Cargo extra exists yet) and `ASYNCQ07` (scoped by Appendix A to languages with no async model).

The five `NA` marks the first draft carried were all of the kinds the guide now names as
insufficient (§3, "When a prescribed test does not apply"): `STUBS02`/`STUBS06` were excused for
running in CI rather than a browser (harness, not applicability); `EVAL05` for a deferred milestone;
`RUNTIME05` was nearly excused as hard to observe; `RUNTIME01` for literal wording that assumed a
native host; and `VALUE08`/`VALUE11` for having no obvious instance, when `ExtValue::Foreign` and a bare
JS function are exactly the instances required. That experience is what the guide section records.

#### Sub-suites required by Phase 2

These expand two inventory rows rather than adding new IDs, and are named as `COMMAND05_*` /
`COMMAND06_*` so they roll up to the feature the guide prescribes.

**Argument inference** (`command05_infer_*`) — one case per row of Phase 2's verified-behaviour
table:

| Case | Expected |
|---|---|
| `(state, count)` | infers `["count"]` |
| `(state, a, b)` | infers `["a","b"]` |
| `(a, b)` no state | infers `["a","b"]` |
| `(state)` | infers `[]` |
| `function (state /*, hidden */, count)` | infers `["count"]` — comments stripped |
| `(state, count = 2)` | **refuses**, `ParameterError` naming `"count = 2"` |
| `(state, f = (x,y) => x)` | **refuses** |
| `(state, {a, b})` | **refuses** |
| `(state, ...rest)` | **refuses** |
| `fn.bind(null)` | **refuses** — token count ≠ `fn.length` |
| `Math.max` | **refuses** |
| minified `(a,b)` | infers with correct arity; emits the minified-name `console.warn` |
| explicit `arguments` + inferable function | explicit wins; no inference occurs |

**Namespaces** (`command06_ns_*`): root default · explicit namespace · replace-on-duplicate ·
**`console.warn` on every replacement**, asserting *both* message texts (JS-replaces-JS and
JS-replaces-Rust) · registration into the reserved `web` namespace rejected with `ParameterError`.

### Tier 3 — Playwright and CI steps

Tiers **P** and **C** in the inventory above. Playwright mirrors the existing
`liquers-lib/examples-web/` harness; the CI steps are `cargo check` / `tsc --noEmit` / artefact
inspection and need no browser.

### Additional corner-case tests beyond the prescribed inventory

These carry no guide ID because the guide does not prescribe them; they cover failure modes specific
to this architecture, and are named `web_*` so they are distinguishable from conformance tests.

| Test | Checks |
|---|---|
| `web_build_matrix` (C) | `liquers-lib` compiles in **all six** configurations — see infrastructure item 4. No longer the sole guard for the match arms (Option Z made them unconditional, so the compiler enforces them everywhere), but still catches feature-interaction breakage |
| `web_evaluate_before_init` (W) | module-level `evaluate` before `init()` rejects with a clear error, not a panic or a hang |
| `web_promise_after_free` (W) | a Promise still pending when its environment is `.free()`d settles rather than hanging |
| `web_encode_param_roundtrip` (W) | `encodeParam` round-trips a URL, a lone colon, a space and a leading minus — the `PARAMETER-ESCAPING-INCOMPLETE` guard |
| `web_no_blocking_api` (C) | the exported surface contains no blocking/sync evaluation entry point; fails if one is ever added. Carries no guide ID — `ASYNCQ07` is `NA` here — but the property is worth pinning |

### The guide's mandatory end-to-end test

Checklist item 10 — "a small end-to-end test that registers a language command and evaluates it
through the real environment" — is `COMMAND01`, run in a real browser against a real
`DefaultEnvironment`, not a mock:

```javascript
test("COMMAND01 register and execute a first command", async () => {
  await init();
  liquers.registerCommand({ name: "hello", run: () => "Hello, world!" });
  expect(await liquers.evaluate("hello")).toBe("Hello, world!");
});
```

---

## Benchmark: the boundary-cost hypothesis

Phase 1 decision 2 asserted opaque retention is O(1) against structural conversion's O(size) and
explicitly deferred the magnitude to this phase. The benchmark:

- Objects of 10, 10², 10³, 10⁴ properties, and a 1 MB `Uint8Array`, crossed JS→Rust→JS
- Structural conversion vs `liquers.opaque()`, `performance.now()`, 100 iterations
- **Reported, not asserted:** this informs how prominent the opt-in should be, so a CI failure
  threshold would be noise. The recorded outcome goes back into decision 2.

**If structural conversion turns out to be cheap** at realistic sizes, the ergonomic pressure to
reach for `opaque()` drops and the docs should say so — the honest outcome of a measurement is that
it can overturn the reason for the feature's prominence, though not the feature itself (identity
pass-through remains its own justification).

---

## How the three hardest tests actually assert

Three tests name a claim that is easy to state and hard to *observe*. Left vague, each would pass
with the bug present. Specified here because Phase 4 cannot invent them.

### `RUNTIME04` — reentrancy

Follows from the corrected analysis (Phase 2, "Reentrancy", point 4): the deadlock case is
unreachable, so the test asserts the three reachable paths rather than a rejection that never fires.

| Case | Assertion |
|---|---|
| async command calls and awaits `evaluate` | outer query resolves with the nested result |
| sync command **returns** the Promise from `evaluate` | `IsAsync::Auto` awaits it; outer query resolves |
| sync command starts `evaluate` and ignores it | outer query resolves immediately; the nested asset still reaches a terminal status |

Each has a **timeout** (2 s). A deadlock manifests as timeout, which is the actual failure mode
being guarded against — an assertion on a returned value would simply hang instead of failing.

### `RUNTIME05` / C11 — closure resources are released

The claim is that `unregister` releases a `js_sys::Function` and everything its closure retained.
JavaScript cannot observe a Rust drop directly, and `FinalizationRegistry`/`WeakRef` depend on GC
timing, so a naive test would be flaky *or* vacuous.

**Primary assertion — deterministic.** A debug-only export
(`#[cfg(feature = "debug-handles")] pub fn live_handle_count() -> usize`) reports how many
`js_sys::Function` handles the registry currently holds. Register N commands → count rises by N;
unregister them → count returns to baseline. This tests exactly the mechanism that justifies
`unregister` (the `Arc` refcount reaching zero), deterministically and with no GC involvement.

**Secondary, opt-in.** A `WeakRef` to a captured object plus `globalThis.gc()` under Chromium's
`--js-flags="--expose-gc"` confirms the JS object really becomes collectable. Marked
allowed-to-be-flaky and excluded from the required set; the primary assertion is what gates.

### `unregister01` — all three stores cleared together

Asserting "evaluation fails" would pass even if only the executors were removed. The sharp assertion
is **which layer** fails: after `unregister`, building the plan must fail with
`ActionNotRegistered` (metadata gone). A failure at execution with `unknown_command_executor` means
metadata survived — the exact bug — and the test must treat that as a **failure**, not a pass.

## Test Infrastructure Requirements

Discovered while planning, each a Phase 4 task:

1. **`console.warn` spy** — replacement warnings (C12, C14) are asserted, and `wasm-bindgen-test`
   does not capture console output. The harness installs a spy, captures the message text (both
   replacement variants are distinguished, so the assertion is on content not just call count), and
   restores the original in a guard that runs even if the test fails.
2. **Fixture commands** — defined once in a shared test module and registered by a
   `register_fixture_commands(env)` helper, so Tiers 2 and 3 use identical definitions. Exact
   signatures, matching the validated queries:

   | Command | State | Arguments | Returns |
   |---|---|---|---|
   | `hello` | none (source) | — | `"Hello, world!"` |
   | `number` | none (source) | `n: int` | that integer |
   | `repeat` | text | `count: int = 2` | input repeated `count` times |
   | `shout` | text | — | input uppercased |

   `repeat` deliberately carries a default, so it exercises the explicit-declaration path that
   inference refuses.
3. **Second value type for the generic bridge** — Phase 2 requires the Tier-2 path be *proven*. A
   minimal `TestValue` with a `JsValueBridge` impl instantiates every generic function. Compilation
   alone is the regression guard for genericity, but it is **not** sufficient for correctness, so
   `TestValue` additionally runs a reduced conversion suite (`VALUE01`, `VALUE04`, `VALUE09`) to
   show the generic path behaves, not merely type-checks.
4. **Build-configuration matrix** — with Option Z the `ExtValue::Foreign` variant is **ungated**,
   so a missing match arm fails to compile in *every* configuration rather than one, and the
   compiler is the primary guard. The matrix remains worth running for feature-interaction
   breakage (and for the one site the compiler cannot check — the pre-existing `_ =>` arm in
   `as_bytes`). Check:
   `--no-default-features`; `--features egui`; `--features polars`; `--features webui`; default; and
   `--target wasm32-unknown-unknown --no-default-features --features webui`. This is the cheapest
   test in the plan and guards the single most mechanical part of the change.
5. **Headless Chromium** — already available (`liquers-lib/examples-web/` Playwright setup).
6. **Build isolation** — per `CLAUDE.md`, browser tests run after `cargo clean` and separately from
   the native loop; the disk allowance does not fit both.
7. **`.d.ts` freshness check** — a CI step regenerates the declarations and fails on any diff, plus
   `tsc --noEmit` over a small usage sample. A **build step, not a browser test** — `STUBS01`/
   `STUBS03` are structural checks on generated text and do not need a browser.

## Review record

Phase 3 was reviewed by the two prescribed reviewers; both found real defects.

**Conformity reviewer.** The inference and namespace sub-suites that Phase 2 carried here were
named but not itemized — now expanded case by case. It also judged three `NA` marks to be evasions
rather than dispositions, which was correct: `STUBS02`/`STUBS06` were deferred on the grounds that
they are CI rather than browser tests, which is a statement about *where* a test runs, not whether
it applies. `STUBS04` was likewise reclassified — it does not merely pass, it passes *by
construction*, because `registerCommand` never wraps or returns the user's function and so cannot
erase its signature the way a decorator would.

**Test-plan reviewer.** Three assertions were unfalsifiable as written — `RUNTIME04`, `RUNTIME05`
and `unregister01` would each have passed with their bug present. All three now have specified
mechanisms (see "How the three hardest tests actually assert"), and `RUNTIME05` gained a
deterministic handle-count assertion in place of GC-dependent observation. It also caught that
nothing tested the **build-configuration matrix**, which is the single most mechanical risk in the
whole change — 14 match arms across six configurations — and that `evaluate()` before
`init()` and Promise-after-free were untested.

**A correction propagated back to Phase 2.** Probing how `RUNTIME04` would assert its claim showed
that the case Phase 2 guarded with a typed error *cannot occur*: JavaScript cannot block on a
Promise, so a sync command re-entering `evaluate` either returns the Promise (and is handled on the
async path) or ignores it. Phase 2's residual-risk item and the corresponding corner case were
rewritten; the error path it specified has been removed rather than implemented.

## Open Questions

None blocking.

1. **`PACKAGE06` is the sole `NA`**, because `liquers-web` exposes no optional Cargo feature as an
   installable extra in this phase. It becomes required the moment one exists — npm packaging or
   `UIUSE`. Recorded with its reversing condition so it is not silently dropped.
2. **`PARAMETER-ESCAPING-INCOMPLETE`** (filed in `specs/ISSUES.md`) is a `liquers-core` defect, not a
   `liquers-web` one. Phase 4 must decide whether to fix `encode_token` and mirror it, or implement
   `encodeParam` against the entity table directly. The latter is smaller; the former fixes the
   defect for every programmatic query builder, not just this one.
