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

`lq.Metadata`'s fields are methods: `type_identifier()`, `status()`, `message()`, `is_error()`,
`filename()`, `query()`. `lq.State.value`, `lq.AssetRef.status` and `lq.AssetRef.metadata` are
properties, and `lq.MetadataRecord` mixes both.

## Impact

Users guess wrong: `state.metadata.type_identifier` is a bound method, which compares unequal to
any string without raising, so a mistaken test passes or fails silently.

## Expected behaviour

Read-only fields are properties throughout, with the method forms kept for a deprecation period,
or the convention is stated in the API reference and applied consistently.

## Discovery

Found while writing `guides/PYTHON_GUIDE.md` for `design/liquers-py/` Phase 5 (2026-10-19); split
from `PY-API-REFERENCE-NOT-GENERATED`.
