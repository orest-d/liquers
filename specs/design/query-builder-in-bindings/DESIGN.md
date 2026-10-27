---
id: QUERY-BUILDER-IN-BINDINGS
kind: design
title: Python and JavaScript expose the core query builder
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [py, web, core/query]
issues: [QUERY-BUILDER-NOT-IN-BINDINGS]
created: 2026-10-27
---
# Python and JavaScript expose the core query builder

Bulk design, 2026-10-27: Phases 1-4 written, not approved, nothing implemented.

## Phase 1: High-Level Design

### Purpose

`query-builder` adds `liquers_core::query_builder::QueryBuilder`, which checks every raw-emitted
token (action names, namespaces, key segments, selectors, filenames) and sets the `-/` header
itself. Python and JavaScript callers still assemble everything but parameters as text. This
design gives each binding a thin wrapper class over the core builder, with no escaping or
validation logic of its own.

### Problem Example

Verified at HEAD (`04667a80`): `liquers-py/src/query.rs` offers `ActionRequest.from_arguments(name,
arguments)`, which escapes parameters but not `name`; `liquers-web/src/encode.rs` `encode_param`
escapes one parameter. Neither has a builder; `liquers-core` has no `query_builder` module yet.

```python
q = "-R/" + folder + "/" + name + "/-/ns-rec/eq-region-" + value   # today: text assembly
# a `name` holding "my file.csv" gives text that does not parse; a forgotten "-/" turns the
# actions into key segments, silently
q = (lq.QueryBuilder().resource(folder, name).ns("rec").action("eq", "region", value).build())
# wanted: an lq.Query encoding as -R/data/report.csv/-/ns-rec/eq-region-EU, or lq.ParameterError
#         naming "my file.csv" and "a key segment"
```

The encoded query was checked with `liquers-validate`.

### Scope and Acceptance Criteria

- **AC-1** Python builds a round-tripping query
  WHEN `lq.QueryBuilder().resource("data", "report.csv").ns("rec").action("eq", "time", "12:30").build()`
  THEN it is an `lq.Query` encoding as `-R/data/report.csv/-/ns-rec/eq-time-12~ncolon~30`, and
  `lq.Query.parse` of that text equals it
- **AC-2** JavaScript builds the same query
  WHEN `new liquers.QueryBuilder().resource(["data", "report.csv"]).ns("rec").action("eq", ["time", "12:30"]).build()`
  THEN it is a `liquers.Query` with the same encoding, and `equals` its parsed text
- **AC-3** An unencodable token is a typed error
  WHEN a step gets a token invalid in its position (`"my file.csv"` as a key segment, `"."`,
  `"bad name"` as an action name)
  THEN it raises `lq.ParameterError` / throws a `LiquersError` with `errorType` `parameter_error`,
  naming the value and the position kind, and the builder it was called on is unchanged
- **AC-4** A hand-made action is checked
  WHEN Python's `action_request(lq.ActionRequest.from_arguments("bad name", []))` is called
  THEN it raises `lq.ParameterError`
- **AC-5** Structure errors
  WHEN `action` follows `filename`, or `selector` has no resource before it
  THEN the step raises `ParameterError`; an empty builder builds the empty query
- **AC-6** Any parameter string is accepted
  WHEN a parameter is `"-5"`, `"a/b"`, `"café"` or `""`
  THEN the built query parses back to exactly that value
- **AC-7** Only strings are parameters in Python
  WHEN a Python parameter or segment is not a `str` (`action("eq", "n", 5)`)
  THEN PyO3 raises `TypeError` and nothing is converted

Non-goals: escaping or validating anything in the binding; a `liquers-encode` equivalent; a
JavaScript `ActionRequest` class; link parameters from JavaScript; `Query.check_encodable` in the
bindings (no binding can assemble an unchecked `Query`: `Query.parse` and the builder are the only
sources).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. It adds a class to each binding (rule 4: new classes are
  interface changes), spans two crates (rule 6), and readiness is `needs-decision` (rule 5).
- **Leading issue:** **Open design - builder semantics in the languages.**
- **Explanation:** the core builder is by-value with a `Result` per step and is `Clone`, so a
  wrapper is a few lines per method. Phases 3-4 hold under the recommendations below and the core
  design's interface.
