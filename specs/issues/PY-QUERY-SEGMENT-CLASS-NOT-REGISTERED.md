---
id: PY-QUERY-SEGMENT-CLASS-NOT-REGISTERED
kind: issue
title: Query.segments returns QuerySegment objects whose class is not in the liquers module
status: draft
priority: P3
complexity: S
area: [py]
design: py-query-segment-registration
created: 2026-10-27
github:
---
## Problem

`liquers-py/src/query.rs` declares `#[pyclass(from_py_object, module = "liquers")] pub struct
QuerySegment` (line 429), and the `Query.segments` getter returns a list of them. But
`liquers-py/src/lib.rs` never calls `m.add_class::<QuerySegment>()`, and
`liquers-py/python/liquers/__init__.py` neither imports it nor lists it in `__all__`. A scan of
every `#[pyclass]` against the `add_class` calls finds no other such class.
`PYTHON_INTEGRATION.md` §2's table of query classes does not list it either.

## Problem Example

```python
import liquers as lq
seg = lq.parse("hello/to_text").segments[0]
type(seg).__module__, type(seg).__name__   # ('liquers', 'QuerySegment')
lq.QuerySegment                            # today: AttributeError
isinstance(seg, lq.QuerySegment)           # today: AttributeError; expected: True
```

## Impact

A user cannot name the type of what `Query.segments` returns: no `isinstance` check, no type
annotation, no `help(lq.QuerySegment)`, and a pickled segment cannot be loaded, because its class
reports a module that does not have it. The docstring coverage walk of `py-docstrings` cannot reach
it through `lq.__all__`. Workaround: `type(q.segments[0])`.

## Expected behaviour

Either `QuerySegment` is a registered, exported class like the other query classes, or
`Query.segments` returns classes that are. These are different interfaces; the design picks one.

## Discovery

Found by review of `design/py-docstrings/` on 2026-10-27 (bulk design of the Python backlog):
`QuerySegment` is returned by `Query.segments` but absent from `__all__`.
