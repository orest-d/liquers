"""DATANODE: Python data read in place (specs/design/liquers-py-data-nodes/, AC-1 to AC-6, AC-9)."""

import collections
import datetime
import enum

import pytest

import liquers as lq


def test_nested_mapping():
    """AC-1: kinds, fields, items, lengths and keys follow the mapping table."""
    node = lq.data_node({"a": [1, {"b": "x"}], "c": None})
    assert node.kind == lq.NodeKind.Map
    assert node.keys() == ["a", "c"]
    a = node["a"]
    assert a.kind == lq.NodeKind.List and len(a) == 2
    assert a[0].scalar == 1 and a[0].kind == lq.NodeKind.Integer
    assert a[1]["b"].scalar == "x"
    assert node["c"].kind == lq.NodeKind.Null and node["c"].scalar is None
    assert node.get("missing") is None
    with pytest.raises(KeyError):
        node["missing"]
    with pytest.raises(IndexError):
        a[5]
    assert [item.scalar for item in a][:1] == [1]
    assert list(node) == ["a", "c"]


CORPUS = [
    {"a": 1, "b": [1.5, "x", True, None], "c": {"d": []}},
    [1, 2, {"x": "y"}],
    ("t", 1),
    "text",
    3,
    None,
]


@pytest.mark.parametrize("data", CORPUS)
def test_adapter_agrees_with_conversion(env, data):
    """AC-2: the adapter's JSON equals the converted value's JSON."""

    @env.first_command
    def cmd():
        return data

    direct = lq.data_node(data)
    converted = lq.data_node(env.evaluate_state("cmd"))
    assert direct.to_json() == converted.to_json()
    if direct.kind == lq.NodeKind.Map:
        assert sorted(direct.keys()) == sorted(converted.keys())


def test_scalar_edges():
    """AC-3: bool, i64 edges, NaN."""
    assert lq.data_node(True).kind == lq.NodeKind.Bool
    assert lq.data_node(2**63 - 1).scalar == 2**63 - 1
    for big in (2**63, 2**64):
        node = lq.data_node(big)
        assert node.kind == lq.NodeKind.Opaque
        with pytest.raises(lq.ConversionError):
            node.to_json()
        with pytest.raises(lq.ConversionError):
            lq.data_node({"n": big}).to_json()
    nan = lq.data_node(float("nan"))
    assert nan.kind == lq.NodeKind.Float
    with pytest.raises(lq.ConversionError):
        nan.to_json()


def test_bytes_are_opaque():
    """AC-4."""
    for data in (b"abc", bytearray(b"abc"), memoryview(b"abc")):
        assert lq.data_node(data).kind == lq.NodeKind.Opaque


def test_refused_structure_still_reads():
    """AC-5: a dict conversion refuses still reads, with only the bad member opaque."""
    node = lq.data_node({"name": "Ann", "when": datetime.datetime.now(), 1: "one"})
    assert node["name"].scalar == "Ann"
    assert node["when"].kind == lq.NodeKind.Opaque
    assert node.keys() == ["name", "when"]


def test_cycles_navigate_and_refuse_json():
    """AC-6."""
    d = {"name": "loop"}
    d["self"] = d
    node = lq.data_node(d)
    assert node["self"]["self"]["self"]["name"].scalar == "loop"
    with pytest.raises(lq.ConversionError):
        node.to_json()


def test_view_is_live():
    """AC-9: a change between two lookups is seen."""
    d = {"x": 1}
    node = lq.data_node(d)
    assert node["x"].scalar == 1
    d["x"] = 2
    assert node["x"].scalar == 2


def test_exact_types_only():
    """Scalar subclasses read as their base; container subclasses do not."""

    class Level(enum.IntEnum):
        HIGH = 3

    assert lq.data_node(Level.HIGH).scalar == 3
    assert lq.data_node(collections.OrderedDict(a=1)).kind == lq.NodeKind.Opaque


def test_to_json_names_a_non_str_key():
    with pytest.raises(lq.ConversionError, match="7"):
        lq.data_node({7: "x"}).to_json()
