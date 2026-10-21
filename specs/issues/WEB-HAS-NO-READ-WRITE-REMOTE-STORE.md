---
id: WEB-HAS-NO-READ-WRITE-REMOTE-STORE
kind: feature
title: A browser page cannot mount a Liquers server's store read/write
status: draft
priority: P3
complexity: M
area: [web]
design:
created: 2026-10-20
github:
---

## Problem

`liquers-web`'s only HTTP store, `FetchStore` (`liquers-web/src/store/fetch.rs`), is read-only over
a fixed key list and knows nothing of Liquers metadata. A page that talks to a `liquers-axum`
server cannot list, write or remove its keys, or read its metadata records, except by writing the
whole protocol in JavaScript and registering it with `registerStoreObject`.

## Impact

The browser half of the client/server shape (`VISION.md`) needs hand-written glue. Workaround: a
page-implemented store object calling the Store API with `fetch`.

## Expected behaviour

A store type (`liquers_api`, as in `design/remote-store-and-asset-manager/` slice 1) in
`liquers-web` mapping `AsyncStore` onto the Store API with `fetch`, the same route table and error
mapping as the native client, run through the web STORE conformance suite against a mocked or
served endpoint.

## Discovery

Bulk design 2026-10-20 (cluster C6), slice 2 of `NO-REMOTE-STORE-OR-ASSET-MANAGER`.
