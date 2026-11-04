---
id: WEB-TEMPLATES
kind: design
title: Templates from JavaScript - render, expand, validate and template commands
form: compact
workflow: liquers-project
status: in_review
phase: implementation
readiness: ready
autofix: not-eligible
area: [web]
issues: [WEB-HAS-NO-TEMPLATE-COMMANDS]
affects_docs: [specs/reference/JAVASCRIPT_INTEGRATION.md, specs/guides/JAVASCRIPT_GUIDE.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md]
created: 2026-11-04
---
# Templates from JavaScript - render, expand, validate and template commands

`TEMPLATE` of `LANGUAGE-INTEGRATION_GUIDE.md` for `liquers-web`, the next item of the web
conformance plan after `PIPELINE` (design `web-pipelines`). Written and implemented together on
2026-11-04 at the owner's request ("design and implement the next thing in the plan"), so Phases
1-4 record what was built and why, and Phase 5 what it changed. Not yet accepted by the owner.

## Phase 1: High-Level Design

### Purpose

The guide's `TEMPLATE` asks for three things: the `ns-template` commands in the built-in set,
template commands declared from the language, and `render` / `expand` / `validate` called
directly, at the top level and inside a command. `liquers-web` had none of them
(`WEB-HAS-NO-TEMPLATE-COMMANDS`): it linked `liquers-lib` without `templates`.

### Problem Example

```js
// Before: no `template` namespace, no render call, no template command.
await liquers.evaluate("hello/ns-template/render-~X~-R/templates/greet.md~E");   // rejects
await liquers.expand("{{name}} has {{$ count}}", { name: "Ann" });                // no such export
liquers.registerCommand({ name: "greeting", template: "-R/templates/greet.md" }); // refused: no run
```

### Scope and Acceptance Criteria

- **AC-1** Built in
  WHEN a page evaluates an `ns-template` query THEN it renders as natively.
- **AC-2** Programmatic render
  WHEN a page calls `render(t, state, data)` or `expand(t, data)` THEN the text equals the
  equivalent command query's, the template's queries run in the environment, and a `Query` names a
  stored template.
- **AC-3** Options
  WHEN `errors` is strict THEN a template error rejects with its `line:column`; WHEN it recovers and
  `full` is set THEN the result carries the warnings; `html` alone turns escaping on; `cwd` (a key
  string or a `Key`) resolves relative partials.
- **AC-4** In a command
  WHEN a command renders through `context.render` THEN the template's queries are dependencies of
  its result, recovered errors go to its log, and a nested render completes.
- **AC-5** Template command
  WHEN a declaration has `template` (a link) in place of `run` THEN it registers as an alias of
  `template/render`, renders, shows the alias in `describeCommand`, and is invalidated when its
  template is rewritten through the asset manager; `run`, `state`, `async`, `pipeline` or
  `arguments` beside it refuse the batch.
- **AC-6** Validate
  WHEN a page validates a template THEN it gets positioned diagnostics and nothing runs.

Out of scope: a JavaScript `DataNode` adapter (`DATANODE`; data is converted first), invalidation
on a page's raw store write (`WEB-STORE-WRITE-DOES-NOT-INVALIDATE-RESULTS`), `liquers-py`.

### Documentation Intent

`JAVASCRIPT_INTEGRATION.md` (§1, §3, §4 template commands and "Templates", §5, limitations),
`JAVASCRIPT_GUIDE.md` §6 "Templates" (runnable examples), the integration guide's §TEMPLATE and
§6 Browser JavaScript, `liquers-web/README.md` conformance row, the web user guide's outline line.
- API docs: `liquers-web/src/template.rs` (new module doc); `liquers-web/src/lib.rs` (main
  components); `liquers-web/src/command/spec.rs` (the declaration gains `template`).

### Design Readiness

Ready: `liquers-lib` has every piece (`register_template_commands!`, `ContextHost`, `render`,
`validate_template`), `liquers-template` already builds for `wasm32`, and `COMMAND07` gives a
JavaScript command its `Context`. Decisions applied from project memory: new calls are
`Environment` methods as well as module functions; one environment, no environment argument;
registration stays `registerCommands`.

## Phase 2: Architecture

### Solution

The guide's eight questions, answered:

1. **Entry point.** `render(template, state?, data?, options?)`, `expand(template, data?, options?)`,
   `validate(template)` on the module (`validateTemplate`, beside `validateCommands`), on
   `Environment`, and on the command `Context`, one signature.
2. **Template.** A string is text; a `liquers.Query` is a link, read with
   `Context::get_dependency_state`. Rejected: a string that is a link when it parses as a query
   (`"Hello"` parses as one).
3. **Frames.** The commands' layout exactly: `render` puts the state at the bottom (`Data`) unless
   empty and `data` above it (`Names`); `expand` has `data` alone (`Data`).
4. **Where it runs.** In a command, through its `Context` and `ContextHost`. At the top level, in a
   temporary asset (`AssetRef::new_temporary`), never cached or shared.
5. **Warnings at the top level.** `full: true` resolves to `{ text, warnings }`. On `wasm32` an
   asset's service messages are drained only inside a run, so the temporary asset's log never
   receives them; `CollectingHost` wraps `ContextHost` and keeps each recovered error. Rejected:
   reading the asset's log (empty), or an ad-hoc `apply` of the render command (the template text
   would travel inside the query, and no host would be ours to collect from).
6. **HTML escaping.** An explicit `html` flag, default off.
7. **Awaitable.** Every call is a `Promise`; a nested render is a nested evaluation, as
   `context.evaluate` (`TEMPLATE10`).
