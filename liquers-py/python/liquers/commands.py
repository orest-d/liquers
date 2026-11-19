"""Commands: the decorators that register Python functions, and the metadata that describes every command.

Rust counterpart: `liquers_core::command_metadata` and `liquers_core::commands`. Every name here is also available directly from `liquers`
(`liquers.command is liquers.commands.command`); `import liquers as lq` and `lq.command` stays
the usual style.

# Main components
- `command` and `first_command`: decorators that register a Python function as a command.
- `CommandMetadata`, `ArgumentInfo`, `ArgumentType`: what a registered command declares.
- `CommandMetadataRegistry`, `CommandKey`: all commands of an environment, by realm, namespace and name.
- `CommandDefinition`, `CommandPreset`, `EnumArgument`, `EnumArgumentType`: aliases, presets and enumerations.

# Where it is used
- `liquers.environment.Environment.command_metadata` returns a `CommandMetadata`.
- Every action of a query names a command.

# Related concepts
- A *command* is one action of a query; its arguments are the action's parameters.
- The Rust `register_command!` macro is the counterpart of `command`.

# Examples
>>> import liquers as lq
>>> from liquers.commands import CommandMetadata, command
>>> env = lq.Environment()
>>> isinstance(env.command_metadata("to_text"), CommandMetadata)
True
>>> env.shutdown()
"""

from liquers._commands import (
    command,
    first_command,
)
from liquers._backend import (
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

__all__ = [
    "command",
    "first_command",
    "CommandMetadata",
    "CommandDefinition",
    "CommandKey",
    "CommandPreset",
    "CommandMetadataRegistry",
    "ArgumentInfo",
    "ArgumentType",
    "EnumArgument",
    "EnumArgumentType",
]
