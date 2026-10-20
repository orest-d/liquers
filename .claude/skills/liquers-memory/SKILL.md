---
name: liquers-memory
description: >-
  Use a Liquers agent memory (the `memory_*` MCP tools of `liquers-agent`, or its CLI) well:
  orient first, search before writing, read tiers before whole documents, and pass versions on
  writes. Use whenever the `memory_overview` / `memory_search` / `memory_new` tools are available,
  when asked to remember, file or look something up in this repository's specs through the
  memory, or when running `liquers-agent` against `memory/liquers-specs.yaml`.
---
# Using an agent memory

The memory is Markdown documents with front-matter, in collections. Every document has an L0 (a
title) and an L1 (a one-paragraph description). Reference:
[`specs/reference/AGENT_MEMORY_SERVICE.md`](../../../specs/reference/AGENT_MEMORY_SERVICE.md).

## Order of work

1. **Orient.** `memory_overview` once (the server's instructions already carry it), and
   `memory_describe` for the collection you will write to: its fields, allowed values and
   transitions, and whether it is writable.
2. **Find before you read.** `memory_search` (text, optionally `path` and `filter`) or
   `memory_list` (`path`, `depth: 0` for everything below). Results are tables of key, L0 and L1;
   the note under them says how many were shown of how many.
3. **Read narrowly.** `memory_read` with `tier: l0` or `l1` first, then `section` or `lines`, and
   the whole document only when you need it. Keep the `version` note: it is what a write needs.
4. **Search before you create.** `memory_new` returns similar memories instead of writing; update
   one of them with `memory_edit` or `memory_write` if it is the same thing. Set `force` only when
   it is genuinely new.
5. **Write with L0/L1 and a reason.** Every `memory_new` takes a `title` and a `description` a
   reader can use without opening the document. Pass `reason`; it goes into the write log.
6. **Pass `expected_version`** on `memory_write` and `memory_edit`. A refusal means someone else
   changed the document: read it again and redo the change.
7. **Prefer `memory_edit` to `memory_write`** for small changes: `old`/`new` replaces text (it must
   match once), `field`/`value` sets a front-matter field, `section`/`row` appends a table row.
   Give several pairs at once to make one write.
8. **Replace, do not delete.** A memory that is wrong is superseded: `memory_write` on the new one
   with `supersedes` set to the old key. Superseded memories drop out of listings and search.

## Beyond the tools

`memory_query` evaluates read-only Liquers queries over the memory — composing a listing with
filters and sorts, or getting CSV or JSON. Read `liquers://memory/query-guide` first; the
commands it may use are in `liquers://commands/mem`, `rec` and `search`. Writes are refused there.

## In this repository

```bash
cargo run -p liquers-agent -- --config memory/liquers-specs.yaml check specs/issues
cargo run -p liquers-agent -- --config memory/liquers-specs.yaml search "expiration" --in specs/issues
```

Run from the repository root. Filing an issue through `memory_new` (collection `issues`) still
needs `python3 scripts/docs_index.py` afterwards to regenerate the index
(`specs/DOCS_STRUCTURE_GUIDE.md` §4.8).
