---
title: Web API reference
status: ready
sources: [reference/WEB_API_SPECIFICATION.md]
reviewed: 2026-11-14
---
# E. Web API reference

PURPOSE: A reference for every route a Liquers server offers, the same in every guide.

This appendix lists what a Liquers server answers over HTTP and its WebSocket: the routes, what
they take and what they return. It is a reference; [chapter 12](12-serving.md) shows how to start the server from Rust, how to put the same routes into an application of your own, and the builders that turn single routes on and off.

The API is the same whatever language the server was started from, and whatever language calls
it. Any program that can send an HTTP request can use it.

{{> ../_shared/web-api.md}}

## E.10 The routes in Rust

Each API is a builder of the crate `liquers-axum` that returns an `axum::Router`:
`QueryApiBuilder`, `StoreApiBuilder`, `AssetsApiBuilder` and `RecipesApiBuilder`. The standard
server puts them together from the `server` settings; section 12.4 composes them by hand. The
builders have options the settings do not: `QueryApiBuilder::with_timeout`, the WebSocket limits
of `AssetsApiBuilder::with_websocket_limits`, and `with_destructive_gets`, which adds a `GET`
form of the routes that change data and need no body, for use from a plain link.

API reference: [`liquers_axum`](https://orest-d.github.io/liquers/api/rust/liquers_axum/index.html).
