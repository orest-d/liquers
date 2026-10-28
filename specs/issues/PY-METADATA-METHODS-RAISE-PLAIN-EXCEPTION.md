---
id: PY-METADATA-METHODS-RAISE-PLAIN-EXCEPTION
kind: issue
title: Metadata, Recipe and expiration methods in liquers-py raise a plain Exception instead of the typed LiquersError
status: closed
priority: P3
complexity: M
area: [py]
design: py-metadata-typed-errors
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

Designed 2026-10-27 in `design/py-metadata-typed-errors/` (bulk design). Re-counted at HEAD
(`04667a80`): 23 sites in these three files (`recipes.rs` also has one written
`PyErr::new::<…PyException, _>`), plus 33 serde sites with the same cause in `command_metadata.rs`
and `plan.rs`, which the design includes. Complexity raised from `S` to `M`: six files in one
crate.

## Resolution

Closed on 2026-10-28 by automatic fix, design `design/py-metadata-typed-errors/`, branch
`claude/project-thread-yi0j65-py-metadata-typed-errors`. All 56 sites now raise the typed
`LiquersError`: the 9 that carry a Liquers `Error` go through `error_to_pyerr`, the 46 serde
failures raise `SerializationError` (new `pub(crate)` helper `serialization_pyerr` in
`liquers-py/src/error.rs`), and a bad `ExpirationTime` spec raises `GeneralError`, as `Expires`
does. Only `error.rs`'s deliberate fallback still mentions `PyException`. Tests:
`test_ERROR_wrapper_deserialization_raises_serialization_error`, `test_ERROR_stored_error_keeps_its_type`,
`test_ERROR_wrapper_constructors_raise_typed_errors`, `test_ERROR_typed_wrapper_errors_are_still_exceptions`
in `liquers-py/tests/test_ERROR_errors.py`; `bash scripts/test-liquers-py.sh` passes.
