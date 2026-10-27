---
id: PY-METADATA-FIELDS-ARE-METHODS
kind: issue
title: Python Metadata exposes its fields as methods while State and AssetRef use properties
status: draft
priority: P3
complexity: S
area: [py]
design:
created: 2026-10-19
github:
---

## Problem

`lq.Metadata` (`liquers-py/src/metadata.rs`, `impl Metadata`), the class `State.metadata` and
`AssetRef.metadata` return, exposes its read-only fields as methods: `type_identifier()`,
`status()`, `message()`, `is_error()`, `filename()`, `query()`, `extension()`, `file_size()`,
`is_dir()`, `unicode_icon()`, `log()`, plus `get_media_type()` and `get_data_format()`.
`lq.State.value`, `lq.AssetRef.status`, `lq.AssetRef.metadata` and every field of
`lq.MetadataRecord` and `lq.AssetInfo` are properties (`#[getter]`).

Re-checked at HEAD (2026-10-26): `MetadataRecord` is consistent (all fields are getters, with
setters; its remaining methods are mutators such as `with_query`, `add_log_entry`), so the earlier
"MetadataRecord mixes both" no longer holds. Only `Metadata` is inconsistent.

## Problem Example

```python
env = lq.Environment()
@env.first_command
def hello() -> str:
    return "hello"

state = env.evaluate_state("hello")
state.metadata.type_identifier == "Text"       # False, silently: a bound method, not a string
state.metadata.type_identifier() == "Text"     # True (guides/PYTHON_GUIDE.md uses this form)
lq.MetadataRecord().type_identifier            # a string: the record's form is a property
```

## Impact

Users guess wrong, and the wrong guess fails silently: a bound method compares unequal to any
string without raising, so a mistaken test passes or fails for the wrong reason.

## Expected behaviour

The read-only fields of `Metadata` become properties under the same names, matching
`MetadataRecord`; `get_media_type` / `get_data_format` become `media_type` / `data_format`. No
deprecation period: PyO3 cannot keep a method and a property under one name, and the package is
unreleased. Callers in `liquers-py/tests/` and `guides/PYTHON_GUIDE.md` (§ on metadata) change
with it. A property that can fail (`query`, `type_identifier`, `is_error`) raises the typed
`LiquersError`, which is `PY-METADATA-METHODS-RAISE-PLAIN-EXCEPTION`; land the two together.

## Discovery

Found while writing `guides/PYTHON_GUIDE.md` for `design/liquers-py/` Phase 5 (2026-10-19); split
from `PY-API-REFERENCE-NOT-GENERATED`. Tightened 2026-10-26 (project-groups plan).
