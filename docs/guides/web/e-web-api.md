---
title: Web API reference
status: ready
sources: [reference/WEB_API_SPECIFICATION.md]
reviewed: 2026-11-14
---
# E. Web API reference

PURPOSE: A reference for every route a Liquers server offers, the same in every guide.

This appendix lists what a Liquers server answers over HTTP and its WebSocket: the routes, what
they take and what they return. It is a reference; [chapter 12](12-serving.md) shows how a page calls a server with `fetch` and follows a result over the WebSocket.

The API is the same whatever language the server was started from, and whatever language calls
it. Any program that can send an HTTP request can use it.

{{> ../_shared/web-api.md}}
