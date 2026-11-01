---
title: Introduction
status: outline
sources: [reference/PROJECT_OVERVIEW.md, guides/JAVASCRIPT_GUIDE.md]
reviewed:
---
# 1. Introduction

PURPOSE: The reader has run a first query from JavaScript in the browser and knows the words query, command, store and asset.

```js
import init, * as liquers from "./liquers_web.js";

await init();
await liquers.init();
const listing = await liquers.evaluate("commands_doc");   // Markdown: every command you can use
assert(listing.startsWith("# Commands"));
```

EDITOR: copied from `liquers-web/tests/guide/run-guide-examples.mjs` (the §3 example of specs/guides/JAVASCRIPT_GUIDE.md), which runs it; keep the two identical.

TBD: 1.1 What Liquers is: queries name computations; commands do the work; stores hold data; assets cache results.
SOURCE: reference/PROJECT_OVERVIEW.md
TBD: 1.2 Hello, world: the example below, in a page, with what the browser build includes.
SOURCE: guides/JAVASCRIPT_GUIDE.md §3
TBD: 1.3 Assets: each result is an asset: value, status and metadata.
SOURCE: guides/JAVASCRIPT_GUIDE.md §3
TBD: 1.4 Next steps: which chapter to read for which task.
