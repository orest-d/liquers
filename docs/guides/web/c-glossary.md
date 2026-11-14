---
title: Glossary
status: ready
sources: [reference/PROJECT_OVERVIEW.md, reference/PIPELINES.md]
reviewed: 2026-11-10
---
# C. Glossary

PURPOSE: The words of Liquers, defined once.

## C.1 Terms

{{$ -R/specs/reference/PROJECT_OVERVIEW.md/-/ns-docs/include-Glossary}}

## C.2 The terms in JavaScript

| Term | In JavaScript |
|---|---|
| Query | a string; parsed, a `liquers.Query` |
| Key | a string; parsed, a `liquers.Key` |
| Pipeline | a plain array of steps, or `{ elements, filename }` |
| Command | a function registered with `liquers.registerCommand`, with its declaration |
| Command metadata | what `liquers.describeCommand(name)` returns, and what `commands_doc` prints |
| Value | a JavaScript value, converted at the border; inside the module, a `liquers.Value` |
| State | `liquers.State`, from `asset.get()`; in a command, `{ value, metadata, status, log }` |
| Metadata | a plain object, from `state.metadata()` |
| Asset | `liquers.Asset`, from `liquers.getAsset(query)` |
| Status | a string: `"ready"`, `"error"`, `"processing"` |
| Recipe | an entry of a `recipes.yaml` file in a store |
| Store | `liquers.Store`, from `env.store()`; configured with `env.configureStore` |
| Environment | the one of the page, `liquers.Environment.global()` |
| Context | the `context` parameter of a command |
| Record stream | `liquers.RecordBatch` (a table) and `liquers.RecordSource` (a table in chunks) |
| Error | `liquers.LiquersError` with an `errorType` |
| Volatile | `volatile: true` in a declaration, or `volatile` in a recipe |

## C.3 Terms this guide adds

| Term | Definition |
|---|---|
| **Argument** | A value given to a command in a query, after the command's name |
| **Browser module** | The ES module `liquers_web.js` with its WebAssembly file: Liquers for a page |
| **Declaration** | The object that describes a command to `registerCommand`: its name, input, arguments and function |
| **Dependency** | Something a result was made from: a key, another query, a command |
| **Handle** | A JavaScript object that refers to data inside the module instead of copying it; released with `free()` |
| **Inline** | Evaluated as part of the call that asked, on the page's one thread |
| **Link** | An argument that is the value of another query |
| **Mount** | One entry of the store configuration: a type of storage and the prefix of the keys it holds. Also the object `mountUI` returns for a user interface placed in the page |
| **Opaque object** | A JavaScript object passed through Liquers by reference, wrapped with `liquers.opaque` |
| **Pipeline command** | A command defined as a pipeline, with placeholders as its arguments |
| **Placeholder** | A named gap in a pipeline, filled by an argument |
| **Props** | The data of a user-interface element defined in JavaScript |
| **Template** | Text with tags that are replaced when it is rendered; a query tag holds a query |
| **Template command** | A command whose result is a rendered template |
| **UI spec** | The description of a layout with a menu, as data |
| **WebAssembly** | The binary format browsers run next to JavaScript; the form in which the Rust code of Liquers reaches the page |
