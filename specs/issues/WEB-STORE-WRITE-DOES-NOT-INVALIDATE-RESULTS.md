---
id: WEB-STORE-WRITE-DOES-NOT-INVALIDATE-RESULTS
kind: issue
title: A page's store write leaves the results that read the key stale
status: draft
priority: P2
complexity: M
area: [web]
design: 
created: 2026-11-04
github:
---
## Problem

`Store.set` (`liquers-web/src/store/wrapper.rs`) writes straight to the `AsyncStore` the
environment holds. It does not go through the asset manager (`AssetManager::set_binary`), so a
result computed from that key earlier, and still held by the environment, is not invalidated: the
next evaluation of the same query returns the old result. A page has no other write: `liquers-web`
exports no asset-level `set`.

Found while writing `TEMPLATE03` (design `web-templates`): editing a template command's template
with `env.store().set(...)` leaves `hello/greeting` rendering the old template. The same edit
through `AssetManager::set_binary`, which is what `liquers-lib`'s own template tests use and what
`TEMPLATE03` now uses, invalidates it.

## Problem Example

```js
liquers.registerCommand({ name: "greeting", template: "-R/templates/greet.md" });
await liquers.evaluate("hello/greeting");                                    // "Hello, hello!"
await env.store().set("templates/greet.md", new TextEncoder().encode("Hi, {{.}}."), {});
await liquers.evaluate("hello/greeting");
// today:    "Hello, hello!" (the cached result)
// expected: "Hi, hello." (the write invalidates what depended on the key)
```

## Impact

Any page that edits stored data and re-evaluates sees stale results until the environment is
rebuilt (a command registration or `configureStore` after the first evaluation does that). It
applies to every result with a store dependency, not only templates.

## Expected behaviour

A page's write of a key invalidates the results that depend on it, as a Rust caller's
`AssetManager::set_binary` does: either `Store.set` routes through the asset manager of the
environment it came from, or the environment gains a `setData` / `setAsset` call and the store
wrapper says it is raw. Check what `liquers-py`'s store handle does (`liquers-py/src/store/handle.rs`
also writes the store directly) and make the two agree.

## Discovery

Implementing `TEMPLATE` in `liquers-web` (2026-11-04).
