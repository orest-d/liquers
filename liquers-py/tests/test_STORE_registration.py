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


def _build_error(env, query="-R/n/x.txt", timeout=10):
    """The error the first use raises, failing the test instead of hanging past ``timeout``."""
    import threading

    outcome = {}

    def run():
        try:
            env.evaluate(query)
        except BaseException as e:  # reported on the test's thread
            outcome["error"] = e

    thread = threading.Thread(target=run, daemon=True)
    thread.start()
    thread.join(timeout)
    assert not thread.is_alive(), "the build hung"
    return outcome.get("error")


def test_STORE_type_declaration_and_failures():
    # AC-16: the type is declared, and each failure is a ConfigurationError naming the cause.
    from store_fixtures import Notes

    def notes(prefix, greeting: str = "Hi"):
        return Notes({f"{prefix}/hello.txt": greeting.encode()})

    env = lq.Environment()
    env.register_store_factory("notes", notes, label="Notes", doc="Notes kept in a dict")
    declared = {t["store_type"]: t for t in env.store_types()}["notes"]
    assert declared["label"] == "Notes" and declared["doc"] == "Notes kept in a dict"
    assert [(a["name"], a["argument_type"], a["required"], a.get("default")) for a in declared["arguments"]] \
        == [("greeting", "string", False, "Hi")]
    for taken in ["memory", "fs", "opendal_fs", "notes"]:
        with pytest.raises(lq.ConfigurationError):
            env.register_store_factory(taken, notes)
    env.shutdown()

    def needs(path: str):
        return Notes({})

    def raises():
        raise ValueError("no backend")

    async def coroutine_factory():
        return Notes({})

    def coroutine_returning():
        return coroutine_factory()

    holder = {}

    def uses_environment():
        holder["env"].evaluate("-R/n/hello.txt")
        return Notes({})

    cases = [
        ({"type": "needs", "prefix": "n"}, needs, "required argument 'path'"),
        ({"type": "needs", "prefix": "n", "config": {"path": "p", "extra": 1}}, needs, "'extra'"),
        ({"type": "needs", "prefix": "n"}, raises, "no backend"),
        ({"type": "needs", "prefix": "n"}, coroutine_returning, "awaitable"),
        ({"type": "needs", "prefix": "n"}, uses_environment, "may not use its environment"),
    ]
    for entry, factory, expected in cases:
        env = lq.Environment(config={"store": {"stores": [entry]}})
        holder["env"] = env
        env.register_store_factory("needs", factory)
        error = _build_error(env)
        assert isinstance(error, lq.ConfigurationError), (expected, error)
        assert expected in str(error) and "'needs'" in str(error), (expected, str(error))
        env.shutdown()
    error_with_traceback = None
    env = lq.Environment(config={"store": {"stores": [{"type": "needs", "prefix": "n"}]}})
    env.register_store_factory("needs", raises)
    error_with_traceback = _build_error(env)
    assert "raises" in (error_with_traceback.traceback or "")
    env.shutdown()


def test_STORE_late_registration_and_release():
    # AC-15: (a) a factory registered after a failed build lets the next use build.
    from store_fixtures import Notes

    assert hasattr(lq.Environment, "live_handle_count"), "build with --features debug-handles"
    baseline = lq.Environment.live_handle_count()
    env = lq.Environment(config={"store": {"stores": [
        {"type": "notes", "prefix": "n"}, {"type": "memory", "prefix": "scratch"}]}})
    with pytest.raises(lq.LiquersError):
        env.evaluate("-R/n/hello.txt")
    env.register_store_factory("notes", lambda prefix: Notes({f"{prefix}/hello.txt": b"Hello"}))
    assert env.evaluate("-R/n/hello.txt") == "Hello"
    # (b) after a use and a write, a late command and a late factory rebuild with the store kept.
    env.store.set("scratch/kept.txt", b"kept")

    @env.command
    def late(text):
        return text.upper()

    env.register_store_factory("other", lambda: Notes({}))
    assert env.evaluate("-R/scratch/kept.txt/-/late") == "KEPT"
    held = env.store
    assert lq.Environment.live_handle_count() > baseline
    env.shutdown()
    assert lq.Environment.live_handle_count() == baseline
    with pytest.raises(lq.LiquersError):
        held.get("scratch/kept.txt")
