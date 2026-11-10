---
title: Query syntax
status: ready
sources: [reference/PROJECT_OVERVIEW.md, reference/RECIPES_AND_PLANS.md, guides/QUERY_ESCAPING_GUIDE.md, reference/PIPELINES.md]
reviewed: 2026-11-09
---
# A. Query syntax

PURPOSE: A reference for the query notation, the same in every guide.

This appendix collects the notation of queries in one place. Chapter 3 introduces it step by
step.

{{> ../_shared/query-syntax.md}}

## A.6 Pipelines in Python

A pipeline is a query written as data (chapter 3). In Python it is a list of steps. Each step is
one of:

| Step | Meaning |
|---|---|
| a `str` | query text, one or more actions or a resource segment |
| `{"action": name, "arguments": {name: value}}` | an action with arguments by name; a value can be any JSON-like data |
| `{"action": name, "parameters": [value, …]}` | an action with arguments by position |
| `{"placeholder": name}` in place of a value | an argument of a pipeline command (chapter 5) |

`lq.Pipeline(steps)` wraps such a list, `lq.Pipeline.from_query(text)` makes one from a query,
and `pipeline.to_query(env)` turns it back when every argument can be written as text.

EDITOR: still to be written: realms. the name in a transform segment's header (`-<realm>/…`) selects the realm in which the commands are looked up. Describe once the Python decorators can set a realm.
