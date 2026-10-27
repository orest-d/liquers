---
id: FOREIGN-VALUE-TEXT-JSON-DELEGATION
kind: design
title: ExtValue's Foreign arm delegates text and JSON conversion to the foreign value
form: compact
status: complete
gh_pr: [209]
readiness: ready
autofix: eligible
area: [lib/value]
issues: [FOREIGN-VALUE-TEXT-AND-JSON-HOOKS-NEVER-CALLED]
created: 2026-10-26
---
# ExtValue's Foreign arm delegates text and JSON conversion to the foreign value

> **Complete on 2026-10-27**: implemented by automatic fix in #209.

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written; Phase 4
implemented on 2026-10-27 (automatic fix).

## Phase 1: High-Level Design

### Purpose

`ForeignValue` (`liquers-lib/src/value/foreign.rs`) lets a language integration offer a faithful
text and JSON form of a retained value, and its module documentation promises that "every `match`
arm on the variant is a one-line delegation to this trait". Two arms break that promise, so the two
hooks can never run. Restore the delegation.

### Problem Example

Verified at HEAD, `impl ValueExtension for ExtValue` (`liquers-lib/src/value/mod.rs`):

```rust
// A foreign value whose language can give a faithful JSON form (a Python dataclass, say):
impl ForeignValue for Point { fn try_into_json_value(&self) -> Result<serde_json::Value, Error> {
    Ok(json!({"x": 1, "y": 2})) } /* … */ }
let v = Value::from(ExtValue::Foreign { value: Arc::new(Point) });
v.try_into_json_value()
```

Today: `Err(ConversionError)` from `ext_scalar_refusal`, because the `Foreign` arm shares the
refusing pattern `ExtValue::Image { .. } | ExtValue::UIElement { .. } | ExtValue::Foreign { .. }`
in `try_into_string` and `try_into_json_value`. Should be: `Ok({"x":1,"y":2})`. The same holds for
`try_into_string`, and so for binding the value to a `String` command argument.

### Scope and Acceptance Criteria

- **AC-1** JSON is delegated
  WHEN a foreign value whose `try_into_json_value` returns `{"x":1,"y":2}` is converted with
  `Value::try_into_json_value`
  THEN the result is that JSON
- **AC-2** Text is delegated, including the option form
  WHEN a foreign value whose `try_into_string` returns `"point(1,2)"` is converted with
  `try_into_string` and `try_into_string_option`
  THEN the results are `"point(1,2)"` and `Some("point(1,2)")`
- **AC-3** A refusing foreign value still refuses
  WHEN a foreign value that keeps the default hooks (the existing `Opaque` mock, as `JsOpaque` and
  `PyObjectValue` do) is converted to text or JSON
  THEN the result is a `ConversionError` naming its identifier, as at HEAD
- **AC-4** The data node follows
  WHEN a foreign value with a faithful JSON form is read through `ValueInterface::data_node`
  THEN today it is still opaque (the `Foreign` arm of `ExtValue::data_node` does not consult JSON);
  this design leaves that to `foreign-value-data-node` and only pins today's answer

Non-goals: the numeric and boolean hooks (`ForeignValue` declares none); the data node
(`FOREIGN-VALUE-HAS-NO-DATA-NODE`); making `JsOpaque` or `PyObjectValue` offer a form.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — a bug fix restoring the behaviour documented on `ForeignValue`
  and its module, in two match arms of `liquers-lib/src/value/mod.rs`; no `pub` signature, type,
  command, format or binding change; no existing implementation returns `Ok` from either hook, so
  nothing observable changes for `liquers-py` or `liquers-web`.
- **Leading issue:** None.
- **Explanation:** both hooks exist with the right signatures and error contract (a
  `ConversionError` by default); only the dispatch is missing.
- **Open questions:** None. The alternative the issue names, removing the two hooks, is rejected:
  it removes a `pub` trait method and the `DATANODE` guidance relies on faithful JSON.

### Design Dependencies

- `foreign-value-data-node` — **overlaps** (same trait and `impl` block, different methods). Kept
  separate by overlap exclusion E1: this design is eligible for automatic fixing, that one adds a
  `pub` trait method. No order; whichever lands second rebases the neighbouring arm.
- `value-type-definition-macro` — **overlaps**: its generated `foreign` arms delegate (its AC-4,
  test `foreign_json_hook_is_delegated`), so it would also fix this; its Phase 2 says "fix
  separately if earlier". If this lands first, the macro's AC-4 test is already green.

