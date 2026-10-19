---
id: COMBINED-VALUE-DATA-NODE-COPIES-BASE-VALUE
kind: issue
title: A base value is copied whole to become a data node
status: draft
priority: P3
complexity: S
area: [lib/value]
design: combined-value-shared-data-node
created: 2026-10-17
github:
---
## Problem

`CombinedValue::data_node` (`liquers-lib/src/value/extended.rs`) receives `Arc<CombinedValue>`
and must return an owned node, so for a `Base` value it clones the base value into a new `Arc`
(`BaseValue::data_node(Arc::new(base.clone()))`). `ValueInterface` does not require the base value
to implement `DataNode`, so the node cannot borrow through the outer `Arc`.

## Problem Example

`ns-template/render` over a state holding a `SimpleValue::Object` of 100 000 entries:
`Value::data_node(arc)` clones the whole object into a new `Arc` before the template reads one
field. Wanted: the node reads through `arc`, with no copy.

## Impact

A template rendered over a large `SimpleValue` (a big `Object` or `Array`) copies it once per
render. Extension values are unaffected, since their payloads are `Arc`s. Small in practice; it
contradicts "no conversion of the whole value" (design/liquers-mustache AC-15) only for base values.

## Expected behaviour

The node shares the value: for example a node type over `Arc<CombinedValue<B, E>>` when
`B: DataNode`, or a `data_node` hook taking the outer `Arc`.

## Discovery

Writing `CombinedValue::data_node` in design/liquers-mustache Step 13.
