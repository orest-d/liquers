---
id: LANGUAGE-STORE-TYPE-REGISTRATION
kind: design
title: A page defines a store type, with its arguments, for the store configuration
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [web, core/store, store/config]
issues: [LANGUAGE-STORE-TYPE-NOT-DEFINABLE]
created: 2026-10-27
---
# A page defines a store type, with its arguments, for the store configuration

Bulk design, 2026-10-27: Phases 1-4 written, not approved, nothing implemented. Scope is `liquers-web`.

**Revised 2026-11-01 after review.** `liquers-py` shipped Python store types the same day
(`Environment.register_store_factory`, `PyStoreFactory`, design `liquers-py-store`), so the Python
half of the issue is done. Orest decided (2026-11-01) that the JavaScript store types follow the
Python rules, and that new registration calls are `Environment` methods that refuse on an explicit
instance. This revision applies both: the method is `registerStoreFactory`, a second registration
of a name is refused, `config:` keys are checked strictly, refusals are `configuration_error`, and
`storeTypes()` lists the types. Phase 1 is still not approved.

## Phase 1: High-Level Design

### Purpose

A page can register a store *object* under a name and reach it through the one Rust-defined type
`js`, but it cannot define a store *type*: a `type:` name of its own, with described arguments,
built by a constructor it supplies. This design adds `Environment.registerStoreFactory(type, create, options)` to
`liquers-web`, backed by the existing factory seam, so a page-defined type is resolved, listed and
documented like the Rust-defined ones. It follows `liquers-py`'s `register_store_factory`
(`liquers-py/src/store/factory.rs`) wherever the two languages allow the same rule.

### Problem Example

Verified at HEAD (`04667a80`): `liquers-web/src/store/builder.rs` `WebStoreFactory` declares
`localstorage`, `js`, `http`, `https` in `store_types()`, and its `create` refuses any other name.
`build_router` / `build_store` chain `core_store_factory()` first and `WebStoreFactory` second.
`StoreTypeInfo` and `StoreArgumentInfo` (`liquers-core/src/store_factory.rs`) are serde types,
but `label` and `doc` (of both) have no serde default. `liquers-web/src/environment.rs` retains page objects
in `STORE_OBJECTS` and replays them in `apply_store`.

```javascript
const env = liquers.Environment.global();
env.registerStoreFactory("kv", (config) => makeKvStore(config.bucket), {   // returns a store object, as for `js`
  doc: "A page-provided key-value store",
  arguments: [{ name: "bucket", label: "Bucket", doc: "Bucket name", argument_type: "string", required: true }],
});
await env.configureStore({ stores: [{ type: "kv", prefix: "kv", config: { bucket: "b1" } }] });
await liquers.evaluate("-R/kv/hello.txt");
env.storeTypes().map((t) => t.store_type);   // [..., "kv"]
// today:    registerStoreFactory does not exist; `type: kv` is refused as unknown, and the message
//           lists only the Rust-defined types
// expected: the store is built through `create`, the query reads from it, and `kv` is listed
```

The query was checked with `liquers-validate`.

### Scope and Acceptance Criteria

- **AC-1** A page-defined type is constructible
  WHEN a type is registered and a configuration entry names it
  THEN the store is built by calling `create(config, prefix)` and adapting its result as a `js`
  store object is adapted, and a resource query reads from it
- **AC-2** It is listed
  WHEN a configuration names an unknown type, or the page calls `env.storeTypes()`
  THEN the refusal lists the page-defined types among the available ones, and `storeTypes()`
  returns the built-in types then the page-defined ones, each as a `StoreTypeInfo` object
  (`store_type`, `label`, `doc`, `arguments`, `availability`, `coverage`), as Python's
  `store_types()` does
- **AC-3** Declared arguments are honoured, strictly
  WHEN an entry omits a required argument, omits one with a default, or gives a key that is not
  declared
  THEN the first fails before `create` runs with `configuration_error`, naming the type, prefix and
  argument; the second reaches `create` with the default filled in; the third fails the same way,
  naming the unknown key, unless the registration set `otherArguments: true`, in which case it
  reaches `create` and the type's `coverage` is `Partial` (Python's `**kwargs` rule)
- **AC-4** A bad constructor is reported
  WHEN `create` throws, returns a Promise, or returns an object without `get`
  THEN building the store fails with `configuration_error` naming the type (a thrown value's
  message kept), and the previous store stays
- **AC-5** A Rust type name is refused
  WHEN the registered name is one a Rust factory in the chain declares (`memory`, `filesystem`,
  which core declares as unavailable on wasm32, `localstorage`, `js`, `http`, `https`)
  THEN `registerStoreFactory` throws `configuration_error`, naming the type, as Python's
  "is a built-in store type; choose another name"
