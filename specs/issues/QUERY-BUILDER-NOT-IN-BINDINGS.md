---
id: QUERY-BUILDER-NOT-IN-BINDINGS
kind: feature
title: Python and JavaScript have no validating query builder
status: draft
priority: P3
complexity: M
area: [py, web]
design:
created: 2026-10-21
github:
---

## Problem

`design/query-builder/` adds `liquers_core::query_builder::QueryBuilder` and
`Query::check_encodable`, but only for Rust. A Python or JavaScript caller can escape a single
parameter (`ActionRequest.from_arguments` in `liquers-py`, `encodeParam` in `liquers-web`). It still
has to assemble resource segments, the `-/` transform header, action names and filenames as text,
with no validation of those raw-emitted tokens.

## Impact

Language integrations are the callers the original issue (`QUERY-BUILDER-TOOLING`) named first.
Until they get the builder, they keep their own string assembly, which is the drift the core builder
exists to remove.

## Expected behaviour

A `QueryBuilder` class in `liquers-py` and an equivalent object in `liquers-web`, each a thin
wrapper over the core builder that returns a `Query` (or its text) and maps a core
`ParameterError` to the language's error class. No escaping or validation logic of their own.

## Discovery

Split off on 2026-10-21 by the bulk design of `QUERY-BUILDER-TOOLING`, whose first slice is
deliberately the core API, the `liquers-encode` CLI and the two existing consumers. Depends on
`design/query-builder/` landing first.
