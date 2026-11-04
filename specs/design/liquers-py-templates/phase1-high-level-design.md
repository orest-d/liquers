# Phase 1: High-Level Design — liquers-py-templates

## Purpose

Let Python fill a query-aware template from a `dict` in a script, render one inside a Python
command with its queries recorded as dependencies, and declare a template command with no Python
function: the `TEMPLATE` feature of `LANGUAGE-INTEGRATION_GUIDE.md` (`TEMPLATE_LANGUAGE.md`,
`TEMPLATE_GUIDE.md`), for `VISION.md`'s reports assembled from queries. Milestone M6, with
[`liquers-py-data-nodes`](../liquers-py-data-nodes/DESIGN.md) and
[`liquers-py-pipelines`](../liquers-py-pipelines/DESIGN.md).

## Design Readiness

- **Readiness:** `needs-decision` (questions 1 and 8)
- **Automatic fixing:** `not-eligible`: an `L` feature with new public Python API.
- **Leading issue:** question 1, the top-level route. Source issue `PY-CANNOT-RENDER-TEMPLATES`
  (P2 L, filed with this design).
- **Explanation:** the in-command half needs nothing new in Rust. `ContextHost::new(context, cwd)`
  and `liquers_template::render` are what the `ns-template/render` command uses. The top-level half
  needs an asset to render in, and that is the open choice. Template commands are aliases
  (`CommandRegistry::register_alias`). The retained declarations (`Declarations` in
  `liquers-py/src/environment.rs`) have no slot for them yet, nor for pipeline commands. The first
  M6 design to land adds one slot for both (question 7).
- **Open questions:**
  1. **Open design: the top-level route** (guide question 4). Recommended: the guide's route,
     an ad-hoc `apply` of `ns-template/render` through `render_in_env(envref, template, state, …)`
     of `template-render-in-env`, which lands first in its own PR. The asset runs through `run`,
     with status, cancellation (liquers-py cancels it when the wait is interrupted) and its log
     (the warnings). The `data` of `expand` becomes the ad-hoc state *unconverted*, as a retained
     `py.Object`. With `liquers-py-data-nodes` and its core hook, the renderer then reads the `dict`
     in place, through the adapter. The cost: two frames at the top level (a state *and* `data`)
     are not possible, so `env.render(template, state, data=…)` with both raises
     `NotSupportedError`. A core primitive that runs a closure as an ad-hoc asset would lift this
     limit; it is filed as an issue in Phase 2. Inside a command, both frames work, because the
     command's `Context` is used.
     Alternatives:
     - That core primitive now (an `AssetManager` change, `L`).
     - A context built on a temporary asset (`AssetRef::new_temporary` and `Context::new`). This
       is rejected, as `template-render-in-env` rejects it: it runs outside `run`, with no status
       transitions, and on wasm no service loop processes its messages.
  2. **Proposed resolution: template as text or link** (question 2). A `str` is template text. An
     `lq.Query` is a link to the template, read as a dependency. An `lq.Key` means `-R/<key>`.
     Nothing is guessed from the string's content.
  3. **Proposed resolution: frames** (question 3). Exactly the commands' layout, as in the guide's
     table. `render(template, state=None, data=None)` puts the state at the bottom (`Data`) and `data`
     on top (`Names`). `expand(template, data=None)` uses `data` alone (`Data`). `data` is a
     mapping or `None`. `state` is any Python value, converted by the value bridge, or an `lq.State`.
  4. **Proposed resolution: receivers** (question 1). The same `render` / `expand` / `validate`
     signatures on `Environment` and on `Context`, with `_async` forms (question 7). Sync forms
     wait like `evaluate`, with the GIL released. `async def` commands use the `_async` forms, and
     the sync forms raise there, as `context.evaluate` does.
  5. **Proposed resolution: warnings at the top level** (question 5). The call returns the text.
     `full=True` returns an `lq.RenderResult` with `text` and `warnings` (`lq.LogEntry` list,
     positions kept). Inside a command, recovered errors also go to the command's log.
  6. **Proposed resolution: HTML escaping** (question 6). An explicit `html=False` keyword, never
     derived from the template's key. This deliberately differs from the commands, which escape when
     the result's filename is `.html` (`liquers-lib/src/template/commands.rs`). So a template
     command and `context.render` inside a command can differ for a `.html` result, and the docs
     say so.
  7. **Proposed resolution: template commands** (question 8). `env.template_command(name,
     template=…, namespace=…, label=…, doc=…)` registers the alias of `template/render` with the
     link as head, and arguments `data`, `errors` and `cwd` (default link `-R-key/.`). It goes
     through the new declaration slot (above), so a rebuild keeps it. A registration error raises
     at the call: the alias is checked against the current registry at once, and applied again at
     each build. `lq.CommandDefinition` gains read-only accessors, specified once for both M6
     designs: `kind` (`"registered"`, `"alias"` or `"pipeline"`), `alias_of`, `head_parameters`
     and `pipeline` (an `lq.Pipeline`). Inspection therefore shows that there is no callable.
  8. **Open design: link defaults on Python commands** (`cwd` inside a command, TEMPLATE11).
     Recommended: here, because TEMPLATE11 needs it. A default `lq.link("-R-key/.")` declares a
     default link argument: `cwd: lq.Key = lq.link("-R-key/.")`. Today `_baseline` in
     `_commands.py` accepts only scalar defaults. Link values are already resolved at execution
     (`liquers-py/src/command.rs`), so only the declaration changes. `lq.Key` annotates
     `argument_type: any`, as for a Rust `Key` argument. `context.render` without `cwd` resolves
     against the root, and the docs say so. This changes the decorator API of every command.
     Alternative: land it in `py-command-decorator-declaration-keywords`, which owns argument
     declarations, and land that design first.

