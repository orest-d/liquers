"""PIPELINE: pipeline commands (`specs/design/liquers-py-pipelines/`, AC-5, AC-6)."""

import pytest

import liquers as lq

QUERY = "-R/data/sales.csv/-/ns-pl/from_csv/is_in-region-EU-US/head-5"
BODY = [
    {"action": "from_csv", "ns": "pl"},
    {"action": "is_in", "ns": "pl",
     "arguments": {"column": "region", "values": {"multiple": [{"placeholder": "regions"}]}}},
    {"action": "head", "ns": "pl", "parameters": [{"placeholder": "n"}]},
]
REGIONS = '{"name": "regions", "multiple": true}'


@pytest.fixture
def penv(env):
    env.store_set("data/sales.csv", b"region,amount\nEU,1\nUS,2\nASIA,3\n")
    return env


def test_pipeline_command_runs_as_the_long_query(penv):
    """AC-5: a pipeline command gives what the long query gives, also after a rebuild."""
    expected = penv.evaluate(QUERY)
    penv.pipeline_command("top_in", BODY, arguments=["n", lq.ArgumentInfo.from_json(REGIONS)])
    assert penv.evaluate("-R/data/sales.csv/-/top_in-5-EU-US").equals(expected)
    definition = penv.command_metadata("top_in").definition
    assert definition.kind == "pipeline" and definition.pipeline == lq.Pipeline(BODY)

    @penv.command
    def later(state):
        return state

    assert penv.evaluate("-R/data/sales.csv/-/top_in-5-EU-US").equals(expected)

    direct = [dict(step) for step in BODY]
    direct[1] = {"action": "is_in", "ns": "pl",
                 "arguments": {"column": "region", "values": {"placeholder": "regions"}}}
    penv.pipeline_command("top_in2", direct, arguments=["n", lq.ArgumentInfo.from_json(REGIONS)])
    assert penv.evaluate("-R/data/sales.csv/-/top_in2-5-EU-US").equals(expected)


def test_bad_pipeline_command_is_refused(penv):
    """AC-6: a body that does not plan raises and registers nothing."""
    with pytest.raises(lq.LiquersError):
        penv.pipeline_command("bad", [{"action": "head", "ns": "pl", "parameters": [{"placeholder": "nope"}]}])
    with pytest.raises(lq.LiquersError):
        penv.pipeline_command("bad", [{"action": "no_such_command"}])
    with pytest.raises(lq.LiquersError):
        penv.evaluate("hello/bad")
    with pytest.raises(TypeError):
        penv.pipeline_command("bad", BODY, arguments=[42])
