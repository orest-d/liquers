---
id: AXUM-CANNOT-SERVE-STATIC-WEB-CONTENT
kind: feature
title: The Liquers HTTP server cannot serve a web application from the store or answer at /index.html
status: draft
priority: P2
complexity: L
area: [axum]
design: axum-server-assembly
created: 2026-11-06
github:
---

## Problem

`liquers-axum` serves the query, store, assets and recipes APIs only. There is no route that serves
a folder of the store as a website, with `Content-Type` from the metadata and `index.html` for a
folder, and nothing answers `/` or `/index.html`. LiQuer, the Python predecessor, did both: a `web`
mount point of its store was served at `/liquer/web/<path>`, a folder gave its `index.html`, and `/`
and `/index.html` redirected to the start page (`liquer/server/blueprint.py`, `liquer/store.py`
`get_web_store`).

**Example.** With a filesystem mount at `web` holding `index.html`, `GET /liquer/web/` and `GET /`
are 404. `GET /liquer/api/store/data/web/index.html` returns the bytes, but without a folder rule,
and it is not where a browser starts.

## Impact

A web application, dashboard or report site cannot be hosted by a Liquers server without a second
server in front of it. Workaround: a reverse proxy rewriting paths onto the store API.

## Expected behaviour

A `web` route over a predefined `web/` prefix of the store, folder → `index.html`, media type from
metadata, conditional requests, and an index page at `/` and `/index.html`.

## Discovery

Orest, 2026-11-06, on the M7 web-server design: "There are (at least) two needs: serving static
content (similarly as the web folder in https://github.com/orest-d/liquer which exposes a predefined
store) and serving /index.html."
