---
id: FOREIGN-VALUE-DATA-NODE
kind: design
title: A foreign value can answer as a data node through a ForeignValue hook
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: not-eligible
area: [lib/value]
issues: [FOREIGN-VALUE-HAS-NO-DATA-NODE]
created: 2026-10-26
---
# A foreign value can answer as a data node through a ForeignValue hook

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented.

## Phase 1: High-Level Design

### Purpose

Templates and store handlers read values through core's `DataNode`. A retained *language value*
(`ExtValue::Foreign`: a Python `py.Object`, a JavaScript `JsOpaque`) is always opaque to them,
because `ExtValue::data_node` answers `AnyNode::opaque()` for the variant and `ForeignValue` has no
hook through which the integration could answer. Add that hook, so an integration can provide the
`DATANODE` feature of `guides/LANGUAGE-INTEGRATION_GUIDE.md` (its test `DATANODE08`) for retained
values.

### Problem Example

Verified at HEAD (`liquers-lib/src/value/mod.rs`, `ValueExtension::data_node` for `ExtValue`,
arm `ExtValue::Image { .. } | ExtValue::UIElement { .. } | ExtValue::Foreign { .. } => AnyNode::opaque()`):

```rust
// liquers-py retains {"name": "Ada", "born": datetime(1815, 12, 10)} as py.Object, because
// structural conversion is all-or-nothing and refuses the datetime.
let node = Value::data_node(Arc::new(Value::from(ExtValue::Foreign { value: py_dict })));
node.kind()                // today: NodeKind::Opaque — a template renders {{name}} as nothing
```

Should be: an integration that overrides the new hook returns its adapter, so `kind()` is `Map` and
`get("name")` is `"Ada"`; an integration that does not override it gets the JSON snapshot when its
`try_into_json_value` succeeds, and the opaque node otherwise.

### Scope and Acceptance Criteria

- **AC-1** The hook is delegated
  WHEN a foreign value overrides `ForeignValue::data_node` to return a map node `{"name": "Ada"}`
  and is read through `ValueInterface::data_node` of `liquers_lib::value::Value`
  THEN the node is a `Map` whose `get("name")` is the text `Ada`
- **AC-2** The default reads a faithful JSON form
  WHEN a foreign value does not override `data_node` but its `try_into_json_value` returns
  `{"x": 1}`
  THEN the node is the JSON snapshot: a `Map` with `x` = 1
- **AC-3** The default stays opaque otherwise
  WHEN a foreign value overrides neither hook (the existing `Opaque` mock, as `JsOpaque` and
  `PyObjectValue` do at HEAD)
  THEN the node is `NodeKind::Opaque`, as at HEAD
- **AC-4** The node outlives the value expression
  WHEN the node returned for a foreign value is kept after the `Value` is dropped
  THEN it still answers (it owns an `Arc` of the foreign payload, as `DataFrameNode` does)

Non-goals: the `liquers-py` and `liquers-web` adapters themselves (their `DATANODE` feature work);
text and JSON conversion (`FOREIGN-VALUE-TEXT-AND-JSON-HOOKS-NEVER-CALLED`); async nodes for
foreign values (the hook returns `AnyNode`, so an adapter may return `AnyNode::Async` later
without an API change).

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** not-eligible — adds a `pub` default method to the `pub` trait
  `ForeignValue` (rule 4), though it is additive and no implementor needs to change.
- **Leading issue:** None.
- **Explanation:** an `Arc<Self>` receiver is dyn-compatible, so the hook works on
  `Arc<dyn ForeignValue>`; the default mirrors `ValueExtension::data_node`'s documented default.
- **Open questions:** None. Settled from repository evidence: the default is "JSON snapshot via
  `try_into_json_value`, else opaque" rather than the issue's plain `opaque()`, because that is the
  documented default of `ValueExtension::data_node` (`liquers-lib/src/value/extended.rs`) for every
  other value without its own node, and it is identical to `opaque()` for every implementation at
  HEAD (all refuse JSON).

### Design Dependencies

- `foreign-value-text-json-delegation` — **overlaps**: same trait and `impl` block; independent
  methods. Kept separate by overlap exclusion E1 (that one is eligible for automatic fixing). It
  pins `foreign_data_node_is_still_opaque` (its AC-4); whichever lands second updates that test to
  this design's AC-2/AC-3 answers.
- `value-type-definition-macro` — **overlaps**: generates `ExtValue::data_node`; once both exist,
  its `foreign` arm must delegate to this hook (its AC-4 "foreign values delegate everywhere").
- `template-render-in-env` (`TEMPLATE-TOP-LEVEL-RENDER-HAS-NO-HOST`, designed in a parallel run)
  — **overlaps**, weak: another `DATANODE`/`TEMPLATE` gap; no order.

### Consolidated Findings

- `AnyNode` holds `Arc<dyn DataNode>`, whose bounds are `MaybeSend + MaybeSync`, the same as
  `ForeignValue`'s; a `JsValue` adapter therefore fits on `wasm32`, a `Py<PyAny>` one natively.
