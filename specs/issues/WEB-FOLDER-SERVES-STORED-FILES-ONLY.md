---
id: WEB-FOLDER-SERVES-STORED-FILES-ONLY
kind: feature
title: The web folder serves stored files only; a page that a recipe would compute is 404
status: draft
priority: P3
complexity: M
area: [axum]
design:
created: 2026-11-06
github:
---

## Problem

The web route of `axum-server-assembly` reads `web/<path>` from the store only (Orest's decision of
2026-11-06). A key under `web/` that is not stored but has a recipe, such as a report page rendered
from a template, is answered 404, although the assets API would compute it.

## Impact

A computed page cannot be served at a stable web URL with the web folder's rules (folder
`index.html`, media type from the extension). Workaround: link to `{prefix}/api/assets/key/data/…`
or `{prefix}/q/…`, or store the rendered page.

## Expected behaviour

A setting of the web route (Orest: "later it can be configurable") that computes a missing key with
a recipe through the asset manager and caches it, while stored files keep being read from the store
so edits show at once. The design reviewed for this (hybrid read, `contains || can_make` before
`get`, so probing URLs creates no assets) was the recommendation of `axum-server-assembly` Phase 1
question 2 before the decision (commit 8198939).

## Discovery

`axum-server-assembly` Phase 1 gate, 2026-11-06.
