---
id: WEB-THROWN-BACK-ERROR-CONTEXT
kind: design
title: A LiquersError thrown back from JavaScript keeps its query, key and position
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [web]
issues: [WEB-THROWN-BACK-LIQUERSERROR-LOSES-CONTEXT]
created: 2026-10-27
---
# A LiquersError thrown back from JavaScript keeps its query, key and position

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-27: Phases 1-4 written.

## Phase 1: High-Level Design

### Purpose

When JavaScript throws a `LiquersError` back into Liquers (a command or store delegate rethrowing
what it caught, or a page-constructed error), `liquers-web` rebuilds the Rust error from its type
and message only. The guide's `ERROR02` requires the fields to survive a Rust-to-language-to-Rust
trip, and the code's own comment says it "round-trips without degradation". This design copies
`query`, `key` and `position` across as well.

### Problem Example

```rust
let mut err = Error::from_error(ErrorType::KeyNotFound, "no such key".to_string());
err.query = Some("-R/data/a.txt".to_string());
err.key = Some("data/a.txt".to_string());
err.position = Position::new(3, 1, 4);
let back = js_error_to_liquers(liquers_error_to_js(err), ErrorType::General);
// today:    back.query == None, back.key == None, back.position.is_unknown()
// expected: the three fields as they were
```

Verified at HEAD (`04667a80`): `liquers-web/src/error.rs` `js_value_as_liquers_error(thrown:
&JsValue) -> Option<Error>` reads `errorType` (mapped by `error_type_from_name`) and `message` by
`js_sys::Reflect::get` and returns `Error::from_error(error_type, message)`. `LiquersError`
exposes getters `query` and `key` (`Option<String>`, `null` when absent) and `position`
(`{offset, line, column}`, or `null` when unknown). `ErrorPayload` (`liquers-core/src/error.rs`)
has public `query`, `key`, `position` and `traceback` fields; `Position` has `offset: usize`,
`line: u32`, `column: usize`, and `is_unknown()` is `line == 0`. The guide's `ERROR02` blueprint
compares message, position, query and key; `liquers-web/tests/objects_OBJECT.rs`
`error02_fields_survive_rust_language_rust` compares only type and message.
`JAVASCRIPT_INTEGRATION.md` §6 states the loss. `-R/data/a.txt` validates as one `GetAsset` step.

### Scope and Acceptance Criteria

- **AC-1** Fields survive a throw back
  WHEN an `Error` with `query`, `key` and a known `position` goes to JavaScript with
  `liquers_error_to_js` and comes back through `js_error_to_liquers`
  THEN type, message, query, key and position are equal to the original's
- **AC-2** Absent fields stay absent
  WHEN the round-tripped error had no query, no key and an unknown position
  THEN the result has `None`, `None` and an unknown position, as today
- **AC-3** A page-constructed error keeps its query
  WHEN a store delegate throws `new LiquersError("key_not_found", "no such key: a.txt",
  "-R/d/a.txt")`
  THEN the store's `get` fails with `KeyNotFound`, that message and `query` `-R/d/a.txt`
- **AC-4** Ill-typed fields on a look-alike are ignored
  WHEN a plain object `{errorType: "key_not_found", message: "m", query: 42, key: {},
  position: "x"}` is thrown
  THEN the error is `KeyNotFound` with message `m`, and query, key and position are absent

Non-goals: the traceback (no `traceback` getter exists; `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT`
adds one); `jsClass` / `jsStack`; the query-versus-key meaning of the fields
(`ERROR-WITH-KEY-SETS-QUERY-FIELD`); the JavaScript `Error` branch.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible. A bug fix that restores documented behaviour (rule 2): the
  guide's `ERROR02` contract and the code comment; the `JAVASCRIPT_INTEGRATION.md` sentence that
  describes the loss is corrected with it. Size `S`, one function in `liquers-web` plus a private
  helper (rules 1, 3, 6). Rule 4: no JavaScript API, `.d.ts`, signature or dependency changes; an
  error now carries the context its own getters already showed.
- **Leading issue:** None
- **Explanation:** the fields and their JavaScript shapes are fixed by the existing getters; the
  core does not overwrite `query` or `key` later, so what is copied is what the caller sees.
- **Open questions:** None. Implementation detail: copy a field only when it has the getter's type
  (AC-4), since look-alike objects are accepted by design (`JAVASCRIPT_INTEGRATION.md` §6).

### Design Dependencies

- `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT` (accepted issue, no design): **overlaps**. When it
  adds a `traceback` getter, `js_value_as_liquers_error` must read it too; whichever lands second
  does it. No order.
- `multi-language-traceback` (needs decision, `L`): **overlaps**. T3 holds: its Phase 4 step 6
  edits the same function, `js_value_as_liquers_error`, to read `traceback` and
  `tracebackSegments` back. Not merged: that design is `L` and not eligible, this one is eligible
  (E1). Whichever lands second rebases onto the other's reads.
