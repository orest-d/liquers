---
id: NO-SOURCE-CODE-AND-API-DOC-NAVIGATION
kind: feature
title: Source code and API documentation cannot be navigated or extracted
status: draft
priority: P3
complexity: L
area: [lib/commands]
design: 
created: 2026-10-11
github:
---
## Problem

A coding agent working on a repository navigates source code by symbol: where a function is
defined, which items a module exports, what a type's methods are, and what its doc comment says.
Liquers can store and search source files as text (`ns-search/catalog-t-t`, `md_split-line` and a
regex, from `store-and-asset-search` revision 8). It has no notion of a symbol, a module, an item,
or API documentation.

## Problem Example

```
-R/liquers-core/src/assets.rs/-/ns-search/md_split-line
```

splits the file into line records; a regex filter over them then finds every line containing `fn get_asset_info` — definitions, calls and comments alike — with no
item kind, signature, doc comment or span. Wanted: one query over a folder of Rust files returning
records `kind, name, path, signature, doc, file, line_start, line_end`, so that
`ns-rec` filters select "the `pub fn` named `get_asset_info` and its doc comment".

Re-checked at commit 18732dd (2026-10-21): no Rust item records anywhere (`liquers-lib/Cargo.toml`
has no `syn`, tree-sitter or rustdoc dependency; no such command in `specs/command_registry.yaml`).
`syn` 2 with `full` is already in the workspace's lock file through `liquers-macro`, so a parser
adds no new crate to the build graph.

## Impact

"Where is `get_asset_info` defined?" works only as a regex over lines (`/fn get_asset_info/`),
which also finds calls, comments and other crates' items with the same name. Doc comments, item
kinds and the module tree cannot be queried. The workaround is regex over `md_split-line`.

## Expected behaviour

Records describing code items, at least for Rust: kind, name, path, signature, doc comment, file
and line span. Search and `ns-rec` would then compose over them, the same way `ns-search/commands`
does for the command registry. Options to weigh:
- parse the source (`syn` for Rust; tree-sitter for several languages, though it is heavier and
  awkward on wasm);
- read rustdoc's JSON output;
- consume an LSP (language server) index.

API documentation could equally come from the generated docs. Any of these is a record producer,
so the search design needs no change.

## Discovery

Raised by the user on 2026-10-11 while reviewing `store-and-asset-search` revision 8, as an
extension similar to the Markdown tools (see `specs/design/store-and-asset-search/agent-queries.md`
§3).
