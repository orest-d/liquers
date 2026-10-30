---
id: JS-COMMAND-CONTEXT
kind: design
title: A JavaScript command receives the execution context
form: compact
workflow: liquers-project
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [web]
issues: [JS-COMMAND-CANNOT-ACCESS-CONTEXT]
affects_docs: [specs/reference/JAVASCRIPT_INTEGRATION.md, specs/guides/JAVASCRIPT_GUIDE.md]
created: 2026-10-30
---
# A JavaScript command receives the execution context

The prerequisite of `web-ui-use` AC-3, decided by the maintainer on 2026-10-27: a JavaScript command
gets a `Context` object, which also says which UI element triggered it. Pre-approved with
`web-ui-use` and `web-ui-define` on 2026-10-30 (`proceed all`).

## Phase 1: High-Level Design

### Purpose

A JavaScript command can only transform its input. It cannot log, evaluate a sub-query, see
cancellation or learn which UI element triggered it, so `COMMAND07` passes without testing anything
(`JS-COMMAND-CANNOT-ACCESS-CONTEXT`). `liquers-py`'s `lq.Context` is the model to follow.

### Problem Example

```js
liquers.registerCommand({
  name: "report", async: true,
  run: async (context) => {
    context.info("fetching");
    return await context.evaluate("commands_doc");
  },
});
await liquers.evaluate("report");
```

Today the call throws `TypeError: context is undefined`. Afterwards the call returns the
`commands_doc` text, and the asset's log contains an info entry "fetching".

### Scope and Acceptance Criteria

- **AC-1** The context arrives where it is declared
  WHEN a declaration lists an argument named `context` (in any position), or the run function's
  inferred parameters include one
  THEN `run` receives a `Context` in that position, and `context` is not a query argument
- **AC-2** The log receives entries (`COMMAND07`)
  WHEN a command calls `context.info`, `warning` or `error`
  THEN the asset's log has the entry; `error` also fails the evaluation once `run` returns
- **AC-3** A nested evaluation
  WHEN an async command awaits `context.evaluate(query)`
  THEN it receives the query's value, and the query is recorded as a dependency
- **AC-4** The element that triggered the command
  WHEN a command runs from a UI action
  THEN `context.element` is the number of the element's handle, and outside a UI it is `null`
- **AC-5** The context dies with the call
  WHEN a page keeps the context and uses it after `run` (or its promise) has settled
  THEN every method throws a `LiquersError` that says the context is no longer valid
- **AC-6** Opting out
  WHEN a declaration says `conventions: { context: false }`
  THEN an argument named `context` is an ordinary argument

Non-goals: `injected` values and the raw payload (only the element handle is exposed), and
cancellation that does anything (`isCancelled()` exists and reports what core reports; on wasm it
is always `false`, see `WEB-CANCELLATION-INERT`).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. It adds an exported `Context` class and changes how an
  argument named `context` is bound (rules 3 and 4).
- **Leading issue:** **Proposed resolution, compatibility:** an existing page command with an
  argument named `context` starts receiving the context. It can opt out with
  `conventions: { context: false }` (AC-6).
- **Explanation:** the shape follows `liquers-py` (`liquers-py/src/context.rs`): `evaluate`,
  `info`, `warning`, `error`, `isCancelled`, and JavaScript adds `element`. The core convention
  already exists. Only the compatibility note above is new to the user.
- **Open questions:** 1. as the leading issue.

## Phase 2: Architecture

### Solution

`spec.rs` runs the core `context` convention instead of disabling it, so the declaration records
`registration.context`. `call_js_command` builds a `LiquersContext` and inserts it at that position.
The class is concrete for JavaScript, but `call_js_command` is generic over `E`, so the class holds
a type-erased `Rc<dyn ContextOps>`. A blanket implementation of `ContextOps` covers every
`Context<E>` whose value bridges to JavaScript. `element` comes from a small `JsPayload` trait on
the payload type: `()` answers `None`, and `SimpleUIPayload` answers its handle. When `run` (or its
promise) settles, the adapter clears the `Rc`'s slot, which invalidates the context, as `lq.Context`
is invalidated.

Rejected: a `JsEnvironment` trait every downstream environment must implement (that is more work
for a crate that substitutes its own value type than the `JsPayload` bound); passing a plain object
of closures (four retained `Closure`s per call, and nothing to invalidate).

### Changes