- **Open questions:**
  1. **Open design - builder semantics.** Recommended: persistent. Each step returns a *new*
     builder and leaves its receiver unchanged, so a failed step loses nothing (AC-3) and a prefix
     can be reused for several queries. The alternative, a mutating builder returning `self`,
     matches Python's usual fluent style but leaves the builder half-changed after an error
     unless every step clones anyway.
  2. **Proposed resolution - argument shapes.** Python takes variadic segments and parameters
     (`resource(*segments)`, `action(name, *parameters)`); JavaScript takes arrays
     (`resource(segments)`, `action(name, parameters = [])`), because a `wasm-bindgen` export
     cannot be variadic.
  3. **Proposed resolution - parameter types.** Strings only, as in the core `action`. A Python
     `int` raises `TypeError` (AC-7); converting numbers would be a formatting rule of its own.

### Design Dependencies

- `query-builder` (in review, `needs-decision`): **requires** the `QueryBuilder` methods and
  `ParameterError` texts of its Phase 2, with its questions 1-3 answered as recommended
  (`ParameterError`; `.` / `..` refused by `resource`; the empty parameter encodable). Its Step 7
  links this issue.
- `resource-name-unicode` (in review): **overlaps**. It decides what a valid key segment is; the
  wrappers inherit it through the core predicate, so neither order matters.
- `PY-REPRS-SHOW-RUST-DEBUG-FORMAT` (designed in this run): **overlaps**. It edits `repr` of
  classes in `liquers-py/src/query.rs`; this design adds a class there. Different functions.

### Consolidated Findings

- `LANGUAGE-INTEGRATION_GUIDE.md` §OBJECT says "`QUERY-BUILDER-TOOLING` remains open — there is
  still no supported query-construction utility", and `query-builder`'s `affects_docs` does not
  list that guide. This design updates the paragraph and adds the conformance test `OBJECT10`.
- Python's `ActionRequest.from_arguments` leaves the action name unchecked; `action_request` is the
  checked path. `from_arguments` stays as it is (changing it would be a second interface change).

## Phase 2: Architecture

### Solution

Each binding holds a `liquers_core::query_builder::QueryBuilder` by value. A step clones it, calls
the core step, and wraps the result in a new wrapper; an `Err` maps through the binding's error
bridge. `build()` clones and builds, so a builder can build more than once; the web wraps the
result with the existing `LiquersQuery::from_query` (`objects.rs:73`, `pub`), Python with
`Query(q)`. `action_request` passes `action.0.clone()` (the field is private to `query.rs`, where
the wrapper lives).

Rejected: re-implementing the token checks in Python or TypeScript (the drift the core builder
removes); a mutating builder (question 1); a JSON description of the query passed to one
`build(spec)` function (no step-level error position).

No known issue blocks; `query-builder` lands first.

### Changes

```rust
// liquers-py/src/query.rs
#[pyclass(from_py_object, module = "liquers")]
#[derive(Clone, Debug, Default)]
pub struct QueryBuilder(liquers_core::query_builder::QueryBuilder);
#[pymethods]
impl QueryBuilder {
    #[new] fn new() -> Self;
    fn rooted(&self) -> Self;
    #[pyo3(signature = (*segments))] fn resource(&self, segments: Vec<String>) -> PyResult<Self>;
    fn resource_key(&self, key: &Key) -> PyResult<Self>;
    fn selector(&self, name: &str) -> PyResult<Self>;
    #[pyo3(signature = (name, *parameters))]
    fn action(&self, name: &str, parameters: Vec<String>) -> PyResult<Self>;
    fn action_request(&self, action: &ActionRequest) -> PyResult<Self>;
    fn ns(&self, namespace: &str) -> PyResult<Self>;
    fn filename(&self, name: &str) -> PyResult<Self>;
    fn build(&self) -> Query;
}
```

```rust
// liquers-web/src/objects.rs
#[wasm_bindgen(js_name = QueryBuilder)]
#[derive(Clone, Default)]
pub struct LiquersQueryBuilder(liquers_core::query_builder::QueryBuilder);
#[wasm_bindgen(js_class = QueryBuilder)]
impl LiquersQueryBuilder {
    #[wasm_bindgen(constructor)] pub fn new() -> Self;
    pub fn rooted(&self) -> Self;
    pub fn resource(&self, segments: Vec<String>) -> Result<LiquersQueryBuilder, JsValue>;
    #[wasm_bindgen(js_name = resourceKey)]
    pub fn resource_key(&self, key: &LiquersKey) -> Result<LiquersQueryBuilder, JsValue>;
    pub fn selector(&self, name: &str) -> Result<LiquersQueryBuilder, JsValue>;
    pub fn action(&self, name: &str, parameters: Option<Vec<String>>) -> Result<LiquersQueryBuilder, JsValue>;
    pub fn ns(&self, namespace: &str) -> Result<LiquersQueryBuilder, JsValue>;
    pub fn filename(&self, name: &str) -> Result<LiquersQueryBuilder, JsValue>;
    pub fn build(&self) -> LiquersQuery;
}
```

