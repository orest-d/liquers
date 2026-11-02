"""Registering Python callables as Liquers commands.

``command`` and ``first_command`` turn a function into a command of an environment::

    @lq.command
    def repeat(text: str, count: int = 2) -> str:
        '''Repeat the input text.'''
        return text * count

    lq.evaluate("hello/repeat-3")

The decorator reads the function (stage 1 of the declaration pipeline,
``specs/reference/COMMAND_DECLARATION.md``) and hands Rust a *baseline* document of every
parameter, plus the keyword arguments as the author's *declaration*. Rust runs the shared stages
2-5, so a Python command follows the same conventions as ``register_command!``:

* the first parameter is the state, and its name chooses the delivery: ``state`` gives an
  ``lq.State``, ``text`` the state's text, anything else its value. ``first_command`` takes no
  state, so its first parameter is an ordinary argument;
* a parameter named ``context`` receives an ``lq.Context`` and is not a query argument;
* ``*args`` becomes a ``multiple`` argument.

A keyword-only parameter or ``**kwargs`` has no place in a positional query, so it is refused with
``TypeError`` naming the parameter; ``context`` after ``*args`` is keyword-only too, and is refused
the same way. A default must be ``None``, a ``str``, an ``int``, a finite ``float`` or a ``bool``.
The function is returned unchanged.
"""

import inspect
import marshal
import math
import os
import sys
import typing
import warnings
from collections.abc import Callable
from typing import Any

from liquers._native import Environment

_SIMPLE_TYPES = {str: "string", int: "int", float: "float", bool: "bool"}
_OPTIONAL_TYPES = {int: "int_opt", float: "float_opt", bool: "bool_opt"}
_DEFAULT_TYPES = (str, int, float, bool)


def _argument_type(annotation: Any) -> str:
    if annotation in _SIMPLE_TYPES:
        return _SIMPLE_TYPES[annotation]
    args = [a for a in typing.get_args(annotation) if a is not type(None)]
    if len(args) == 1 and type(None) in typing.get_args(annotation) and args[0] in _OPTIONAL_TYPES:
        return _OPTIONAL_TYPES[args[0]]
    return "any"


def _type_hints(fn: Callable[..., Any]) -> dict[str, Any]:
    try:
        return typing.get_type_hints(fn)
    except Exception:  # a forward reference that does not resolve: fall back to no types
        return {}


def _implementation_bytes(fn: Callable[..., Any]) -> bytes | None:
    """What the command's ``impl_version`` hashes, or ``None`` when the code does not decide it.

    ``None`` gives the command a fresh version on every registration. That is the answer for a
    callable with no code object, and for a closure or a bound method: there the result also
    depends on captured cells or on ``self``, which the code object does not show, so hashing the
    code alone could reuse a result computed from other captured values.
    """
    if getattr(fn, "__self__", None) is not None or getattr(fn, "__closure__", None):
        return None
    code = getattr(fn, "__code__", None)
    if code is None:
        return None
    try:
        return marshal.dumps((code.co_code, code.co_consts, code.co_names))
    except ValueError:
        return None


