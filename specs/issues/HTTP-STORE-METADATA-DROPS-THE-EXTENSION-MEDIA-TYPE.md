---
id: HTTP-STORE-METADATA-DROPS-THE-EXTENSION-MEDIA-TYPE
kind: issue
title: liquers-web's http store reports no media type for a key whose extension determines one
status: closed
priority: P2
complexity: S
area: [web, core/store]
design: store-conformance-backlog
created: 2026-09-27
github:
---
# `liquers-web`'s `http` store reports no media type for a key whose extension determines one

## Problem

The e2e test `STORE10 metadata comes from the extension and the response`
(`liquers-web/tests/e2e/store.spec.ts`) configures an `http` store serving `input.csv` and `blob`,
then reads both keys' metadata:

- `blob`, which has no extension, gets `application/octet-stream` from the response. The test only
  requires a non-empty string, so that half passes.
- `input.csv` gets `media_type: null`. The test expects `text/csv`, which the extension determines,
  and which by the design's precedence rule wins over the response header.

So the extension half of the precedence rule does not reach the metadata the store returns, although
the rule itself is unit-tested.

## Expected behaviour

`getMetadata('data/input.csv')` reports `text/csv`. Where the metadata is assembled, the extension's
media type should be applied before the response header is consulted, as the rule requires.

## Discovery

Found 2026-09-27 in record-streams Step 8.3, the first full `liquers-web` e2e run in some time.
The design's base commit (`686c038`) fails identically, so this predates `record-streams`.

## Resolution

Closed 2026-09-30 by `design/store-conformance-backlog/` step 11 (`671121c`). The store was
correct. `infer_metadata` sets `data_format = csv` from the extension and leaves `media_type`
unset, because under the metadata level model an absent `media_type` means "derived from the
format". The e2e expectation predated that model. `STORE10` now asserts `data_format === 'csv'`
and an absent raw `media_type`, and passes. A page that needs the effective media type has no way to
ask for it, which is filed as `JS-STORE-WRAPPER-HAS-NO-EFFECTIVE-MEDIA-TYPE`.