## Problem Example

```python
import liquers as lq
env = lq.Environment()

env.expand("{{name}} has {{$ number-42}}", data={"name": "Ann"})

@env.command
def report(context) -> str:
    return context.render(lq.parse_query("-R/templates/count.md"))   # "{{$ -R/d/count.txt}}"
```

**Today** `env.expand` raises `AttributeError`, and `lq.Context` has no `render`. A script must
encode the template into a query (`ns-template/render-<encoded text>`), call `evaluate` on a state
holding the data, and lose the recovered errors. **After** `env.expand` returns `"Ann has 42"`, and
`report/out.txt` gives the stored count, recomputed when `d/count.txt` changes. `number`, like
`hello`, `tick`, `inner` and `outer` below, is a test command, declared by the test (`number` a
source command returning its argument). The queries validate with those commands declared
(`liquers-validate --command number --command report`, `Ok`).

## Scope and Acceptance Criteria

- **AC-1** Top-level expand over a dict
  WHEN `env.expand("{{name}} has {{$ number-42}}", data={"name": "Ann"})` is called
  THEN it returns `"Ann has 42"`, and the query tag ran through an asset (TEMPLATE04)
- **AC-2** The call matches the command
  WHEN `env.render(lq.parse_query("-R/templates/greet.md"), state="hello")` and `env.expand(text)` are compared with the equivalent `ns-template/render` and `ns-template/expand` queries
  THEN each pair gives the same text (TEMPLATE05)
- **AC-3** The built-in commands work
  WHEN `hello/ns-template/render-~X~-R/templates/greet.md~E` and `-R/templates/greet.md/-/ns-template/validate` are evaluated
  THEN they give `"Hello, hello!"` and an empty list (TEMPLATE01)
- **AC-4** A template command needs no function
  WHEN `env.template_command("greeting", template="-R/templates/greet.md")` is declared and `hello/greeting` evaluated, before and after a rebuild
  THEN it gives `"Hello, hello!"`, and its metadata shows an alias of `template/render` with no callable (TEMPLATE02)
- **AC-5** Editing the template invalidates
  WHEN the stored template of a template command, or of a template rendered by a Python command, is rewritten
  THEN the next evaluation gives the new text (TEMPLATE03, TEMPLATE06)
- **AC-6** In-command renders record dependencies
  WHEN the problem example's `report` is evaluated, `d/count.txt` changed, and `report` evaluated again
  THEN the second result shows the new count (TEMPLATE06)
- **AC-7** Strict raises with the template position, and recovering modes warn
  WHEN `ok\n{{$ missing_command}}` is expanded in `strict` mode, and in `silent` mode with `full=True`
  THEN the first raises `lq.LiquersError` with the tag's line 2, column 1, and the second returns text `"ok\n"` with one warning (TEMPLATE07)
