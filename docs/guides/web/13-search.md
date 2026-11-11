---
title: Search and agent memory
status: ready
sources: [guides/SEARCH_GUIDE.md]
reviewed: 2026-11-10
---
# 13. Search and agent memory

PURPOSE: The reader can search stored documents and records, and knows what the agent memory offers.

Native Liquers has a search library, `ns-search`, which lists, reads and searches the documents
and tables of a store, and an *agent memory*: a program that gives AI agents a store of
documents and Liquers commands as tools. Neither is in the browser module. This chapter says
what a page has instead, and where the full versions are described.

## 13.1 What the browser module has

One command of the record library searches the rows of a table: `ns-rec/filter` keeps the rows
that match a *search expression*. A bare word matches in any column, and `column:word` in one:

<!-- example: needs records -->
```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({
  name: "people",
  run: () => [
    { name: "Ann", age: 31, city: "Oslo" },
    { name: "Bob", age: 25, city: "Rome" },
    { name: "Oslo Olsen", age: 40, city: "Bergen" },
  ],
});

const anywhere = await liquers.evaluate(
  `people/ns-rec/to_record/filter-${liquers.encodeParam("Oslo")}/select_columns-name/to_json`);
assert.deepStrictEqual(anywhere, [{ name: "Ann" }, { name: "Oslo Olsen" }]);

const inCity = await liquers.evaluate(
  `people/ns-rec/to_record/filter-${liquers.encodeParam("city:Oslo")}/select_columns-name/to_json`);
assert.deepStrictEqual(inCity, [{ name: "Ann" }]);
```

A search expression contains characters such as `:` and spaces, so build the query with
`encodeParam`, as here, whenever the expression comes from a search box. Together with the
comparison commands of section 9.2 (`eq`, `contains`, `starts_with`, …), this covers searching
a table that is already in the page.

## 13.2 What it does not have

| Native Liquers | In the browser |
|---|---|
| `ns-search`: list and search the files of a folder, by text and by fields, with ranking | not available; the queries fail with `action_not_registered` |
| search over Markdown documents, their sections and front matter | not available |
| the agent memory (`liquers-agent`): documents with versions, served to AI agents as tools over the Model Context Protocol | a native program; a page cannot run it |
| commands served as tools to an agent (`liquers-mcp`) | a native library |

The reasons are the usual ones for a browser: the search library reads folders of a file system
or of cloud storage and builds indexes over them, and a tool server must listen for connections.

When a page needs search over documents, run it on a server and ask from the page. A Liquers
server with the search library answers a search as it answers any query (chapter 12), so the
page fetches a query whose result is a table of matches and shows it. The search queries
themselves are described in [13. Search and agent memory](../rust/13-search.md) of the Rust
guide, and in [the same chapter](../python/13-search.md) of the Python guide.
