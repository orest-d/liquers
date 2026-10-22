"""EVAL: evaluating queries (LANGUAGE-INTEGRATION_GUIDE.md)."""

import time

import pytest

import liquers as lq


def test_EVAL04_invalid_query_maps_through_error(env):
    with pytest.raises(lq.ActionNotRegisteredError) as raised:
        env.evaluate("no_such_command_anywhere")
    assert raised.value.type == lq.ErrorType.ActionNotRegistered
    assert "no_such_command_anywhere" in raised.value.message
    with pytest.raises(lq.ParseError):
        env.evaluate("a/b-~")


def test_EVAL_text_resource_read_from_store(env):
    @env.command
    def repeat(text: str, count: int = 2) -> str:
        return text * count

    env.store_set("d/in.txt", "abc")
    assert env.evaluate("-R/d/in.txt") == "abc"
    assert env.evaluate("-R/d/in.txt/-/repeat-2") == "abcabc"


def test_EVAL01_evaluate_builtin_query(env):
    assert env.evaluate("hello/to_text") == "hello"


def test_EVAL02_string_and_wrapped_query_agree(env):
    assert env.evaluate(lq.parse("hello/to_text")) == env.evaluate("hello/to_text")


def test_EVAL03_metadata_and_logs_available(env):
    @env.command
    def noted(text, context):
        context.info("noted it")
        return text

    asset = env.get_asset("hello/noted")
    assert asset.get() == "hello"
    assert asset.status == lq.Status("Ready")
    assert any("noted it" in str(entry) for entry in asset.metadata.log())
    state = env.evaluate_state("hello/noted")
    assert state.value == "hello"
    assert state.metadata.status() == "Ready"


def test_EVAL05_payload_and_context_reach_a_command(env):
    # The context half. The payload half is NA: the environment's payload type is `()`, so there
    # is no payload to deliver (README, Conformance).
    assert env.evaluate("outer") == "outer:hello"


def test_EVAL06_cancellation_has_defined_terminal_result():
    from conftest import CALLS, register_fixture_commands

    environment = lq.Environment(config={"assets": {"job_capacity": 1}})
    register_fixture_commands(environment)
    try:
        running = environment.get_asset("slow-5")
        deadline = time.monotonic() + 10
        while CALLS["slow"] == 0 and time.monotonic() < deadline:
            time.sleep(0.01)
        assert CALLS["slow"] == 1, "slow-5 should be running"

        waiting = environment.get_asset("slow-6")
        assert waiting.status == "Submitted"
        waiting.cancel()
        assert waiting.status == lq.Status("Cancelled")
        with pytest.raises(lq.CancelledError):
            waiting.get()

        running.cancel()
        with pytest.raises(lq.CancelledError):
            running.get()
        assert running.status == "Cancelled"
        assert CALLS["slow"] == 1, "the cancelled slow-6 must never run"
    finally:
        environment.shutdown()


def test_EVAL_context_error_fails_the_result():
    # An error-level entry fails the evaluation once the command returns, whatever it returned.
    env = lq.Environment()

    @env.first_command
    def logs_an_error(context):
        context.error("logged error")
        return "value"

    with pytest.raises(lq.ExecutionError, match="logged error"):
        env.evaluate("logs_an_error")
    env.shutdown()
