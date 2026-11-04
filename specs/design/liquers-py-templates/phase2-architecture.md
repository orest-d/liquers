# Phase 2: Solution & Architecture — liquers-py-templates

## Overview

A new `liquers-py/src/template.rs` turns Python arguments into a `RenderRequest` of
`template-render-in-env` and runs it: at the top level through `render_top_level` (a closure asset)
waited on with `closure::wait_for_run`, and inside a command through `render_in_context` with the
command's context, waited on like `context.evaluate`. `data` is a `PyNode` frame read in place
(`liquers-py-data-nodes`). Validation calls `validate_template` and runs nothing. Template commands
and pipeline commands go into one new declaration slot, `Declarations::declared`, applied at every
build after the Python commands. A default `lq.link(query)` declares a default link argument, which
the declaration document already carries as `{"Query": …}`.

Rejected:
- The ad-hoc `apply` of `ns-template/render` (Phase 1 Decision 1).
- Converting `data` to a `Value` first: one unconvertible member blanks the whole frame (the
  data-nodes problem).
- An alias kept only in the built registry: a rebuild (a new Python command) would drop it.
- Checking an alias against the current built registry: the next build may differ (a command
  declared later). The check builds the next registry instead (below).
- A string default meaning a link (`cwd: lq.Key = "-R-key/."`): a string default is a value today.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `AD-HOC-ASSET-CANNOT-RUN-A-CLOSURE` (design `ad-hoc-closure-asset`) | draft, design approved | P2 | The top-level asset | Yes | Yes | Own PR first |
| `TEMPLATE-TOP-LEVEL-RENDER-HAS-NO-HOST` (design `template-render-in-env`) | draft, design approved | P2 | `RenderRequest`, `render_in_context`, `render_top_level` | Yes | Yes | Own PR first |
| `PY-STRUCTURES-ARE-OPAQUE-TO-DATA-NODE-READERS` (design `liquers-py-data-nodes`) | draft | P2 | `data` read in place | Yes | Yes | Lands first (Phase 1 order) |
| `TEMPLATE-DIAGNOSTICS-CHECK-COMMANDS-A-Q-QUOTES` | draft | P3 | `validate` may report a quoted command | No | No | Inherited; documented |
| `TEMPLATE-NON-SCALAR-HAS-NO-TEXT-FORM` | draft | P3 | An `lq.Query` in `data` renders as JSON | No | No | Inherited |
| `PY-COMMAND-DECORATOR-CANNOT-DECLARE-ARGUMENT-METADATA` | draft | P2 | Overlaps `lq.link` (both touch `_baseline`) | No | No | `lq.link` is a default value, not argument metadata; that design reuses it |
| `PY-REPRS-SHOW-RUST-DEBUG-FORMAT` | draft | P3 | New classes need reprs | No | No | `RenderResult('…', 1 warning)` |

Command namespaces: `template` (the existing `ns-template` commands, aliased by template commands);
no new command.

## Interfaces

**Python API.** Template: a `str` is text, an `lq.Query` a link, an `lq.Key` the link `-R/<key>`.
Keyword arguments common to the render calls: `errors="strict"` (`ErrorMode::parse`), `cwd=None`
(an `lq.Key`; `None` is the root), `html=False`, `full=False`.

```python
Environment.render(template, state=None, data=None, *, errors, cwd, html, full) -> str | RenderResult
Environment.expand(template, data=None, *, errors, cwd, html, full) -> str | RenderResult
Environment.validate(template: str) -> list[Diagnostic]
Environment.render_async(...), Environment.expand_async(...)          # awaitables
Context.render / expand / validate / render_async / expand_async      # same signatures
Environment.template_command(name, template, *, namespace="", label=None, doc=None,
                             filename=None) -> None
lq.link(query) -> Link                         # a default link argument
class RenderResult: text: str; warnings: list[LogEntry]
class Diagnostic: severity: str; message: str; line: int; column: int
CommandDefinition.kind / alias_of / head_parameters / pipeline        # read-only
```

**Frames** (`liquers-py/src/template.rs`, sync helpers under the GIL):

