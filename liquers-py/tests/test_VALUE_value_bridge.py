"""VALUE: the value bridge (LANGUAGE-INTEGRATION_GUIDE.md)."""

import pytest

import liquers as lq
from conftest import Opaque, na


@pytest.fixture
def echo(env):
    @env.first_command(volatile=True)
    def emit(n: int = 0):
        return PAYLOADS[n]

    return env


PAYLOADS = []


def through(env, value):
    """Returns ``value`` after it crossed into Liquers and back out."""
    PAYLOADS.clear()
    PAYLOADS.append(value)
    return env.evaluate("emit-0/same_obj")


@pytest.mark.parametrize("value", [None, True, False, 0, -1, 1.5, "s", b"bytes"])
def test_VALUE01_primitive_roundtrip(echo, value):
    result = through(echo, value)
    assert result == value and type(result) is type(value)


def test_VALUE02_nested_array_object_roundtrip(echo):
    value = {"a": [1, "two", {"b": None}], "c": {"d": [True, 2.5]}}
    result = through(echo, value)
    assert result == value
    assert type(result) is dict and type(result["a"]) is list


@pytest.mark.parametrize("n", [2**63 - 1, -(2**63)])
def test_VALUE03_integer_boundaries(echo, n):
    assert through(echo, n) == n


def test_VALUE03_integer_beyond_i64_is_refused(echo):
    with pytest.raises(lq.ConversionError):
        through(echo, 2**63)


BYTES_CORPUS = [
    b"",
    b"hello",
    "h\u00e9llo \u2014 ok".encode(),
    "\U0001F980".encode(),
    b"a\x00b",
    b"\xff\xfe",
    b"\xed\xa0\x80",
    bytes(range(256)),
]


def test_VALUE04_bytes_are_not_confused_with_text(echo):
    assert type(through(echo, b"abc")) is bytes
    assert type(through(echo, "abc")) is str
    for data in BYTES_CORPUS:
        result = through(echo, data)
        assert type(result) is bytes and result == data, f"corrupted: {data!r}"


def test_VALUE05_unknown_object_follows_configured_policy(echo):
    obj = Opaque("x")
    assert through(echo, obj) is obj
    strict = lq.Environment(conversion="strict")

    @strict.first_command
    def make():
        return Opaque("x")

    with pytest.raises(lq.ConversionError, match="Opaque"):
        strict.evaluate("make")
    strict.shutdown()


def test_VALUE06_opaque_serialization_fails_or_uses_its_codec(env):
    with pytest.raises(lq.SerializationError):
        env.store_set("o/obj", Opaque("x"))
    pickling = lq.Environment(pickle=True)
    pickling.store_set("o/obj", Opaque("x"))
    assert pickling.evaluate("-R/o/obj") == Opaque("x")
    pickling.shutdown()


def test_VALUE07_cycles_follow_policy(echo):
    cyclic = []
    cyclic.append(cyclic)
    with pytest.raises(lq.ConversionError):
        through(echo, cyclic)
    shared = [1]
    assert through(echo, [shared, shared]) == [[1], [1]]  # a shared reference is not a cycle


def test_VALUE08_representative_extvalue_roundtrip(env):
    pl = pytest.importorskip("polars")

    @env.first_command
    def make_frame():
        return pl.DataFrame({"a": [1, 2, 3]})

    df = env.evaluate("make_frame/ns-pl/head-2")
    assert isinstance(df, pl.DataFrame) and df.height == 2
    assert df["a"].to_list() == [1, 2]


def test_VALUE08_image_roundtrip(env):
    image = pytest.importorskip("PIL.Image")

    @env.first_command
    def make_image():
        return image.new("RGB", (3, 2), (255, 0, 0))

    result = env.evaluate("make_image/same_obj")
    assert isinstance(result, image.Image) and result.size == (3, 2)
    assert result.convert("RGB").getpixel((0, 0)) == (255, 0, 0)


def test_VALUE09_checked_upcast_and_downcast(env):
    @env.command
    def as_text(text):
        return text

    assert env.evaluate("number-3/as_text") == "3"  # the value's text form
    with pytest.raises(lq.ConversionError):
        env.evaluate("make_obj/as_text")  # an opaque object has no text form


def test_VALUE10_language_only_object_retains_documented_identity(echo):
    obj = Opaque("identity")
    assert through(echo, obj) is obj


def test_VALUE11_callable_retention_or_rejection_follows_policy(echo):
    def add_one(x):
        return x + 1

    returned = through(echo, add_one)
    assert returned is add_one and returned(1) == 2  # retained by identity, like any object
    with pytest.raises(lq.SerializationError):
        echo.store_set("o/fn", add_one)  # and never serialized without the pickle codec


@na("liquers-py exposes no Value wrapper with operators; values cross as Python objects")
def test_VALUE12_scalar_operators_produce_documented_result():
    pass


def test_VALUE13_state_operations_preserve_or_discard_metadata(env):
    state = env.evaluate_state("hello/to_text")
    assert state.value == "hello"
    assert state.metadata.type_identifier() == "Text"
    with pytest.raises(AttributeError):
        state.value = "changed"  # a State is a read-only snapshot
    # The design's choice: no derived-state operations, so no state is made without metadata.
    assert not hasattr(state, "map")


def test_STATUS_names_every_variant_and_refuses_unknown_ones():
    for name in ["None", "Directory", "Recipe", "Submitted", "Dependencies", "Processing",
                 "Partial", "Error", "Storing", "Ready", "Expired", "Cancelled", "Source",
                 "Override", "Volatile"]:
        status = lq.Status(name)
        assert status.name() == name and status == name and status == lq.Status(name)
    with pytest.raises(ValueError):
        lq.Status("Finished")


class Tripwire:
    """Records being unpickled, so a test can prove it never was."""

    unpickled = False

    def __reduce__(self):
        return (_tripped, ())


def _tripped():
    Tripwire.unpickled = True
    return Tripwire()


def _store_tripwire(tmp_path):
    """Stores a pickled object with a pickle-on environment; returns the shared store config."""
    config = {"store": {"stores": [{"type": "filesystem", "prefix": "o",
                                    "config": {"path": str(tmp_path)}}]}}
    writer = lq.Environment(config=config, pickle=True)
    writer.store_set("o/obj", Tripwire())
    writer.shutdown()
    Tripwire.unpickled = False
    return config


def test_VALUE_pickle_off_never_unpickles(tmp_path):
    reader = lq.Environment(config=_store_tripwire(tmp_path))
    with pytest.raises(lq.LiquersError):
        reader.evaluate("-R/o/obj")
    reader.shutdown()
    assert not Tripwire.unpickled, "a pickle-off environment unpickled stored bytes"


@pytest.mark.xfail(
    strict=True,
    reason="BLOCKED: DEGRADED-STORED-VALUE-WITHOUT-RECIPE-REPORTS-NO-RECIPE: the refused read "
    "falls through to recipe lookup and reports 'No recipe found'",
)
def test_VALUE_pickle_off_error_names_the_type(tmp_path):
    reader = lq.Environment(config=_store_tripwire(tmp_path))
    with pytest.raises(lq.LiquersError) as raised:
        reader.evaluate("-R/o/obj")
    reader.shutdown()
    assert "py.Object" in raised.value.message
