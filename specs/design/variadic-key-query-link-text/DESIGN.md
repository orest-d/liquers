---
id: VARIADIC-KEY-QUERY-LINK-TEXT
kind: design
title: A key or query link inside a variadic argument arrives as its encoded text
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/plan, core/value]
issues: [VARIADIC-KEY-OR-QUERY-LINK-FAILS-TO-MATERIALIZE]
created: 2026-10-26
---
# A key or query link inside a variadic argument arrives as its encoded text

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented. Split out of `sql-several-tables` (its former AC-7), which now
requires this design.

## Phase 1: High-Level Design

### Purpose

A link inside a `multiple` argument is materialised through the linked value's JSON form, because a
variadic argument is a list of JSON parameter values. `Value::Key` and `Value::Query` (and
`SimpleValue`'s) have no JSON form, so a `-R-key/…` link or a `…/q` link inside a variadic argument
fails the whole action. Both values have a canonical text form (`encode()`), which is also what a
`Key` argument is parsed from (`FromParameterValue<Key>` reads a JSON string). Let such a link
arrive as that text.

This **reverses a recorded decision**: `plan-relative-resolution` (complete) wrote that a nested
link yielding a key or query "returns the existing `ConversionError`. It is not silently
stringified", and pinned it with `multiple_parameter_non_json_link_reports_positioned_conversion_error`
(`liquers-core/src/interpreter.rs`), which uses a `-R-key/./target` link. Hence `needs-decision`.

### Problem Example

Verified at HEAD (`70b9005`). `materialize_nested_parameter` → `materialize_link_json`
(`interpreter.rs`) evaluates the link, then calls `value.try_into_json_value()`; core
`Value::try_into_json_value` (`value.rs`) and `SimpleValue::try_into_json_value`
(`liquers-lib/src/value/simple.rs`) have no `Key` / `Query` arm and end in
`Error::conversion_error(identifier, "JSON value")`. `-R-key/data/x.csv` plans to
`UseKeyValue[data/x.csv]` and `-R/data/x.csv/-/q` to `UseQueryValue` (both checked with
`liquers-validate`).

A command `join_text(state, parts: Vec<String> multiple)` evaluated with

```
join_text-a-~X~-R-key/data/x.csv~E
```

fails today with a `ConversionError` ("Key" to "JSON value") at the link's position. Should be:
`parts == ["a", "data/x.csv"]`.

### Scope and Acceptance Criteria

- **AC-1** Key link as text
  WHEN a `multiple` argument's element is a link whose value is a `Key`
  THEN the element is the JSON string `key.encode()`, so `Vec<String>` receives the text and
  `Vec<Key>` the parsed key
- **AC-2** Query link as text
  WHEN the element is a link whose value is a `Query` (a `…/q` link)
  THEN the element is the JSON string `query.encode()`
- **AC-3** Other non-JSON values still fail, positioned
  WHEN the linked value has no JSON form and is neither a key nor a query (`Bytes`, a DataFrame)
  THEN the action fails with the original `ConversionError` carrying the link's position
- **AC-4** JSON-capable values unchanged
  WHEN the linked value has a JSON form (text, numbers, arrays, a record view)
  THEN the element is that JSON, exactly as today (`multiple_parameter_links_materialize_in_order`,
  `record_cell_binds_inside_a_multiple_parameter_through_a_link`)
- **AC-5** Top-level links unchanged
  WHEN a key or query link fills a non-variadic argument
  THEN the argument still receives the value itself (the lossless `set_value` path)

Non-goals: a JSON form for `Value::Key` / `Value::Query` everywhere (`try_into_json_value`,
`as_bytes("json")`, the type registry's write formats stay as they are); expanding array-valued
links (`variadic-link-array-expansion`); a variadic argument carrying non-JSON values such as
DataFrames.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — it reverses a behaviour that a complete design recorded and a
  test pins, so it is a design choice rather than the restoration of documented behaviour (rule 2),
  and readiness is `needs-decision` (rule 5). Otherwise small: one private function and its tests
  in `liquers-core`, no `pub` change. It becomes eligible once the decision is made.
- **Leading issue:** **Proposed resolution - stringify keys and queries:** recommended **yes**.
- **Explanation:** the change is local to `materialize_link_json`, uses only `ValueInterface`
  methods every value type already has, and leaves every JSON-capable value untouched.
- **Open questions:**
  1. **Proposed resolution - stringify keys and queries.** Reverse `plan-relative-resolution`'s
     "not silently stringified" for exactly these two types. Recommended **yes**: their encoded text
     is their canonical, lossless, parseable form (a `Key` parameter is parsed from it), it is how
     both are written as `txt`, and `sql-several-tables` needs `/q` links in a variadic table list.
     The decision named "bytes, or another non-JSON-capable custom value" alongside them; those
     have no canonical text and keep failing (AC-3). Consequence of *no*: `sql-several-tables` must
     choose another channel for its tables, and the issue is closed as not planned.
  2. **Proposed resolution - where:** in `materialize_link_json` only, as a fallback after
     `try_into_json_value` fails. Recommended over a JSON arm in `Value` / `SimpleValue`
     `try_into_json_value` (two crates, and every other caller of the conversion — template data,
     `to_json`-like paths — would start producing strings for keys).

### Design Dependencies

- `sql-several-tables` — **required-by**: its table list is a `multiple` argument of `/q` links;
  its former AC-7 moved here.
- `plan-relative-resolution` (complete, E4) — origin of the behaviour this reverses; not edited.
- `variadic-link-array-expansion` — **overlaps** (finished Phase 4, not edited): changes
  `materialize_nested_parameter`'s `MultipleParameters` branch to expand array-valued links; this
  design changes `materialize_link_json`, which that branch calls. Either order; its Risks row
  names `multiple_parameter_non_json_link_reports_positioned_conversion_error` as staying green,
  which this design changes (the test keeps its name and purpose with a `Bytes` link).

### Consolidated Findings

- Fallback order: `try_into_json_value`, then `try_into_query`, then `try_into_key`, each only after
  the previous one failed. A `Text` value never reaches the fallback (it has a JSON form), so text
  is never re-parsed as a query; `CombinedValue::Extended` refuses both conversions, so an
  extended value keeps the original error (AC-3).
- The original JSON error is returned when both fallbacks fail, so error type and position are as
  today.
- The pinned test is rewritten, not deleted: its link becomes a command returning `Value::Bytes`.

## Phase 2: Architecture

### Solution

`liquers-core/src/interpreter.rs`, private `materialize_link_json`: replace the last line with

```rust
match value.try_into_json_value() {
    Ok(json) => Ok(json),
    Err(json_error) => match value.try_into_query() {
        Ok(query) => Ok(serde_json::Value::String(query.encode())),
        Err(_) => match value.try_into_key() {
            Ok(key) => Ok(serde_json::Value::String(key.encode())),
            Err(_) => Err(attach_context(json_error)),
        },
    },
}
```

with a doc comment stating the rule and that only values without a JSON form reach the fallback.

Rejected: a JSON arm for `Key` / `Query` in `try_into_json_value` (question 2); matching on
`identifier()` strings (`"Key"`, `"Query"`), which ties the interpreter to identifier spelling
rather than to the conversion contract.

### Changes

- `liquers-core/src/interpreter.rs`: `materialize_link_json` body and doc comment; tests in
  `mod tests`. Async as today; no new error type.
- `specs/reference/REGISTER_COMMAND_FSD.md` §Variadic Parameters: a "Links" paragraph — a link
  element is evaluated and bound through its JSON form; a key or query arrives as its encoded text;
  any other value without a JSON form is a `ConversionError` at the link's position. History row,
  `reviewed:` bump.
- No `pub` signature, command, registry, format or binding change.

### Risks

| Risk | Handling | Certainty |
|---|---|---|
| Likely files | `interpreter.rs`, `REGISTER_COMMAND_FSD.md` | high |
| Existing tests | `multiple_parameter_non_json_link_reports_positioned_conversion_error` asserts today's failure with a key link; rewritten to a `Bytes` link (same assertion). `multiple_parameter_links_materialize_in_order` unchanged (its key link is top-level) | high |
| A custom value type whose `try_into_query` parses something | only reached when that type has no JSON form; core, `SimpleValue` and `CombinedValue` convert only real keys/queries there | medium |
| Python / JavaScript values | they implement their own conversions; a value with no JSON form and no query/key conversion keeps today's error | medium |
| Recovery | revert the function body and the test | high |

## Phase 3: Examples and Tests

### Examples

- `join_text-a-~X~-R-key/data/x.csv~E` → `parts = ["a", "data/x.csv"]` (AC-1).
- `join_text-~X~-R/data/x.csv/-/q~E` → `parts = ["-R/data/x.csv"]` (AC-2; the `q` quotes the query
  before it).
- `join_text-~X~bytes_value~E`, `bytes_value` returning `Value::Bytes` → `ConversionError` at the
  link (AC-3).

Queries validated with `liquers-validate --command join_text`: `Ok` (parse level; `-R-key/…` plans
to `UseKeyValue`).

### Tests

In `liquers-core/src/interpreter.rs` `mod tests`, with `ImmediateEnvironment<Value>` and
`join_text` registered by `register_command!(cr, fn join_text(state, parts: Vec<String> multiple)
-> result)` (returns `parts.join("|")`):

- `multiple_parameter_key_link_arrives_as_text` — `evaluate(envref,
  "join_text-a-~X~-R-key/data/x.csv~E", None)` gives `"a|data/x.csv"`. Fails at HEAD. Covers AC-1.
- `multiple_parameter_key_link_binds_vec_key` — a `Vec<Key> multiple` command returning the keys'
  encodings joined; same link; gives `"data/x.csv"`. Covers AC-1.
- `multiple_parameter_query_link_arrives_as_text` — `"join_text-~X~-R/data/x.csv/-/q~E"` gives
  `"-R/data/x.csv"`. Covers AC-2.
- `multiple_parameter_non_json_link_reports_positioned_conversion_error` — rewritten: the link is
  `bytes_value`, a registered command returning `Value::Bytes(vec![1])`; the assertions (error
  type `ConversionError`, the link's position) stay. Covers AC-3.
- Existing `multiple_parameter_links_materialize_in_order` and
  `liquers-lib/tests/record_scalar_reading.rs`
  `record_cell_binds_inside_a_multiple_parameter_through_a_link` unchanged. Covers AC-4, AC-5.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib interpreter::tests::multiple_parameter`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `interpreter.rs` tests — the three new tests (fail at HEAD) and the rewritten
  `…_non_json_link_…` test (passes at HEAD and after) — `cargo test -p liquers-core --lib interpreter::tests::multiple_parameter`
- [ ] 2. `interpreter.rs` `materialize_link_json` — the fallback and doc comment — all step-1 tests
  pass: same command, then `cargo test -p liquers-core --lib interpreter`
- [ ] 3. `REGISTER_COMMAND_FSD.md` §Variadic Parameters "Links" paragraph, History row,
  `reviewed:`; issue `status: closed` with a resolution note naming the decision; tell
  `sql-several-tables` its requirement is met — `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --test record_scalar_reading`
(the record-cell variadic link). Diff review: no `println!`, no `unwrap` in library code, no `_ =>`
arm added.
