---
id: JS-STORE-RESOURCE-NOT-FOUND-WITHOUT-A-RECIPE
kind: issue
title: A resource served by a page-implemented js store fails with "No recipe found"
status: draft
priority: P2
complexity: M
area: [web, core/assets]
design:
created: 2026-09-27
github:
---
# A resource served by a page-implemented `js` store fails with "No recipe found"

## Problem

The e2e test `STORE07 a JavaScript store evaluates end to end`
(`liquers-web/tests/e2e/store.spec.ts`) registers a store object implementing only `get` and
`contains`. It routes the `mem` prefix to that object and evaluates `-R/mem/greeting.txt/-/to_text`.
The object answers `contains('mem/greeting.txt')` with `true` and `get` with the data. The query
fails with `No recipe found for key mem/greeting.txt`.

So the asset layer does not see the resource as present in the store, and it falls through to
recipe evaluation. One likely cause is that a method the object does not implement (`getMetadata`,
for one) is read as "absent" rather than derived from `get`. This is unconfirmed.

## Expected behaviour

A store object implementing `get` and `contains` serves a resource query, as the e2e test
expects. Otherwise, the store should refuse the object at registration, naming the
method it lacks.

## Discovery

Found 2026-09-27 in record-streams Step 8.3, the first full `liquers-web` e2e run in some time.
The design's base commit (`686c038`) fails identically, so this predates `record-streams`.
