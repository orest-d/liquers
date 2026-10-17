---
id: PY-VALUE-ENUM-OVERSIZED
kind: issue
title: liquers-py's own Value enum is over 700 bytes because large variants are stored inline
status: draft
priority: P3
complexity: M
area: [py]
design: py-value-arc-payloads
created: 2026-10-12
github:
---
## Problem

`liquers-py/src/value.rs` defines its own `Value` enum with struct variants that hold
`MetadataRecord`, `Recipe` and `CommandMetadata` (among others) inline, the same shape that makes
`liquers_core::value::Value` 752 bytes (`CORE-VALUE-ENUM-OVERSIZED`, measured 2026-10-12). Every
Python-side `Value`, including `None`, `Bool` and `I32`, occupies roughly that size, and every clone
copies it.

## Problem Example

```rust
// liquers-py, expected to print roughly 750 today (not measured: liquers-py was not built)
eprintln!("{}", std::mem::size_of::<liquers_py::value::Value>());
```

Expected after a fix: 64 bytes or less, matching the bound the core design sets.

## Impact

Memory and copy cost for values crossing the Python binding; lower than the core case because
Python-side collections hold `PyObject`s, not `Vec<Value>`. No workaround needed.

## Expected behaviour

The large payloads sit behind `Arc`, as in the core design, with a `size_of` guard test.

## Discovery

Found on 2026-10-12 while writing the `core-value-enum-oversized` bulk design, which scopes
liquers-py out because the binding defines its own enum. Weak overlap with that design: same remedy,
independent change site; do it after it so the bound and convention are settled.
