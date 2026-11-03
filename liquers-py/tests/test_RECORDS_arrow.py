"""RECORDS: Arrow, pandas and polars exits and entrances (AC-9, AC-11; RECORDS03).

Design: specs/design/liquers-py-records/phase3-examples.md, "test_RECORDS_arrow.py".
"""

import json
import sys
from pathlib import Path

import pytest

import liquers as lq

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


def test_RECORDS03_exits_carry_rows_and_metadata(renv):
    pytest.importorskip("pyarrow")
    view = renv.evaluate("ipc_bytes/ns-rec/to_record-feather")
    table = view.to_arrow()
    assert table.num_rows == 4 and table.column_names == view.schema.names
    assert table.column("count").to_pylist() == view.column("count").to_list()
    assert table.column("name").to_pylist() == view.column("name").to_list()
    metadata = table.schema.metadata
    # The same JSON the IPC file's batch message holds (a key serializes as its segments).
    chunk_id = json.loads(metadata[b"liquers.chunk_id"])
    assert "/".join(segment["name"] for segment in chunk_id["Key"]) == view.chunk_id.encode()
    origins = json.loads(metadata[b"liquers.origins"])
    assert [o["asset"] for o in origins] == [o.asset.encode() for o in view.origins]
    schema = json.loads(metadata[b"liquers.schema"])
    assert [f["key"] for f in schema["fields"]][:2] == ["Id", "Source"]
    # Back in, roles and values survive (the chunk id and origins live on the batch message).
    back = lq.RecordView.from_arrow(table)
    assert back.schema.field("id").key == lq.KeyRole.Id
    assert back.to_list() == view.to_list()


def test_RECORDS_pandas_and_polars_exits(renv):
    view = renv.evaluate("table/ns-rec/to_record-csv")
    pd = pytest.importorskip("pandas")
    pytest.importorskip("pyarrow")
    df = view.to_pandas()
    assert isinstance(df, pd.DataFrame) and list(df["id"]) == ["a", "b", "c"]
    pl = pytest.importorskip("polars")
    frame = view.to_polars()
    assert isinstance(frame, pl.DataFrame) and frame["amount"].to_list() == [1, 2, None]


def test_RECORDS_exits_name_their_extra(renv, monkeypatch):
    view = renv.evaluate("table/ns-rec/to_record-csv")
    has_polars = "polars" in sys.modules or _importable("polars")
    monkeypatch.setitem(sys.modules, "pyarrow", None)
    with pytest.raises(lq.ConversionError, match=r"liquers-py\[arrow\]"):
        view.to_arrow()
    with pytest.raises(lq.ConversionError, match=r"liquers-py\[(pandas|arrow)\]"):
        view.to_pandas()
    with pytest.raises(lq.ConversionError, match=r"liquers-py\[arrow\]"):
        lq.RecordView.from_arrow(object())
    if has_polars:
        import polars as pl

        built = lq.RecordView.from_polars(pl.DataFrame({"a": [1, 2]}))  # no pyarrow needed
        assert built.to_dict() == {"a": [1, 2]}
        assert view.to_polars()["id"].to_list() == ["a", "b", "c"]
    monkeypatch.setitem(sys.modules, "polars", None)
    with pytest.raises(lq.ConversionError, match=r"liquers-py\[polars\]"):
        view.to_polars()


def _importable(name):
    try:
        __import__(name)
        return True
    except ImportError:
        return False


def test_RECORDS_arrow_table_enters_records():
    pa = pytest.importorskip("pyarrow")
    table = pa.table(
        {
            "i8": pa.array([1, None], type=pa.int8()),
            "u16": pa.array([1, 2], type=pa.uint16()),
            "f32": pa.array([0.5, None], type=pa.float32()),
            "big": pa.array(["x", "y"], type=pa.large_string()),
            "dict": pa.array(["a", "a"]).dictionary_encode(),
            "d64": pa.array([0, 86_400_000], type=pa.date64()),
            "ms": pa.array([1, 2], type=pa.timestamp("ms", tz="UTC")),
            "vec": pa.array([[1.0, 2.0], [3.0, 4.0]], type=pa.list_(pa.float64(), 2)),
        }
    )
    if hasattr(pa, "string_view"):
        table = table.append_column("sv", pa.array(["p", "q"], type=pa.string_view()))
    view = lq.RecordView.from_arrow(table)
    types = {f.name: f.type for f in view.schema.fields}
    assert types["i8"] == lq.FieldType.Int and types["u16"] == lq.FieldType.UInt
    assert types["f32"] == lq.FieldType.Float and types["big"] == lq.FieldType.Text
    assert types["dict"] == lq.FieldType.Text and types["d64"] == lq.FieldType.Date
    assert types["ms"] == lq.FieldType.Timestamp and types["vec"] == lq.FieldType.Vector
    assert view.column("i8").to_list() == [1, None]
    assert memoryview(view.column("ms")).tolist() == [1000, 2000]
    if "sv" in types:
        assert view.column("sv").to_list() == ["p", "q"]

    # A table crossing into a query enters records too.
    env = lq.Environment()

    @env.first_command
    def arrow_table():
        return table

    assert isinstance(env.evaluate("arrow_table"), lq.RecordView)
    assert env.evaluate("arrow_table/ns-rec/to_json") is not None
    env.shutdown()

    # A compressed feather is read: pyarrow decompresses before liquers sees it.
    import io

    import pyarrow.feather as feather

    sink = io.BytesIO()
    feather.write_feather(pa.table({"a": [1, 2]}), sink, compression="lz4")
    compressed = feather.read_table(io.BytesIO(sink.getvalue()))
    assert lq.RecordView.from_arrow(compressed).to_dict() == {"a": [1, 2]}


def test_RECORDS_lossy_timestamp_is_refused():
    pa = pytest.importorskip("pyarrow")
    table = pa.table({"t": pa.array([1], type=pa.timestamp("ns"))})
    with pytest.raises(lq.ConversionError, match="'t'"):
        lq.RecordView.from_arrow(table)
    nested = pa.table({"l": pa.array([[1, 2]], type=pa.list_(pa.int64()))})
    with pytest.raises(lq.ConversionError, match="'l'"):
        lq.RecordView.from_arrow(nested)


def test_RECORDS_polars_unbridgeable_column():
    pl = pytest.importorskip("polars")
    with pytest.raises(lq.ConversionError, match="'l'"):
        lq.RecordView.from_polars(pl.DataFrame({"l": [[1, 2], [3]]}))


def test_RECORDS_pandas_nullable_int_needs_a_declaration():
    pd = pytest.importorskip("pandas")
    pytest.importorskip("pyarrow")
    frame = pd.DataFrame({"n": [1, None]})
    assert lq.RecordView.from_pandas(frame).schema.field("n").type == lq.FieldType.Float
    declared = lq.RecordView.from_pandas(frame, fields={"n": "int"})
    assert declared.schema.field("n").type == lq.FieldType.Int
    assert declared.column("n").to_list() == [1, None]
