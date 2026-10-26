---
id: PY-METADATA-METHODS-RAISE-PLAIN-EXCEPTION
kind: issue
title: Metadata, Recipe and expiration methods in liquers-py raise a plain Exception instead of the typed LiquersError
status: draft
priority: P3
complexity: S
area: [py]
design:
created: 2026-10-20
github:
---

## Problem

`reference/PYTHON_INTEGRATION.md` §6 says a Liquers error becomes the `LiquersError` subclass for
its `ErrorType` (`error_to_pyerr`). The older wrapper classes bypass that: 22 call sites map a
`liquers_core::error::Error` with `pyo3::exceptions::PyException::new_err(e.to_string())` —
15 in `liquers-py/src/metadata.rs` (`Metadata.get_asset_info`, `from_json`, `is_error`,
`to_json`, `query`, `type_identifier`, `set_extension`, `error_result`, and the same on
`MetadataRecord`), 6 in `liquers-py/src/recipes.rs`, 1 in `liquers-py/src/expiration.rs`.

Re-checked at HEAD (2026-10-26): unchanged — 15 + 6 + 1 sites. The 23rd
`PyException::new_err` (`liquers-py/src/error.rs`, `error_to_pyerr`) is the deliberate fallback when
building the typed exception itself fails, and stays.

## Problem Example

```python
try:
    lq.Metadata.from_json("not json")
except lq.LiquersError:
    pass                      # expected: caught, as SerializationError
# today: escapes as a plain Exception("..."), with no `type`, `query` or `key`
```

## Impact

`except lq.LiquersError` does not catch these failures, and the error's `type`, `query` and `key`
fields are lost; only the rendered message survives. `Metadata.error_result()`, whose purpose is
to raise the stored error, raises it untyped.

## Expected behaviour

Every `Error` crossing into Python goes through `error_to_pyerr`, so the exception is the typed
subclass with its fields; a JSON parse failure that is not a Liquers `Error` maps to the matching
`ErrorType` (for example `SerializationError`). A test checks one method per class.

## Discovery

Found 2026-10-20 while checking `PY-METADATA-FIELDS-ARE-METHODS` at HEAD during the bulk-design
pass (`grep PyException::new_err liquers-py/src`).
