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
| `AD-HOC-ASSET-CANNOT-RUN-A-CLOSURE` (design `ad-hoc-closure-asset`) | draft, design approved | P2 | The top-level asset | Yes (lands first) | No: a prerequisite, ordered first | Its own PR |
| `TEMPLATE-TOP-LEVEL-RENDER-HAS-NO-HOST` (design `template-render-in-env`) | draft, design approved | P3 | `RenderRequest`, `render_in_context`, `render_top_level` | Yes (lands first) | No: a prerequisite, ordered first | Its own PR |
| `PY-STRUCTURES-ARE-OPAQUE-TO-DATA-NODE-READERS` (design `liquers-py-data-nodes`) | draft | P2 | `data` read in place; `closure.rs` | Yes (lands first) | No: a prerequisite, ordered first | Lands first (Phase 1 order) |
| `TEMPLATE-DIAGNOSTICS-CHECK-COMMANDS-A-Q-QUOTES` | draft | P3 | `validate_template` may report a quoted command | No | No | Inherited; documented |
| `TEMPLATE-NON-SCALAR-HAS-NO-TEXT-FORM` | draft | P3 | An `lq.Query` in `data` renders as JSON | No | No | Inherited |
| `PY-COMMAND-DECORATOR-CANNOT-DECLARE-ARGUMENT-METADATA` | draft | P3 | Overlaps `lq.link` (both touch `_baseline`) | No | No | `lq.link` is a default value, not argument metadata; that design reuses it |
| `PY-REPRS-SHOW-RUST-DEBUG-FORMAT` | draft | P3 | New classes need reprs | No | No | `RenderResult('…', 1 warning)` |
| `PY-ADD-PYTHON-COMMAND-STILL-DECLARES-PYCALL-ALIASES` | draft | P3 | Such aliases report `kind == "alias"` with a target that does not exist | No | No | `alias_of` reports what the metadata says; noted in §Templates |
| `PY-COMMAND-DECORATOR-CANNOT-SET-REALM` | draft | P3 | A declared command has no realm either | No | No | `DeclaredCommand` keys use realm `""`, as decorators do; that issue adds realm to both |
| `PY-METADATA-FIELDS-ARE-METHODS` | draft | P3 | New members | No | No | Fields are properties (`text`, `warnings`, `kind`), operations methods |
| `PY-EXIT-WITH-UNCANCELLABLE-COMMAND-CRASHES-BEFORE-3-14` | draft | P3 | A top-level render running at exit | No | No | Its closure asset is cancelled at shutdown like a command |

Command namespaces: `template` (the existing `ns-template` commands, aliased by template commands);
no new command.

## Interfaces

**Python API.** Template: a `str` is text, an `lq.Query` a link, an `lq.Key` the link `-R/<key>`.
Keyword arguments common to the render calls: `errors="strict"` (`ErrorMode::parse`), `cwd=None`
(an `lq.Key`; `None` is the root), `html=False`, `full=False`.

