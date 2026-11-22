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
| 2026-11-11 | 11-ui.md | rewritten for the UI model and the window: `env.ui()`, status callbacks and who runs them, elements and actions (`context.element`), `ui.run()` on the main thread, Ctrl-C, a server beside the window; examples run |
| 2026-11-11 | 02-installing.md | 2.1: every package opens the window (about 8 MB); 2.3: a build without the window, a virtual display on Linux |
| 2026-11-11 | 01-introduction.md, d-sister-guides.md, 15-troubleshooting.md | user interfaces are available from Python; the window needs a display |
| 2026-11-14 | 11-ui.md | reviewed against `reference/PYTHON_INTEGRATION.md` §18: `wait_idle_async` inside a command, `async def` ones too, raises |
| 2026-11-14 | 01-introduction.md | chapter 13 is now *Records*: the table of chapters and the paragraph on search follow |
| 2026-11-14 | 03-queries.md | 3.9 is *Building and editing a query*: the query editor first, with typed values and the editing of an existing query; the older way after it. 3.10 rewritten around the shared fragments `_shared/pipelines.md` and `_shared/pipeline-or-query.md`: creating, reading, editing a pipeline, and when to prefer a query. Links to the API reference. Examples run |
| 2026-11-14 | 07-assets.md | a recipe with a pipeline: the shared section in `_shared/assets.md` and a tested example |
| 2026-11-14 | 09-data.md | points to chapter 13 for schemas and manifests |
| 2026-11-14 | 12-serving.md | points to the new appendix E for the routes |
| 2026-11-14 | 13-records.md | renamed from `13-search.md` and rewritten as *Records*: the shared fragment `_shared/records.md` (views, sources, streams, schemas, manifests and recipes, records and Polars), records in this language, then search and the agent memory as applications of records. Examples run |
| 2026-11-14 | c-glossary.md | adds chunk, manifest, query editor, record, record source, record stream, schema |
| 2026-11-14 | d-sister-guides.md | chapter 13 is *Records*; adds appendix E |
| 2026-11-14 | e-web-api.md | new appendix: the web API reference, from the shared fragment `_shared/web-api.md` (written from `reference/WEB_API_SPECIFICATION.md`, example answers taken from a running server); `ready` |
| 2026-11-14 | 11-ui.md | adds 11.6, the reference for UI specs and the `ns-lui` commands, from the shared fragment `_shared/ui-reference.md` |
| 2026-11-14 | 11-ui.md | restructured to go from a first application to the details: 11.1 is a complete window (or page) built from a UI spec, followed by what the user interface is and is not, from the shared fragment `_shared/ui-idea.md` |
| 2026-11-15 | 02-installing.md | §2.4: the submodules (`liquers.query`, …, `liquers.lui`, `liquers.data_nodes`) and the private `_liquers`, from `guides/PYTHON_GUIDE.md` §2; example runs |
| 2026-11-15 | 03-queries.md | 3.8 and 3.10: the examples import `parse`, `parse_key`, `Key`, `ActionRequest` from `liquers.query` and `Pipeline` from `liquers.pipeline`; the API links point to the submodule pages. Examples run |
| 2026-11-15 | 04-values.md | 4.5: the examples import `data_node` and `NodeKind` from `liquers.data_nodes`. Examples run |
| 2026-11-15 | 06-stores.md | the `Store.from_object` example imports `Store` from `liquers.store`. Examples run |
| 2026-11-15 | 07-assets.md | 7.9: `Recipe` from `liquers.recipes`, `Expires` named with `liquers.metadata`; the API link points to the submodule page. Examples run |
| 2026-11-15 | 09-data.md | 9.2-9.5: the examples import `RecordView` and `FieldType` from `liquers.records`. Examples run |
| 2026-11-15 | 13-records.md | the examples import the record classes from `liquers.records`, and 13.6 says so; the API links point to the submodule page. Examples run |
| 2026-11-15 | 15-troubleshooting.md | 15.3: the error classes live in `liquers.error`; the example imports from it. Examples run |
| 2026-11-15 | c-glossary.md | C.2 gains a column naming each term's module |
| 2026-11-15 | 05-commands.md | reviewed against the package layout of `design/py-package-submodules/`: its examples use only entry points of `liquers as lq` (`Environment`, `evaluate`, the decorators, the error classes), unchanged; no change |
| 2026-11-15 | 08-environment.md | reviewed against the package layout of `design/py-package-submodules/`: its examples use only entry points of `liquers as lq` (`Environment`, `evaluate`, the decorators, the error classes), unchanged; no change |
| 2026-11-15 | 10-templates.md | reviewed against the package layout of `design/py-package-submodules/`: its examples use only entry points of `liquers as lq` (`Environment`, `evaluate`, the decorators, the error classes), unchanged; no change |
| 2026-11-15 | 11-ui.md | reviewed against the package layout of `design/py-package-submodules/`: its examples use only entry points of `liquers as lq` (`Environment`, `evaluate`, the decorators, the error classes), unchanged; no change |
| 2026-11-15 | 14-testing.md | reviewed against the package layout of `design/py-package-submodules/`: its examples use only entry points of `liquers as lq` (`Environment`, `evaluate`, the decorators, the error classes), unchanged; no change |
| 2026-11-16 | 04-values.md | reviewed against `design/settable-value-accessor/`: new 4.6 on editing a value through `lq.Accessor` (tokens, conflicts, projections, events); example runs |
| 2026-11-16 | 12-serving.md | reviewed against `design/settable-value-accessor/`: the shared settings table names the opt-in `accessors` API |
| 2026-11-16 | e-web-api.md | reviewed against `design/settable-value-accessor/`: new E.10, the Accessor API (routes, `ETag` preconditions, the WebSocket), from `_shared/web-api.md` |
| 2026-11-21 | 11-ui.md | reviewed against `design/query-editor-ergonomics/`: new section on editing queries (Simple and Advanced modes, cards, keys, icons, themes and the `ui` section), from `_shared/ui-editors.md`; the reference section is renumbered |
| 2026-11-21 | 05-commands.md | reviewed against `design/query-editor-ergonomics/`: still true; a Python command cannot set an icon, presets or next yet, recorded as an issue |
| 2026-11-22 | 11-ui.md | reviewed: a bare icon name means the theme's icon set, Phosphor unless the theme changes it; `ph:` is always Phosphor |
| 2026-11-15 | 02-installing.md | the default environment can be configured (`lq.configure`, `lq.configure_from_command_line`) |
| 2026-11-15 | 08-environment.md | §8.1 configuring the default environment with `lq.configure`; §8.2 `env.configure`; §8.3–8.4 when files and settings are read; new §8.7 "Starting an application" (`add_arguments`, `configure_from_args`, `configure_from_command_line`, `overrides`); examples run |
| 2026-11-15 | 11-ui.md | every UI example starts from a `liquers.yaml` configuration; the window with a server from its `server` section; examples run |
| 2026-11-15 | 12-serving.md | the service, server and settings examples load a YAML configuration; a server configured from its command line; examples run |
| 2026-11-15 | 01-introduction.md | the default environment can be pointed at a configuration file with `lq.configure` |
| 2026-11-15 | 03-queries.md | reviewed for `py-default-environment-config` / `py-command-line-config`: no claim about configuring the default environment or reading a configuration file changed; no edit |
| 2026-11-15 | 04-values.md | reviewed for `py-default-environment-config` / `py-command-line-config`: no claim about configuring the default environment or reading a configuration file changed; no edit |
| 2026-11-15 | 05-commands.md | reviewed for `py-default-environment-config` / `py-command-line-config`: no claim about configuring the default environment or reading a configuration file changed; no edit |
| 2026-11-15 | 06-stores.md | reviewed for `py-default-environment-config` / `py-command-line-config`: no claim about configuring the default environment or reading a configuration file changed; no edit |
| 2026-11-15 | 07-assets.md | reviewed for `py-default-environment-config` / `py-command-line-config`: no claim about configuring the default environment or reading a configuration file changed; no edit |
| 2026-11-15 | 09-data.md | reviewed for `py-default-environment-config` / `py-command-line-config`: no claim about configuring the default environment or reading a configuration file changed; no edit |
| 2026-11-15 | 10-templates.md | reviewed for `py-default-environment-config` / `py-command-line-config`: no claim about configuring the default environment or reading a configuration file changed; no edit |
| 2026-11-15 | 14-testing.md | reviewed for `py-default-environment-config` / `py-command-line-config`: no claim about configuring the default environment or reading a configuration file changed; no edit |
| 2026-11-15 | 15-troubleshooting.md | reviewed for `py-default-environment-config` / `py-command-line-config`: no claim about configuring the default environment or reading a configuration file changed; no edit |
| 2026-11-15 | c-glossary.md | reviewed for `py-default-environment-config` / `py-command-line-config`: no claim about configuring the default environment or reading a configuration file changed; no edit |
