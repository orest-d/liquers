# Phase 1: High-Level Design — liquers-py-templates

## Purpose

Let Python fill a query-aware Mustache template from a `dict` in a script, render one inside a
Python command with its queries recorded as dependencies, and declare a template command with no
Python function. This is the `TEMPLATE` feature of `LANGUAGE-INTEGRATION_GUIDE.md` (§TEMPLATE,
`TEMPLATE_LANGUAGE.md`, `TEMPLATE_GUIDE.md`), for `VISION.md`'s reports assembled from queries. It
is milestone M6 of the liquers-py plan, with [`liquers-py-data-nodes`](../liquers-py-data-nodes/DESIGN.md)
and [`liquers-py-pipelines`](../liquers-py-pipelines/DESIGN.md).

## Design Readiness

- **Readiness:** `needs-decision` (question 1 chooses a core route and changes another design)
- **Automatic fixing:** `not-eligible`: an `L` feature with new public Python API.
- **Leading issue:** `PY-CANNOT-RENDER-TEMPLATES` (P2 L, filed with this design).
- **Explanation:** the in-command half needs nothing new in Rust. `ContextHost::new(context, cwd)`
  and `liquers_template::render` are what the `ns-template/render` command uses. The top-level half
  needs an asset to render in, and that is the open choice. Template commands are aliases
  (`CommandRegistry::register_alias`), which the retained declarations do not yet carry.
