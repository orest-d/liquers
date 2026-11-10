---
id: INDEX-PAGE-IS-A-FIXED-REDIRECT
kind: feature
title: A Liquers server's index page can only redirect to the web folder
status: draft
priority: P3
complexity: L
area: [axum, core/value, core/commands]
design:
created: 2026-11-06
github:
---

## Problem

`axum-server-assembly` answers `GET /` and `GET /index.html` with a fixed redirect to
`{prefix}/web/` (Orest's decision of 2026-11-06). A server cannot choose another start page, generate
one, or let a command decide.

## Impact

A deployment whose start page is a dashboard query or a generated page needs a stored
`web/index.html` that redirects again, or a proxy.

## Expected behaviour

Options Orest listed for later, not yet chosen:

1. default recipes that generate responses, such as an `index.html` produced when none is stored;
2. a configurable redirect target in the `server` section;
3. a response value with which a command can answer with a redirect (a new value or metadata kind
   that the HTTP layer turns into a 3xx).

## Discovery

`axum-server-assembly` Phase 1 gate, 2026-11-06.
