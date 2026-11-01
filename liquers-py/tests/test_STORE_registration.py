"""Store factory declaration, late registration and release (design liquers-py-store, AC-15, AC-16)."""

import functools
import json

import pytest

import liquers as lq
from liquers._factories import _declare


def _arguments(factory, **options):
    arguments, takes_kwargs, taken = _declare(factory, options.pop("arguments", None),
                                              exempt=("prefix",), reserved=("type",))
    return {a["name"]: a for a in json.loads(arguments)}, takes_kwargs, taken


def test_STORE_declare_from_signature():
    # AC-16: the declaration comes from the factory's keyword parameters.
    def notes(prefix, greeting: str = "Hi", flag: bool = False, count: int = 1, root=None, tags=[]):
        return None

    arguments, takes_kwargs, taken = _arguments(notes)
    assert taken == ["prefix"] and not takes_kwargs
    assert list(arguments) == ["greeting", "flag", "count", "root", "tags"]
    assert arguments["greeting"] == {"name": "greeting", "label": "greeting", "doc": "",
                                     "required": False, "default": "Hi", "argument_type": "string"}
    assert arguments["flag"]["argument_type"] == "boolean"      # bool before int
    assert arguments["count"]["argument_type"] == "number"
    assert arguments["root"]["argument_type"] == "any" and "default" not in arguments["root"]
    assert arguments["tags"]["argument_type"] == "array"

    def required(path: str, **rest):
        return None

    arguments, takes_kwargs, taken = _arguments(required)
    assert arguments["path"]["required"] and takes_kwargs and taken == []

    class Factory:                                  # a class is a factory like a function
        def __init__(self, folder: str):
            self.folder = folder

    assert list(_arguments(Factory)[0]) == ["folder"]
    assert list(_arguments(functools.partial(required, "x"))[0]) == []

    for refused in [lambda a, /: None, lambda *a: None, lambda type: None, lambda x=object(): None]:
        with pytest.raises((TypeError, ValueError)):
            _arguments(refused)

    async def coroutine_factory():
        return None

    with pytest.raises(TypeError):
        _arguments(coroutine_factory)


def test_STORE_declare_with_explicit_arguments():
    # AC-16: `arguments=` declares what a signature cannot (here, an uninspectable callable).
    info = lq.StoreArgumentInfo("bucket", required=True, doc="The bucket")
    assert (info.name, info.argument_type, info.required, info.default, info.label) == \
        ("bucket", "string", True, None, "bucket")
    optional = lq.StoreArgumentInfo("depth", "number", default=3)
    assert optional.default == 3 and not optional.required
    with pytest.raises(ValueError):
        lq.StoreArgumentInfo("x", "integer")
    arguments, takes_kwargs, _ = _arguments(dict, arguments=[info, optional])
    assert list(arguments) == ["bucket", "depth"] and arguments["depth"]["default"] == 3
    with pytest.raises(ValueError):
        _arguments(dict, arguments=[lq.StoreArgumentInfo("type")])
