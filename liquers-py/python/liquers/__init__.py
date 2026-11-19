"""Liquers: query-driven data pipelines, from Python.

The classes and functions are implemented in Rust, in the private native package ``_liquers``.
The public modules of this package re-export them, grouped like the Rust modules, and this package
re-exports every public name once more, so ``liquers.Key`` and ``liquers.query.Key`` are the same
object. A name added to or removed from ``liquers-py/src/lib.rs`` is added to or removed from
``liquers/_backend.py``, its public module, and this file.

# Modules
| Module | Contents | Rust counterpart |
|---|---|---|
| `liquers.query` | `Query`, `Key`, `parse`, `link`, … | `liquers_core::query` |
| `liquers.commands` | `command`, `CommandMetadata`, … | `liquers_core::command_metadata` |
| `liquers.metadata` | `Metadata`, `Status`, `Expires`, … | `liquers_core::metadata` |
| `liquers.dependencies` | `DependencyKey`, `PlanDependency`, … | `liquers_core::dependencies` |
| `liquers.plan` | `Plan`, `Step`, `build_plan`, … | `liquers_core::plan` |
| `liquers.recipes` | `Recipe`, `RecipeList`, `RecipeProviders` | `liquers_core::recipes` |
| `liquers.store` | `Store`, `StoreArgumentInfo` | `liquers_core::store` |
| `liquers.environment` | `Environment`, `Context`, `evaluate`, … | `liquers_core::context` |
| `liquers.assets` | `State`, `AssetRef` | `liquers_core::assets` |
| `liquers.accessors` | `Accessor` and its data classes | `liquers_core::accessor` |
| `liquers.error` | `LiquersError` and its subclasses, `ErrorType` | `liquers_core::error` |
| `liquers.data_nodes` | `data_node`, `DataNode`, `NodeKind` | `liquers_core::data_node` |
| `liquers.pipeline` | `Pipeline` and its editors | `liquers_core::pipeline` |
| `liquers.template` | `RenderResult`, `Diagnostic` | `liquers-template` |
| `liquers.records` | `RecordView`, `RecordSource`, … | `liquers-records` |
| `liquers.server` | `serve`, `Server` | `liquers-axum` |
| `liquers.lui` | `ui`, `UI`, `UIElement`, … | `liquers_lib::ui` |

# Main components
- `liquers.evaluate` and `liquers.evaluate_async` — run a query in the process's default
  environment and return its value as a Python object.
- `liquers.command` — the decorator that registers a Python function as a command.
- `liquers.default_environment` and `liquers.Environment` — the environment: its store, commands
  and assets; `liquers.configure` sets the default's settings before its first build.
- `liquers.add_arguments`, `liquers.configure_from_args` and `liquers.configure_from_command_line`
  — start an application from a configuration file with command-line overrides.
- `liquers.Query` and `liquers.parse` — the query language, parsed and inspected without running.
- `liquers.Environment.serve` and `liquers.Server` — serve an environment over the Liquers HTTP
  API.
- `liquers.Environment.ui` and `liquers.UI` — a Liquers UI model: show queries as elements,
  submit actions, follow their statuses.

# Where it is used
- [`liquers-py/tests/test_GUIDE_examples.py`](https://github.com/orest-d/liquers/blob/main/liquers-py/tests/test_GUIDE_examples.py)
  runs every example of the Python guide against this package.
- [`liquers-py/tests/test_DOCS_docstrings.py`](https://github.com/orest-d/liquers/blob/main/liquers-py/tests/test_DOCS_docstrings.py)
  checks that every public name here carries a docstring.
- The user guide *Liquers for Python* (<https://orest-d.github.io/liquers/guides/python/index.html>)
  is written against it.

Every other test under `liquers-py/tests/` imports it as `lq`.

# Related concepts
- `_liquers` is the compiled extension. Import `liquers`, never `_liquers`: the extension's
  layout is an implementation detail with no stability promise.
- The Rust crate `liquers_py` (`liquers-py/src/`) implements the extension; the Rust API
  reference on the documentation site documents it for contributors.
- A *command* is one action of a query; a *recipe* (`liquers.Recipe`) names a query to run for a
  store key, so its result can be read like a file.

# Examples
>>> import liquers as lq
>>> lq.evaluate("commands_doc").startswith("# Commands")
True

Longer examples:
- [`specs/guides/PYTHON_GUIDE.md`](https://github.com/orest-d/liquers/blob/main/specs/guides/PYTHON_GUIDE.md)
  — installing, a first query, commands, values, errors and stores, with an example for each.
- [`liquers-py/tests/test_COMMAND_registration.py`](https://github.com/orest-d/liquers/blob/main/liquers-py/tests/test_COMMAND_registration.py)
  — every form of the `@command` decorator.
- [`liquers-py/tests/test_STORE_python_protocol.py`](https://github.com/orest-d/liquers/blob/main/liquers-py/tests/test_STORE_python_protocol.py)
  — a store written in Python.

More are under `liquers-py/tests/`, one file per conformance group.
"""

