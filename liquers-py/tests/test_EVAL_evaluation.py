"""EVAL: evaluating queries (LANGUAGE-INTEGRATION_GUIDE.md)."""

import pytest

import liquers as lq


@pytest.fixture
def env():
    environment = lq.Environment()
    yield environment
    environment.shutdown()


def test_EVAL04_invalid_query_maps_through_error(env):
    with pytest.raises(lq.ActionNotRegisteredError) as raised:
        env.evaluate("no_such_command_anywhere")
    assert raised.value.type == lq.ErrorType.ActionNotRegistered
    assert "no_such_command_anywhere" in raised.value.message
    with pytest.raises(lq.ParseError):
        env.evaluate("a/b-~")


def test_EVAL_text_value_read_from_store(env):
    env.store_set("d/in.txt", "abc")
    assert env.evaluate("-R/d/in.txt") == "abc"
