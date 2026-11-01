"""RECORDS: record sources, traversed lazily (AC-6, AC-7, AC-8, AC-13, AC-15).

Design: specs/design/liquers-py-records/phase3-examples.md, "test_RECORDS_sources.py" and
Example 2.
"""

import asyncio
import threading
import time

import pytest

import liquers as lq
from conftest import na, run_async

FLOAT_SCHEMA = {
    "fields": [
        {"name": "id", "data_type": "Text"},
        {"name": "n", "data_type": "Float", "label": "Amount"},
    ]
}


def manifest_env(chunks=3):
    """Example 2: a manifest of ``chunks`` CSV chunks, ``part`` counting its calls."""
    env = lq.Environment()
    calls = {"part": 0}

    # Each chunk is a view. A command returning raw CSV bytes cannot be a chunk yet: bytes do not
    # serialize in the `csv` format (BYTES-VALUE-HAS-NO-NAMED-DATA-FORMAT).
    @env.first_command
    def part(i: int = 0):
        calls["part"] += 1
        return lq.RecordView.from_records([{"id": f"r{i}", "n": float(i)}])

    @env.first_command
    def manifest():
        return {
            "manifest": "record-stream",
            "uniform_schema": FLOAT_SCHEMA,
            "chunks": [{"query": f"part-{i}"} for i in range(chunks)],
        }

    @env.first_command
    def table() -> bytes:
        return b"id,amount\na,1\nb,2\nc,\n"

    return env, calls


@pytest.fixture
def menv():
    env, calls = manifest_env()
    yield env, calls
    env.shutdown()


def test_RECORDS07_stream_reads_one_chunk_per_step(menv):
    env, calls = menv
    source = env.evaluate("manifest/ns-rec/to_record_source")
    assert isinstance(source, lq.RecordSource)
    assert calls["part"] == 0 and source.schema.field("n").label == "Amount"
    seen = []
    for step, chunk in enumerate(source):
        assert calls["part"] == step + 1
        assert isinstance(chunk, lq.RecordView)
        assert [(f.name, f.type) for f in chunk.schema.fields] == [("id", lq.FieldType.Text), ("n", lq.FieldType.Float)]
        seen.append(chunk.to_list())
    assert seen == [[{"id": f"r{i}", "n": float(i)}] for i in range(3)]


def test_RECORDS08_async_stream_yields_same_rows(menv):
    env, _ = menv
    source = env.evaluate("manifest/ns-rec/to_record_source")
    expected = [chunk.to_list() for chunk in source]

    async def main():
        return [chunk.to_list() async for chunk in source]

    assert run_async(main) == expected


def test_RECORDS_materialize_is_bounded(menv):
    env, _ = menv
    source = env.evaluate("manifest/ns-rec/to_record_source")
    whole = source.materialize()
    assert len(whole) == 3 and whole.to_dict() == {"id": ["r0", "r1", "r2"], "n": [0.0, 1.0, 2.0]}
    with pytest.raises(lq.LiquersError, match="max_rows"):
        source.materialize(2)

    async def main():
        view = await source.materialize_async(10)
        with pytest.raises(lq.LiquersError, match="max_rows"):
            await source.materialize_async(1)
        return len(view)

    assert run_async(main) == 3


def test_RECORDS_chunks_list_without_evaluation(menv):
    env, calls = menv
    source = env.evaluate("manifest/ns-rec/to_record_source")
    chunks = source.chunks()
    assert calls["part"] == 0
    assert [(c.index, c.kind, c.query.encode(), c.evaluable) for c in chunks] == [
        (i, "query", f"part-{i}", True) for i in range(3)
    ]
    assert source.bounded and source.manifest["chunks"][1]["query"] == "part-1"
    one = source.chunk(1)
    assert calls["part"] == 1
    assert one.to_list() == [{"id": "r1", "n": 1.0}]
    assert one.schema.field("n").type == lq.FieldType.Float

    async def main():
        return (await source.chunk_async(2)).to_list()

    assert run_async(main) == [{"id": "r2", "n": 2.0}]

    memory = env.evaluate("table/ns-rec/to_record-csv/to_record_source")
    listed = memory.chunks()
    assert len(listed) == 1 and listed[0].kind == "memory" and not listed[0].evaluable
    assert memory.manifest is None and memory.chunk(0).to_dict()["id"] == ["a", "b", "c"]
    with pytest.raises(lq.LiquersError, match="out of range"):
        memory.chunk(1)


def test_RECORDS_source_round_trips_into_a_command(menv):
    env, calls = menv

    @env.first_command
    def produce():
        return env.evaluate("manifest/ns-rec/to_record_source")

    assert env.evaluate("produce/ns-rec/materialize").to_dict()["id"] == ["r0", "r1", "r2"]


