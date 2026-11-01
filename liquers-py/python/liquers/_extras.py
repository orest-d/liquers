"""The optional extras of the ``liquers-py`` package, and :func:`require`.

``pip install 'liquers-py[<extra>]'`` installs the Python packages one feature needs. Where a
package publishes no wheel for a platform, the extra's environment marker leaves it out, so an
install never falls back to building a third-party package from source; on such a platform the
extra is *not available*, and :func:`require` says so instead of advising an install.

``EXTRAS`` is the one table: ``liquers-py/pyproject.toml``'s ``[project.optional-dependencies]``
mirrors it, the record classes' error messages read it, and the wheel tests are parametrized
over it. Wheel availability was checked on PyPI on 2026-11-01
(``specs/design/liquers-py-package/phase2-architecture.md``).
"""

from __future__ import annotations

import importlib
import platform
import sys
import sysconfig
from typing import Callable, NamedTuple


class _Module(NamedTuple):
    module: str
    package: str
    unavailable: Callable[[], str | None]


def _windows_arm64() -> str | None:
    if sys.platform == "win32" and platform.machine().upper() == "ARM64":
        return "Windows arm64"
    return None


def _free_threaded() -> str | None:
    if sysconfig.get_config_var("Py_GIL_DISABLED"):
        return "a free-threaded interpreter"
    return None


def _always() -> str | None:
    return None


_POLARS = _Module("polars", "polars", _free_threaded)
_PIL = _Module("PIL", "pillow", _always)
_PYARROW = _Module("pyarrow", "pyarrow", _windows_arm64)
_PANDAS = _Module("pandas", "pandas", _always)

EXTRAS: dict[str, tuple[_Module, ...]] = {
    "polars": (_POLARS,),
    "images": (_PIL,),
    "arrow": (_PYARROW,),
    "pandas": (_PYARROW, _PANDAS),
}
"""Each extra's modules, in import order, with the platforms where its package has no wheel."""


def _unavailable(extra: str) -> str | None:
    """Why ``extra`` cannot be installed here, or ``None`` when it can."""
    for entry in EXTRAS[extra]:
        where = entry.unavailable()
        if where is not None:
            return f"{entry.package} has no wheel for {where}"
    return None


def require(extra: str) -> None:
    """Import the modules ``extra`` provides, or raise an error that says what to do.

    ``extra`` is one of ``"polars"``, ``"images"``, ``"arrow"``, ``"pandas"`` or ``"all"``.
    ``"all"`` imports every extra this platform can have. A missing module raises
    ``ModuleNotFoundError`` naming the module and the ``pip install`` that provides it; an extra
    this platform cannot have raises ``ModuleNotFoundError`` saying it is not available here; an
    unknown extra raises ``ValueError``.
    """
    if extra == "all":
        for name in EXTRAS:
            if _unavailable(name) is None:
                require(name)
        return
    if extra not in EXTRAS:
        known = ", ".join(sorted([*EXTRAS, "all"]))
        raise ValueError(f"unknown liquers-py extra {extra!r}; the extras are {known}")
    reason = _unavailable(extra)
    if reason is not None:
        raise ModuleNotFoundError(
            f"liquers-py[{extra}] is not available on this platform: {reason}"
        )
    for entry in EXTRAS[extra]:
        try:
            importlib.import_module(entry.module)
        except ModuleNotFoundError as error:
            raise ModuleNotFoundError(
                f"No module named {entry.module!r}; install it with "
                f"pip install 'liquers-py[{extra}]'",
                name=entry.module,
            ) from error
