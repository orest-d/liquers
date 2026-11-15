"""Starting a Liquers application from a configuration file with command-line overrides.

A program adds the Liquers options to its own ``argparse`` parser, parses its command line, and
configures an environment: the configuration file named by ``--config`` (or a default file, if it
exists), with the other options merged over it. An option that was not given leaves the file's
value. The merge happens on the parsed document (``Environment.configure``'s ``overrides=``), so a
relative ``command_registry`` in the file still resolves against the file's folder, while a path
given on the command line resolves against the working directory.

# Main components
- `add_arguments(parser)` — adds a "Liquers" option group: ``--config``, ``--command-registry``,
  ``--default-namespace``, ``--host``, ``--port``, ``--prefix``, ``--server-read-only``,
  ``--job-capacity``, ``--async-loops``, ``--conversion`` and ``--pickle``.
- `configure_from_args(args)` — configures the default environment (or ``env=``) from the parsed
  arguments.
- `configure_from_command_line(parser)` — both, plus parsing; returns the parsed arguments.

# Where it is used
- `liquers/__init__.py` re-exports the three functions at the top level.
- The user guide *Liquers for Python*, chapter 8 "Starting an application", and the server and UI
  examples of chapters 11 and 12.

# Related concepts
- `liquers/_env.py` `configure` and `Environment.configure`: settings stay open until the first
  build, so configuring after imports that registered commands works.
- `ConfigDocument::overlay` (liquers-core): the merge rule. Mappings merge key by key, and any
  other value, a list included, replaces.

# Example
```python
import argparse, pathlib, tempfile
import liquers as lq

folder = pathlib.Path(tempfile.mkdtemp())
(folder / "app.yaml").write_text("server: {port: 8123}\\n")
parser = argparse.ArgumentParser()
args = lq.configure_from_command_line(
    parser, argv=["--config", str(folder / "app.yaml"), "--port", "0"],
    env=(env := lq.Environment()),
)
server = env.serve()             # port 0 from the command line: a free port
assert server.port != 8123
env.shutdown()
```
"""

from __future__ import annotations

import argparse
import os
import pathlib
from collections.abc import Iterable, Sequence
from typing import Any

from liquers._native import Environment

DEFAULT_CONFIG = "liquers.yaml"

# Option name -> (argparse keyword arguments, where its value goes in the overlay). A path in the
# overlay is a sequence of keys; `None` marks `--config`, which names the file rather than a value.
_OPTIONS: dict[str, tuple[dict[str, Any], tuple[str, ...] | None]] = {
    "config": ({"metavar": "FILE", "help": "the configuration file (default: %(liquers_default)s if it exists)"}, None),
    "command-registry": ({"metavar": "FILE", "help": "a command metadata registry file"}, ("command_registry",)),
    "default-namespace": (
        {"metavar": "NS", "action": "append", "help": "a default command namespace (repeatable)"},
        ("default_namespaces",),
    ),
    "host": ({"help": "the address the server binds"}, ("server", "host")),
    "port": ({"type": int, "help": "the port the server binds (0: a free port)"}, ("server", "port")),
    "prefix": ({"help": "the server's URL prefix"}, ("server", "prefix")),
    "server-read-only": (
        {"action": argparse.BooleanOptionalAction, "help": "refuse writes through the server's APIs"},
        ("server", "read_only"),
    ),
    "job-capacity": ({"type": int, "metavar": "N", "help": "how many commands run at the same time"}, ("assets", "job_capacity")),
    "async-loops": ({"type": int, "metavar": "N", "help": "how many event loops async commands run on"}, ("python", "async_loops")),
    "conversion": ({"choices": ["permissive", "strict"], "help": "what happens to an object Liquers has no type for"}, ("python", "conversion")),
    "pickle": (
        {"action": argparse.BooleanOptionalAction, "help": "allow opaque objects to be stored with pickle (trusted stores only)"},
        ("python", "pickle"),
    ),
}

