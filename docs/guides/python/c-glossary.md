---
title: Glossary
status: ready
sources: [reference/PROJECT_OVERVIEW.md, reference/PIPELINES.md, guides/PYTHON_GUIDE.md]
reviewed: 2026-11-09
---
# C. Glossary

PURPOSE: The words of Liquers, defined once.

## C.1 Terms

{{$ -R/specs/reference/PROJECT_OVERVIEW.md/-/ns-docs/include-Glossary}}

## C.2 The terms in Python

| Term | In Python |
|---|---|
| Query | text, or `lq.Query` (from `lq.parse`) |
| Key | text, or `lq.Key` (from `lq.parse_key`) |
| Action | `lq.ActionRequest` |
| Pipeline | a list of steps, or `lq.Pipeline` |
| Command | a function decorated with `@lq.command` or `@lq.first_command`; described by `lq.CommandMetadata` |
| First command | a function decorated with `@lq.first_command` |
| State | `lq.State`, with `.value` and `.metadata` |
| Metadata | `lq.Metadata` |
| Asset | `lq.AssetRef`, from `env.get_asset(query)` |
| Status | `lq.Status` |
| Recipe | `lq.Recipe`; the recipes of an environment are `env.recipes` |
| Store | `lq.Store`; the store of an environment is `env.store` |
| Environment | `lq.Environment`; the shared one is `lq.default_environment()` |
| Context | `lq.Context`, received by a command parameter named `context` |
| Record stream | `lq.RecordView` (a table) and `lq.RecordSource` (a table in chunks) |
| Volatile | `volatile=True` on a decorator or an `lq.Recipe` |

## C.3 Terms this guide adds

| Term | Definition |
|---|---|
| **Argument** | A value given to a command in a query, after the command's name |
| **Data node** | A read-only view that reads a value field by field (`lq.DataNode`) |
| **Dependency** | Something a result was made from: a key, another query, a command |
| **Extra** | A group of optional Python packages installed with `liquers-py[name]` |
| **Link** | An argument that is the value of another query |
| **Mount** | One entry of the store configuration: a type of storage and the prefix of the keys it holds |
| **Opaque object** | A Python object Liquers has no type for; it is passed along unchanged |
| **Record view** | A table with named, typed columns |
| **Template** | Text with tags that are replaced when it is rendered; a query tag holds a query |
| **Template command** | A command whose result is a rendered template |
