---
id: AXUM-SERVER-SENDS-NO-CORS-HEADERS
kind: feature
title: The standard server cannot be called from a page on another origin
status: draft
priority: P3
complexity: S
area: [axum]
design:
created: 2026-11-10
github:
---

## Problem

`liquers-axum` depends on `tower-http` with its `cors` feature, but no router adds a CORS layer and
the `server` configuration section has no setting for one. A page served from another origin, a
static site that uses the browser module and asks a Liquers server for the heavy work, is refused
by the browser.

## Impact

The only arrangements that work are serving the page from the server's own web folder or putting a
reverse proxy in front (`docs/guides/web/12-serving.md`, section 12.3). A Rust program can add the
layer to the router itself; a server started from Python or from a configuration file cannot.

## Expected behaviour

A `server` setting names the origins allowed to call the APIs (off by default, since the server has
no authentication), and `LiquersServerBuilder` has the matching method.

## Discovery

Writing chapter 12 of the Web user guide (2026-11-10): `grep -rn "CorsLayer" liquers-axum/` finds
nothing.
