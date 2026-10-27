---
id: FOREIGN-VALUE-HAS-NO-DATA-NODE
kind: issue
title: A retained foreign value is always opaque to data-node consumers
status: draft
priority: P2
complexity: S
area: [lib/value]
design: foreign-value-data-node
created: 2026-10-25
github:
---
## Problem

`ExtValue`'s `data_node` (`liquers-lib/src/value/mod.rs`) returns `AnyNode::opaque()` for the
`Foreign` variant, and `ForeignValue` (`liquers-lib/src/value/foreign.rs`) has no hook through
which an integration could answer instead. A retained *language value* — a Python `py.Object`, a
JavaScript `JsOpaque` — is therefore always opaque to every `DataNode` consumer: a template renders
it as nothing, and a store handler cannot read a field of it.

This bites harder than it sounds in Python: `liquers-py`'s structural conversion is all-or-nothing,
so a `dict` with one non-`str` key or one unconvertible member (a `datetime`) becomes `py.Object`
as a whole, and its readable fields vanish from a template.

## Problem Example

`liquers-py` retains `{"name": "Ada", "born": datetime(1815, 12, 10)}` as one `py.Object`
(structural conversion refuses the `datetime`). `Value::data_node` of that value is
`NodeKind::Opaque` today, so a template renders `{{name}}` as nothing. Should be: the
integration's adapter answers, `get("name")` is `Ada`. Verified at HEAD (2026-10-26): the arm is
`ExtValue::Image { .. } | ExtValue::UIElement { .. } | ExtValue::Foreign { .. } => AnyNode::opaque()`.

## Impact

Integrations cannot offer the `DATANODE` feature of `guides/LANGUAGE-INTEGRATION_GUIDE.md` for
retained values; `DATANODE08` cannot pass. The workaround is structural conversion, which loses
whatever conversion refuses, or handing an adapter to a consumer directly (the programmatic render
of `TEMPLATE`), which covers only that one path.

## Expected behaviour

An additive default method on `ForeignValue`, object-safe through an `Arc` receiver:

```rust
fn data_node(self: Arc<Self>) -> AnyNode { AnyNode::opaque() }
```

and the `Foreign` arm of `ExtValue::data_node` delegating to it. An integration overrides it with
its adapter (`liquers-py` over `Py<PyAny>`, `liquers-web` over `JsValue`). A unit test with a mock
foreign value returning a map node proves the delegation. Related, same shape:
`FOREIGN-VALUE-TEXT-AND-JSON-HOOKS-NEVER-CALLED`.

## Discovery

Writing the `DATANODE` section of the language integration guide (2026-10-25), reading the
`Foreign` arm of `ExtValue::data_node`.