- Python errors: `error_to_pyerr` (`ParameterError` → `lq.ParameterError`); a non-`str` argument
  is PyO3's `TypeError`. Web errors: `liquers_error_to_js`.
- `liquers-py/src/lib.rs` `add_class::<QueryBuilder>()`; `liquers-py/python/liquers/__init__.py`
  imports it and lists it in `__all__`.
- `liquers-web/tests/stubs/valid_usage.ts` and `invalid_usage.ts`: one use each.
- Sync only. No command; `specs/command_registry.yaml` unchanged.
- Documents (History row, `reviewed:` bump): `guides/PYTHON_GUIDE.md` and `guides/JAVASCRIPT_GUIDE.md`
  query sections; `guides/QUERY_ESCAPING_GUIDE.md` §Escaping a value programmatically (the
  languages' row); `guides/LANGUAGE-INTEGRATION_GUIDE.md` §OBJECT paragraph and test `OBJECT10`;
  both READMEs' `OBJECT` rows (`01`–`10`). Proposed `affects_docs`: the four under `specs/`.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| Core builder API shifts before landing | Wrappers are one call per method; Step 1 re-reads it | medium |
| `Vec<String>` from a JavaScript array with a non-string | `wasm-bindgen` throws a `TypeError`; tested | high |
| A clone per step is slow for long queries | A few segments; negligible against evaluation | high |
| `OBJECT08` naming check fails for the new class | `module = "liquers"`, the class's name in `__all__` | high |

Recovery: revert both crates; the core builder is unaffected.

## Phase 3: Examples and Tests

### Examples

```javascript
const base = new liquers.QueryBuilder().resource(["data", "report.csv"]).ns("rec");
const eu = base.action("eq", ["region", "EU"]).build();
eu.encode();                                  // "-R/data/report.csv/-/ns-rec/eq-region-EU"
base.action("eq", ["region", "US"]).build();  // base reused (AC-2, question 1)
```

### Tests

Python, new `liquers-py/tests/test_OBJECT_query_builder.py` (`bash scripts/test-liquers-py.sh`):

- `test_OBJECT10_builder_round_trips`: AC-1's query and the AC-6 values (AC-1, AC-6)
- `test_OBJECT10_unencodable_token_raises_parameter_error`: the three tokens; the message names the
  value and position kind; the receiver still builds its earlier query (AC-3)
- `test_OBJECT10_action_request_is_checked` (AC-4)
- `test_OBJECT10_structure_errors_and_empty_builder` (AC-5)
- `test_OBJECT10_non_string_parameter_raises_type_error`: an `int` parameter and segment (AC-7)

JavaScript, `liquers-web/tests/objects_OBJECT.rs`
(`cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`):

- `object10_builder_round_trips`: AC-2's query and the AC-6 values (AC-2, AC-6)
- `object10_unencodable_token_is_a_parameter_error`, receiver unchanged (AC-3)
- `object10_structure_errors_and_empty_builder` (AC-5)

Stubs: `./liquers-web/scripts/check-stubs.sh --build` passes with the two stub uses. Guide
examples: `./liquers-web/scripts/test-guide-examples.sh` runs the Phase 3 example once it is in
`JAVASCRIPT_GUIDE.md`.

## Phase 4: Implementation Plan

### Steps

Prerequisite: `query-builder` implemented.

- [ ] 1. Re-read `liquers-core/src/query_builder.rs` at HEAD; update the wrapper signatures here if they moved — review only
- [ ] 2. `liquers-py/src/query.rs` `QueryBuilder`; `lib.rs`; `python/liquers/__init__.py` — the Python tests and `test_OBJECT08` — `bash scripts/test-liquers-py.sh`
- [ ] 3. `liquers-web/src/objects.rs` `LiquersQueryBuilder` — the JavaScript tests — `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles object`
- [ ] 4. `liquers-web/tests/stubs/*.ts` — `./liquers-web/scripts/check-stubs.sh --build` and `--build --records`
- [ ] 5. Documents of Phase 2 with History rows — `./liquers-web/scripts/test-guide-examples.sh` and `python3 scripts/docs_index.py --check`
- [ ] 6. `specs/issues/QUERY-BUILDER-NOT-IN-BINDINGS.md` closed with a resolution note; this design `complete` — `python3 scripts/docs_index.py` then `--check`

Rollback: Python (2) and web (3-4) revert independently.

### Validation

Both bindings' loops as in `CLAUDE.md`; `cargo fmt --check -p liquers-py -p liquers-web`. Diff
review: two crates, the stubs, the documents; no core change.
