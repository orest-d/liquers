"""Type declarations for ``liquers.error``; the classes it exports report ``liquers.error`` as their module."""

import os
from collections.abc import Awaitable, Callable, Coroutine, Iterable, Mapping, Sequence
from typing import Any, ClassVar, Literal, Protocol, TypeVar, final, overload

from typing_extensions import ParamSpec, Self, TypeAlias

from liquers.query import Position

__all__ = [
    "Error",
    "ErrorType",
    "LiquersError",
    "ActionNotRegisteredError",
    "ArgumentMissingError",
    "AssetNotFoundError",
    "AssetNotSupportedError",
    "CacheNotSupportedError",
    "CancelledError",
    "CommandAlreadyRegisteredError",
    "ConfigurationError",
    "ConversionError",
    "DependencyCycleError",
    "DependencyVersionMismatchError",
    "ExecutionError",
    "GeneralError",
    "KeyNotAbsoluteError",
    "KeyNotFoundError",
    "KeyNotSupportedError",
    "KeyReadError",
    "KeyWriteError",
    "KeyedAssetError",
    "NotAvailableError",
    "NotFoundError",
    "NotSupportedError",
    "ParameterError",
    "ParseError",
    "SerializationError",
    "StatusConflictError",
    "StoreError",
    "TooManyParametersError",
    "UnexpectedError",
    "UnknownCommandError",
]

@final
class ErrorType:
    ArgumentMissing: ClassVar[ErrorType]
    ActionNotRegistered: ClassVar[ErrorType]
    CommandAlreadyRegistered: ClassVar[ErrorType]
    ParseError: ClassVar[ErrorType]
    ParameterError: ClassVar[ErrorType]
    TooManyParameters: ClassVar[ErrorType]
    ConversionError: ClassVar[ErrorType]
    SerializationError: ClassVar[ErrorType]
    General: ClassVar[ErrorType]
    CacheNotSupported: ClassVar[ErrorType]
    UnknownCommand: ClassVar[ErrorType]
    NotSupported: ClassVar[ErrorType]
    NotAvailable: ClassVar[ErrorType]
    KeyNotFound: ClassVar[ErrorType]
    AssetNotFound: ClassVar[ErrorType]
    AssetNotSupported: ClassVar[ErrorType]
    KeyNotSupported: ClassVar[ErrorType]
    KeyNotAbsolute: ClassVar[ErrorType]
    KeyReadError: ClassVar[ErrorType]
    KeyWriteError: ClassVar[ErrorType]
    UnexpectedError: ClassVar[ErrorType]
    ExecutionError: ClassVar[ErrorType]
    DependencyVersionMismatch: ClassVar[ErrorType]
    DependencyCycle: ClassVar[ErrorType]
    StatusConflict: ClassVar[ErrorType]
    Cancelled: ClassVar[ErrorType]
    ConfigurationError: ClassVar[ErrorType]
    def __repr__(self) -> str: ...
    def __lt__(self, value: Self, /) -> bool: ...
    def __le__(self, value: Self, /) -> bool: ...
    def __eq__(self, value: object, /) -> bool: ...
    def __ne__(self, value: object, /) -> bool: ...
    def __gt__(self, value: Self, /) -> bool: ...
    def __ge__(self, value: Self, /) -> bool: ...
    def __int__(self) -> int: ...
    def categories(self) -> list[str]: ...
    @staticmethod
    def all_categories() -> list[str]: ...
    __hash__: ClassVar[None]  # type: ignore[assignment]

@final
class Error:
    def __new__(cls, error_type: ErrorType, message: str) -> Self: ...
    def with_position(self, position: Position) -> Self: ...
    def throw(self) -> None: ...

# Errors. `LiquersError` is defined in Python; the classes below it are generated from `ErrorType`
# when the package is imported (`_errors._install`): one per category, then one per variant, whose
# bases are its categories in `ErrorType.categories()` order. `test_error_classes_are_declared`
# compares this list with what `_install` built.
class LiquersError(Exception):
    type: ErrorType
    message: str
    query: str | None
    key: str | None
    position: Position | None
    traceback: str | None
    @overload
    def __init__(self, error_type: ErrorType, message: str = ..., /, *, query: str | None = None,
                 key: str | None = None, position: Position | None = None,
                 traceback: str | None = None) -> None: ...
    @overload
    def __init__(self, message: str = ..., /, *, query: str | None = None, key: str | None = None,
                 position: Position | None = None, traceback: str | None = None) -> None: ...

class StoreError(LiquersError): ...

class KeyedAssetError(LiquersError): ...

class NotFoundError(LiquersError, LookupError): ...

class ActionNotRegisteredError(LiquersError): ...

class ArgumentMissingError(LiquersError): ...

class AssetNotFoundError(KeyedAssetError, NotFoundError): ...

class AssetNotSupportedError(KeyedAssetError): ...

class CacheNotSupportedError(LiquersError): ...

class CancelledError(LiquersError): ...

class CommandAlreadyRegisteredError(LiquersError): ...

class ConfigurationError(LiquersError): ...

class ConversionError(LiquersError): ...

class DependencyCycleError(LiquersError): ...

class DependencyVersionMismatchError(LiquersError): ...

class ExecutionError(LiquersError): ...

class GeneralError(LiquersError): ...

class KeyNotAbsoluteError(StoreError): ...

class KeyNotFoundError(StoreError, NotFoundError): ...

class KeyNotSupportedError(StoreError): ...

class KeyReadError(StoreError): ...

class KeyWriteError(StoreError): ...

class NotAvailableError(LiquersError): ...

class NotSupportedError(LiquersError): ...

class ParameterError(LiquersError): ...

class ParseError(LiquersError): ...

class SerializationError(LiquersError): ...

class StatusConflictError(LiquersError):
    #: The current token, when the conflict is an accessor's and the value still exists.
    token: str | None

class TooManyParametersError(LiquersError): ...

class UnexpectedError(LiquersError): ...

class UnknownCommandError(LiquersError): ...
