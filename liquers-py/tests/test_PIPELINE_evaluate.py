"""PIPELINE: evaluating pipelines from Python (`specs/design/liquers-py-pipelines/`, AC-1, AC-2,
AC-9)."""

import _thread
import threading
import time

import pytest
from conftest import CALLS, run_async

import liquers as lq

QUERY = "-R/data/sales.csv/-/ns-pl/from_csv/is_in-region-EU-US/head-5"
TOP = [
    "-R/data/sales.csv/-/ns-pl/from_csv",
    {"action": "is_in", "ns": "pl", "arguments": {"column": "region", "values": ["EU", "US"]}},
    {"action": "head", "ns": "pl", "parameters": [5]},
]


@pytest.fixture
def penv(env):
    env.store_set("data/sales.csv", b"region,amount\nEU,1\nUS,2\nASIA,3\n")
    return env


def test_pipeline_evaluates_as_its_query(penv):
    """AC-1: a list, a dict and an `lq.Pipeline` evaluate to what their query gives."""
    expected = penv.evaluate(QUERY)
    assert penv.evaluate(TOP).equals(expected)
    assert penv.evaluate({"elements": TOP}).equals(expected)
    assert penv.evaluate(lq.Pipeline(TOP)).equals(expected)
    assert penv.evaluate(tuple(TOP)).equals(expected)
    assert penv.evaluate_state(TOP).value.equals(expected)

    async def main():
        return await penv.evaluate_async(TOP), await penv.evaluate_state_async(lq.Pipeline(TOP))

    value, state = run_async(main)
    assert value.equals(expected) and state.value.equals(expected)


def test_failing_step_raises_like_its_query(penv):
    """AC-1: a failing step raises the error type its query raises."""
    with pytest.raises(lq.LiquersError) as from_query:
        penv.evaluate("hello/boom")
    with pytest.raises(lq.LiquersError) as from_pipeline:
        penv.evaluate([{"action": "hello"}, {"action": "boom"}])
    assert type(from_pipeline.value) is type(from_query.value)


def test_structured_argument_arrives_whole(penv):
    """AC-2: a structured value in an `any` argument reaches a Python command unchanged."""

    @penv.command
    def echo(state, payload=None):
        return payload

    payload = {"a": [1, 2], "b": None}
    got = penv.evaluate([{"action": "hello"}, {"action": "echo", "arguments": {"payload": payload}}])
    assert got == payload


def test_pipeline_values_keep_their_type(penv):
    """AC-9: a pipeline value, a recipe's pipeline and a pipeline default are `lq.Pipeline`."""

    @penv.first_command
    def make():
        return lq.Pipeline(TOP)

    assert penv.evaluate("make") == lq.Pipeline(TOP)
    assert penv.evaluate_state("make").metadata.type_identifier() == "Pipeline"

    penv.store_set(
        "r/recipes.yaml",
        "recipes:\n  - pipeline:\n      - {action: hello}\n    query: ns-lui/to_text/out.txt\n",
    )
    recipe = penv.recipes.recipe("r/out.txt")
    assert recipe.pipeline == lq.Pipeline([{"action": "hello"}])

    @penv.first_command
    def ghost(p=None):
        return p

    penv.import_metadata(
        "commands:\n  - name: ghost\n    arguments:\n      - name: p\n"
        "        default: {pipeline: [{action: hello}]}\n"
    )
    default = penv.command_metadata("ghost").arguments[0].default
    assert default == lq.Pipeline([{"action": "hello"}])


def test_non_representable_in_a_command(penv):
    """Decision 3: a pipeline no query expresses runs in a command through an ad-hoc asset."""

    @penv.first_command
    def inner(context):
        return context.evaluate([{"value": "v"}, {"action": "same_obj"}])

    assert penv.evaluate("inner") == "v"


def test_get_asset_refuses_a_pipeline(penv):
    with pytest.raises(TypeError):
        penv.get_asset(TOP)
    with pytest.raises(TypeError):
        penv.get_asset(lq.Pipeline(TOP))
    with pytest.raises(TypeError):
        penv.get_asset(tuple(TOP))


def test_relative_key_pipeline(penv):
    """AC-1: at the top level a relative key resolves against the working directory, as in its
    query."""
    penv.store_set("data/x.txt", "in data")
    pipeline = [{"cwd": "data"}, {"resource": "./x.txt"}]
    assert lq.Pipeline(pipeline).to_query(penv).encode() == "-R-cwd/data/-R/./x.txt"
    assert penv.evaluate(pipeline) == penv.evaluate("-R-cwd/data/-R/./x.txt") == "in data"


def test_relative_key_in_a_command_is_refused(penv):
    """A relative key in a pipeline evaluated in a command is refused, as a relative query is."""

    @penv.first_command
    def rel(context):
        return context.evaluate([{"value": 1}, {"resource": "./x.csv"}])

    with pytest.raises(lq.LiquersError, match="relative"):
        penv.evaluate("rel")


def test_interrupt_cancels_the_pipeline(penv):
    """Ctrl-C during a pipeline whose step is slow: the asset is cancelled and the call raises."""
    timer = threading.Timer(0.3, _thread.interrupt_main)
    timer.start()
    started = time.monotonic()
    with pytest.raises(KeyboardInterrupt):
        penv.evaluate([{"value": 1}, {"action": "slow", "parameters": [5]}])
    timer.join()
    assert time.monotonic() - started < 3
