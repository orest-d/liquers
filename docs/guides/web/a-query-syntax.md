---
title: Query syntax
status: ready
sources: [reference/PROJECT_OVERVIEW.md, reference/RECIPES_AND_PLANS.md, guides/QUERY_ESCAPING_GUIDE.md, reference/PIPELINES.md]
reviewed: 2026-11-10
---
# A. Query syntax

PURPOSE: A reference for the query notation, the same in every guide.

This appendix collects the notation of queries in one place. Chapter 3 introduces it step by
step.

{{> ../_shared/query-syntax.md}}

## A.6 The syntax in JavaScript

Each part of the notation has a counterpart in the module:

| Notation | In JavaScript | Made by |
|---|---|---|
| a whole query | `Query` | `liquers.Query.parse(text)` |
| a key | `Key` | `liquers.Key.parse(text)` |
| one argument, escaped | a string | `liquers.encodeParam(value)` |
| a query built step by step | `QueryEditor` | `new liquers.QueryEditor(text)` |
| a query as data | a pipeline: a plain array or `{ elements, filename }` | `liquers.queryToPipeline(text)` |

`encode()` on a `Query` or a `Key` gives the text back, with every escape applied.

A pipeline is a query written as data (section 3.10). Each of its elements is one of:

| Element | Meaning |
|---|---|
| a string | query text, one or more actions or a resource segment |
| `{ resource: key }` | reading a key from the store |
| `{ action: name, ns: namespace, arguments: { name: value } }` | an action with arguments by name; a value can be any JSON data |
| `{ action: name, parameters: [value, …] }` | an action with arguments by position |
| `{ placeholder: name }` in place of a value | an argument of a pipeline command (section 5.9) |

A pipeline made by `queryToPipeline` also carries `pos`, the position of each element and
argument in the text, and writes each argument as `{ value, pos }`.
