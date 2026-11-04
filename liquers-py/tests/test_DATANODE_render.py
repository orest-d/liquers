"""DATANODE: retained Python values read in place by templates (AC-8, AC-10)."""

import dataclasses
import datetime
import threading

import pytest

import liquers as lq

QUERY = "person/ns-template/render-~X~-R/templates/name.md~E"


class Mapping:
    __liquers_data__ = True

    def keys(self):
        return ["name"]

    def __getitem__(self, key):
        if key == "name":
            return "Ann"
        raise KeyError(key)


@dataclasses.dataclass
class Person:
    name: str
    when: datetime.datetime

    def __liquers_data__(self):
        return {"name": self.name}


@pytest.mark.parametrize(
    "make",
    [
        lambda: {"name": "Ann", "when": datetime.datetime.now(), 1: "one"},
        Mapping,
        lambda: Person("Ann", datetime.datetime.now()),
    ],
    ids=["dict", "opted-in-mapping", "dataclass-method"],
)
def test_retained_value_renders_in_place(env, make):
    """AC-8: the three forms all render "Ann"."""
    env.store_set("templates/name.md", "{{name}}")

    @env.first_command
    def person() -> object:
        return make()

    assert env.evaluate(QUERY) == "Ann"


def test_worker_lookup_does_not_deadlock(env):
    """AC-10: a render reading a Python dict while the caller waits in evaluate."""
    env.store_set("templates/name.md", "{{name}}")

    @env.first_command
    def person() -> object:
        return {"name": "Ann", "when": datetime.datetime.now()}

    result = {}
    thread = threading.Thread(target=lambda: result.setdefault("v", env.evaluate(QUERY)))
    thread.start()
    thread.join(5)
    assert not thread.is_alive() and result["v"] == "Ann"


def test_reentrant_getitem_completes(env):
    """AC-10: an opted-in __getitem__ that evaluates a query mid-render."""
    env.store_set("templates/name.md", "{{name}}")

    class Reentrant:
        __liquers_data__ = True

        def keys(self):
            return ["name"]

        def __getitem__(self, key):
            if key == "name":
                return env.evaluate("hello")
            raise KeyError(key)

    @env.first_command
    def person() -> object:
        return Reentrant()

    result = {}
    thread = threading.Thread(target=lambda: result.setdefault("v", env.evaluate(QUERY)))
    thread.start()
    thread.join(5)
    assert not thread.is_alive() and result["v"] == "hello"