```rust
fn template_source(object: &Bound<'_, PyAny>) -> Result<TemplateSource, Error>;
/// `render`: `state` (bottom, `Data`), then `data` (`Names`). `expand`: `data` (`Data`).
fn frames(py: Python<'_>, state: Option<&Bound<'_, PyAny>>, data: Option<&Bound<'_, PyAny>>,
          below: FrameKind, conversion: ConversionMode, pickle: bool)
    -> Result<Vec<(FrameData, FrameKind)>, Error>;
```

- `state`: an `lq.State` gives `FrameData::State` of its core state (no conversion, its metadata
  as `_meta`); any other value is converted by `py_to_value` into a `State` (`py.Object` when
  refused, read through the hook). `None` adds no frame.
- `data`: `None` adds no frame; otherwise `FrameData::Node(AnyNode::Sync(Arc::new(node_of(x))))`,
  whose kind must be `Map` (`TypeError` otherwise, before any asset exists).

**Running** (`template.rs`):

- Top level (sync): build the request, `render_top_level(&envref, request)?` (a parse error raises
  here, AC-9 of `template-render-in-env`), keep `asset().clone()`, then
  `closure::wait_for_run(py, shut, asset, render.run())`. `Ok(None)` (cancelled before producing)
  raises `lq.CancelledError`. Async: `aio::awaitable(py, shut, render.run(), ToPy::Rendered)`.
- In a command (sync): `check_blocking_call`, then `block_on(py, shut,
  render_in_context(core_context, request))`, as `Context.evaluate` does. Recovered errors go to the
  command's log by `ContextHost::warn` and also into the result. Async: `awaitable`.
- `aio.rs`: `ToPy::Rendered(Result<(Rendered, bool /* full */), Error>)`, which builds `str` or
  `RenderResult`.
- `validate` (both receivers, sync, no wait): `validate_template(text, Mustache,
  Some(envref.get_command_metadata_registry()))`, each into an `lq.Diagnostic`.

**Declarations** (`liquers-py/src/environment.rs`). Shared with `liquers-py-pipelines`:

```rust
/// A command defined by declaration, with no Python callable: re-applied at every build.
pub(crate) enum DeclaredCommand {
    Alias { key: CommandKey, target: CommandKey, head_parameters: Vec<CommandParameterValue>,
            arguments: Vec<ArgumentInfo>, overrides: serde_json::Value /* label, doc, filename */ },
    Pipeline { key: CommandKey, pipeline: Pipeline, arguments: Vec<ArgumentInfo>,
               overrides: serde_json::Value },
}
struct Declarations { /* …, */ declared: Vec<DeclaredCommand> }   // registration order
impl EnvCell { pub(crate) fn declare(&self, py: Python<'_>, command: DeclaredCommand) -> Result<(), Error>; }
```

- `configured_builder` applies `declared` after `specs` and before `imported`
  (`CommandRegistry::register_alias` / `register_pipeline`, then the overrides on the returned
  `CommandMetadata`).
- `declare` replaces an entry with the same key (and a Python command with that key, as
  `register_py_command` replaces), then runs `configured_builder` on the new declarations plus
  `builder.validate()`. An error raises and leaves the declarations unchanged; otherwise the state
  becomes `Pending`, as for a new Python command. `unregister` removes a declared command too.
- `template_command` builds `Alias { target: template/render, head_parameters: [the link],
  arguments: data (default ""), errors (default "strict"), cwd (default link "-R-key/.") }`, the
  same three arguments as the target after its `template` head.

**Link defaults** (`_commands.py`, `liquers-py/src/query.rs`):

```rust
#[pyclass(frozen, name = "Link", module = "liquers")]
pub struct Link(pub liquers_core::query::Query);   // `lq.link(q)` parses a str or takes an lq.Query/lq.Key
impl Link { fn _declaration(&self) -> serde_json::Value /* {"Query": <the query's serde form>} */; }
```

`_baseline` accepts an `lq.Link` default and writes `argument["default"] = link._declaration()`;
the existing `CommandParameterValue` deserializer reads it as a `Query` default, which the
interpreter resolves at execution (`liquers-py/src/command.rs` receives the resolved value). An
`lq.Key` annotation keeps `argument_type: any`. Nothing else in the decorators changes.

