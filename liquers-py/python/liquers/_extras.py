"""The optional extras of the ``liquers-py`` package, and :func:`require`.

``pip install 'liquers-py[<extra>]'`` installs the Python packages one feature needs. Where a
package publishes no wheel for a platform, the extra is *not available* there, and when its module
is missing :func:`require` says so instead of advising an install. A module that is importable is
used wherever it came from (built from source, say), so the check comes after the import.

Where an environment marker can name the platform (Windows arm64), the extra's marker leaves the
package out and the install builds nothing from source. No marker names a free-threaded
interpreter, so there ``[polars]`` and ``[all]`` would make pip build polars' runtime from its
sdist: leave them out (``specs/guides/PYTHON_GUIDE.md`` §1).

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
    """Why ``extra`` has no wheel here, or ``None`` when it has."""
    for entry in EXTRAS[extra]:
        where = entry.unavailable()
        if where is not None:
            return f"{entry.package} has no wheel for {where}"
    return None


def require(extra: str) -> None:
    """Import the modules ``extra`` provides, or raise an error that says what to do.

    ``extra`` is one of ``"polars"``, ``"images"``, ``"arrow"``, ``"pandas"`` or ``"all"``.
    ``"all"`` imports every extra, skipping one this platform has no wheel for when its module
    is missing. A missing module raises ``ModuleNotFoundError`` naming the module and the
    ``pip install`` that provides it, or, when the platform has no wheel for it, saying the extra
    is not available here; an unknown extra raises ``ValueError``.
    """
    if extra == "all":
        for name in EXTRAS:
            try:
                require(name)
            except ModuleNotFoundError:
                if _unavailable(name) is None:
                    raise
        return
    if extra not in EXTRAS:
        known = ", ".join(sorted([*EXTRAS, "all"]))
        raise ValueError(f"unknown liquers-py extra {extra!r}; the extras are {known}")
    for entry in EXTRAS[extra]:
        try:
            importlib.import_module(entry.module)
        except ModuleNotFoundError as error:
            reason = _unavailable(extra)
            if reason is not None:
                raise ModuleNotFoundError(
                    f"liquers-py[{extra}] is not available on this platform: {reason}",
                    name=entry.module,
                ) from None
            raise ModuleNotFoundError(
                f"No module named {entry.module!r}; install it with "
                f"pip install 'liquers-py[{extra}]'",
                name=entry.module,
            ) from error
