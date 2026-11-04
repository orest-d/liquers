# Phase 1: High-Level Design — liquers-py-templates

## Purpose

Let Python fill a query-aware template from a `dict` in a script, render one inside a Python
command with its queries recorded as dependencies, and declare a template command with no Python
function: the `TEMPLATE` feature of `LANGUAGE-INTEGRATION_GUIDE.md` (`TEMPLATE_LANGUAGE.md`,
`TEMPLATE_GUIDE.md`), for `VISION.md`'s reports assembled from queries. Milestone M6, with
[`liquers-py-data-nodes`](../liquers-py-data-nodes/DESIGN.md) and
[`liquers-py-pipelines`](../liquers-py-pipelines/DESIGN.md).

## Design Readiness

- **Readiness:** `needs-decision` until Phases 2-4 are written and reviewed; no user decision is
  open (question 8 taken as recommended under `proceed all`, Decision 2, an assumption reported
  with the pre-implementation summary; no stop is needed)
- **Automatic fixing:** `not-eligible`: an `L` feature with new public Python API.
- **Leading issue:** link defaults (question 8, Decision 2). Source issue `PY-CANNOT-RENDER-TEMPLATES`
  (P2 L, filed with this design).
- **Explanation:** the in-command half needs nothing new in Rust. `ContextHost::new(context, cwd)`
  and `liquers_template::render` are what the `ns-template/render` command uses. The top-level half
  needs an asset to render in, and that is the open choice. Template commands are aliases
  (`CommandRegistry::register_alias`). The retained declarations (`Declarations` in
  `liquers-py/src/environment.rs`) have no slot for them yet, nor for pipeline commands. The first
  M6 design to land adds one slot for both (open question 7).
- **Open questions:**
  1. **Decided (Decision 1): the top-level route** (guide question 4). Everything that works in a
     command works at the top level. The top-level render runs the same code as inside a command
     (`ContextHost` and `liquers_template::render`, with frames given directly), inside an ad-hoc
     asset that a new core primitive provides: the asset manager runs a closure as an ad-hoc asset,
     through `run`, with status, cancellation, log and dependencies, and hands the closure the
     asset's `Context` (`AD-HOC-ASSET-CANNOT-RUN-A-CLOSURE`, filed with this decision; its own
     design and PR, landed first). So two frames work at the top level, the `data` `dict` is read
     in place through the adapter, and the template text never travels inside a query.
     The primitive returns the asset handle before it runs, so liquers-py cancels the asset when a
     wait is interrupted and waits until it settles. `template-render-in-env` is then
     replaced by a thin helper over the primitive (`render_top_level(&envref, request)` in `liquers-lib`), which that design's Scope Change records. Rejected: the ad-hoc
     `apply` of `ns-template/render` (one frame only, the data converted to a `Value`), and a
     context built on a temporary asset (it runs outside `run`: no status transitions, and on wasm
     no service loop processes its messages).
  2. **Proposed resolution: template as text or link** (guide question 2). A `str` is template text. An
     `lq.Query` is a link to the template, read as a dependency. An `lq.Key` means `-R/<key>`.
     Nothing is guessed from the string's content.
  3. **Proposed resolution: frames** (guide question 3). Exactly the commands' layout, as in the guide's
     table. `render(template, state=None, data=None)` puts the state at the bottom (`Data`) and `data`
     on top (`Names`). `expand(template, data=None)` uses `data` alone (`Data`). `data` is a
     mapping or `None`. `state` is any Python value, converted by the value bridge, or an `lq.State`.
  4. **Proposed resolution: receivers** (guide question 1). The same `render` / `expand` / `validate`
     signatures on `Environment` and on `Context`, with `_async` forms (guide question 7). Sync forms
     wait like `evaluate`, with the GIL released. `async def` commands use the `_async` forms, and
     the sync forms raise there, as `context.evaluate` does.
  5. **Proposed resolution: warnings at the top level** (guide question 5). The call returns the text.
     `full=True` returns an `lq.RenderResult` with `text` and `warnings` (`lq.LogEntry` list,
     positions kept). Inside a command, recovered errors also go to the command's log.
  6. **Proposed resolution: HTML escaping** (guide question 6). An explicit `html=False` keyword, never
     derived from the template's key. This deliberately differs from the commands, which escape when
     the result's filename is `.html` (`liquers-lib/src/template/commands.rs`). So a template
     command and `context.render` inside a command can differ for a `.html` result, and the docs
     say so.
  7. **Proposed resolution: template commands** (guide question 8). `env.template_command(name,
     template=…, namespace=…, label=…, doc=…, filename=…)` registers the alias of `template/render` with the
     link as head, and arguments `data`, `errors` and `cwd` (default link `-R-key/.`). It goes
     through the new declaration slot (above), so a rebuild keeps it. A registration error raises
     at the call: the alias is checked at once against the registry the next build will have (all
     current declarations plus this one), and applied again at each build (Phase 2 refinement;
     the first wording said "the current registry"). `lq.CommandDefinition` gains read-only accessors, specified once for both M6
     designs: `kind` (`"registered"`, `"alias"` or `"pipeline"`), `alias_of`, `head_parameters`
     and `pipeline` (an `lq.Pipeline`). Inspection therefore shows that there is no callable.
  8. **Decided (Decision 2): link defaults on Python commands** (`cwd` inside a command, TEMPLATE11).
     Recommended: here, because TEMPLATE11 needs it. A default `lq.link("-R-key/.")` declares a
     default link argument: `cwd: lq.Key = lq.link("-R-key/.")`. Today `_baseline` in
     `_commands.py` accepts only scalar defaults. Link values are already resolved at execution
     (`liquers-py/src/command.rs`), so only the declaration changes. `lq.Key` annotates
     `argument_type: any`, as for a Rust `Key` argument. `context.render` without `cwd` resolves
     against the root, and the docs say so. This changes the decorator API of every command.
     Alternative: land it in `py-command-decorator-declaration-keywords`, which owns argument
     declarations, and land that design first.