**`CommandDefinition`** (`liquers-py/src/command_metadata.rs`): getters over the core enum:
`kind` (`"registered"`, `"alias"`, `"pipeline"`), `alias_of` (`CommandKey` or `None`),
`head_parameters` (Python values; a link is an `lq.Query`), `pipeline` (`lq.Pipeline` or `None`,
supplied by `liquers-py-pipelines`; `None` until it lands).

## Integration Points

- `liquers-py/src/template.rs` (new): API above, `RenderResult`, `Diagnostic`.
- `liquers-py/src/environment.rs`: `render`, `expand`, `validate`, `_async`, `template_command`;
  `DeclaredCommand`, `declared`, `declare`, `configured_builder`, `unregister`.
- `liquers-py/src/context.rs`: the same render methods.
- `liquers-py/src/aio.rs`: `ToPy::Rendered`.
- `liquers-py/src/query.rs`: `Link`, `link`.
- `liquers-py/src/command_metadata.rs`: `CommandDefinition` getters.
- `liquers-py/python/liquers/_commands.py` (`_baseline`), `__init__.py`, stubs.
- `liquers-py/tests/test_templates.py` (new).

## Error Handling

| Case | Error |
|---|---|
| Template text does not parse | `parse_template`'s error → `lq.ParseError` (or its mapped class), before any asset |
| `strict` render, a tag fails | the tag's error with its position → `lq.LiquersError` subclass (AC-7) |
| `data` not a mapping | Python `TypeError`, before any asset |
| Template is neither `str`, `lq.Query` nor `lq.Key`; `cwd` not an `lq.Key` | `TypeError` |
| `errors` not a mode | `ErrorMode::parse` error → `lq.ParameterError` |
| A link that is not text | `State::try_into_string` → `lq.ConversionError` |
| Sync form inside an `async def` command, or on liquers-py's loop thread | `check_blocking_call` (as `evaluate`) |
| Top-level wait interrupted / environment shut | the asset is cancelled, then `KeyboardInterrupt` / the shutdown error |
| `template_command` whose alias does not apply | `register_alias`'s error (`Error::invalid_alias`, …) → `lq.LiquersError`, declarations unchanged |
| `lq.link` of text that does not parse | `parse_query`'s error |

## Relevant Commands

`ns-template/render(state, template: String, data: Value = "", errors: String = "strict", cwd: Key =
query "-R-key/.", context)` is the alias target of a template command, unchanged; `expand` and
`validate` are not aliased. No new command; `specs/command_registry.yaml` is unchanged.

## Documentation Architecture

| Path | Kind | Audience | Change |
|---|---|---|---|
| `specs/reference/PYTHON_INTEGRATION.md` | reference | integrators | New §Templates: receivers, frames, warnings, escaping, declared commands, link defaults |
| `specs/guides/PYTHON_GUIDE.md` | guide | Python users | "Templates" chapter |
| `specs/guides/TEMPLATE_GUIDE.md` | guide | template authors | §3: the Python form of a template command |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | guide | command authors | Link defaults from Python (`lq.link`) |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | guide | integrators | §TEMPLATE, §6 Python |
| `liquers-py/README.md`, `specs/README.md` | readme, map | all | TEMPLATE row; status |
| `docs/guides/python/` chapters | user guide | Python users | per `docs_index.py chapters-for` (Phase 5) |

Module docs: `template.rs` (new), `environment.rs`, `context.rs`, `query.rs`,
`command_metadata.rs`, `aio.rs`; docstrings on every new Python name.

## Risks

| Risk | Mitigation |
|---|---|
| `declare` builds a whole registry per call (built-ins included) | A few milliseconds; measured in Phase 3; declarations are made at start-up |
| Escaping differs from a command for a `.html` result (question 6) | Stated in §Templates and the guide; AC-8 pins both |
| A nested Python render (AC-10) deadlocks | Every wait releases the GIL; `wait_for_run` and `block_on` are the only waits |
| `lq.link` changes the decorator document for every command | Only an `lq.Link` default is new; every other default is checked as today |
| The slot is added by whichever M6 design lands first | Templates lands before pipelines (order "ok"), so it adds the slot and the `Alias` arm; pipelines adds the `Pipeline` arm |