- **AC-6** Order and retention
  WHEN the type is registered after `configureStore`, and later registrations of any kind follow
  THEN the configuration is re-applied with the type, and the type stays resolvable after each,
  whether that registration rebuilds the environment or updates it in place
- **AC-7** A name is registered once
  WHEN a type is registered again under the same name
  THEN `registerStoreFactory` throws `configuration_error` ("store type 'kv' is already
  registered"), as Python does, and the first registration stays in effect
- **AC-9** Only on the page environment
  WHEN `registerStoreFactory` is called on an explicit `new Environment()`
  THEN it throws `not_supported`, as `registerCommand` on an instance does, and nothing is
  registered anywhere (the decision of 2026-11-01; it lifts with
  `WEB-EXPLICIT-ENVIRONMENT-CONFIGURES-THE-SINGLETON`)
- **AC-8** A re-applied configuration keeps the store object
  WHEN the configuration is re-applied (a later `registerStoreObject` / `registerStoreFactory`, a
  rebuild) and an entry's type, prefix and `config` are unchanged
  THEN `create` is not called again, and an in-memory page store keeps its contents

Non-goals: Python (done in `liquers-py-store`); asynchronous constructors; unregistering a type;
`store_config` handlers defined in
the language (`LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE`); removing `js`, which stays the quick
path for a one-off store.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. It adds methods to the JavaScript `Environment` class and a
  TypeScript type (rule 4), and Phase 1 is not approved.
- **Leading issue:** none open. The two open designs were decided by Orest on 2026-11-01
  (questions 1 and 7); the rest are proposed resolutions.
- **Explanation:** the language-neutral half already exists (`StoreTypeInfo` is serde), and
  `JsStore::new` already adapts a page object. The bridge is a table of declarations plus
  constructor callbacks inside `WebStoreFactory`, retained like `STORE_OBJECTS`. That is `M` in
  one crate.
- **Open questions:**
  1. **Decided 2026-11-01 - placement.** `Environment.registerStoreFactory` and
     `Environment.storeTypes`, beside `registerStoreObject` and `configureStore`. On an explicit
     instance both refuse with `not_supported` (AC-9) rather than acting on the singleton, which
     is what `configureStore` still does (`WEB-EXPLICIT-ENVIRONMENT-CONFIGURES-THE-SINGLETON`).
     The rule for every new registration call: an `Environment` method, refused on an explicit
     instance until instances can be configured.
  2. **Proposed resolution - synchronous constructor.** `StoreFactory::create` is synchronous, so
     `create` must return the object; a thenable is refused with a message saying so. An object
     whose methods return Promises is fine, as for `js`.
  3. **Proposed resolution - collisions.** A name declared by `core_store_factory()` or
     `WebStoreFactory`'s own types is refused at registration, since under first-wins chaining it
     could never resolve (Python's rule; the browser chain has no `opendal_*` types to reserve).
     An empty name is refused too.
  4. **Decided 2026-11-01 (follow Python) - arguments and coverage.** A JavaScript function's
     parameters cannot be inspected, so `options.arguments` is the declaration; without it the
     type takes no arguments. Required arguments are checked and defaults filled before `create`.
     An undeclared key is refused (AC-3), unless `options.otherArguments` is `true`, which makes
     `coverage` `{Partial: {authority: "the JavaScript factory of store type 'kv'"}}`, the
     authority text Python uses; otherwise `coverage` is `Complete`. `availability` is always
     `Available`. Absent `label` / `doc`, on the type and on each `arguments[]` entry, default to
     the name / empty, as Python and `StoreTypeInfo::new` do; `arguments[]` is read through a
     crate-private serde mirror with those defaults, then converted. Refusals use
     `Error::configuration_error`, as Python's do.
  5. **Resolved - scope.** `liquers-web` only. The Python half shipped in `liquers-py-store`
     (2026-11-01), so the issue closes when this lands (Step 5).
  6. **Proposed resolution - constructed objects are retained.** `apply_store` builds a fresh
     `WebStoreFactory` each time, so `create` would run on every re-apply and a page's in-memory
     store would lose its contents. Each object `create` returns is kept in `STORE_TYPE_INSTANCES`
     under (type, prefix, canonical `config` JSON) and reused; `reset_global` drops them. This
     differs from Python, whose rebuild keeps the built store whole, because the web build
     re-applies its configuration (AC-6); the observable result, a store that keeps its contents,
     is the same. The alternative documents the loss.
  7. **Decided 2026-11-01 (follow Python) - re-registration and naming.** A name is registered
     once (AC-7); the method is named for the factory, as in Python (`registerStoreFactory` /
     `register_store_factory`), and the listing is `storeTypes()` / `store_types()`.

### Design Dependencies

- `language-store-config-function` (this run): **overlaps**. The same asymmetry for `store_config`
  handlers; a different seam (`StoreConfigFunctions`, not `StoreFactory`). Both may add retained
  state applied in `apply_store`; T3 rejected, since adjacent additions force no order.
- `WEB-EXPLICIT-ENVIRONMENT-CONFIGURES-THE-SINGLETON` (issue): **overlaps**. `registerStoreFactory`
  refuses on an explicit instance instead of inheriting that limitation (AC-9); fixing the issue
  lifts the refusal for both.
- `language-late-registration-without-rebuild` (this run): **overlaps**. Both edit
  `liquers-web/src/environment.rs`. After it lands, command registration no longer rebuilds or
  re-applies the store; store paths still do. AC-6 holds in both cases, so either order.
- None `requires`: everything needed exists at HEAD.

### Consolidated Findings

- `StoreTypeMap` (core) cannot be used: `build_router` and `build_store` take the concrete
  `WebStoreFactory` by value (`store/builder.rs:235`, `:249`), not a `dyn StoreFactory`, so the
  constructors go into `WebStoreFactory` itself and both signatures stay.
- Calling `create` happens inside `build_store`, which `apply_store` already runs with no
  `RefCell` borrow held, as the crate's borrow rule requires.
- `LANGUAGE-INTEGRATION_GUIDE.md` §STORE (2026-11-01) describes the bridging factory Python uses
  and says `liquers-web` has none yet; it has no test ID for it. This design adds `STORE19` for a
  language-defined store type, and `liquers-py`'s `tests/test_STORE_registration.py` takes the ID
  in the same change, so both integrations are checked against one test.

## Phase 2: Architecture

### Solution

`WebStoreFactory` gains `types: BTreeMap<String, (StoreTypeInfo, js_sys::Function)>`.
`store_types()` appends their infos; `create()`'s fallback arm looks the name up, binds the
arguments as `liquers-py`'s `store::bind_arguments` does (required, defaults, unknown keys refused
unless `Partial`), converts the bound arguments to a plain object
(`bridge::serialize_to_js`), calls `create.call2(&JsValue::NULL, &config, &prefix)`, refuses a
thenable (`crate::command::is_thenable`) or a non-object, and returns
`JsStore::new(&prefix, &store_type, object)`. Before calling, it looks in the instances it was
given (question 6); a new object is recorded for `apply_store` to retain. A thrown value goes through
`error::js_error_to_liquers(thrown, ErrorType::ConfigurationError)` with the type named.

`environment.rs`: `STORE_TYPES: RefCell<Vec<(StoreTypeInfo, js_sys::Function)>>` and
`STORE_TYPE_INSTANCES` beside `STORE_OBJECTS`; `apply_store` registers both and retains new
instances; `reset_global` clears them; `register_store_type_on(type, create, options)` parses the options,
checks the name (AC-5, AC-7), appends, and re-applies the configuration when one is installed,
restoring the previous list on failure, exactly as `register_store_object_on` does.
`LiquersEnvironment::register_store_factory` and `store_types` refuse an `EnvHandle::Explicit`
handle with `not_supported` (AC-9).

Rejected: a second factory in the chain (changes `build_router` / `build_store`); module-level
placement (question 1); replacing a registered type (question 7); passing undeclared keys through
(question 4); awaiting a Promise from `create` (the factory trait is synchronous).

Known issues: none blocks.

### Changes

```rust
// liquers-web/src/store/builder.rs
impl WebStoreFactory {
    /// Adds a page-defined store type. Errors when `info.store_type` is a built-in type or
    /// already registered.
    pub fn register_type(&mut self, info: StoreTypeInfo, create: js_sys::Function) -> Result<(), Error>;
    /// Instances to reuse (question 6); `apply_store` keeps only those the new config uses.
    pub fn with_instances(self, instances: Vec<(InstanceKey, JsValue)>) -> Self;
}
// liquers-web/src/environment.rs
pub fn register_store_type_on(store_type: &str, create: js_sys::Function, options: &JsValue) -> Result<(), Error>;
// #[wasm_bindgen] impl LiquersEnvironment
#[wasm_bindgen(js_name = registerStoreFactory)]
pub fn register_store_factory(&self, store_type: String, create: js_sys::Function,
    options: Option<crate::typescript::JsStoreFactoryOptions>) -> Result<(), JsValue>;
#[wasm_bindgen(js_name = storeTypes)]
pub fn store_types(&self) -> Result<crate::typescript::JsStoreTypeInfoArray, JsValue>;
```

- `liquers-web/src/typescript.rs`: `LiquersStoreFactoryOptions` (`label?`, `doc?`, `arguments?`,
  `otherArguments?`), `LiquersStoreTypeInfo`, the factory type
  `(config, prefix) => LiquersStoreObject`, and the extern types.
- Errors: `Error::configuration_error` naming the type for refusals; the thrown value's message
  kept; `not_supported` on an explicit instance.
- Sync only; no command; `specs/command_registry.yaml` unchanged.
- Documents (History row, `reviewed:` bump): `guides/JAVASCRIPT_GUIDE.md` store section (a
  `registerStoreFactory` example); `reference/JAVASCRIPT_INTEGRATION.md` (the two methods);
  `guides/LANGUAGE-INTEGRATION_GUIDE.md` §STORE (`liquers-web` now bridges the factory too, and
  `STORE19`); `liquers-web/README.md` store table and `STORE` row; `liquers-py/README.md` `STORE`
  row (`STORE19`). Proposed `affects_docs`: the three under `specs/`.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A page declares arguments its constructor ignores | Stated in the guide: the declaration is the page's specification | medium |
| `create` blocks on slow work | It must return synchronously; slow work belongs in the store's async methods | high |
| A refused registration leaves a half-applied store | Swap-and-restore, as `register_store_object_on` | high |
| Stub check fails on the new type | `check-stubs.sh --build` in Step 3 | high |

Recovery: revert the crate; `js` stores are untouched.

## Phase 3: Examples and Tests

### Examples

The Problem Example, with `makeKvStore` returning `{ get: (key) => ({ data: bytes[key], metadata: {} }) }`.
An entry without `bucket` fails with `configuration_error` "store type 'kv' at prefix 'kv': the
required argument 'bucket' is missing from `config:`", Python's wording; an entry with
`config: { bucket: "b1", colour: "red" }` fails naming `colour`.

### Tests

`liquers-web/tests/store_js_STORE.rs`
(`cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`):

- `store19_page_defined_type_is_constructible`: the Problem Example reads `hello.txt` (AC-1)
- `store19_unknown_type_lists_page_defined_types` and `store19_store_types_lists_page_defined_types` (AC-2)
- `store19_required_argument_and_default`, `store19_undeclared_key_is_refused` and
  `store19_other_arguments_is_partial` (AC-3)
- `store19_bad_constructor_keeps_the_previous_store`: throwing, a Promise, no `get` (AC-4)
- `store19_built_in_name_is_refused` for each of the six names (AC-5)
- `store19_type_registered_after_configuration_applies` and
  `store19_type_survives_later_registrations` (a command, a store object) (AC-6)
- `store19_second_registration_is_refused` (AC-7)
- `store19_explicit_instance_refuses` (AC-9)
- `store19_reapplied_configuration_keeps_the_object`: a counting `create`, a written value still
  read after `registerStoreObject` (AC-8)
- `store19_argument_label_and_doc_default`: an `arguments[]` entry with only `name` (question 4)

Unit, `liquers-web/src/store/builder.rs` `tests`: `register_type_refuses_built_in_names` (AC-5).
Stubs: one `registerStoreFactory` and one `storeTypes` use in `tests/stubs/valid_usage.ts`, an
async `create` in `invalid_usage.ts` (AC-4).

Python: rename the store-type tests in `liquers-py/tests/test_STORE_registration.py` to carry
`STORE19` (no behaviour change).

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-web/src/store/builder.rs` `WebStoreFactory::{register_type, store_types, create}` — `register_type_refuses_built_in_names` — `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles --lib builder`
- [ ] 2. `liquers-web/src/environment.rs` `STORE_TYPES`, `STORE_TYPE_INSTANCES`, `apply_store`, `register_store_type_on`, `LiquersEnvironment::{register_store_factory, store_types}` — all `store19_*` tests — `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles store19`, also with `,records`
- [ ] 3. `liquers-web/src/typescript.rs` and `tests/stubs/*.ts` — `./liquers-web/scripts/check-stubs.sh --build` and `--build --records`
- [ ] 4. Documents of Phase 2 with History rows — `./liquers-web/scripts/test-guide-examples.sh` and `python3 scripts/docs_index.py --check`
- [ ] 5. `liquers-py/tests/test_STORE_registration.py`: the `STORE19` names — `bash scripts/test-liquers-py.sh`
- [ ] 6. `specs/issues/LANGUAGE-STORE-TYPE-NOT-DEFINABLE.md`: close with a resolution note naming both designs; this design `complete` — `python3 scripts/docs_index.py` then `--check`

Rollback: steps 1-3 revert together; documents separately.

### Validation

The `liquers-web` loops of `CLAUDE.md` (with and without `records`, stubs, guide examples);
`cargo fmt --check -p liquers-web`. Diff review: one crate and the documents.
