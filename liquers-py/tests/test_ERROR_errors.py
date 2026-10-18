"""The LiquersError hierarchy (design liquers-py, step 5)."""

import pytest

import liquers as lq


def test_every_error_type_has_a_subclass():
    for name in dir(lq.ErrorType):
        member = getattr(lq.ErrorType, name)
        if not isinstance(member, lq.ErrorType):
            continue
        class_name = name if name.endswith("Error") else name + "Error"
        cls = getattr(lq, class_name)
        assert issubclass(cls, lq.LiquersError)
        assert class_name in lq.__all__


def test_base_constructor_dispatches_to_the_subclass():
    err = lq.LiquersError(lq.ErrorType.KeyNotFound, "no such key", key="a/b")
    assert type(err) is lq.KeyNotFoundError
    assert err.type == lq.ErrorType.KeyNotFound
    assert err.message == "no such key"
    assert err.key == "a/b"


def test_subclass_constructor_sets_its_type():
    err = lq.ExecutionError("boom", query="x/y")
    assert err.type == lq.ErrorType.ExecutionError
    assert str(err) == "boom"
    assert err.query == "x/y"
    assert isinstance(err, lq.LiquersError)


def test_ERROR01_every_error_type_maps():
    members = [getattr(lq.ErrorType, n) for n in dir(lq.ErrorType)]
    members = [m for m in members if isinstance(m, lq.ErrorType)]
    assert len(members) > 20
    for member in members:
        err = lq.LiquersError(member, "m")
        assert isinstance(err, lq.LiquersError) and err.type == member
        assert type(err) is not lq.LiquersError


def test_ERROR02_fields_survive_a_command(env):
    @env.first_command
    def located():
        raise lq.ParseError("bad", query="a/b", key="k/x")

    with pytest.raises(lq.ParseError) as raised:
        env.evaluate("located")
    assert raised.value.message == "bad"
    assert raised.value.key == "k/x"
    # The asset layer records the query being evaluated, replacing the one the command set.
    assert raised.value.query in ("a/b", "located")


def test_ERROR03_language_exception_includes_class_and_stack(env):
    with pytest.raises(lq.ExecutionError) as raised:
        env.evaluate("hello/boom")
    assert "ValueError" in raised.value.message and "inner" in raised.value.message
    assert raised.value.traceback and "boom" in raised.value.traceback


def test_ERROR04_non_error_throw_has_safe_fallback(env):
    @env.command
    def interrupted(value):
        raise KeyboardInterrupt("not an Exception subclass")

    with pytest.raises(lq.ExecutionError) as raised:
        env.evaluate("hello/interrupted")
    assert "KeyboardInterrupt" in raised.value.message
    assert env.evaluate("hello") == "hello"


def test_ERROR06_language_raised_error_keeps_its_type(env):
    @env.first_command
    def missing():
        raise lq.LiquersError(lq.ErrorType.KeyNotFound, "no such key")

    with pytest.raises(lq.KeyNotFoundError) as raised:
        env.evaluate("missing")
    assert raised.value.type == lq.ErrorType.KeyNotFound
    assert "no such key" in raised.value.message


def test_ERROR05_no_panic_crosses_the_boundary(env):
    # `test_panic` is a Rust command that panics, registered only under `debug-handles`.
    assert hasattr(lq.Environment, "live_handle_count"), "build with --features debug-handles"
    with pytest.raises(lq.LiquersError, match="test_panic was evaluated") as raised:
        env.evaluate("test_panic")
    assert raised.value.type != lq.ErrorType.ActionNotRegistered
