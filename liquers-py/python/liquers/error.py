"""Errors: the exception classes Liquers raises, one per error type, and the native error value.

Rust counterpart: `liquers_core::error`. Every name here is also available directly from `liquers`
(`liquers.Error is liquers.error.Error`); `import liquers as lq` and `lq.Error` stays
the usual style.

# Main components
- `LiquersError`: the base class of every Liquers exception.
- One subclass per `ErrorType` variant (`KeyNotFoundError`, `ExecutionError`, …) and one class per category (`StoreError`, `NotFoundError`, …), generated when the module is imported.
- `ErrorType`, `Error`: the native error type and value.

# Where it is used
- Every failing query raises a subclass of `LiquersError`.

# Related concepts
- A Python exception raised in a command travels back with its traceback.

# Examples
>>> import liquers as lq
>>> from liquers.error import KeyNotFoundError, LiquersError
>>> issubclass(KeyNotFoundError, LiquersError)
True
"""

from liquers._backend import Error, ErrorType
from liquers._errors import LiquersError, _install

# One LiquersError subclass per ErrorType variant (KeyNotFoundError, ExecutionError, ...), and one
# base class per error category (StoreError, KeyedAssetError, NotFoundError).
_ERROR_SUBCLASSES = _install(ErrorType)
globals().update(_ERROR_SUBCLASSES)

__all__ = ["Error", "ErrorType", "LiquersError", *sorted(_ERROR_SUBCLASSES)]