# The public submodules, `liquers.error` first: it installs the error classes the others raise.
from liquers import error
from liquers import (
    query,
    commands,
    metadata,
    dependencies,
    plan,
    recipes,
    store,
    environment,
    assets,
    accessors,
    data_nodes,
    pipeline,
    template,
    records,
    server,
    lui,
)
from liquers._backend import __version__  # deliberately not in __all__ (OBJECT08)
from liquers.error import (
    Error,
    ErrorType,
    LiquersError,
)
from liquers.query import (
    Query,
    Key,
    ActionRequest,
    ActionParameter,
    ResourceName,
    Position,
    SegmentHeader,
    TransformQuerySegment,
    ResourceQuerySegment,
    QuerySource,
    parse,
    parse_key,
    Link,
    link,
)
from liquers.commands import (
    command,
    first_command,
    CommandMetadata,
    CommandDefinition,
    CommandKey,
    CommandPreset,
    CommandMetadataRegistry,
    ArgumentInfo,
    ArgumentType,
    EnumArgument,
    EnumArgumentType,
)
from liquers.metadata import (
    Metadata,
    MetadataRecord,
    AssetInfo,
    Status,
    LogEntry,
    LogEntryKind,
    Version,
    Expires,
    ExpirationTime,
)
from liquers.dependencies import (
    DependencyKey,
    DependencyRecord,
    DependencyRelation,
    PlanDependency,
)
from liquers.plan import (
    Plan,
    Step,
    ParameterValue,
    ResolvedParameterValues,
    build_plan,
)
from liquers.recipes import (
    Recipe,
    RecipeList,
    RecipeProviders,
)
from liquers.store import (
    Store,
    StoreArgumentInfo,
)
from liquers.environment import (
    Environment,
    Context,
    Issue,
    default_environment,
    configure,
    evaluate,
    evaluate_async,
    require,
    add_arguments,
    configure_from_args,
    configure_from_command_line,
)
from liquers.assets import (
    State,
    AssetRef,
)
from liquers.accessors import (
    Accessor,
    AccessorCapabilities,
    AccessorSnapshot,
    AccessorInfo,
    AccessorEvent,
    PartDescription,
    AccessorSubscription,
)
from liquers.data_nodes import (
    data_node,
    DataNode,
    NodeKind,
)
from liquers.pipeline import (
    Pipeline,
    PipelineEditor,
    QueryEditor,
    ActionEditor,
)
from liquers.template import (
    RenderResult,
    Diagnostic,
)
from liquers.records import (
    RecordView,
    RecordSource,
    RecordStream,
    RecordChunk,
    RecordColumn,
    RecordBuffer,
    RecordSchema,
    FieldSchema,
    FieldType,
    KeyRole,
    ChunkOrigin,
)
from liquers.server import (
    serve,
    serve_async,
    Server,
)
from liquers.lui import (
    ui,
    UI,
    UIElement,
    UIElementInfo,
    UIStatusEvent,
    UISubscription,
)

# One LiquersError subclass per ErrorType variant (KeyNotFoundError, ExecutionError, ...), and one
# base class per error category (StoreError, KeyedAssetError, NotFoundError), as `liquers.error`
# generated them.
_ERROR_SUBCLASSES = error._ERROR_SUBCLASSES
globals().update(_ERROR_SUBCLASSES)

__all__ = [
    "Position",
    "ActionParameter",
    "ResourceName",
    "ActionRequest",
    "SegmentHeader",
    "TransformQuerySegment",
    "Key",
    "ResourceQuerySegment",
    "Query",
    "QuerySource",
    "parse",
    "parse_key",
    "EnumArgumentType",
    "EnumArgument",
    "ArgumentType",
    "ArgumentInfo",
    "CommandDefinition",
    "CommandPreset",
    "CommandMetadata",
    "CommandKey",
    "CommandMetadataRegistry",
    "Metadata",
    "Version",
    "DependencyKey",
    "DependencyRecord",
    "MetadataRecord",
    "AssetInfo",
    "Status",
    "LogEntry",
    "LogEntryKind",
    "ErrorType",
    "Error",
    "Expires",
    "ExpirationTime",
    "DependencyRelation",
    "PlanDependency",
    "Plan",
    "Step",
    "ParameterValue",
    "ResolvedParameterValues",
    "build_plan",
    "Recipe",
    "RecipeList",
    "RecipeProviders",
    "Environment",
    "Issue",
    "Server",
    "UI",
    "UIElement",
    "UIElementInfo",
    "UIStatusEvent",
    "UISubscription",
    "Context",
    "State",
    "AssetRef",
    "Accessor",
    "AccessorCapabilities",
    "AccessorSnapshot",
    "AccessorInfo",
    "AccessorEvent",
    "PartDescription",
    "AccessorSubscription",
    "NodeKind",
    "DataNode",
    "data_node",
    "RenderResult",
    "Diagnostic",
    "Link",
    "link",
    "Pipeline",
    "PipelineEditor",
    "QueryEditor",
    "ActionEditor",
    "StoreArgumentInfo",
    "Store",
    "FieldType",
    "KeyRole",
    "FieldSchema",
    "RecordSchema",
    "ChunkOrigin",
    "RecordView",
    "RecordColumn",
    "RecordBuffer",
    "RecordSource",
    "RecordStream",
    "RecordChunk",
    "default_environment",
    "configure",
    "add_arguments",
    "configure_from_args",
    "configure_from_command_line",
    "evaluate",
    "evaluate_async",
    "serve",
    "serve_async",
    "ui",
    "command",
    "first_command",
    "require",
    "LiquersError",
    *sorted(_ERROR_SUBCLASSES),
]
