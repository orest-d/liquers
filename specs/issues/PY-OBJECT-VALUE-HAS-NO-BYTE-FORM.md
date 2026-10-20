---
id: PY-OBJECT-VALUE-HAS-NO-BYTE-FORM
kind: feature
title: A retained Python object (py.Object) cannot be stored as bytes
status: closed
priority: P3
complexity: M
area: [py, core/value]
design: liquers-py
created: 2026-10-19
github:
---
## Resolution

Closed by the `liquers-py` design (PR #150), which took option 2: `py.Object` is serialized with
`pickle` only in an environment built with `Environment(pickle=True)`, and a pickle-off environment
declares no format for it and never unpickles stored bytes
(`specs/reference/PYTHON_INTEGRATION.md`, values section). The `Value::Py` variant described below
no longer exists; a Python object is now `py.Object` in `ExtValue::Foreign`.

## Problem

`liquers-py`'s `Value::Py` (identifier `py.Object`, `liquers-py/src/value.rs`) declares `pickle` as
its default extension and data format name but no supported data format, and its serializer refuses
it. So an asset whose value is an arbitrary Python object is stored as metadata only and is always
recomputed on reload.

## Impact

Python commands returning objects that are not one of the neutral variants (a NumPy array, a user
class) cannot be cached in a store. Workaround: convert to a neutral value (text, JSON-like
`Object`/`Array`, bytes) inside the command.

## Expected behaviour

A deliberate decision, not an omission. `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` says pickle
"may be an opt-in codec, never an implicit trusted interchange format": unpickling bytes read from a
store executes code. Options to weigh:

1. Keep it metadata-only and change the declared default extension away from `pickle`, so nothing
   suggests a byte form exists.
2. An opt-in pickle codec, enabled per environment (or per store), refusing to read by default.
3. A protocol hook (`__liquers_bytes__` / `__liquers_from_bytes__`) the object's class implements.

## Discovery

Split off on 2026-10-19 from `PY-VALUE-SERIALIZER-IS-A-STUB` (design `py-value-serializer`), which
makes every other `liquers-py` variant serializable by delegating to `liquers-core` and leaves
`py.Object` as it is.
