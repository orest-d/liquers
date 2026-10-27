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

Bulk design, 2026-10-27: Phases 1-4 written, not approved, nothing implemented. Scope is `liquers-web`; the Python half is a follow-up (question 5).

## Phase 1: High-Level Design

### Purpose

A page can register a store *object* under a name and reach it through the one Rust-defined type
`js`, but it cannot define a store *type*: a `type:` name of its own, with described arguments,
built by a constructor it supplies. This design adds `registerStoreType(spec)` to `liquers-web`,
backed by the existing factory seam, so a page-defined type is resolved, listed and documented
like the Rust-defined ones.

### Problem Example

Verified at HEAD (`04667a80`): `liquers-web/src/store/builder.rs` `WebStoreFactory` declares
`localstorage`, `js`, `http`, `https` in `store_types()`, and its `create` refuses any other name.
`build_router` / `build_store` chain `core_store_factory()` first and `WebStoreFactory` second.
`StoreTypeInfo` and `StoreArgumentInfo` (`liquers-core/src/store_factory.rs`) are serde types,
but `label` and `doc` (of both) have no serde default. `liquers-web/src/environment.rs` retains page objects
in `STORE_OBJECTS` and replays them in `apply_store`.

```javascript
const env = liquers.Environment.global();
env.registerStoreType({
  store_type: "kv", doc: "A page-provided key-value store",
  arguments: [{ name: "bucket", label: "Bucket", doc: "Bucket name", argument_type: "string", required: true }],
  create: (config) => makeKvStore(config.bucket),     // returns a store object, as for `js`
});
await env.configureStore({ stores: [{ type: "kv", prefix: "kv", config: { bucket: "b1" } }] });
await liquers.evaluate("-R/kv/hello.txt");
// today:    registerStoreType does not exist; `type: kv` is refused as unknown, and the message
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
  WHEN a configuration names an unknown type
  THEN the refusal lists the page-defined types among the available ones
- **AC-3** Declared arguments are honoured
  WHEN an entry omits a required argument, or omits one with a default
  THEN the first fails before `create` runs, naming the type and the argument; the second reaches
  `create` with the default filled in
- **AC-4** A bad constructor is reported
  WHEN `create` throws, returns a Promise, or returns an object without `get`
  THEN building the store fails with an error naming the type, and the previous store stays
- **AC-5** A Rust type name is refused
  WHEN the registered name is one a Rust factory in the chain declares (`memory`, `filesystem`,
  which core declares as unavailable on wasm32, `localstorage`, `js`, `http`, `https`)
  THEN `registerStoreType` throws, naming the type
- **AC-6** Order and retention
  WHEN the type is registered after `configureStore`, and later registrations of any kind follow
  THEN the configuration is re-applied with the type, and the type stays resolvable after each,
  whether that registration rebuilds the environment or updates it in place
- **AC-7** Replacement
  WHEN a type is registered again under the same name
  THEN the new declaration and constructor are used from the next build on
- **AC-8** A re-applied configuration keeps the store object
  WHEN the configuration is re-applied (a later `registerStoreObject` / `registerStoreType`, a
  rebuild) and an entry's type, prefix and `config` are unchanged
  THEN `create` is not called again, and an in-memory page store keeps its contents

Non-goals: Python (question 5); asynchronous constructors; a `storeTypes()` listing API (the
refusal message is today's only listing, for Rust types too); `store_config` handlers defined in
the language (`LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE`); removing `js`, which stays the quick
path for a one-off store.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. It adds a method to the JavaScript `Environment` class and a
  TypeScript type (rule 4), and readiness is `needs-decision` (rule 5).
- **Leading issue:** **Open design - where the method lives.**
- **Explanation:** the language-neutral half already exists (`StoreTypeInfo` is serde), and
  `JsStore::new` already adapts a page object. The bridge is a table of declarations plus
  constructor callbacks inside `WebStoreFactory`, retained like `STORE_OBJECTS`. That is `M` in
  one crate.
- **Open questions:**
  1. **Open design - placement.** Recommended: `Environment.registerStoreType(spec)`, beside
     `registerStoreObject` and `configureStore`, with the same singleton semantics
     (`WEB-EXPLICIT-ENVIRONMENT-CONFIGURES-THE-SINGLETON`). The alternative, module-level
     `liquers.registerStoreType` as the issue sketches, would split the store API across two
     receivers.
  2. **Proposed resolution - synchronous constructor.** `StoreFactory::create` is synchronous, so
     `create` must return the object; a thenable is refused with a message saying so. An object
     whose methods return Promises is fine, as for `js`.
  3. **Proposed resolution - collisions.** A name declared by `core_store_factory()` or
     `WebStoreFactory`'s own types is refused at registration, since under first-wins chaining it
     could never resolve.
  4. **Proposed resolution - arguments and coverage.** Required arguments are checked and defaults
     filled before `create`; unlisted keys pass through, as Rust types allow. `coverage` defaults
     to `Complete` and may be `{Partial: {authority}}`; `availability` is always `Available`.
     Absent `label` / `doc`, on the type and on each `arguments[]` entry, default to the name /
     empty, as `StoreTypeInfo::new` and `StoreArgumentInfo::new` do; the spec is read through a
     crate-private serde mirror with those defaults, then converted.
  5. **Proposed resolution - scope.** `liquers-web` only. `liquers-py` has no `STORE` feature: no
     store can be written in Python yet, so a Python store *type* first needs the store adapter
     itself. Covering both would be `L`. The issue keeps its Python half until a follow-up issue
     takes it (Step 5).
  6. **Proposed resolution - constructed objects are retained.** `apply_store` builds a fresh
     `WebStoreFactory` each time, so `create` would run on every re-apply and a page's in-memory
     store would lose its contents. Each object `create` returns is kept in `STORE_TYPE_INSTANCES`
     under (type, prefix, canonical `config` JSON) and reused; re-registering a type drops its
     instances (AC-7). The alternative documents the loss.

### Design Dependencies

- `language-store-config-function` (this run): **overlaps**. The same asymmetry for `store_config`
  handlers; a different seam (`StoreConfigFunctions`, not `StoreFactory`). Both may add retained
  state applied in `apply_store`; T3 rejected, since adjacent additions force no order.
- `WEB-EXPLICIT-ENVIRONMENT-CONFIGURES-THE-SINGLETON` (issue): **overlaps**. `registerStoreType`
  inherits that limitation; fixing it fixes both methods.
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
- `LANGUAGE-INTEGRATION_GUIDE.md` §STORE offers no direction for a store type contributed by the
  language; this design adds it and the test `STORE19`.

## Phase 2: Architecture

### Solution

`WebStoreFactory` gains `types: BTreeMap<String, (StoreTypeInfo, js_sys::Function)>`.
`store_types()` appends their infos; `create()`'s fallback arm looks the name up, checks
required arguments, fills defaults, converts `config.config` to a plain object
(`bridge::serialize_to_js`), calls `create.call2(&JsValue::NULL, &config, &prefix)`, refuses a
thenable (`crate::command::is_thenable`) or a non-object, and returns
`JsStore::new(&prefix, &store_type, object)`. Before calling, it looks in the instances it was
given (question 6); a new object is recorded for `apply_store` to retain. A thrown value goes through
`error::js_error_to_liquers(thrown, ErrorType::General)` with the type named.

`environment.rs`: `STORE_TYPES: RefCell<Vec<(StoreTypeInfo, js_sys::Function)>>` and
`STORE_TYPE_INSTANCES` beside `STORE_OBJECTS`; `apply_store` registers both and retains new
instances; `reset_global` clears them; `register_store_type_on(spec)` parses the spec,
checks the name (AC-5), replaces by name, and re-applies the configuration when one is installed,
restoring the previous list on failure, exactly as `register_store_object_on` does.

Rejected: a second factory in the chain (changes `build_router` / `build_store`); module-level
placement (question 1); awaiting a Promise from `create` (the factory trait is synchronous).

Known issues: none blocks.

### Changes

```rust
// liquers-web/src/store/builder.rs
impl WebStoreFactory {
    /// Adds a page-defined store type. Errors when `info.store_type` is a built-in type.
    pub fn register_type(&mut self, info: StoreTypeInfo, create: js_sys::Function) -> Result<(), Error>;
    /// Instances to reuse (question 6); `apply_store` keeps only those the new config uses.
    pub fn with_instances(self, instances: Vec<(InstanceKey, JsValue)>) -> Self;
}
// liquers-web/src/environment.rs
pub fn register_store_type_on(spec: &JsValue) -> Result<(), Error>;
// #[wasm_bindgen] impl LiquersEnvironment
#[wasm_bindgen(js_name = registerStoreType)]
pub fn register_store_type(&self, spec: crate::typescript::JsStoreTypeSpec) -> Result<(), JsValue>;
```

- `liquers-web/src/typescript.rs`: `LiquersStoreTypeSpec` (`store_type`, `label?`, `doc?`,
  `arguments?`, `coverage?`, `create(config, prefix): LiquersStoreObject`) and the extern
  `JsStoreTypeSpec`.
- Errors: `Error::general_error` naming the type for refusals; the thrown value's message kept.
- Sync only; no command; `specs/command_registry.yaml` unchanged.
- Documents (History row, `reviewed:` bump): `guides/JAVASCRIPT_GUIDE.md` store section (a
  `registerStoreType` example); `guides/LANGUAGE-INTEGRATION_GUIDE.md` §STORE (a fourth
  direction: a store type from the language, and `STORE19`); `liquers-web/README.md` store table
  and `STORE` row. Proposed `affects_docs`: the two under `specs/`.

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
An entry without `bucket` fails with "store type 'kv' requires argument 'bucket'".

### Tests

`liquers-web/tests/store_js_STORE.rs`
(`cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`):

- `store19_page_defined_type_is_constructible`: the Problem Example reads `hello.txt` (AC-1)
- `store19_unknown_type_lists_page_defined_types` (AC-2)
- `store19_required_argument_and_default` (AC-3)
- `store19_bad_constructor_keeps_the_previous_store`: throwing, a Promise, no `get` (AC-4)
- `store19_built_in_name_is_refused` for each of the six names (AC-5)
- `store19_type_registered_after_configuration_applies` and
  `store19_type_survives_later_registrations` (a command, a store object) (AC-6)
- `store19_reregistration_replaces` (AC-7)
- `store19_reapplied_configuration_keeps_the_object`: a counting `create`, a written value still
  read after `registerStoreObject` (AC-8)
- `store19_argument_label_and_doc_default`: an `arguments[]` entry with only `name` (question 4)

Unit, `liquers-web/src/store/builder.rs` `tests`: `register_type_refuses_built_in_names` (AC-5).
Stubs: one `registerStoreType` use in `tests/stubs/valid_usage.ts`, an async `create` in
`invalid_usage.ts` (AC-4).

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-web/src/store/builder.rs` `WebStoreFactory::{register_type, store_types, create}` — `register_type_refuses_built_in_names` — `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles --lib builder`
- [ ] 2. `liquers-web/src/environment.rs` `STORE_TYPES`, `STORE_TYPE_INSTANCES`, `apply_store`, `register_store_type_on`, `LiquersEnvironment::register_store_type` — all `store19_*` tests — `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles store19`, also with `,records`
- [ ] 3. `liquers-web/src/typescript.rs` and `tests/stubs/*.ts` — `./liquers-web/scripts/check-stubs.sh --build` and `--build --records`
- [ ] 4. Documents of Phase 2 with History rows — `./liquers-web/scripts/test-guide-examples.sh` and `python3 scripts/docs_index.py --check`
- [ ] 5. `specs/issues/LANGUAGE-STORE-TYPE-NOT-DEFINABLE.md`: file the Python half as its own issue (it requires `PY-NO-LANGUAGE-DEFINED-STORE`, the Python `STORE` feature, filed 2026-10-27), then close this one with a resolution note; this design `complete` — `python3 scripts/docs_index.py` then `--check`

Rollback: steps 1-3 revert together; documents separately.

### Validation

The `liquers-web` loops of `CLAUDE.md` (with and without `records`, stubs, guide examples);
`cargo fmt --check -p liquers-web`. Diff review: one crate and the documents.
