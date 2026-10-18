"""OBJECT: the wrapper objects (LANGUAGE-INTEGRATION_GUIDE.md, Appendix A)."""

import inspect

import pytest

import liquers as lq


def test_OBJECT01_query_parse_encode_roundtrip():
    for text in ["hello/repeat-3", "-R/d/in.txt/-/repeat-2", "make_frame/ns-pl/head-2"]:
        assert lq.parse(text).encode() == text


def test_OBJECT02_key_equality_and_hash():
    first, second = lq.parse_key("a/b"), lq.parse_key("a/b")
    assert first == second and hash(first) == hash(second)
    assert len({first, second, lq.parse_key("a/c")}) == 2


def test_OBJECT03_command_metadata_roundtrip(env):
    # Compared as JSON: `metadata_version` is derived, not serialized, so a deserialized copy
    # carries zero until a registry refreshes it.
    json = env.command_metadata("hello").to_json()
    assert lq.CommandMetadata.from_json(json).to_json() == json
    assert lq.CommandMetadata.from_yaml(lq.CommandMetadata.from_json(json).to_yaml()).to_json() == json


def test_OBJECT04_invalid_parse_produces_error():
    with pytest.raises(lq.ParseError) as raised:
        lq.parse("a/b-~")
    assert raised.value.type == lq.ErrorType.ParseError
    assert raised.value.position is not None


def test_OBJECT05_wrapper_valid_for_documented_lifetime(env):
    meta = env.command_metadata("hello")
    state = env.evaluate_state("hello")
    query = lq.parse("hello/to_text")
    env.shutdown()
    del env
    assert meta.label == "Hello"
    assert state.value == "hello"
    assert query.encode() == "hello/to_text"


def _members(enum):
    return [getattr(enum, n) for n in dir(enum) if isinstance(getattr(enum, n), enum)]


def test_OBJECT06_every_enum_variant_roundtrips():
    for member in _members(lq.ErrorType):
        assert lq.LiquersError(member, "m").type == member
    for kind in ["debug", "info", "warning", "error"]:
        assert lq.LogEntryKind(kind) == lq.LogEntryKind(kind)
    for name in ["Ready", "Cancelled", "Volatile"]:
        assert lq.Status(name).name() == name


def test_OBJECT07_unknown_enum_variant_follows_policy():
    with pytest.raises(ValueError):
        lq.Status("VariantFromAFutureRelease")
    with pytest.raises(ValueError):
        lq.LogEntryKind("VariantFromAFutureRelease")


def test_OBJECT08_wrappers_follow_naming_and_ownership_conventions():
    for name in lq.__all__:
        assert not name.startswith("_")
        exported = getattr(lq, name)
        if inspect.isclass(exported):
            assert exported.__module__ == "liquers", f"{name} reports {exported.__module__}"


def test_OBJECT09_any_parameter_value_round_trips():
    values = ["two words", "12:30", "a,b", "a?b", "café", "日本", "-5", "~X~",
              "a/b", "\U0001F600"]
    for value in values:
        encoded = lq.ActionRequest.from_arguments("filter", [value]).encode()
        assert lq.parse(encoded).action().to_list() == ["filter", value]
