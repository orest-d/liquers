---
id: WEB-EVALUATE-CANNOT-RETURN-THE-SERIALIZED-RESULT
kind: feature
title: A page cannot get the bytes of a result in the format its file name asks for
status: draft
priority: P3
complexity: M
area: [web]
design:
created: 2026-11-10
github:
---

## Problem

A query ending in a file name (`…/eu.csv`) records the name and the data format in the asset's
metadata, but `liquers.evaluate` and `Asset.get()` resolve with the value, and no method of
`Asset` returns the serialized bytes. For a table the page receives a `RecordBatch` handle and has
to write the CSV itself.

## Impact

The serializers of the value types exist in the module and cannot be reached from JavaScript. A
page that offers a download, or writes a result to its store, re-implements them
(`docs/guides/web/09-data.md`, section 9.6). Native Liquers and the HTTP API return the bytes.
Workaround: a recipe, whose result is stored and can then be read with `store.get`.

## Expected behaviour

`Asset.getBinary()` (the name of the Rust method) resolves with `{ data: Uint8Array, metadata }`
in the format of the query's file name, or of the type's default format.

## Discovery

Writing chapter 9 of the Web user guide (2026-11-10): `evaluate("…/eu.csv")` resolved with a
`RecordBatch`.
