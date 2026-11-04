# Phase 3: Examples & Use-cases — liquers-py-templates

## Overview Table

| # | Kind | Name | Scenarios |
|---|---|---|---|
| 1 | Example | Expand and render from a script | AC-1, AC-2, AC-12 |
| 2 | Example | A template command and a render inside a command | AC-4, AC-6, AC-11 |
| 3 | Test | `liquers-py/tests/test_TEMPLATE_top_level.py` | AC-1, AC-2, AC-7, AC-8, AC-12, AC-13 |
| 4 | Test | `liquers-py/tests/test_TEMPLATE_commands.py` | AC-3, AC-4, AC-5, AC-6, AC-10, AC-11, AC-13 |
| 5 | Test | `liquers-py/tests/test_TEMPLATE_validate.py` | AC-9 |

**Validated queries** (`liquers-validate --command hello --command number --command report
--command greeting --command page --command outer --command inner --command tick --command ab
--command b2 --command bold --command show`; all `Ok`, each
`encoded` equal to its source):
1. `hello/ns-template/render-~X~-R/templates/greet.md~E`
2. `-R/templates/greet.md/-/ns-template/validate`
3. `hello/greeting`, `hello/greeting/page.html`
4. `number-42`, `report/out.txt`, `outer`
5. `-R-cwd/templates/-/page`
6. `ab/ns-template/render-~X~-R/templates/ab.md~E-~X~b2~E`, `bold/show/page.html`

Test commands, declared by the tests: `hello` (source, returns `"hello"`), `number(n)` (source,
returns `n`), `tick` (counts its calls), `report`, `page`, `inner`, `outer`, `ab` (returns
`{"a": 1}`), `b2` (returns `{"b": 2}`), `bold` (returns `"<b>"`), `show` (`template_command` over
`-R/templates/show.html`).
Store fixture: `templates/greet.md` = `Hello, {{.}}!`, `templates/header.md` = `Header`,
`templates/page.md` = `{{> ./header.md}} body`, `templates/ab.md` = `{{a}}{{b}}`,
`templates/show.html` = `{{.}}`, `d/count.txt`.

## Example 1: expand and render from a script

Each call builds a `RenderRequest` and runs it in a closure asset, waiting with the GIL released.

```python
import liquers as lq
env = lq.Environment()
# … the fixture and `number` …
assert env.expand("{{name}} has {{$ number-42}}", data={"name": "Ann"}) == "Ann has 42"  # AC-1
greet = lq.parse("-R/templates/greet.md")
assert env.render(greet, state="hello") == \
    env.evaluate("hello/ns-template/render-~X~-R/templates/greet.md~E")                # AC-2
assert env.render("{{a}}{{b}}", state={"a": 1}, data={"b": 2}) == "12"                  # AC-12
r = env.expand("ok\n{{$ missing_command}}", errors="silent", full=True)                  # AC-7
assert r.text == "ok\n" and len(r.warnings) == 1 and r.warnings[0].position.line == 2
```

## Example 2: a template command, and a render inside a command

```python
env.template_command("greeting", template="-R/templates/greet.md")       # a link; no function (AC-4)
assert env.evaluate("hello/greeting") == "Hello, hello!"
d = env.command_metadata("greeting").definition
assert d.kind == "alias" and d.alias_of.name == "render" and d.head_parameters[0] == greet

@env.command
def page(state, cwd: lq.Key = lq.link("-R-key/."), context=None) -> str:    # AC-11
    return context.render(lq.parse("-R/templates/page.md"), cwd=cwd)
assert env.evaluate("-R-cwd/templates/-/page") == "Header body"
assert env.render(lq.parse("-R/templates/page.md"),
                  cwd=lq.parse_key("templates")) == "Header body"
```