- **Open questions:**
  1. **Open design: the top-level route** (guide question 4). The guide's route is an ad-hoc
     `apply` of `ns-template/render`, which `template-render-in-env` wraps as
     `render_in_env(envref, template, state, …)`. It has two costs: the template text travels
     encoded in a query, and the data must be a `Value`, so a `dict` is converted, not read in
     place, and two frames (state *and* data) are impossible (that design's question 2).
     Recommended instead: render inside a **temporary asset**, as `interpreter::evaluate_pipeline`
     already does (`AssetRef::new_temporary` and `Context::new`). This uses the same
     `ContextHost` as in a command, with frames given directly: a `DATANODE` adapter, live and
     unconverted, or a converted value. The temporary asset keeps the lifecycle that `EVAL` asks
     for (log, cancellation, dependencies, and no cache). Put the helper in `liquers-lib`
     (`template::render_top_level(envref, template, frames, options)`), so `liquers-web` can reuse
     it. That revises `template-render-in-env` to the issue's original frames signature, landing in
     its own PR first. Alternative: take `render_in_env` as designed, and drop the two-frame
     `render`.
  2. **Proposed resolution: template as text or link** (question 2). A `str` is template text. An
     `lq.Query` is a link to the template, read as a dependency. An `lq.Key` means `-R/<key>`.
     Nothing is guessed from the string's content.
  3. **Proposed resolution: frames** (question 3). Exactly the commands' layout, as in the guide's
     table. `render(template, state, data=None)` puts the state at the bottom (`Data`) and `data`
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
     derived from the template's key.
  7. **Proposed resolution: template commands** (question 8). `env.template_command(name,
     template=…, namespace=…, label=…, doc=…)` registers the alias of `template/render` with the
     link as head, and arguments `data`, `errors` and `cwd` (default link `-R-key/.`). It goes
     through the retained declarations, so a rebuild keeps it. `CommandDefinition` gains read-only
     accessors (`kind`, `alias_of`, `head_parameters`), so inspection shows that there is no
     callable.
  8. **Proposed resolution: `cwd` inside a command.** Python commands gain default **link**
     arguments: `cwd: lq.Key = lq.link("-R-key/.")` (TEMPLATE11). Today `_baseline` accepts only
     scalar defaults. `context.render` without `cwd` resolves against the root, and the docs say so.
     This overlaps `py-command-decorator-declaration-keywords` (argument declarations), which has
     no link defaults.

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
`report/out.txt` gives the stored count, recomputed when `d/count.txt` changes. The queries
validate (`liquers-validate`, `Ok`; `report` with `--command report`).

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
  THEN the results are `<b>` and `&lt;b&gt;` (TEMPLATE08)
- **AC-9** Validation runs nothing
  WHEN `env.validate("{{$ tick}} {{$ no_such_command}}")` is called with a counting `tick` command
  THEN `tick` never runs, and one `lq.Diagnostic` with severity, message and position is returned (TEMPLATE09)
- **AC-10** Nested Python renders complete
  WHEN a Python command renders a template that runs another Python command which renders, under a 5 s timeout
  THEN the outer result is `[in]` (TEMPLATE10)
- **AC-11** Relative partials follow cwd
  WHEN a command with `cwd: lq.Key = lq.link("-R-key/.")` renders `-R/templates/page.md` under `-R-cwd/templates/-/page`, and `env.render` gets `cwd=lq.parse_key("templates")`
  THEN both give `"Header body"` (TEMPLATE11)
- **AC-12** Two frames at the top level
  WHEN `env.render(template, state={"a": 1}, data={"b": 2})` renders `{{a}}{{b}}`
  THEN it returns `12`, as the command does with the same state and `data` link
- **AC-13** Async forms agree
  WHEN `await env.expand_async(…)` and `await context.render_async(…)` in an `async def` command are used for AC-1 and AC-6
  THEN they give the same text, and the sync form inside an `async def` command raises

Non-goals: a template editor (`template-editor-ui`); new template syntax; `liquers-web`
(`WEB-HAS-NO-TEMPLATE-COMMANDS`).

## Core Interactions

Commands: aliases in the retained declarations, and link defaults. Assets: a temporary asset at
the top level, the command's `Context` inside one. Value types: frames from the bridge or from
`DATANODE` adapters. Bindings: `Environment` and `Context` members, `lq.RenderResult`,
`lq.Diagnostic`, `lq.link`.

## Crate Placement

`liquers-py` (`src/template.rs` new; `environment.rs`, `context.rs`, `command.rs`,
`command_metadata.rs`, `_commands.py`). `liquers-lib/src/template/` gets the top-level helper
(question 1), in the revised `template-render-in-env`, landed first in its own PR.

## Documentation Intent

- Reference: `PYTHON_INTEGRATION.md`, a new §Templates (receivers, frames, warnings, escaping,
  template commands, link defaults).
- Guide: a "Templates" chapter in `PYTHON_GUIDE.md`; `TEMPLATE_GUIDE.md` §3 gains the Python form.
- Other documents: the `liquers-py/README.md` conformance row; `LANGUAGE-INTEGRATION_GUIDE.md`
  §TEMPLATE and §6 Python "Templates and data nodes"; `specs/README.md`.
- Documents to update: `COMMAND_REGISTRATION_GUIDE.md` (link defaults from Python); the user guide
  chapters that `docs_index.py chapters-for` lists.
- API docs: the module doc of the new `liquers-py/src/template.rs` and of `context.rs`;
  `liquers-lib/src/template/` for the helper; docstrings on every new Python name.

## Design Dependencies

- `template-render-in-env` / `TEMPLATE-TOP-LEVEL-RENDER-HAS-NO-HOST`: **requires**, revised by
  question 1, in its own PR.
- `liquers-py-data-nodes`: **soft**. With it, `data` is read in place. Without it, `data` is
  converted structurally, and a `dict` with one unconvertible member renders as nothing (documented).
- `py-command-decorator-declaration-keywords`: **overlaps** (argument declaration; question 8).
- `liquers-py-stubs` (complete): **required-by**. The stubs declare the new names.

## References

- Guide §TEMPLATE and Appendix A; `liquers-lib/src/template/commands.rs`, `host.rs`;
  `liquers-core/src/commands.rs` `register_alias`; `liquers-py/python/liquers/_commands.py` `_baseline`
