"""Declarations of store and recipe provider factories, derived from their signatures.

A factory is any callable that is not ``async def``: a function, a class, a ``functools.partial``.
``Environment.register_store_factory`` and ``Environment.register_recipe_provider_factory`` call
``_declare`` to turn its signature into the argument list the configuration is checked against:

* keyword and positional-or-keyword parameters become arguments; one with a default is optional,
  and the default must be JSON (``None``, ``bool``, ``int``, ``float``, ``str``, ``list``,
  ``dict``);
* the type comes from the annotation (``str``, ``bool``, ``int``/``float``, ``list``, ``dict``;
  ``bool`` is checked before ``int``), else from the default, else it is ``any``;
* ``**kwargs`` accepts unlisted keys (the argument coverage becomes partial);
* positional-only parameters and ``*args`` are refused, since a configuration entry has only
  names;
* names in ``exempt`` are passed by liquers rather than taken from the configuration (``prefix``
  for a store factory), and names in ``reserved`` are refused (``type``).

A callable whose signature cannot be inspected (some builtins) needs ``arguments=``, a list of
``lq.StoreArgumentInfo``. See ``specs/reference/PYTHON_INTEGRATION.md`` §Stores.
"""

import inspect
import json
import typing
from collections.abc import Callable, Collection, Iterable
from typing import Any

_JSON_SCALARS = (type(None), bool, int, float, str)


def _is_json(value: object) -> bool:
    if isinstance(value, _JSON_SCALARS):
        return True
    if isinstance(value, (list, tuple)):
        return all(_is_json(v) for v in value)
    if isinstance(value, dict):
        return all(isinstance(k, str) and _is_json(v) for k, v in value.items())
    return False


def _type_of_annotation(annotation: Any) -> str | None:
    if annotation is inspect.Parameter.empty or annotation is typing.Any:
        return None
    origin = typing.get_origin(annotation) or annotation
    if origin is bool:
        return "boolean"
    if origin in (int, float):
        return "number"
    if origin is str:
        return "string"
    if origin in (list, tuple):
        return "array"
    if origin is dict:
        return "object"
    return "any"


def _type_of_value(value: object) -> str:
    if isinstance(value, bool):
        return "boolean"
    if isinstance(value, (int, float)):
        return "number"
    if isinstance(value, str):
        return "string"
    if isinstance(value, (list, tuple)):
        return "array"
    if isinstance(value, dict):
        return "object"
    return "any"


def _declare(
    factory: Callable[..., Any],
    arguments: Iterable[Any] | None = None,
    *,
    exempt: Collection[str] = (),
    reserved: Collection[str] = (),
) -> tuple[str, bool, list[str]]:
    """The declaration of ``factory``: ``(arguments_json, takes_kwargs, exempt_taken)``.

    ``arguments_json`` is a JSON list of argument records (the fields of ``lq.StoreArgumentInfo``);
    ``exempt_taken`` lists the names of ``exempt`` the factory accepts. Raises ``TypeError`` or
    ``ValueError`` naming the parameter at fault.
    """
    if not callable(factory):
        raise TypeError(f"a factory must be callable, not {type(factory).__name__}")
    if inspect.iscoroutinefunction(factory):
        raise TypeError("a factory may not be `async def`: it is called while the environment is built")
    try:
        signature = inspect.signature(factory)
    except (TypeError, ValueError):
        signature = None
    if arguments is not None:
        declared = [json.loads(a.to_json()) for a in arguments]
        for entry in declared:
            if entry["name"] in reserved:
                raise ValueError(f"'{entry['name']}' is reserved and cannot be a factory argument")
        if signature is None:
            return json.dumps(declared), True, [n for n in exempt]
        kwargs = any(p.kind is p.VAR_KEYWORD for p in signature.parameters.values())
        return json.dumps(declared), kwargs, [n for n in exempt if n in signature.parameters]
    if signature is None:
        raise TypeError(
            f"the signature of {factory!r} cannot be inspected; declare its arguments with arguments=[lq.StoreArgumentInfo(...)]"
        )
    records: list[dict[str, Any]] = []
    takes_kwargs = False
    taken: list[str] = []
    for name, parameter in signature.parameters.items():
        if parameter.kind is parameter.VAR_KEYWORD:
            takes_kwargs = True
            continue
        if parameter.kind is parameter.VAR_POSITIONAL:
            raise ValueError(f"a factory may not take *{name}: a configuration entry has only names")
        if parameter.kind is parameter.POSITIONAL_ONLY:
            raise ValueError(f"parameter '{name}' is positional-only: a configuration entry passes arguments by name")
        if name in exempt:
            taken.append(name)
            continue
        if name in reserved:
            raise ValueError(f"parameter '{name}' is reserved and cannot be a factory argument")
        record: dict[str, Any] = {"name": name, "label": name, "doc": "", "required": parameter.default is parameter.empty}
        argument_type = _type_of_annotation(parameter.annotation)
        if parameter.default is not parameter.empty:
            if not _is_json(parameter.default):
                raise ValueError(
                    f"the default of parameter '{name}' is a {type(parameter.default).__name__}, not a JSON value"
                )
            if parameter.default is not None:
                record["default"] = parameter.default
            if argument_type is None:
                argument_type = _type_of_value(parameter.default)
        record["argument_type"] = argument_type or "any"
        records.append(record)
    return json.dumps(records), takes_kwargs, taken
