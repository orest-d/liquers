"""Liquers: query-driven data pipelines, from Python.

The classes and functions are implemented in Rust, in the native extension ``liquers._native``;
this package re-exports them under the ``liquers`` name. Every name the extension registers is
listed here explicitly, so a name added to or removed from ``liquers-py/src/lib.rs`` has to be
added to or removed from this file too.
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
    # Environments
    Environment,
    Issue,
    Context,
    State,
)
from liquers._env import default_environment, evaluate
from liquers._commands import command, first_command
from liquers._errors import LiquersError, _install as _install_errors

# One LiquersError subclass per ErrorType variant (KeyNotFoundError, ExecutionError, ...).
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
    "Environment",
    "Issue",
    "Context",
    "State",
    "default_environment",
    "evaluate",
    "command",
    "first_command",
    "LiquersError",
    *sorted(_ERROR_SUBCLASSES),
]
