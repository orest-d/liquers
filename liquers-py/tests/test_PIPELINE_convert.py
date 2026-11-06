"""PIPELINE: `lq.Pipeline` and its conversions (`specs/design/liquers-py-pipelines/`, AC-3, AC-4,
AC-10)."""

import datetime

import pytest

import liquers as lq

QUERY = "-R/data/sales.csv/-/ns-pl/from_csv/is_in-region-EU-US/head-5"


def test_round_trip(env):
    """AC-3: a query becomes a pipeline and back; the canonical query is stable, and JSON and
    lists round-trip unchanged."""
    link = "hello/ns-template/render-~X~-R/templates/greet.md~E"
    assert lq.Pipeline.from_query(QUERY).to_query(env).encode() == QUERY
    canonical = lq.Pipeline.from_query(link).to_query(env).encode()
    assert canonical == "ns-template/hello/render-~X~-R/templates/greet.md~E"
    assert lq.Pipeline.from_query(canonical).to_query(env).encode() == canonical
    for query in [QUERY, link]:
        pipeline = lq.Pipeline.from_query(query)
        assert lq.Pipeline.from_json(pipeline.to_json()) == pipeline
        assert lq.Pipeline(pipeline.to_list()) == pipeline


def test_not_representable_names_the_rule(env):
    """AC-4: a pipeline no query expresses raises, naming the rule."""
    with pytest.raises(lq.ParameterError, match="P1"):
        lq.Pipeline([{"value": 1}]).to_query(env)
    with pytest.raises(lq.ParameterError, match="S7"):
        lq.Pipeline([{"action": "number", "arguments": {"n": {"placeholder": "n"}}}]).to_query(env)


def test_only_json_in_json_slots():
    """AC-10: what JSON cannot hold raises `ConversionError`, naming where it was."""
    with pytest.raises(lq.ConversionError, match="parameters"):
        lq.Pipeline([{"action": "hello", "parameters": [datetime.datetime.now()]}])
    looped = {}
    looped["self"] = looped
    with pytest.raises(lq.ConversionError):
        lq.Pipeline([{"action": "hello", "arguments": {"x": looped}}])
    with pytest.raises(lq.ConversionError):
        lq.Pipeline([{"action": "hello", "parameters": [2**64]}])


def test_constructor_takes_a_list():
    with pytest.raises(TypeError):
        lq.Pipeline("-R/x")
    with pytest.raises(TypeError):
        lq.Pipeline(42)


def test_non_finite_floats():
    """NaN and ±inf are written as the reference serializer writes them."""
    pipeline = lq.Pipeline([{"action": "hello", "parameters": [float("nan"), float("inf"), float("-inf")]}])
    assert '"NaN"' in pipeline.to_json() and '"inf"' in pipeline.to_json() and '"-inf"' in pipeline.to_json()
