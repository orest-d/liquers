---
id: COMBINED-VALUE-SHARED-DATA-NODE
kind: design
title: A base value's data node shares the value instead of copying it
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [lib/value, core/value]
issues: [COMBINED-VALUE-DATA-NODE-COPIES-BASE-VALUE]
created: 2026-10-17
---
# A base value's data node shares the value instead of copying it

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-17 backlog compaction. Phases 1-4
are written but not approved, and nothing is implemented.

## Phase 1: High-Level Design

### Purpose

`CombinedValue::data_node` (`liquers-lib/src/value/extended.rs`) gets `Arc<CombinedValue<B, E>>`
and, for `CombinedValue::Base(base)`, returns `BaseValue::data_node(Arc::new(base.clone()))`: a
deep copy of the base value per call. The generic `impl ValueInterface for CombinedValue<B, E>`
only knows `B: ValueInterface`, which does not expose a `DataNode` view, so the node cannot borrow
the base through the outer `Arc`.

### Problem Example

`ns-template/render` over a state holding a `SimpleValue::Object` with 100 000 entries:
`ContextHost::node_of` (`liquers-lib/src/template/host.rs`) calls `Value::data_node(value.clone())`
— the `Arc` clone is cheap, but `CombinedValue::data_node` then copies the whole map. Should be: the
node reads the map in place through the existing `Arc<Value>`, with no copy.

### Scope and Acceptance Criteria

- **AC-1** No copy
  - WHEN `Value::data_node(arc)` is called on a `Base` value
  - THEN the returned node holds `arc` itself (`Arc::strong_count(&arc)` grows by one; no new
    `SimpleValue` is allocated)
- **AC-2** Same view
  - WHEN a template walks the node (kind, scalar, get, items, keys, index, to_json)
  - THEN every answer equals the one from the `SimpleValue` node today
- **AC-3** Apply input keeps the value
  - WHEN such a node is an apply tag's input (`ContextHost::input_state`)
  - THEN it is recovered as the value (via `as_any` → `Arc<Value>`), with its type, not as JSON
- **AC-4** A base without a data view
  - WHEN `B` exposes no `DataNode` view
  - THEN the node is today's `B::data_node` result (JSON snapshot), unchanged

Non-goals: extension values (their payloads are `Arc`s; the `ext.clone()` is cheap).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds a `pub` default method to `ValueInterface` in
  `liquers-core` and a new struct in `liquers-lib` (rules 3, 4); two crates (rule 6).
- **Leading issue:** the hook shape on `ValueInterface`.
- **Explanation:** A working design exists under the recommended hook; it extends a core trait that
  every value type and the bindings implement, so it is a maintainer choice.
- **Open questions:**
  1. **Open design - hook:** (a) `fn as_data_node(&self) -> Option<&dyn DataNode> { None }` on
     `ValueInterface`, overridden by core `Value` and `SimpleValue`; (b) a `B: DataNode` bound on
     the `CombinedValue` impl (breaks any base type without `DataNode`; none in tree, but
     downstream crates via `liquers-web`'s documented `CombinedValue<SimpleValue, MyExt>` path keep
     `SimpleValue`, so it would hold today); (c) a projecting hook
     `data_node_of(outer: Arc<O>, project: fn(&O) -> &Self)`. *Recommended:* (a): additive, a
     default, no bound change.

### Design Dependencies

- `overlaps` `template-non-scalar-text-form` (weak: both touch `DataNode` for `SimpleValue`;
  independent sites, either order).

### Consolidated Findings

The wrapper's `as_any` must return the `Arc<CombinedValue<B, E>>`, which for liquers-lib's `Value`
is exactly the `Arc<Value>` that `ContextHost::input_state` already downcasts first — so AC-3 gets
cheaper rather than needing new code. Size stays **S**.

## Phase 2: Architecture

### Solution

1. `liquers-core/src/value.rs` `ValueInterface`: add
   `fn as_data_node(&self) -> Option<&dyn crate::data_node::DataNode> { None }`; core `Value`
   overrides it with `Some(self)`.
2. `liquers-lib/src/value/simple.rs` `impl ValueInterface for SimpleValue`: `Some(self)`.
3. `liquers-lib/src/value/extended.rs`: a private
   `struct BaseNode<B, E>(Arc<CombinedValue<B, E>>)` implementing `DataNode` by delegating every
   method to `self.base()`, where `fn base(&self) -> &dyn DataNode` matches
   `CombinedValue::Base(b)` → `b.as_data_node()` and otherwise (unreachable by construction) a
   static `OpaqueNode`; `as_any` returns `Some(&self.0)`.
4. `CombinedValue::data_node`, `Base` arm: if `base.as_data_node().is_some()` →
   `AnyNode::Sync(Arc::new(BaseNode(value)))`, else today's copy (AC-4). `Extended` arm unchanged.

`B, E: 'static + MaybeSend + MaybeSync` already follow from `ValueInterface`, so `Arc<dyn DataNode>`
coerces. `DataNode` methods return `'_`-borrowed refs, which borrow `self.0` through `&self`.

Rejected: (b), (c) as above; `Arc::try_unwrap` (the caller always holds another reference).

### Changes

`ValueInterface::as_data_node` (new default); overrides on core `Value` and `SimpleValue`;
`BaseNode` (private); `CombinedValue::data_node`. Doc: `specs/reference/TEMPLATE_LANGUAGE.md` §3
node table gains nothing (still "in place"); `VALUE_TYPE_SYSTEM.md` mentions the hook, with a
History row. `liquers-py` / `liquers-web` implementors get the default.

### Risks

`as_any` now yields `Arc<Value>` where it yielded `SimpleValue`: `input_state` handles both, in that
order. Certainty: high.

## Phase 3: Examples and Tests

### Examples

The Problem Example.

### Tests

`liquers-lib/src/value/extended.rs` tests:

- `base_data_node_shares_the_value` — strong count before/after; node `as_any` downcasts to
  `Arc<Value>` pointing at the same allocation (`Arc::ptr_eq`) — AC-1, AC-3
- `base_data_node_answers_like_simple_value` — an `Object` with a nested `Array`: kind, get, items,
  keys, index, to_json equal `SimpleValue`'s node — AC-2
- `base_without_data_view_falls_back` — a test base type with the default `as_data_node` — AC-4

`liquers-lib/tests/template_commands.rs` existing `apply_tag_keeps_extended_value`,
`data_argument_supplies_names` — AC-2, AC-3.

Run: `cargo test -p liquers-lib --lib value::extended`, `cargo test -p liquers-lib --test template_commands`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Decision 1 answered (recommended: a)
- [ ] 2. `liquers-core/src/value.rs` `as_data_node` + core `Value` override — `cargo test -p liquers-core --lib`
- [ ] 3. `liquers-lib/src/value/simple.rs` override; `extended.rs` `BaseNode`, `data_node` — `cargo test -p liquers-lib --lib value::extended`
- [ ] 4. `cargo test -p liquers-lib --test template_commands`; `bash scripts/check-build-matrix.sh` (wasm row: `MaybeSend`)
- [ ] 5. `VALUE_TYPE_SYSTEM.md` and History; close the issue — `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-lib --lib --tests`; the build matrix. Rollback: revert; additive change.
