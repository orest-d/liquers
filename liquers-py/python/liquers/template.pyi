"""Type declarations for ``liquers.template``; the classes it exports report ``liquers.template`` as their module."""

import os
from collections.abc import Awaitable, Callable, Coroutine, Iterable, Mapping, Sequence
from typing import Any, ClassVar, Literal, Protocol, TypeVar, final, overload

from typing_extensions import ParamSpec, Self, TypeAlias

from liquers.metadata import LogEntry
from liquers.query import Position

__all__ = [
    "RenderResult",
    "Diagnostic",
]

@final
class RenderResult:
    @property
    def text(self) -> str: ...
    @property
    def warnings(self) -> list[LogEntry]: ...
    def __str__(self) -> str: ...
    def __repr__(self) -> str: ...

@final
class Diagnostic:
    @property
    def severity(self) -> str: ...
    @property
    def message(self) -> str: ...
    @property
    def position(self) -> Position: ...
    def __repr__(self) -> str: ...
