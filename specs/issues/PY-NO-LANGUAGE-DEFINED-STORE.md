---
id: PY-NO-LANGUAGE-DEFINED-STORE
kind: feature
title: A store cannot be written in Python, and liquers-py has no STORE feature
status: draft
priority: P3
complexity: L
area: [py, core/store, store/config]
design: liquers-py-store
created: 2026-10-27
github:
---
## Problem

> **2026-10-31:** the sketch below is superseded. Orest decided that a configuration names a
> Python-registered *factory* (`type` is its unique name, the entry's arguments its parameters);
> see the design folder in `design:`.

`liquers-py` cannot adapt a Python object to `AsyncStore`. Direction 1 of the language-integration
guide's `STORE` feature ("a store written in the *integrated language*",
`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §STORE) has no implementation: there is no Python
store protocol or base class, no Rust adapter calling it, and no way to name such a store in a
store configuration. What exists is directions 2 and 3 through Rust: the environment builds an
`AsyncMemoryStore`, or the router from `config.store.stores` through
`liquers_store::store_factory::default_store_factory()` (`specs/reference/PYTHON_INTEGRATION.md`
"Store"), and `store_set` writes values.

`liquers-py/README.md`'s conformance table has no `STORE` row at all, so the gap is not even
recorded as `NA`. Two designs already depend on it:

- `design/language-store-type-registration/` (question 5, Phase 4 step 5) keeps the Python half
  of a page-defined store *type* waiting because "`liquers-py` has no `STORE` feature"; a store
  type needs the store adapter first.
- `design/language-store-config-function/` lists "a Python language store" as a non-goal; its
  Python handlers run over the built-in stores only.

## Problem Example

```python
import liquers as lq

class DictStore:                      # the shape a Python store would take
    def __init__(self): self.d = {}
    def get(self, key): return self.d[key.encode()]
    def set(self, key, data, metadata): self.d[key.encode()] = data

env = lq.Environment()
env.register_store_object("notes", DictStore())   # today: no such method, no store protocol
# expected: a mount `{type: py, prefix: notes, config: {object: notes}}` (or equivalent) routes
#           `-R/notes/a.txt` to DictStore, and STORE01-STORE07 pass against it
```

## Impact

A Python user cannot back Liquers with storage that only Python reaches (an internal API, an ORM, a
library with no Rust binding) except by copying data in with `store_set`. The bridging work for
store types and Python `store_config` handlers waits on it. Workaround: write the store in Rust in
`liquers-store`, or push values in with `store_set`.

## Expected behaviour

`liquers-py` selects `STORE` direction 1: a documented Python store protocol (sync and async
methods, data-only access per `STORE_IMPLEMENTATION_GUIDE.md` §4a), a Rust `AsyncStore` adapter
that calls it (as `liquers-web`'s `JsStore` does for JavaScript), a store type in the
configuration document that names a registered Python object, the conformance suite (`STORE01`
to `STORE07`, `STORE14` to `STORE17` where they apply) run against it, and a `STORE` row in
`liquers-py/README.md`. How the GIL is held across store calls and how absence is signalled
(`KeyNotFound` versus a read error, as `WEB-JS-STORE-CANNOT-EXPRESS-KEY-NOT-FOUND` settled for
JavaScript) are design questions.

**No design yet.** It is `L` (a new public Python API, an adapter, a factory entry and a
conformance suite across `liquers-py` and the store configuration), and the 2026-10-27 bulk design
run took only `S`/`M` items. `docs_index.py --check` flags the missing `design:`, which is the
intended state until one is written (`DOCS_STRUCTURE_GUIDE.md` §4.8.3).

## Discovery

Found during the 2026-10-27 bulk design of the Python backlog: `language-store-type-registration`
Phase 4 step 5 and `language-store-config-function` both point at a Python `STORE` feature that
no issue tracked. Checked against `liquers-py/README.md` (no `STORE` row) and
`LANGUAGE-INTEGRATION_GUIDE.md` §STORE (direction 1). Not a duplicate of
`LANGUAGE-STORE-TYPE-NOT-DEFINABLE`, which is about store *types* and whose design covers only
`liquers-web`; that design's step 5 should file its Python half as requiring this feature.