`report` renders `{{$ -R/d/count.txt}}` with `context.render`; after `d/count.txt` is rewritten,
`report/out.txt` gives the new count, because the query tag was a dependency of `report` (AC-6).

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| `strict` render with a failing tag on line 2 | `lq.LiquersError` with position line 2, column 1 | `test_strict_raises_with_position` |
| Text that does not parse, containing `{{$ tick}}` | raises before any asset; `tick` never ran | `test_parse_error_raises_early` |
| `data=[1, 2]` | `TypeError` | `test_data_must_be_a_mapping` |
| `{{v}}` with `"<b>"`, without and with `html=True`; `bold/show/page.html` | `<b>`, `&lt;b&gt;`; the command escapes | `test_escaping_is_explicit` |
| `outer` expands `[{{$ inner}}]`, `inner` expands `{{x}}`, under 5 s | `[in]` | `test_nested_python_renders` |
| A sync `render` inside an `async def` command | raises as `evaluate` does | `test_sync_form_in_async_command_raises` |
| A render rewritten after `template_command` redeclares `greeting` | the new alias wins, at once and after a rebuild | `test_template_command_survives_rebuild` |
| `template_command` with a template link to a non-text value | the evaluation raises `lq.ConversionError` | `test_template_link_must_be_text` |
| `KeyboardInterrupt` during a top-level render whose tag runs the conftest `slow` | the closure asset ends `Cancelled`; `slow` saw `context.is_cancelled()` | `test_interrupt_cancels_the_render` |
| A second `KeyboardInterrupt` while the render settles | `KeyboardInterrupt` raised once, within `STOP_GRACE` | `test_second_interrupt_is_deferred` |
| A run that never started (`Ok(None)`: the asset cancelled before its claim) | `lq.CancelledError` | `test_unstarted_render_is_cancelled` |
| `errors="loud"` | `lq.GeneralError` | `test_bad_errors_mode` |
| `template=42`; `state=object()` that conversion refuses; `cwd="x"` | `TypeError` | `test_argument_types` |
| A Python command registered under a declared name, and the reverse | the last registration wins, both ways | `test_declared_and_python_commands_replace_each_other` |
| `env.unregister("greeting")` | `greeting` is gone after a rebuild too | `test_unregister_declared` |
| `template_command` with a body that does not validate | raises; declarations and registry unchanged | `test_failed_declare_changes_nothing` |
| `declare` cost | measured, recorded in the Learning Log | `test_declare_cost` |

## Test Plan

- `test_TEMPLATE_top_level.py`: `test_expand_over_a_dict` (AC-1);
  `test_calls_match_the_commands` (AC-2); `test_strict_raises_with_position`,
  `test_recovering_modes_warn` (AC-7); `test_escaping_is_explicit` (AC-8);
  `test_two_frames_at_top_level` (AC-12: `env.render`, `context.render` in a command and the
  `ab/ns-template/render-…-~X~b2~E` query all give `12`); `test_async_forms_agree` (AC-13); edge
  cases, no AC: `test_parse_error_raises_early`, `test_data_must_be_a_mapping`,
  `test_interrupt_cancels_the_render`, `test_second_interrupt_is_deferred`,
  `test_unstarted_render_is_cancelled`, `test_bad_errors_mode`, `test_argument_types`.
- `test_TEMPLATE_commands.py`: `test_builtin_commands` (AC-3);
  `test_template_command_needs_no_function`, `test_template_command_survives_rebuild` (AC-4);
  `test_editing_the_template_invalidates` (AC-5); `test_in_command_render_records_dependencies`
  (AC-6, AC-13 with `render_async`); `test_nested_python_renders` (AC-10);
  `test_link_default_cwd` (AC-11); `test_sync_form_in_async_command_raises` (AC-13);
  edge cases, no AC: `test_template_link_must_be_text`, `test_declare_cost`,
  `test_declared_and_python_commands_replace_each_other`, `test_unregister_declared`, `test_failed_declare_changes_nothing`.
- `test_TEMPLATE_validate.py`: `test_validation_runs_nothing` (AC-9): `env.validate_template`;
  `tick` never runs, one
  `lq.Diagnostic` with severity, message and a position (line, column).
- Run: `bash scripts/test-liquers-py.sh`.

## Learning Log

- A Python render and the `ns-template/render` command share `render_in_context`, so AC-2 tests the
  argument mapping, not two renderers.
- A link default is just a `{"Query": …}` default in the declaration document; the interpreter
  already resolves it, so `lq.link` needs no change in the call path.
