"""Liquers: query-driven data pipelines, from Python.

The classes and functions are implemented in Rust, in the native extension ``liquers._native``;
this package re-exports them under the ``liquers`` name. Every name the extension registers is
listed here explicitly, so a name added to or removed from ``liquers-py/src/lib.rs`` has to be
added to or removed from this file too.

# Main components
- `liquers.evaluate` and `liquers.evaluate_async` — run a query in the process's default
  environment and return its value as a Python object.
- `liquers.command` — the decorator that registers a Python function as a command.
- `liquers.default_environment` and `liquers.Environment` — the environment: its store, commands
  and assets; configure it before the first query.
- `liquers.Query` and `liquers.parse` — the query language, parsed and inspected without running.
- `liquers.Environment.serve` and `liquers.Server` — serve an environment over the Liquers HTTP
  API.

# Where it is used
- [`liquers-py/tests/test_GUIDE_examples.py`](https://github.com/orest-d/liquers/blob/main/liquers-py/tests/test_GUIDE_examples.py)
  runs every example of the Python guide against this package.
- [`liquers-py/tests/test_DOCS_docstrings.py`](https://github.com/orest-d/liquers/blob/main/liquers-py/tests/test_DOCS_docstrings.py)
  checks that every public name here carries a docstring.
- The user guide *Liquers for Python* (<https://orest-d.github.io/liquers/guides/python/index.html>)
  is written against it.

Every other test under `liquers-py/tests/` imports it as `lq`.

# Related concepts
- `liquers._native` is the compiled extension. Import `liquers`, never `_native`: the extension's
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

from liquers._native import (
    # Query language
    Position,
    ActionParameter,
    ResourceName,
    ActionRequest,
    SegmentHeader,
    TransformQuerySegment,
    Key,
    ResourceQuerySegment,
    Query,
    QuerySource,
    parse,
    parse_key,
    # Command metadata
    EnumArgumentType,
    EnumArgument,
    ArgumentType,
    ArgumentInfo,
    CommandDefinition,
    CommandPreset,
    CommandMetadata,
    CommandKey,
    CommandMetadataRegistry,
    # Asset metadata
    Metadata,
    Version,
    DependencyKey,
    DependencyRecord,
    MetadataRecord,
    AssetInfo,
    Status,
    LogEntry,
    LogEntryKind,
    # Errors
    ErrorType,
    Error,
    # Expiration
    Expires,
    ExpirationTime,
    # Dependencies
    DependencyRelation,
    PlanDependency,
    # Plans
    Plan,
    Step,
    ParameterValue,
    ResolvedParameterValues,
    build_plan,
    # Recipes
    Recipe,
    RecipeList,
    RecipeProviders,
    # Environments
    Environment,
    Issue,
    Server,
    Context,
    State,
    AssetRef,
    # Data nodes
    NodeKind,
    DataNode,
    data_node,
    # Templates
    RenderResult,
    Diagnostic,
    Link,
    link,
    # Pipelines
    Pipeline,
    PipelineEditor,
    QueryEditor,
    ActionEditor,
    StoreArgumentInfo,
    Store,
    # Records
    FieldType,
    KeyRole,
    FieldSchema,
    RecordSchema,
    ChunkOrigin,
    RecordView,
    RecordColumn,
    RecordBuffer,
    RecordSource,
    RecordStream,
    RecordChunk,
    # The Liquers version in PEP 440 form; deliberately not in __all__ (OBJECT08).
    __version__,
)
from liquers._env import default_environment, evaluate, evaluate_async, serve, serve_async
from liquers._commands import command, first_command
from liquers._extras import require
from liquers._errors import LiquersError, _install as _install_errors

# One LiquersError subclass per ErrorType variant (KeyNotFoundError, ExecutionError, ...), and one
# base class per error category (StoreError, KeyedAssetError, NotFoundError).
_ERROR_SUBCLASSES = _install_errors(ErrorType)
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
    "Context",
    "State",
    "AssetRef",
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
    "evaluate",
    "evaluate_async",
    "serve",
    "serve_async",
    "command",
    "first_command",
    "require",
    "LiquersError",
    *sorted(_ERROR_SUBCLASSES),
]