- The default body is type-checked with `Self: ?Sized`; it only calls `self.try_into_json_value()`
  through the `Arc`, which is allowed.

## Phase 2: Architecture

### Solution

`liquers-lib/src/value/foreign.rs`, `trait ForeignValue`, a defaulted method:

```rust
/// This value as a data node, for templates and store handlers. Override with an adapter that
/// reads the language value in place. The default is the JSON snapshot when
/// [`ForeignValue::try_into_json_value`] succeeds, and an opaque node otherwise.
fn data_node(self: std::sync::Arc<Self>) -> liquers_core::data_node::AnyNode {
    match self.try_into_json_value() {
        Ok(json) => liquers_core::data_node::AnyNode::from_json(json),
        Err(_) => liquers_core::data_node::AnyNode::opaque(),
    }
}
```

`liquers-lib/src/value/mod.rs`, `ValueExtension::data_node` for `ExtValue`: split the arm —
`ExtValue::Foreign { value } => value.clone().data_node()`; `Image` and `UIElement` stay opaque.
Explicit arms. Sync.

Rejected: `fn data_node(&self) -> AnyNode` (the node must own its payload; with `&self` the
integration would have to clone the language handle itself, which `Arc<Self>` gives for free);
plain `opaque()` default (see Design Readiness); a separate adapter registry keyed by identifier
(more machinery for the same dispatch the trait already does).

Known-issue preflight: nothing open blocks.

### Changes

- `liquers-lib/src/value/foreign.rs`: `ForeignValue::data_node` (default) and a sentence in the
  module documentation's list of hooks; `mod tests`: a `Mapped` mock and the tests below.
- `liquers-lib/src/value/mod.rs`: the `Foreign` arm of `ExtValue`'s `data_node`; its doc comment.
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §`DATANODE`: the hook an integration overrides for
  `DATANODE08` (History row, `reviewed:`).
- No command, registry, format or binding change. `liquers-py` and `liquers-web` compile unchanged.

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `foreign.rs`, `value/mod.rs`, the guide |
| Existing tests | `foreign_data_node_is_still_opaque`, if `foreign-value-text-json-delegation` lands first, is updated (AC-2/AC-3) |
| Compatibility | additive defaulted method; every existing implementor refuses JSON, so answers stay opaque |
| wasm32 | `ForeignValue` is not target-gated; `bash scripts/check-build-matrix.sh` (webui rows) and `cargo check -p liquers-web --target wasm32-unknown-unknown` |
| Recovery | revert the commit |
| Certainty | High |

## Phase 3: Examples and Tests

### Examples

```rust
#[derive(Debug)] struct Mapped;   // identifier "mock.Mapped"
impl ForeignValue for Mapped {
    // origin, as_any, identifier, type_name, default_* as the existing mocks
    fn data_node(self: Arc<Self>) -> AnyNode { AnyNode::from_json(json!({"name": "Ada"})) }
}
let node = Value::data_node(Arc::new(Value::from(ExtValue::Foreign { value: Arc::new(Mapped) })));
assert_eq!(node.kind(), NodeKind::Map);
```

### Tests

Unit, `liquers-lib/src/value/foreign.rs` `mod tests`:

- `foreign_data_node_hook_is_delegated` — `Mapped` through `Value::data_node`: kind `Map`,
  `get("name")` text `Ada`: AC-1
- `foreign_data_node_defaults_to_json_snapshot` — a mock overriding only `try_into_json_value`
  (`{"x": 1}`), called on `Arc<dyn ForeignValue>` directly (so the test does not depend on
  `foreign-value-text-json-delegation`): `Map`, `x` = 1: AC-2
- `foreign_data_node_defaults_to_opaque` — the existing `Opaque` mock: `NodeKind::Opaque`: AC-3
- `foreign_data_node_owns_its_payload` — build the node, drop the `Value`, then read
  `get("name")`: AC-4

Run: `cargo test -p liquers-lib --lib value::foreign`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-lib/src/value/foreign.rs` — `ForeignValue::data_node` with the default, module doc line — `cargo test -p liquers-lib --lib value::foreign`.
- [ ] 2. `liquers-lib/src/value/mod.rs` — `Foreign` arm of `data_node` delegates; doc comment — same command. Depends on 1.
- [ ] 3. `foreign.rs` `mod tests` — `Mapped`, the JSON-only mock, the four tests; update `foreign_data_node_is_still_opaque` if present — `cargo test -p liquers-lib --lib value::foreign`. Rollback for 1-3: revert.
- [ ] 4. `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §`DATANODE` — name the hook (History row, `reviewed:`); close `FOREIGN-VALUE-HAS-NO-DATA-NODE` with a resolution note — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-lib --lib --tests`; `bash scripts/check-build-matrix.sh`;
`cargo check -p liquers-py`; diff review: only the named files.
