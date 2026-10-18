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
``TypeError`` naming the parameter. The function is returned unchanged.
"""

import inspect
import marshal
import typing
import warnings

_SIMPLE_TYPES = {str: "string", int: "int", float: "float", bool: "bool"}
_OPTIONAL_TYPES = {int: "int_opt", float: "float_opt", bool: "bool_opt"}
_DEFAULT_TYPES = (str, int, float, bool)


def _argument_type(annotation):
    if annotation in _SIMPLE_TYPES:
        return _SIMPLE_TYPES[annotation]
    args = [a for a in typing.get_args(annotation) if a is not type(None)]
    if len(args) == 1 and type(None) in typing.get_args(annotation) and args[0] in _OPTIONAL_TYPES:
        return _OPTIONAL_TYPES[args[0]]
    return "any"


def _type_hints(fn):
    try:
        return typing.get_type_hints(fn)
    except Exception:  # a forward reference that does not resolve: fall back to no types
        return {}


def _implementation_bytes(fn):
    """What the command's ``impl_version`` hashes, or ``None`` for a callable with no code."""
    code = getattr(fn, "__code__", None)
    if code is None:
        return None
    try:
        return marshal.dumps((code.co_code, code.co_consts, code.co_names))
    except ValueError:
        return None


def _baseline(fn, name, source):
    """Stage 1: the parameters of ``fn`` as a declaration document.

    Returns the document and the index of the ``context`` parameter among the positional
    parameters, if there is one.
    """
    signature = inspect.signature(fn)
    hints = _type_hints(fn)
    arguments = [{"name": "none"}] if source else []
    context_index = None
    position = 0
    for parameter in signature.parameters.values():
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
        argument = {"name": parameter.name}
        if parameter.kind is inspect.Parameter.VAR_POSITIONAL:
            argument["multiple"] = True
        annotation = hints.get(parameter.name, inspect.Parameter.empty)
        if annotation is not inspect.Parameter.empty:
            argument["argument_type"] = _argument_type(annotation)
        default = parameter.default
        if default is not inspect.Parameter.empty and default is not None:
            if not isinstance(default, _DEFAULT_TYPES):
                raise TypeError(
                    f"command {name!r}: the default of {parameter.name!r} must be a str, int, "
                    f"float or bool, not {type(default).__name__}"
                )
            argument["default"] = default
        arguments.append(argument)
        position += 1
    document = {"name": name, "arguments": arguments}
    doc = inspect.getdoc(fn)
    if doc:
        document["doc"] = doc
    return document, context_index


def _register(env, fn, source, name, namespace, label, doc, filename, volatile, cached):
    name = name or fn.__name__
    baseline, context_index = _baseline(fn, name, source)
    declaration = {"namespace": namespace}
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
    replaced = env._register_command(
        fn, baseline, declaration, context_index, _implementation_bytes(fn)
    )
    if replaced != "nothing":
        where = f"{namespace}/{name}" if namespace else name
        warnings.warn(f"command {where!r} replaces a {replaced} command", stacklevel=3)
    return fn


def _decorator(source):
    def decorate(fn=None, *, name=None, namespace="", label=None, doc=None, filename=None,
                 volatile=None, cached=None, env=None):
        def apply(fn):
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


def _bound(env, source):
    decorate = _decorator(source)

    def bound(fn=None, **options):
        options.setdefault("env", env)
        return decorate(fn, **options)

    return bound