### Consolidated Findings

- `try_into_string_option`'s `Foreign` arm already calls `self.try_into_string()`, so it follows
  automatically; only two arms change.
- The refusal for the other two variants keeps `ext_scalar_refusal`; the foreign hooks' own default
  produces the same error type and message shape, so AC-3 holds with no special case.

## Phase 2: Architecture

### Solution

In `liquers-lib/src/value/mod.rs`, `impl ValueExtension for ExtValue`:

```rust
fn try_into_string(&self) -> Result<String, Error> {
    match self {
        ExtValue::Foreign { value } => value.try_into_string(),
        ExtValue::Image { .. } | ExtValue::UIElement { .. } => Err(ext_scalar_refusal(self, "string")),
        // … remaining arms unchanged
    }
}
// try_into_json_value: the same split, `ExtValue::Foreign { value } => value.try_into_json_value()`.
```

Explicit arms, no `_ =>`. Sync, no allocation beyond what the hook returns.

Rejected: remove the hooks from `ForeignValue` (a `pub` API removal; contradicts the trait's
documented purpose); wait for `value-type-definition-macro` (XL, `needs-decision`).

Known-issue preflight: `FOREIGN-VALUE-HAS-NO-DATA-NODE` (independent, see dependencies). Nothing
open blocks.

### Changes

- `liquers-lib/src/value/mod.rs`: the `Foreign` arms of `try_into_string` and
  `try_into_json_value`; the doc comment above `try_into_string` mentions the delegation.
- `liquers-lib/src/value/foreign.rs` `mod tests`: a `Faithful` mock and three tests (Phase 3).
- No command, registry, store, binding or reference-document change (the trait docs already say
  this is the behaviour).

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-lib/src/value/mod.rs`, `liquers-lib/src/value/foreign.rs` |
| Existing tests | none asserts the refusal for a foreign value; all stay green |
| Compatibility | `JsOpaque` and `PyObjectValue` refuse in both hooks, so behaviour is unchanged for them |
| Feature matrix | the arms are not feature-gated; `bash scripts/check-build-matrix.sh` confirms |
| Recovery | revert the one commit |
| Certainty | High |

## Phase 3: Examples and Tests

### Examples

```rust
#[derive(Debug)] struct Faithful;            // identifier "mock.Faithful"
impl ForeignValue for Faithful {
    // origin, as_any, identifier, type_name, default_* as the existing mocks
    fn try_into_string(&self) -> Result<String, Error> { Ok("point(1,2)".into()) }
    fn try_into_json_value(&self) -> Result<serde_json::Value, Error> { Ok(json!({"x":1,"y":2})) }
}
let v = Value::from(ExtValue::Foreign { value: Arc::new(Faithful) });
assert_eq!(v.try_into_json_value()?, json!({"x":1,"y":2}));
assert_eq!(v.try_into_string_option()?, Some("point(1,2)".to_string()));
```

### Tests

Unit, `liquers-lib/src/value/foreign.rs` `mod tests`:

- `foreign_json_hook_is_delegated` — `Faithful` through `Value::try_into_json_value`: AC-1
- `foreign_text_hook_is_delegated` — `try_into_string` and `try_into_string_option`: AC-2
- `default_foreign_hooks_still_refuse` — the existing `Opaque` mock: both conversions are
  `ErrorType::ConversionError` and the message names `mock.Value`: AC-3
- `foreign_data_node_is_still_opaque` — `ValueInterface::data_node` of `Faithful` has
  `NodeKind::Opaque` (to be flipped by `foreign-value-data-node`): AC-4

Run: `cargo test -p liquers-lib --lib value::foreign`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-lib/src/value/foreign.rs` `mod tests` — add `Faithful` and the four tests; AC-1/AC-2 fail at HEAD — `cargo test -p liquers-lib --lib value::foreign`.
- [x] 2. `liquers-lib/src/value/mod.rs` `ValueExtension for ExtValue` — split the `Foreign` arm off in `try_into_string` and `try_into_json_value`, delegating — `cargo test -p liquers-lib --lib value::foreign`. Rollback: revert.
- [x] 3. Close `FOREIGN-VALUE-TEXT-AND-JSON-HOOKS-NEVER-CALLED` (`status: closed`, resolution note) — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-lib --lib --tests`; `bash scripts/check-build-matrix.sh`; diff review: only
the two source files and the issue.