- **AC-8** Escaping is explicit
  WHEN `{{v}}` is expanded with `v = "<b>"`, without and with `html=True`
  THEN the results are `<b>` and `&lt;b&gt;`; a template command evaluated as `…/page.html` escapes, as the command does (TEMPLATE08, question 6)
- **AC-9** Validation runs nothing
  WHEN `env.validate("{{$ tick}} {{$ no_such_command}}")` is called with a counting `tick` command
  THEN `tick` never runs, and one `lq.Diagnostic` with severity, message and position is returned (TEMPLATE09)
- **AC-10** Nested Python renders complete
  WHEN `outer` expands `[{{$ inner}}]` and `inner` expands `{{x}}` with `x = "in"`, both Python commands, under a 5 s timeout
  THEN the outer result is `[in]` (TEMPLATE10)
- **AC-11** Relative partials follow cwd
  WHEN a command with `cwd: lq.Key = lq.link("-R-key/.")` renders `-R/templates/page.md` under `-R-cwd/templates/-/page`, and `env.render` gets `cwd=lq.parse_key("templates")`
  THEN both give `"Header body"` (TEMPLATE11)
- **AC-12** Two frames at the top level
  WHEN `context.render(template, state={"a": 1}, data={"b": 2})` renders `{{a}}{{b}}` inside a command, and `env.render` is called with the same arguments
  THEN the first returns `12`, and the second raises `NotSupportedError` naming the two-frame limit (question 1)
- **AC-13** Async forms agree
  WHEN `await env.expand_async(…)` and `await context.render_async(…)` in an `async def` command are used for AC-1 and AC-6
  THEN they give the same text, and the sync form inside an `async def` command raises

Non-goals: a template editor (`template-editor-ui`); new template syntax; `liquers-web`
(`WEB-HAS-NO-TEMPLATE-COMMANDS`).

## Core Interactions

Commands: aliases and link defaults. Assets: an ad-hoc asset at the top level, the command's
`Context` inside one. Value types: frames from the bridge or `DATANODE` adapters. Bindings: new
`Environment`, `Context` and `CommandDefinition` members, `lq.RenderResult`, `lq.Diagnostic`, `lq.link`.

## Crate Placement

`liquers-py`: `src/template.rs` new; `environment.rs`, `context.rs`, `command.rs`,
`command_metadata.rs`, `_commands.py`, the stubs. `render_in_env` comes from
`template-render-in-env` (`liquers-lib`), in its own PR first.

## Documentation Intent

- Reference: `PYTHON_INTEGRATION.md`, a new §Templates.
- Guide: a "Templates" chapter in `PYTHON_GUIDE.md`; `TEMPLATE_GUIDE.md` §3 gains the Python form.
- Other documents: the `liquers-py/README.md` conformance row; `LANGUAGE-INTEGRATION_GUIDE.md`
  §TEMPLATE and §6 Python "Templates and data nodes"; `specs/README.md`.
- Documents to update: `COMMAND_REGISTRATION_GUIDE.md` (link defaults from Python); the user guide
  chapters that `docs_index.py chapters-for` lists.
- API docs: the module docs of the new `liquers-py/src/template.rs`, `context.rs` and
  `command_metadata.rs`; docstrings on every new Python name.

## Design Dependencies

- `template-render-in-env` / `TEMPLATE-TOP-LEVEL-RENDER-HAS-NO-HOST`: **requires** (question 1),
  in its own PR first.
- `liquers-py-data-nodes`: **requires** (lands first), so `data` is read in place. Without it,
  `expand` would convert `data`, and one unconvertible member would blank the whole `dict`.
- `liquers-py-pipelines`: **overlaps**. Both add the declaration slot and the
  `lq.CommandDefinition` accessors (question 7); whichever lands first adds them.
- `py-command-decorator-declaration-keywords`: **overlaps** (question 8).
- `liquers-mustache` (complete): **requires** the commands and `ContextHost`.

## References

- Guide §TEMPLATE and Appendix A; `liquers-lib/src/template/commands.rs`, `host.rs`;
  `liquers-core/src/commands.rs` `register_alias`; `liquers-py/python/liquers/_commands.py` `_baseline`
