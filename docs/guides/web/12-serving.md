---
title: Serving and embedding
status: draft
sources: [guides/WEB_API_GUIDE.md, reference/WEB_API_SPECIFICATION.md]
reviewed:
---
# 12. Serving and embedding

PURPOSE: The reader can make Liquers available to other programs: over HTTP, or embedded in an application.

A page can talk to a Liquers server over HTTP. The server itself is started by a Rust or Python
program ([12. Serving and embedding](../rust/12-serving.md) in the Rust guide,
[12. Serving and embedding](../python/12-serving.md) in the Python guide). This chapter covers what
a page sees of it.

## 12.1 Pages served by the server

The standard server serves files from its store's `web/` folder at `/liquer/web/`, and its start
page, `/`, sends the browser there. A page served this way calls the APIs with relative
addresses, on the same origin, without any cross-origin setup. With a server whose program
registered a `greet` command:

```js
const response = await fetch("/liquer/q/greet-Ann");
console.log(await response.text());       // Hello, Ann!
```

## 12.2 The server's settings

{{> ../_shared/serving.md}}

## 12.3 Calling a server from a page

TBD: 12.3 Calling a server from a page: the HTTP API: evaluate, submit and poll, the store API.
SOURCE: guides/WEB_API_GUIDE.md

## 12.4 Live updates

TBD: 12.4 Live updates: the WebSocket from a page.
SOURCE: guides/WEB_API_GUIDE.md §8
