# Liquers for Python: review history

One row per review of a chapter against its `sources:`. A chapter's `reviewed:` date must
have a row here (`specs/guides/USER_DOCS_EDITING_GUIDE.md`, maintenance).

| Date | Chapter | Change |
|---|---|---|
| 2026-11-01 | (all) | skeleton created, every chapter `outline` |
| 2026-11-09 | index.md | preface written from `reference/VISION.md` and `reference/POSITIONING.md` |
| 2026-11-09 | 01-introduction.md | introduction written from `reference/VISION.md`: the toolkit, the query as its centre, one section per tool; examples run |
| 2026-11-09 | 02-installing.md | written from `guides/PYTHON_GUIDE.md` §1-2; `CHECK:` on the PyPI release |
| 2026-11-09 | 03-queries.md | written with the shared fragment `_shared/queries.md`; running, parsing, the query editor, pipelines; examples run |
| 2026-11-09 | 04-values.md | written from `guides/PYTHON_GUIDE.md` §6-7; examples run |
| 2026-11-09 | 05-commands.md | written from `guides/PYTHON_GUIDE.md` §5; examples run |
| 2026-11-09 | 06-stores.md | written from `guides/PYTHON_GUIDE.md` §4 and `reference/ENVIRONMENT_CONFIG.md`; the two ways to write checked against the package; examples run |
| 2026-11-09 | 07-assets.md | written with the shared fragment `_shared/assets.md`; caching, volatility, `AssetRef`, recipes, recipe providers; examples run |
| 2026-11-09 | 08-environment.md | written from `reference/ENVIRONMENT_CONFIG.md` and `guides/PYTHON_GUIDE.md` §4, §6; examples run |
| 2026-11-09 | 09-data.md | written from `guides/PYTHON_GUIDE.md` §7 and `command_registry.yaml`; examples run |
| 2026-11-09 | 10-templates.md | written with the shared fragment `_shared/templates.md`; examples run |
| 2026-11-09 | 11-ui.md | written: no UI binding in Python, where to look instead |
| 2026-11-09 | 12-serving.md | written from `guides/WEB_API_GUIDE.md` §1-3 and `guides/PYTHON_GUIDE.md` §6; the HTTP example is not run |
| 2026-11-09 | 13-records.md | written from `guides/SEARCH_GUIDE.md` and `reference/AGENT_MEMORY_SERVICE.md`; `TBD:` a worked memory example; examples run |
| 2026-11-09 | 14-testing.md | written from `guides/PYTHON_GUIDE.md` §9; examples run under pytest |
| 2026-11-09 | 15-troubleshooting.md | written with the shared fragment `_shared/errors.md`; exceptions, frequent mistakes, limitations from `reference/PYTHON_INTEGRATION.md` §8; examples run |
| 2026-11-09 | a-query-syntax.md | written with the new shared fragment `_shared/query-syntax.md`, checked against `reference/RECIPES_AND_PLANS.md` §Planner instructions; `TBD:` realms |
| 2026-11-09 | b-command-library.md | introduction added; the table stays generated |
| 2026-11-09 | c-glossary.md | the Python names of the terms and the terms this guide adds |
| 2026-11-09 | d-sister-guides.md | introduction to the chapter map |
| 2026-11-10 | 01-introduction.md | records named in 1.4 and given a row of their own in 1.9 |
| 2026-11-10 | 02-installing.md | 2.3 Installing from source: prerequisites per system, checking, optional packages, updating, building a wheel |
| 2026-11-10 | all chapters | promoted from `draft` to `ready` so that release builds publish the guide; the open `CHECK:` (chapter 2) became reader text, the two `TBD:` notes (chapter 13, appendix A) became `EDITOR:` notes |
| 2026-11-10 | 08-environment.md | merged with the M7 server work: `config=` takes a `pathlib.Path`; keywords override the configuration; the `command_registry`, `python` and `server` sections; unknown sections are errors; command metadata from a file; examples run |
| 2026-11-10 | 12-serving.md | merged with the M7 server work: new 12.3 Serving an environment (`env.serve`, foreground, asyncio, settings with the shared `_shared/serving.md`, stopping); 12.4-12.6 renumbered; a Rust server cannot run Python commands, a served environment can; examples run |
| 2026-11-14 | 01-introduction.md | chapter 13 is now *Records*: the table of chapters and the paragraph on search follow |
| 2026-11-14 | 03-queries.md | 3.9 is *Building and editing a query*: the query editor first, with typed values and the editing of an existing query; the older way after it. 3.10 rewritten around the shared fragments `_shared/pipelines.md` and `_shared/pipeline-or-query.md`: creating, reading, editing a pipeline, and when to prefer a query. Links to the API reference. Examples run |
| 2026-11-14 | 07-assets.md | a recipe with a pipeline: the shared section in `_shared/assets.md` and a tested example |
| 2026-11-14 | 09-data.md | points to chapter 13 for schemas and manifests |
| 2026-11-14 | 12-serving.md | points to the new appendix E for the routes |
| 2026-11-14 | 13-records.md | renamed from `13-search.md` and rewritten as *Records*: the shared fragment `_shared/records.md` (views, sources, streams, schemas, manifests and recipes, records and Polars), records in this language, then search and the agent memory as applications of records. Examples run |
| 2026-11-14 | c-glossary.md | adds chunk, manifest, query editor, record, record source, record stream, schema |
| 2026-11-14 | d-sister-guides.md | chapter 13 is *Records*; adds appendix E |
| 2026-11-14 | e-web-api.md | new appendix: the web API reference, from the shared fragment `_shared/web-api.md` (written from `reference/WEB_API_SPECIFICATION.md`, example answers taken from a running server); `ready` |