_DEFAULT_CONFIG_DEST = "liquers_default_config"


def _dest(option: str) -> str:
    return "liquers_" + option.replace("-", "_")


def add_arguments(
    parser: argparse.ArgumentParser,
    *,
    options: Iterable[str] | None = None,
    default_config: str | os.PathLike[str] | None = DEFAULT_CONFIG,
) -> argparse._ArgumentGroup:
    """Adds the Liquers options to ``parser``, in a group named "Liquers", and returns the group.

    ``options`` limits them to the names given, without dashes (``["config", "port"]``).
    ``default_config`` is the file used when ``--config`` is not given and the file exists;
    ``None`` reads no file then. Every option defaults to "not given", so the configuration file
    keeps its value. An option name the parser already has raises ``argparse.ArgumentError`` under
    the default ``conflict_handler="error"``; with ``"resolve"``, Liquers' option replaces it.
    """
    names = list(_OPTIONS) if options is None else [name.lstrip("-") for name in options]
    unknown = [name for name in names if name not in _OPTIONS]
    if unknown:
        raise ValueError(f"unknown Liquers option(s) {unknown}; known: {sorted(_OPTIONS)}")
    group = parser.add_argument_group("Liquers")
    for name in names:
        keywords = dict(_OPTIONS[name][0])
        if "help" in keywords:
            keywords["help"] = keywords["help"].replace("%(liquers_default)s", str(default_config))
        group.add_argument(f"--{name}", dest=_dest(name), default=None, **keywords)
    parser.set_defaults(**{_DEFAULT_CONFIG_DEST: default_config})
    return group


def _overlay(args: argparse.Namespace) -> dict[str, Any]:
    overlay: dict[str, Any] = {}
    for name, (_, path) in _OPTIONS.items():
        value = getattr(args, _dest(name), None)
        if path is None or value is None:
            continue
        if name == "command-registry":
            value = str(pathlib.Path(value).resolve())
        target = overlay
        for key in path[:-1]:
            target = target.setdefault(key, {})
        target[path[-1]] = value
    return overlay


def _config_file(args: argparse.Namespace) -> pathlib.Path | None:
    from liquers import ConfigurationError

    named = getattr(args, _dest("config"), None)
    if named is not None:
        path = pathlib.Path(named)
        if not path.is_file():
            raise ConfigurationError(f"the configuration file {path} does not exist")
        return path
    default = getattr(args, _DEFAULT_CONFIG_DEST, DEFAULT_CONFIG)
    if default is None:
        return None
    path = pathlib.Path(default)
    return path if path.is_file() else None


def configure_from_args(args: argparse.Namespace, *, env: Environment | None = None) -> None:
    """Configures ``env`` (the default environment if ``None``) from arguments parsed by a parser
    that ``add_arguments`` prepared.

    The file is ``--config``, or the default file if it exists, or none; the other options given
    are merged over it. A ``--config`` that does not exist raises ``ConfigurationError``. Like
    ``configure``, it must come before the environment's first build.
    """
    if env is None:
        from liquers._env import default_environment

        env = default_environment()
    env.configure(config=_config_file(args), overrides=_overlay(args) or None)


def configure_from_command_line(
    parser: argparse.ArgumentParser | None = None,
    argv: Sequence[str] | None = None,
    *,
    env: Environment | None = None,
    options: Iterable[str] | None = None,
    default_config: str | os.PathLike[str] | None = DEFAULT_CONFIG,
) -> argparse.Namespace:
    """Adds the Liquers options to ``parser`` (a new one if ``None``), parses ``argv`` (default
    ``sys.argv[1:]``), configures ``env`` (default: the default environment) and returns the
    parsed arguments, the program's own options among them.
    """
    if parser is None:
        parser = argparse.ArgumentParser()
    add_arguments(parser, options=options, default_config=default_config)
    args = parser.parse_args(argv)
    configure_from_args(args, env=env)
    return args
