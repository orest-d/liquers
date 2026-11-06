"""DATANODE: the `__liquers_data__` opt-in (AC-7, AC-13)."""

import dataclasses

import pytest

import liquers as lq

CALLS = []


class Recording:
    def keys(self):
        CALLS.append("keys")
        return ["a"]

    def __getitem__(self, key):
        CALLS.append(key)
        return 1


class OptedIn(Recording):
    __liquers_data__ = True


def test_only_opted_in_code_runs():
    """AC-7: a dict subclass and an unmarked class are opaque, and none of their code runs."""
    CALLS.clear()

    class Sub(dict):
        pass

    assert lq.data_node({"a": 1}).kind == lq.NodeKind.Map
    assert lq.data_node(Sub(a=1)).kind == lq.NodeKind.Opaque
    assert lq.data_node(Recording()).kind == lq.NodeKind.Opaque
    assert CALLS == []
    node = lq.data_node(OptedIn())
    assert node.kind == lq.NodeKind.Map and node["a"].scalar == 1
    # Only the opted-in class's lookups ran (the view looks a field up on each read).
    assert CALLS and set(CALLS) == {"a"}


class Seq:
    __liquers_data__ = True

    def __len__(self):
        return 2

    def __getitem__(self, i):
        if i >= 2:
            raise IndexError(i)
        return i * 10


class Idx:
    __liquers_data__ = True

    def __index__(self):
        return 7


class Flt:
    __liquers_data__ = True

    def __float__(self):
        return 2.5


def test_opted_in_protocols():
    """AC-13: sequence, __index__ and __float__."""
    seq = lq.data_node(Seq())
    assert seq.kind == lq.NodeKind.List and len(seq) == 2 and seq[1].scalar == 10
    assert lq.data_node(Idx()).scalar == 7
    assert lq.data_node(Flt()).scalar == 2.5


class Raising:
    __liquers_data__ = True

    def keys(self):
        return ["good", "bad"]

    def __getitem__(self, key):
        if key == "bad":
            raise ValueError("no")
        if key == "good":
            return "yes"
        raise KeyError(key)


def test_raising_entry_is_opaque():
    """AC-13: the raising entry is opaque; the others read; a missing one is None."""
    node = lq.data_node(Raising())
    assert node["good"].scalar == "yes"
    assert node["bad"].kind == lq.NodeKind.Opaque
    assert node.get("other") is None
    with pytest.raises(lq.LiquersError):
        node.to_json()


@dataclasses.dataclass
class Person:
    name: str

    def __liquers_data__(self):
        return dataclasses.asdict(self)


class Wrapper:
    """Its method returns an object that itself opts in by method: not re-entered."""

    def __liquers_data__(self):
        return Person("Ann")


def test_method_result_is_not_reentered():
    assert lq.data_node(Person("Ann"))["name"].scalar == "Ann"
    assert lq.data_node(Wrapper()).kind == lq.NodeKind.Opaque


def test_opt_in_is_read_from_the_type():
    class Plain(dict):
        pass

    obj = Plain(a=1)
    obj.__liquers_data__ = True
    assert lq.data_node(obj).kind == lq.NodeKind.Opaque


def test_method_form_is_a_snapshot():
    calls = []

    class Snap:
        def __liquers_data__(self):
            calls.append(1)
            return {"x": len(calls)}

    node = lq.data_node(Snap())
    assert node["x"].scalar == 1 and node["x"].scalar == 1
    assert calls == [1]


def test_raising_protocols_are_opaque():
    class BadMethod:
        def __liquers_data__(self):
            raise RuntimeError("boom")

    class BadIndex:
        __liquers_data__ = True

        def __index__(self):
            raise RuntimeError("boom")

    class BadFloat:
        __liquers_data__ = True

        def __float__(self):
            raise RuntimeError("boom")

    for obj in (BadMethod(), BadIndex(), BadFloat()):
        node = lq.data_node(obj)
        assert node.kind == lq.NodeKind.Opaque
        with pytest.raises(lq.LiquersError, match="boom"):
            node.to_json()


def test_protocol_map_json_rejects_a_non_str_key():
    """An opted-in mapping with a key JSON cannot hold fails `to_json`, as a dict does, rather
    than dropping the entry."""

    class Mapping(dict):
        __liquers_data__ = True

    node = lq.data_node(Mapping({7: "x", "a": 1}))
    with pytest.raises(lq.ConversionError, match="key 7 is not a str"):
        node.to_json()
    assert lq.data_node(Mapping({"a": 1})).to_json() == {"a": 1}