## Decisions

1. Orest, 2026-11-04, at the Phase 1 gate, on question 1 (the two-frame limit at the top level):
   "why would template work only in commands - if the environment is available, it should be
   possible outside the commands too?" A core primitive gives the top level the same render as a
   command. Landing order data nodes, templates, pipelines: "ok".
2. Orest, 2026-11-04: "proceed all", with question 8 still open. Taken as recommended and logged as
   an assumption, which Orest can still reverse before implementation: `lq.link` lands in this
   design. The core closure primitive (`AD-HOC-ASSET-CANNOT-RUN-A-CLOSURE`) gets its own design
   folder, `ad-hoc-closure-asset`, and its own PR, landing first.

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
  WHEN `env.render(template, state={"a": 1}, data={"b": 2})` renders `{{a}}{{b}}` at the top level, and `context.render` does the same inside a command
  THEN both return `12`, as the command does with the same state and `data` (Decision 1)
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
`command_metadata.rs`, `_commands.py`, the stubs. The closure primitive (`liquers-core`) and
`render_top_level` (`liquers-lib`) land first, each in its own PR (Decision 1).

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

- `AD-HOC-ASSET-CANNOT-RUN-A-CLOSURE` and `template-render-in-env` /
  `TEMPLATE-TOP-LEVEL-RENDER-HAS-NO-HOST`: **requires** (Decision 1). The primitive needs a design
  of its own (`L`); `template-render-in-env` is re-based on it (its Scope Change of 2026-11-04).
  Each lands first, in its own PR.
- `liquers-py-data-nodes`: **requires** (lands first), so `data` is read in place. Without it,
  `expand` would convert `data`, and one unconvertible member would blank the whole `dict`.
- `liquers-py-pipelines`: **overlaps**. Both add the declaration slot and the
  `lq.CommandDefinition` accessors (question 7); whichever lands first adds them.
- `py-command-decorator-declaration-keywords`: **overlaps** (question 8).

## References

- Guide §TEMPLATE and Appendix A; `liquers-lib/src/template/`; `commands.rs` `register_alias`;
  `_commands.py` `_baseline`