- `liquers-web/src/command/context.rs` (new):
  ```rust
  pub trait JsPayload { fn ui_handle(&self) -> Option<u64>; }
  impl JsPayload for () { … }                                   // None
  impl JsPayload for liquers_lib::ui::SimpleUIPayload { … }      // self.handle().map(|h| h.0)
  pub(crate) trait ContextOps {
      fn info(&self, m: &str) -> Result<(), Error>;  fn warning(&self, m: &str) -> Result<(), Error>;
      fn error(&self, m: &str) -> Result<(), Error>; fn is_cancelled(&self) -> bool;
      fn element(&self) -> Option<u64>;
      fn evaluate(&self, query: String) -> BoxFuture<'static, Result<JsValue, Error>>;
  }
  impl<E: Environment> ContextOps for Context<E> where E::Value: JsValueBridge, E::Payload: JsPayload
  #[wasm_bindgen(js_name = Context)] pub struct LiquersContext { slot: Rc<RefCell<Option<Rc<dyn ContextOps>>>> }
  // methods: info, warning, error, isCancelled, element (getter, number | null), evaluate → Promise
  ```
  `evaluate` runs `context.evaluate(&query)` and then `wait_for_dependency`, then
  `value_to_js_in(&envref, …)`. The method clones the `Rc` out of the slot before it awaits, so
  the slot is never borrowed across the await or across a call into JavaScript.
- `liquers-web/src/command/spec.rs`: `prepare_javascript_document` writes `conventions` as an
  object, with every convention off except `context` (unless the page set `context: false`).
  `JsCommandSpec` gains `context_position: Option<usize>`, read from `registration.context`.
  `infer_arguments` treats a parameter named `context` the same way.
- `liquers-web/src/command/adapter.rs` `call_js_command`: adds the bound `E::Payload: JsPayload`,
  inserts the context at `context_position`, and invalidates it after `apply` and after the
  awaited promise settles, on success and on failure.
- `register_js_command` and the environment functions take the new bound. `WebEnvironment` meets it
  today, with `()`, and still meets it after `web-ui-use` changes the payload to `SimpleUIPayload`.
- Errors: a context used after its call uses `Error::general_error("the context is used after its
  command returned; it is valid only during the call")`, the same text `liquers-py` uses. A failure
  inside `evaluate` is passed through unchanged.
- `liquers-web/src/typescript.rs`: a hand-declared `Context` interface is not needed, because the
  class is generated. The `run` type in `CommandDeclaration` gains a `context: Context` parameter
  in its documentation comment.

### Risks

Files: `spec.rs`, `adapter.rs`, a new `context.rs`, `lib.rs`; tests `commands_COMMAND.rs` and
`commands_DECLARATION.rs`. A page whose command already has an argument named `context` sees it
become the context. That case is rare, and the page can opt out with AC-6; the change is listed in
`liquers-web/README.md`. Certainty: high. The convention and the `registration.context` record
already exist in core (`reference/COMMAND_DECLARATION.md`).

## Phase 3: Examples and Tests

### Examples

The problem example above, and this sync command:
`run: (x, context) => { context.warning("odd"); return x; }`, declared with
`arguments: [{name: "x"}, {name: "context"}]`.

### Tests

In `liquers-web/tests/commands_COMMAND.rs` (Node loop,
`cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`):

- `command07_context_injection`, rewritten: an info entry is in the evaluated state's log (AC-2).
- `command07b_context_position_declared_and_inferred`: a declared argument and an inferred
  parameter both receive the context, and `describeCommand(n).arguments` omits it (AC-1).
- `command07c_context_error_fails_evaluation` (AC-2).
- `command07d_context_evaluate_returns_value_and_records_dependency`: the value is checked, and the
  dependency is checked through the `dep` introspection commands (AC-3).
- `command07e_context_element_is_null_outside_ui` (AC-4; the UI half is `web-ui-use`'s
  `uiuse03_action_reaches_command_with_element`).
- `command07f_context_invalid_after_return`: a context kept in a global throws after the call
  (AC-5).
- In `commands_DECLARATION.rs`, `decl04_context_convention_can_be_disabled` (AC-6).

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `spec.rs` — the context convention and `context_position`; `infer_arguments` — proof:
  `command07b`, `decl04`
- [ ] 2. `command/context.rs` — `JsPayload`, `ContextOps`, `LiquersContext` — proof: builds for
  `wasm32-unknown-unknown`
- [ ] 3. `adapter.rs` — insert the context and invalidate it — proof: `command07` through `command07f`
- [ ] 4. Docs: `reference/JAVASCRIPT_INTEGRATION.md` "Context" section with a `## History` row;
  `guides/JAVASCRIPT_GUIDE.md` example (run by `scripts/test-guide-examples.sh`);
  `liquers-web/README.md` (`COMMAND07` no longer overstated); close
  `JS-COMMAND-CANNOT-ACCESS-CONTEXT` — proof: `docs_index.py --check`

### Validation

The whole Node loop with and without `records`, `check-stubs.sh --build` (the new `Context` class in
the `.d.ts`), and `scripts/check-build-matrix.sh` for the wasm rows.
