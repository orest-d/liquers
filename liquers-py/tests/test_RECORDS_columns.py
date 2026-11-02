"""RECORDS: columns, lent buffers and their lifetimes (LANGUAGE-INTEGRATION_GUIDE.md §RECORDS).

Design: specs/design/liquers-py-records/phase3-examples.md, "test_RECORDS_columns.py".
"""

import gc
import struct
import sys
import threading
from pathlib import Path

import pytest

import liquers as lq
from liquers import _native

FIXTURE = Path(__file__).parent / "data" / "records_all_types.feather"


@pytest.fixture
def renv():
    e = lq.Environment()

    @e.first_command
    def ipc_bytes() -> bytes:
        return FIXTURE.read_bytes()

    @e.first_command
    def table() -> bytes:
        return b"id,amount\na,1\nb,2\nc,\n"

    yield e
    e.shutdown()


def fixture_view(renv):
    return renv.evaluate("ipc_bytes/ns-rec/to_record-feather")


def bits(buffer, n):
    """The first ``n`` bits of a packed, LSB-first bitmap."""
    data = bytes(memoryview(buffer))
    return [bool(data[i // 8] >> (i % 8) & 1) for i in range(n)]


FIXED = {"count": "q", "src": "Q", "score": "d", "day": "i", "at": "q"}


def test_RECORDS02_lent_and_copied_columns_agree(renv):
    view = fixture_view(renv)
    # The fixture is a stored batch, so its columns are lent; a narrowed view's are read once.
    copied = view.select_columns(*view.schema.names).slice(0, 4)
    for name in view.schema.names:
        for source in (view, copied):
            column = source.column(name)
            values = column.to_list()
            assert len(column) == 4 and column.name == name
            assert column.null_count == sum(v is None for v in values)
            if column.validity is not None:
                assert bits(column.validity, 4) == [v is not None for v in values]
            if name in FIXED:
                buffer = memoryview(column)
                assert buffer.format == FIXED[name] and buffer.readonly
                for i, value in enumerate(values):
                    if value is not None and name in ("count", "src", "score"):
                        assert buffer[i] == value
        assert view.column(name).to_list() == copied.column(name).to_list()
    vectors = memoryview(view.column("vec"))
    assert vectors.format == "f" and vectors.shape == (4, 2) and view.column("vec").dim == 2
    assert vectors[0, 1] == 2.0 and vectors[3, 1] == -6.0
    assert memoryview(view.column("day"))[0] == 19700
    assert view.column(0).name == "id" and view.column(-1).name == "vec"
    assert [c.name for c in view.columns()] == view.schema.names
    assert view.column("count")[-1] == 2**63 - 1


def test_RECORDS04_buffers_are_read_only(renv):
    column = renv.evaluate("table/ns-rec/to_record-csv").column("amount")
    # The export refuses a writable request (BufferError); `struct` reports it as a TypeError.
    with pytest.raises(TypeError, match="read-write"):
        struct.pack_into("q", column, 0, 0)
    assert memoryview(column).readonly
    with pytest.raises(TypeError):
        memoryview(column)[0] = 0
    assert memoryview(column).tolist()[:2] == [1, 2]


def test_RECORDS_text_and_bool_columns_expose_parts(renv):
    view = fixture_view(renv)
    name = view.column("name")
    with pytest.raises(BufferError):
        memoryview(name)
    offsets = memoryview(name.offsets).tolist()
    data = bytes(memoryview(name.data))
    assert name.offsets.format == "i" and len(offsets) == 5
    assert data[offsets[0] : offsets[1]] == b"alpha"
    assert data[offsets[3] : offsets[4]].decode() == "été"
    flag = view.column("flag")
    with pytest.raises(BufferError):
        memoryview(flag)
    assert bits(flag.data, 4)[:2] == [True, False]
    assert flag.offsets is None and view.column("count").offsets is None
    assert flag.data.format == "B"


def test_RECORDS05_buffer_outlives_its_view(renv):
    view = renv.evaluate("table/ns-rec/to_record-csv")
    buffer = memoryview(view.column("amount"))
    del view
    gc.collect()
    ballast = bytearray(200 * 1024 * 1024)
    assert buffer.tolist()[:2] == [1, 2]
    del ballast


@pytest.mark.skipif(not hasattr(_native, "live_record_handle_count"), reason="needs a debug-handles build")
def test_RECORDS06_record_handles_are_released(renv):
    gc.collect()
    baseline = _native.live_record_handle_count()
    view = fixture_view(renv)
    column = view.column("count")
    buffer = memoryview(column)
    validity = column.validity
    assert _native.live_record_handle_count() > baseline
    del view, column, buffer, validity
    gc.collect()
    assert _native.live_record_handle_count() == baseline


def test_RECORDS_concurrent_column_reads(renv):
    view = fixture_view(renv)
    expected = {name: view.column(name).to_list() for name in view.schema.names}
    results = []
    errors = []

    def read():
        try:
            for _ in range(50):
                results.append({name: view.column(name).to_list() for name in view.schema.names})
        except BaseException as e:
            errors.append(e)

    threads = [threading.Thread(target=read) for _ in range(8)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    assert not errors
    assert all(result == expected for result in results) and len(results) == 400