def _baseline(
    fn: Callable[..., Any], name: str, source: bool
) -> tuple[dict[str, Any], int | None]:
    """Stage 1: the parameters of ``fn`` as a declaration document.

    Returns the document and the index of the ``context`` parameter among the positional
    parameters, if there is one.
    """
    try:
        signature = inspect.signature(fn)
    except (TypeError, ValueError) as e:
        raise TypeError(f"command {name!r}: cannot read the parameters of {fn!r}: {e}") from None
    hints = _type_hints(fn)
    arguments: list[dict[str, Any]] = [{"name": "none"}] if source else []
    context_index: int | None = None
    position = 0
    for parameter in signature.parameters.values():
        if parameter.kind is inspect.Parameter.KEYWORD_ONLY and parameter.name == "context":
            raise TypeError(
                f"command {name!r}: 'context' after *args is keyword-only and cannot be passed; "
                "put it before *args"
            )
        if parameter.kind is inspect.Parameter.KEYWORD_ONLY:
            raise TypeError(
                f"command {name!r}: keyword-only parameter {parameter.name!r} cannot be passed "
                "from a query; make it positional"
            )
        if parameter.kind is inspect.Parameter.VAR_KEYWORD:
            raise TypeError(
                f"command {name!r}: **{parameter.name} cannot be passed from a query"
            )
        if parameter.name == "context":
            context_index = position
        argument: dict[str, Any] = {"name": parameter.name}
        if parameter.kind is inspect.Parameter.VAR_POSITIONAL:
            argument["multiple"] = True
        annotation = hints.get(parameter.name, inspect.Parameter.empty)
        if annotation is not inspect.Parameter.empty:
            argument["argument_type"] = _argument_type(annotation)
        default = parameter.default
        if default is not inspect.Parameter.empty:
            if default is not None and not isinstance(default, _DEFAULT_TYPES):
                raise TypeError(
                    f"command {name!r}: the default of {parameter.name!r} must be None, a str, "
                    f"int, float or bool, not {type(default).__name__}"
                )
            if isinstance(default, float) and not math.isfinite(default):
                raise TypeError(
                    f"command {name!r}: the default of {parameter.name!r} must be finite, "
                    f"not {default!r}"
                )
            argument["default"] = default
        arguments.append(argument)
        position += 1
    document: dict[str, Any] = {"name": name, "arguments": arguments}
    doc = inspect.getdoc(fn)
    if doc:
        document["doc"] = doc
    return document, context_index


_PACKAGE_DIR = os.path.dirname(os.path.abspath(__file__))


def _caller_stacklevel() -> int:
    """The ``stacklevel`` that points a warning at the first frame outside this package."""
    level = 1
    frame: Any = sys._getframe(1)
    while frame is not None and os.path.dirname(os.path.abspath(frame.f_code.co_filename)) == _PACKAGE_DIR:
        frame = frame.f_back
        level += 1
    return level


def _register(
    env: Environment,
    fn: Callable[..., Any],
    source: bool,
    name: str | None,
    namespace: str,
    label: str | None,
    doc: str | None,
    filename: str | None,
    volatile: bool | None,
    cached: bool | None,
) -> Callable[..., Any]:
    if name is None:
        name = getattr(fn, "__name__", None)
        if not isinstance(name, str):
            raise TypeError(
                f"{fn!r} has no __name__; give the command a name with name=..."
            )
    baseline, context_index = _baseline(fn, name, source)
    declaration: dict[str, Any] = {"namespace": namespace}
    if label is not None:
        declaration["label"] = label
    if doc is not None:
        declaration["doc"] = doc
    if filename is not None:
        declaration["filename"] = filename
    if volatile is not None:
        declaration["volatile"] = volatile
    if cached is not None:
        declaration["cached"] = cached
    replaced, messages = env._register_command(
        fn, baseline, declaration, context_index, _implementation_bytes(fn)
    )
    for message in messages:
        warnings.warn(message, stacklevel=_caller_stacklevel())
    if replaced != "nothing":
        where = f"{namespace}/{name}" if namespace else name
        warnings.warn(
            f"command {where!r} replaces a {replaced} command", stacklevel=_caller_stacklevel()
        )
    return fn


def _decorator(source: bool) -> Callable[..., Any]:
    def decorate(
        fn: Callable[..., Any] | None = None,
        *,
        name: str | None = None,
        namespace: str = "",
        label: str | None = None,
        doc: str | None = None,
        filename: str | None = None,
        volatile: bool | None = None,
        cached: bool | None = None,
        env: Environment | None = None,
    ) -> Any:
        def apply(fn: Callable[..., Any]) -> Callable[..., Any]:
            from liquers._env import default_environment

            target = env if env is not None else default_environment()
            return _register(
                target, fn, source, name, namespace, label, doc, filename, volatile, cached
            )

        return apply(fn) if fn is not None else apply

    return decorate


command = _decorator(source=False)
command.__doc__ = """Registers a function whose first parameter is the state (``@lq.command``)."""

first_command = _decorator(source=True)
first_command.__doc__ = """Registers a source function: it takes no state (``@lq.first_command``)."""


def _bound(env: Environment, source: bool) -> Callable[..., Any]:
    decorate = _decorator(source)

    def bound(fn: Callable[..., Any] | None = None, **options: Any) -> Any:
        options.setdefault("env", env)
        return decorate(fn, **options)

    return bound