def test_RECORDS_source_outlives_its_command(menv):
    env, calls = menv
    kept = {}

    @env.command
    def keep(value):
        kept["source"] = value
        first = next(iter(value))  # traversed through the command's context
        return len(first)

    assert env.evaluate("manifest/ns-rec/to_record_source/keep") == 1
    # The command returned: the source now reads through the environment.
    assert [len(v) for v in kept["source"]] == [1, 1, 1]


def test_RECORDS_concurrent_next_is_refused():
    env = lq.Environment()
    entered = threading.Event()
    release = threading.Event()

    @env.first_command(volatile=True)
    def gate():
        entered.set()
        release.wait(10)
        return lq.RecordView.from_records([{"id": "x"}])

    @env.first_command
    def manifest():
        return {"manifest": "record-stream", "chunks": [{"query": "gate"}]}

    stream = iter(env.evaluate("manifest/ns-rec/to_record_source"))
    outcome = {}

    def first():
        outcome["view"] = next(stream)

    thread = threading.Thread(target=first)
    thread.start()
    assert entered.wait(10)
    try:
        with pytest.raises(lq.GeneralError, match="another step"):
            next(stream)
    finally:
        release.set()
        thread.join(10)
    assert outcome["view"].to_list() == [{"id": "x"}]
    with pytest.raises(StopIteration):
        next(stream)
    env.shutdown()


def test_RECORDS_blocking_calls_inside_async_command_raise(menv):
    env, _ = menv
    errors = {}
    has_pyarrow = _has("pyarrow")

    @env.command
    async def walk(value):
        calls = [
            ("for", lambda: list(value)),
            ("chunk", lambda: value.chunk(0)),
            ("materialize", lambda: value.materialize()),
        ]
        for name, call in calls:
            try:
                call()
            except RuntimeError as error:
                errors[name] = str(error)
        views = [chunk async for chunk in value]
        if has_pyarrow:
            try:
                views[0].to_arrow()
            except RuntimeError as error:
                errors["to_arrow"] = str(error)
        return len(views)

    assert env.evaluate("manifest/ns-rec/to_record_source/walk") == 3
    expected = {"for", "chunk", "materialize"} | ({"to_arrow"} if has_pyarrow else set())
    assert set(errors) == expected
    assert all("_async" in message or "block" in message for message in errors.values())


def _has(module):
    try:
        __import__(module)
        return True
    except ImportError:
        return False


def test_RECORDS_shutdown_ends_traversal():
    env, _ = manifest_env()
    source = env.evaluate("manifest/ns-rec/to_record_source")
    held = source.chunk(0)
    env.shutdown()
    with pytest.raises(lq.GeneralError, match="shut down"):
        list(source)
    with pytest.raises(lq.GeneralError, match="shut down"):
        source.materialize()
    assert held.to_list() == [{"id": "r0", "n": 0.0}]  # a held view still reads

    # Shut down while a step waits: that step ends Cancelled.
    env = lq.Environment()
    entered = threading.Event()

    @env.first_command(volatile=True)
    def stall():
        entered.set()
        time.sleep(3)
        return lq.RecordView.from_records([{"id": "x"}])

    @env.first_command
    def manifest():
        return {"manifest": "record-stream", "chunks": [{"query": "stall"}]}

    stream = iter(env.evaluate("manifest/ns-rec/to_record_source"))
    outcome = {}

    def walk():
        try:
            next(stream)
        except BaseException as error:
            outcome["error"] = error

    thread = threading.Thread(target=walk)
    thread.start()
    assert entered.wait(10)
    env.shutdown()
    thread.join(10)
    assert isinstance(outcome.get("error"), lq.CancelledError)


def test_RECORDS_materialize_of_differing_chunks():
    env = lq.Environment()

    @env.first_command
    def a():
        return lq.RecordView.from_records([{"id": "x", "n": 1}])

    @env.first_command
    def b():
        return lq.RecordView.from_records([{"id": "y", "m": 2}])

    @env.first_command
    def manifest():
        return {"manifest": "record-stream", "chunks": [{"query": "a"}, {"query": "b"}]}

    source = env.evaluate("manifest/ns-rec/to_record_source")
    with pytest.raises(lq.LiquersError, match="'n'|'m'"):
        source.materialize()
    env.shutdown()


@na("Python has an async model (`async for`), so the sync-only route of RECORDS09 does not apply")
def test_RECORDS09_no_async_model_route():
    pass


@na("Python does not implement the RecordView / RecordSource traits in M5 (liquers-py-records Decision 4)")
def test_RECORDS11_language_defined_views():
    pass
