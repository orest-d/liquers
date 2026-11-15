"""The native extension of Liquers: the classes and functions implemented in Rust.

Private: import `liquers` instead. This package exists because maturin builds the compiled module
inside a Python package when the project has Python sources; it re-exports that module,
`_liquers._liquers`, unchanged.

# Main components
- Every class and function that `liquers-py/src/lib.rs` registers, with `__all__` as the native
  module lists it (`__version__` and, in a `debug-handles` build, the live-handle counts included).

# Where it is used
- `liquers._backend` imports every native name from here; it is the only module that does.

# Related concepts
- The public modules of `liquers` (`liquers.query`, `liquers.environment`, ...) re-export these
  names; each class reports its public module in `__module__`, not `_liquers`.
- A pure-Python fallback (`PY-HAS-NO-PURE-PYTHON-FALLBACK`) would replace this package behind
  `liquers._backend`.

# Examples
>>> import _liquers, liquers
>>> _liquers.Key is liquers.Key
True
"""

from . import _liquers as _extension
from ._liquers import *  # noqa: F401,F403 - the whole native module, re-exported

__all__ = list(_extension.__all__)
