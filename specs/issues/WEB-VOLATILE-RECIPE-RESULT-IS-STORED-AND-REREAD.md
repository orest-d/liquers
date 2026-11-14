---
id: WEB-VOLATILE-RECIPE-RESULT-IS-STORED-AND-REREAD
kind: issue
title: A volatile recipe in a page stores its result, and the next read returns the stored bytes
status: draft
priority: P2
complexity: M
area: [web, core/assets]
design:
created: 2026-11-10
github:
---

## Problem

With a writable store in the browser module, a recipe marked `volatile: true` is evaluated once;
its result is written to the store, and the next request for the key reads the stored file instead
of evaluating again. The value even changes type, because the second read deserializes the file:

```js
// data/recipes.yaml:  - {query: now/stamp.txt, volatile: true}     `now` is a volatile command
await liquers.evaluate("-R/data/stamp.txt");   // 2      (a number, computed)
await liquers.evaluate("-R/data/stamp.txt");   // "2"    (a string, read back from data/stamp.txt)
```

The plain query `now` is evaluated on every request, as it should be.

## Impact

A volatile recipe is not volatile in a page, and its value depends on whether it was read before.
The Web user guide avoids volatile recipes. Whether the native managers behave the same was not
checked; `liquers-web` uses the inline manager.

## Expected behaviour

A volatile recipe's result is not written to the store and is computed on every request.

## Discovery

A probe while writing chapter 7 of the Web user guide (2026-11-10), with a page-implemented store
that logged its `set` calls: `set data/stamp.txt` appeared after the first evaluation.