```python
Environment.render(template, state=None, data=None, *, errors, cwd, html, full) -> str | RenderResult
Environment.expand(template, data=None, *, errors, cwd, html, full) -> str | RenderResult
Environment.validate_template(template: str) -> list[Diagnostic]   # `validate()` stays the registry check
Environment.render_async(...), Environment.expand_async(...)          # awaitables
Context.render / expand / validate_template / render_async / expand_async   # same signatures
Environment.template_command(name, template, *, namespace="", label=None, doc=None,
                             filename=None) -> None
lq.link(query) -> Link                         # a default link argument
class RenderResult: text: str; warnings: list[LogEntry]
class Diagnostic: severity: str; message: str; position: Position   # as LogEntry.position
CommandMetadata.definition -> CommandDefinition                        # new getter
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
  `closure::wait_for_run(py, shut, asset, render.run())`. `Ok(None)` (cancelled before it ran)
  raises `lq.CancelledError`, as the cancellation error of a mid-run cancel does (its type is
  `Cancelled`). After an interrupt, `wait_for_run` waits for the run to settle with signals
  deferred, bounded by `STOP_GRACE`, so a second Ctrl-C cannot leave it unsettled. Async: `closure::awaitable_run(py, shut, asset, render.run(),
  move |r| ToPy::Rendered(r.map(|o| (o, full))))` (`liquers-py-data-nodes`), never `aio::awaitable`, whose `select!` against
  shutdown would drop the run.
- `closure.rs` gains the sync wait this design is the first user of:

  ```rust
  /// Waits for `run`, spawned as its own task on the liquers-py runtime, with the GIL released
  /// (`block_on`). On an interrupt or shutdown it cancels `asset`, waits for the task to settle,
  /// then raises: dropping a run is not cancelling it (`ad-hoc-closure-asset`).
  pub(crate) fn wait_for_run<T: Send + 'static>(py: Python<'_>, shut: &Arc<Shut>,
      asset: AssetRef<PyEnvironment>,
      run: impl Future<Output = Result<Option<T>, Error>> + Send + 'static)
      -> Result<Option<T>, PyFailure>;
  ```
- In a command (sync): `check_blocking_call`, then `block_on(py, shut,
  render_in_context(core_context, request))`, as `Context.evaluate` does. Recovered errors go to the
  command's log by `ContextHost::warn` and also into the result. Async: `awaitable`.
- `aio.rs`: `ToPy::Rendered(Result<(Option<Rendered>, bool /* full */), Error>)`, which builds `str`
  or `RenderResult`, and raises `lq.CancelledError` for `None`, as the sync form does.
- `validate_template` (both receivers, sync, no wait; named apart from the existing
  `Environment.validate() -> list[Issue]`): `validate_template(text, Mustache,
  Some(envref.get_command_metadata_registry()))`, each into an `lq.Diagnostic`.

**Declarations** (`liquers-py/src/environment.rs`). Shared with `liquers-py-pipelines`:

```rust
/// A command defined by declaration, with no Python callable: re-applied at every build.
pub(crate) enum DeclaredCommand {
    Alias { key: CommandKey, target: CommandKey, head_parameters: Vec<CommandParameterValue>,
            arguments: Vec<ArgumentInfo>, overrides: serde_json::Value /* label, doc, filename */ },
    // `liquers-py-pipelines` adds `Pipeline { key, pipeline, arguments, overrides }`.
}
struct Declarations { /* …, */ declared: Vec<DeclaredCommand> }   // registration order
impl EnvCell { pub(crate) fn declare(&self, py: Python<'_>, command: DeclaredCommand) -> Result<(), Error>; }
```

- `configured_builder` applies `declared` after `specs` and before `imported`
  (`CommandRegistry::register_alias` / `register_pipeline`, then the overrides on the returned
  `CommandMetadata`).
- `declare` replaces an entry with the same key (and a Python command with that key, as
  `register_py_command` replaces); symmetrically, registering a Python command removes a declared
  command with its key, so the last registration wins either way (tested). Then runs `configured_builder` on the new declarations plus
  `builder.validate()`. An error raises and leaves the declarations unchanged; otherwise the state
  becomes `Pending`, as for a new Python command. `unregister` removes a declared command too.
- `template_command`'s `template` is always a link: a `str` is parsed as a query, an `lq.Query`
  taken as is, an `lq.Key` read as `-R/<key>` (never template text, unlike the render calls).
  It builds `Alias { target: template/render, head_parameters: [the link],
  arguments: data (default ""), errors (default "strict"), cwd (default link "-R-key/.") }`, the
  same three arguments as the target after its `template` head.

**Link defaults** (`_commands.py`, `liquers-py/src/query.rs`):

```rust
#[pyclass(frozen, name = "Link", module = "liquers")]
pub struct Link(pub liquers_core::query::Query);   // `lq.link(q)` parses a str or takes an lq.Query/lq.Key
```

`_DEFAULT_TYPES` in `_commands.py` admits `Link`, and `_baseline` keeps the `lq.Link` object as the
default in the declaration document. `py_to_json` (`environment.rs`), which turns that document
into JSON with `json.dumps`, passes it a `default=` hook that turns an `lq.Link` into
`{"Query": …}`: `Link._declaration()` builds `serde_json::to_value(&query)` (the derived struct form
of `Query` that `specs/command_registry.yaml` also holds) and returns it through
`store::json_to_py`. The existing `CommandParameterValue` deserializer reads it as a `Query` default, which the
interpreter resolves at execution (`liquers-py/src/command.rs` receives the resolved value). An
`lq.Key` annotation keeps `argument_type: any`. Nothing else in the decorators changes.

**`CommandDefinition`** (`liquers-py/src/command_metadata.rs`): `CommandMetadata` gains a
`definition` getter (none exists today); `CommandDefinition` gains getters over the core enum:
`kind` (`"registered"`, `"alias"`, `"pipeline"`), `alias_of` (`CommandKey` or `None`),
`head_parameters` (Python values; a link is an `lq.Query`), `pipeline` (`lq.Pipeline` or `None`,
supplied by `liquers-py-pipelines`; `None` until it lands).

## Integration Points

- `liquers-py/src/template.rs` (new): API above, `RenderResult`, `Diagnostic`.
- `liquers-py/src/closure.rs`: `wait_for_run`.
- `liquers-py/src/environment.rs`: `render`, `expand`, `validate_template`, `_async`, `template_command`;
  `DeclaredCommand`, `declared`, `declare`, `configured_builder`, `unregister`.
- `liquers-py/src/context.rs`: the same render methods.
- `liquers-py/src/aio.rs`: `ToPy::Rendered`.
- `liquers-py/src/query.rs`: `Link`, `link`.
- `liquers-py/src/command_metadata.rs`: `CommandDefinition` getters.
- `liquers-py/python/liquers/_commands.py` (`_baseline`), `__init__.py`, stubs.
- `liquers-py/tests/test_TEMPLATE_*.py` (new).

## Error Handling

| Case | Error |
|---|---|
| Template text does not parse | `parse_template`'s error → `lq.ParseError` (or its mapped class), before any asset |
| `strict` render, a tag fails | the tag's error with its position → `lq.LiquersError` subclass (AC-7) |
| `data` not a mapping | Python `TypeError`, before any asset |
| Template is neither `str`, `lq.Query` nor `lq.Key`; `cwd` not an `lq.Key` | `TypeError` |
| `errors` not a mode | `ErrorMode::parse`'s `Error::general_error` → `lq.GeneralError` |
| A link that is not text | `State::try_into_string` → `lq.ConversionError` |
| Sync form inside an `async def` command, or on liquers-py's loop thread | `check_blocking_call` (as `evaluate`) |
| Top-level wait interrupted / environment shut | the asset is cancelled, then `KeyboardInterrupt` / the shutdown error |
| `template_command` whose alias does not apply | `register_alias`'s error (`Error::invalid_alias`, `alias_target_not_registered`), or an issue found by `builder.validate()` as `Error::configuration_error` (as `build` reports it) → `lq.LiquersError`; declarations unchanged |
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

Module docs (`API_DOCUMENTATION_GUIDE.md` §3 parts): `template.rs` (new) all five;
`environment.rs`, `context.rs` main components and example (the render calls, `template_command`);
`query.rs`, `command_metadata.rs` main components (`Link`, `CommandDefinition` accessors);
`closure.rs` main components (`wait_for_run`); `aio.rs` main components (`ToPy::Rendered`).
Docstrings on every new Python name.

`affects_docs` (unchanged from Phase 1): `PYTHON_INTEGRATION.md`, `PYTHON_GUIDE.md`,
`LANGUAGE-INTEGRATION_GUIDE.md`, `liquers-py/README.md`, `specs/README.md`, `TEMPLATE_GUIDE.md`,
`COMMAND_REGISTRATION_GUIDE.md`. `specs/README.md` links: the M6 entry gains
`PYTHON_INTEGRATION.md` §Templates and the guide chapter when built.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-py/src/{template,environment,context,aio,query,command_metadata,closure,lib}.rs`, `_commands.py`, the stubs |
| Crates and workflows affected | `liquers-py` only (`bash scripts/test-liquers-py.sh`, CI on 3.10 / 3.14 / 3.14t) |
| Existing tests likely to change | `test_COMMAND_declaration_edges.py` cases on refused defaults keep passing (an `lq.Link` is the only new accepted default); `test_STUBS_declarations.py` gains the new names |
| New validation | `test_TEMPLATE_*.py`; stubtest over the new names |
| Compatibility | Escaping differs from a command for a `.html` result (question 6), stated and pinned by AC-8. `lq.link` is new; every other default is checked as today |
| Concurrency | A nested Python render (AC-10) waits only in `block_on` / `wait_for_run`, both with the GIL released |
| Performance | `declare` builds a whole registry per call (built-ins included), a few milliseconds, measured in Phase 3 (`test_declare_cost`) |
| Ordering | Templates lands before pipelines (order "ok"), so it adds the slot and the `Alias` arm; pipelines adds the `Pipeline` arm |
| Recovery | Revert the PR; no stored format changes |
| Certainty | High, given `template-render-in-env` and `ad-hoc-closure-asset` as designed; Decision 2 (`lq.link`) is an assumption reported before implementation |
