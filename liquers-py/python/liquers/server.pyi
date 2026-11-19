"""Type declarations for ``liquers.server``; the classes it exports report ``liquers.server`` as their module."""

import os
from collections.abc import Awaitable, Callable, Coroutine, Iterable, Mapping, Sequence
from typing import Any, ClassVar, Literal, Protocol, TypeVar, final, overload

from typing_extensions import ParamSpec, Self, TypeAlias

__all__ = [
    "serve",
    "serve_async",
    "Server",
]

@final
class Server:
    def shutdown(self, timeout: float = ...) -> None: ...
    def shutdown_async(self, timeout: float = ...) -> Awaitable[None]: ...
    def __enter__(self) -> Self: ...
    def __exit__(self, *_exc: object) -> bool: ...
    def __repr__(self) -> str: ...
    @property
    def address(self) -> str: ...
    @property
    def port(self) -> int: ...
    @property
    def url(self) -> str: ...
    @property
    def running(self) -> bool: ...

def serve(*, host: str | None = ..., port: int | None = ..., prefix: str | None = ...,
          apis: Sequence[Literal["query", "store", "assets", "recipes"]] | None = ..., read_only: bool | None = ...,
          web: bool | str | None = ..., background: bool = ...) -> Server: ...

def serve_async(*, host: str | None = ..., port: int | None = ..., prefix: str | None = ...,
                apis: Sequence[Literal["query", "store", "assets", "recipes"]] | None = ..., read_only: bool | None = ...,
                web: bool | str | None = ...) -> Coroutine[Any, Any, None]: ...