8. **Template commands.** A declaration with `template` in place of `run`, the guide's one-call
   form over the declaration path: `registerCommand({ name, template })`. It becomes
   `Alias { template/render, [Query(link)] }` with `render`'s remaining arguments. With pipeline
   commands it forms the *definition commands* (no `run`): checked against the environment's
   registry plus the batch, registered after the batch's function commands, as metadata only;
   `complete_definition` copies what `register_alias` copies from the target.

`TEMPLATE11` needs a command to receive its `cwd` argument, a `Key`; `Key` and `Query` values had no
JSON form, so the value bridge now crosses them as the `Key` / `Query` wrappers, both ways.

### Changes

- `Cargo.toml`: `liquers-lib` features `["webui", "templates"]` (not optional); `check-stubs.sh`
  (`PACKAGE05`) follows. `src/builtins.rs`: `register_template_commands!`.
- `src/template.rs` (new): `TemplateSource`, `Frames`, `TemplateOptions`, `Rendered`,
  `render_in`, `render_top_level`, `validate`, `validate_top_level`, `CollectingHost`, the
  JavaScript argument readers.
- `src/lib.rs`: `render`, `expand`, `validateTemplate`; `src/environment.rs`: `Environment.render`,
  `expand`, `validate`; `src/command/context.rs`: `Context.render`, `expand`, `validate`
  (`ContextOps::as_any` downcasts to `Context<WebEnvironment>`).
- `src/command/spec.rs`: `template`, `is_definition` (was `is_pipeline`),
  `from_definition_metadata`; `src/command/adapter.rs`: `complete_definition`,
  `register_definition_metadata`; `src/command/import.rs`: imported alias metadata needs no
  implementation; `src/environment.rs` `validate_batch` adds `alias_target`.
- `src/objects.rs`: `__liquersQuery` / `__liquersKey` markers and wrapper readers;
  `src/bridge.rs`: `Key` / `Query` both ways. `src/typescript.rs`: `LiquersTemplateCommandSpec`,
  `LiquersTemplateOptions`, `LiquersRendered`, `LiquersTemplateDiagnostic`.

### Risks

`templates` is always linked, so every page carries the template parser (see Phase 5 for the
size). The top-level render is the integration's own, as `TEMPLATE-RENDER-IN-ENV` proposes for
`liquers-lib`; when that lands, `render_top_level` should call it. A `Key` value that reached a page
as a JSON string before now arrives as a `Key`.

## Phase 3: Examples and Tests

### Examples

The two examples of `JAVASCRIPT_GUIDE.md` §6 "Templates", run under Node by
`scripts/test-guide-examples.sh`.

### Tests

`liquers-web/tests/template_TEMPLATE.rs`, under Node, after the guide's `TEMPLATE01`–`11`, over
the guide's store content in a page-implemented store, written through `AssetManager::set_binary`:
`template01_template_commands_are_built_in` (AC-1),
`template02_declared_template_command_renders` (AC-5),
`template03_editing_the_template_invalidates_results` (AC-5),
`template04_top_level_expand_over_an_object` (AC-2),
`template05_programmatic_call_matches_the_command` (AC-2),
`template06_in_command_render_records_dependencies` (AC-4),
`template07_strict_throws_with_position_recovering_mode_warns` (AC-3, AC-4),
`template08_html_escaping_follows_the_flag` (AC-3),
`template09_validate_runs_nothing` (AC-6),
`template10_nested_render_does_not_deadlock` (AC-4),
`template11_relative_partial_resolves_against_cwd` (AC-3, AC-4).
`tests/stubs/valid_usage.ts` type-checks the new declarations (`STUBS02`).

```
cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles --test template_TEMPLATE
```

## Phase 4: Implementation Plan

### Steps

- [x] 1. `Cargo.toml`, `builtins.rs` — `template01`
- [x] 2. `template.rs`, module, `Environment` and `Context` calls — `template04`–`11`
- [x] 3. `spec.rs`, `adapter.rs`, `import.rs`, `environment.rs` — `template02`, `template03`
- [x] 4. `bridge.rs` `Key` / `Query` — `template11`
- [x] 5. `typescript.rs`, `valid_usage.ts` — `check-stubs.sh --build`
- [x] 6. Documents, README row, issues — `docs_index.py --check`, `check_api_docs.py`

### Validation

Both Node loops (`--features debug-handles`, and `debug-handles,records`), the guide examples,
`check-stubs.sh --build`, `check_api_docs.py` on the touched modules, `docs_index.py --check`,
`liquers-docs check`.

## Phase 5: Documentation

Built as designed. Documents changed: `reference/JAVASCRIPT_INTEGRATION.md` (§1, §3, §4 template
commands, context members and "Templates", §5 `Key`/`Query`, limitations), `guides/JAVASCRIPT_GUIDE.md`
§2 and §6 "Templates", §13; `guides/LANGUAGE-INTEGRATION_GUIDE.md` §TEMPLATE and §6 Browser
JavaScript; `liquers-web/README.md` conformance row (`TEMPLATE` complete); `docs/guides/web/10-templates.md`
outline line. Filed: `WEB-STORE-WRITE-DOES-NOT-INVALIDATE-RESULTS`. Closed:
`WEB-HAS-NO-TEMPLATE-COMMANDS`. A link default in a declaration is written
`{ pipeline: ["-R-key/."] }` until `REGISTRY-QUERY-DEFAULT-NOT-WRITABLE-AS-TEXT` is resolved.
