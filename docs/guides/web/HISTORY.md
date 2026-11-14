# Liquers for the Web: review history

One row per review of a chapter against its `sources:`. A chapter's `reviewed:` date must
have a row here (`specs/guides/USER_DOCS_EDITING_GUIDE.md`, maintenance).

| Date | Chapter | Change |
|---|---|---|
| 2026-11-01 | (all) | skeleton created, every chapter `outline` |
| 2026-11-09 | a-query-syntax.md | includes the new shared fragment `_shared/query-syntax.md`; still `outline` |
| 2026-11-10 | index.md | preface written: who the guide is for, the browser as a platform, conventions of the examples; `ready` |
| 2026-11-10 | 01-introduction.md | written: a first query, the tools one by one, what is different in the browser; `ready` |
| 2026-11-10 | 02-installing.md | written: the files, loading, hosting on the project site, a CDN and your own site, server requirements, building; `ready` |
| 2026-11-10 | 03-queries.md | written: running, inspecting and building queries, pipelines, queries in the address of a page; `ready` |
| 2026-11-10 | 04-values.md | written: value conversion, opaque objects, state and metadata, values as files, handles; `ready` |
| 2026-11-10 | 05-commands.md | written: declarations, input, arguments, context, working directory, options, asynchronous commands, errors, pipeline and template commands, batches; `ready` |
| 2026-11-10 | 06-stores.md | written: configuration, the four kinds of storage in a browser, the store handle, a store in JavaScript; `ready` |
| 2026-11-10 | 07-assets.md | written: the asset in a page, what is kept, results are not refreshed yet, recipes, cancelling; `ready` |
| 2026-11-10 | 08-environment.md | written: one environment per page, setup order, late registration, shutdown, comparison with native; `ready` |
| 2026-11-10 | 09-data.md | written: record tables, the record commands, `RecordBatch` and `RecordSource`, what is not in the browser, saving; `ready` |
| 2026-11-10 | 10-templates.md | written: `expand`, `render`, `validateTemplate`, template commands, rendering in a command, HTML escaping; `ready` |
| 2026-11-10 | 11-ui.md | written: the model, `mountUI`, UI specs, elements in JavaScript, `mountRenderer`, limits; `ready` |
| 2026-11-10 | 12-serving.md | completed: calling a server from a page, live updates, the module and a server together, embedding; `ready` |
| 2026-11-10 | 13-records.md | written: `ns-rec/filter`; search and agent memory are native only; `ready` |
| 2026-11-10 | 14-testing.md | written: tests under Node, a fresh environment per test, checks without running, browser tests; `ready` |
| 2026-11-10 | 15-troubleshooting.md | written: `LiquersError`, frequent mistakes, known limitations, loading problems; `ready` |
| 2026-11-10 | a-query-syntax.md | A.6 added: the syntax in JavaScript; `ready` |
| 2026-11-10 | b-command-library.md | written: which namespaces the browser module has; `ready` |
| 2026-11-10 | c-glossary.md | C.2 and C.3 added: the terms in JavaScript, terms this guide adds; `ready` |
| 2026-11-10 | d-sister-guides.md | introduction written; `ready` |
| 2026-11-10 | (all) | every chapter written and set to `ready`; the examples import the module from the project site and are run under Node by `liquers-web/tests/guide/run-guide-examples.mjs` |
| 2026-11-14 | 01-introduction.md | chapter 13 is now *Records*: the table of chapters and the paragraph on search follow |
| 2026-11-14 | 03-queries.md | 3.9 is *Building and editing a query*: the query editor first, with typed values and the editing of an existing query; the older way after it. 3.10 rewritten around the shared fragments `_shared/pipelines.md` and `_shared/pipeline-or-query.md`: creating, reading, editing a pipeline, and when to prefer a query. Links to the API reference. Examples run |
| 2026-11-14 | 07-assets.md | a recipe with a pipeline: the shared section in `_shared/assets.md` and a tested example |
| 2026-11-14 | 09-data.md | points to chapter 13 for schemas and manifests |
| 2026-11-14 | 11-ui.md | adds the reference section for UI specs and the `ns-lui` commands, from the shared fragment `_shared/ui-reference.md` (the full spec was run in the browser module) |
| 2026-11-14 | 12-serving.md | points to the new appendix E for the routes |
| 2026-11-14 | 13-records.md | renamed from `13-search.md` and rewritten as *Records*: the shared fragment `_shared/records.md` (views, sources, streams, schemas, manifests and recipes, records and Polars), records in this language, then search and the agent memory as applications of records. Examples run |
| 2026-11-14 | 15-troubleshooting.md | section numbers of chapter 13 follow its rewrite |
| 2026-11-14 | c-glossary.md | adds chunk, manifest, query editor, record, record source, record stream, schema |
| 2026-11-14 | d-sister-guides.md | chapter 13 is *Records*; adds appendix E |
| 2026-11-14 | e-web-api.md | new appendix: the web API reference, from the shared fragment `_shared/web-api.md` (written from `reference/WEB_API_SPECIFICATION.md`, example answers taken from a running server); `ready` |