- `error-with-key-field` (needs decision): **overlaps**. It may change what `query` and `key`
  hold, and it adds a `LiquersError.context` getter (its `phase2-architecture.md`, `liquers-web`
  changes) that this function will then also need to read back; that design's step does it.

## Phase 2: Architecture

### Solution

In `liquers-web/src/error.rs` `js_value_as_liquers_error`, after building
`Error::from_error(error_type, message)`:

- `query` and `key`: `Reflect::get(thrown, "query"|"key").ok()`, kept when `as_string()` is
  `Some`.
- `position`: `Reflect::get(thrown, "position").ok()`, then a private helper
  `fn position_from_js(value: &JsValue) -> Option<Position>` (with
  `use liquers_core::query::Position;`). It returns `None` unless `value` is an object
  (`is_object()`, so `null` and `"x"` are refused before any read), reads `offset`, `line` and
  `column` each with `Reflect::get(..).ok()?.as_f64()`, requires all three to be finite and
  non-negative, and builds `Position::new(offset as usize, line as u32, column as usize)`. A
  result that `is_unknown()` is left as the default.

Every `Reflect::get` goes through `.ok()`, as the existing `message` read does, so a throwing
getter or a non-object value yields an absent field, never an error or a panic.

The function keeps its signature and stays private; `js_error_to_liquers` is unchanged.

Rejected: recovering the Rust struct from the wasm-bindgen handle (it would take ownership of the
page's object and does not work for look-alikes); serializing the error to JSON in `errorType`
(changes what JavaScript sees).

Known-issue preflight: none blocks.

### Changes

- `liquers-web/src/error.rs`: `js_value_as_liquers_error` body, new private `position_from_js`.
- Tests: `liquers-web/tests/objects_OBJECT.rs` (`ERROR02` strengthened, two new tests) and
  `liquers-web/tests/store_js_STORE.rs` (one new test).
- Documents: `specs/reference/JAVASCRIPT_INTEGRATION.md` §6, "its `query` and `key` are not
  carried over" becomes "its `query`, `key` and `position` are carried over" (History row,
  `reviewed:` bump). `liquers-web/README.md`'s `ERROR` row needs no change.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A thrown-back query now appears where callers saw none | That is the restored contract; no test in `liquers-web/tests/` asserts an error's `query` is absent (`grep -rn "\.query" liquers-web/tests`) | high |
| A float `offset` or `line` from a look-alike | Only finite non-negative numbers are accepted; truncation is harmless | high |
| `Reflect::get` throws (a throwing getter, or reading a property of `"x"` or `null`) | Every read is `.ok()`, and `position_from_js` checks `is_object()` first (AC-4) | high |

Recovery: revert the function. Nothing is persisted.

## Phase 3: Examples and Tests

### Examples

The Problem Example is the strengthened `ERROR02`. For a page:

```javascript
const store = { get(key) { throw new LiquersError("key_not_found", "no such key: a.txt", "-R/d/a.txt"); } };
// the store's failure now reports query "-R/d/a.txt"
```

### Tests

Run with `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`
(and `debug-handles,records`):

- `tests/objects_OBJECT.rs` `error02_fields_survive_rust_language_rust`: also sets and compares
  query, key and position: AC-1
- `tests/objects_OBJECT.rs` `error02b_absent_fields_stay_absent`: AC-2
- `tests/objects_OBJECT.rs` `error02c_ill_typed_look_alike_fields_are_ignored`: a
  `js_sys::Function` builds the plain object: AC-4
- `tests/store_js_STORE.rs` `store_delegate_throwing_a_constructed_error_keeps_its_query`: as
  `ERROR06`, with the `query` argument: AC-3

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-web/tests/objects_OBJECT.rs` and `tests/store_js_STORE.rs` — the four tests — `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles` fails on AC-1 and AC-3
- [ ] 2. `liquers-web/src/error.rs` `js_value_as_liquers_error`, `position_from_js` — the same command passes, and with `--features debug-handles,records`; `cargo fmt -p liquers-web --check`
- [ ] 3. `specs/reference/JAVASCRIPT_INTEGRATION.md` §6 sentence, History row, `reviewed:`; issue `status: closed` with a resolution note; this design `complete`; `specs/README.md` row; `python3 scripts/docs_index.py` — `python3 scripts/docs_index.py --check`

Rollback: revert per step.

### Validation

Both Node-loop feature sets pass, `bash scripts/check-build-matrix.sh` is not needed (no `cfg`
change), and `python3 scripts/docs_index.py --check` passes. The diff touches one function and
helper, two test files, one reference paragraph, the issue, the design and the index.
