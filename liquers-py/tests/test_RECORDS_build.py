"""RECORDS: building record views from Python data (AC-16, AC-17, AC-18).

Design: specs/design/liquers-py-records/phase3-examples.md, "test_RECORDS_build.py".
"""

import datetime

import pytest

import liquers as lq

ROWS = [{"id": "a", "n": 1}, {"id": "b"}]
COLUMNS = {"id": ["a", "b"], "n": [1, None]}


def _pandas():
    pd = pytest.importorskip("pandas")
    pytest.importorskip("pyarrow")
    return pd.DataFrame({"id": ["a", "b"], "n": pd.array([1, None], dtype="Int64")})


def _polars():
    pl = pytest.importorskip("polars")
    return pl.DataFrame({"id": ["a", "b"], "n": [1, None]})


def _arrow():
    pa = pytest.importorskip("pyarrow")
    return pa.table({"id": ["a", "b"], "n": pa.array([1, None], type=pa.int64())})


ROUTES = {
    "from_records": lambda: lq.RecordView.from_records(ROWS),
    "from_dict": lambda: lq.RecordView.from_dict(COLUMNS),
    "from_pandas": lambda: lq.RecordView.from_pandas(_pandas()),
    "from_polars": lambda: lq.RecordView.from_polars(_polars()),
    "from_arrow": lambda: lq.RecordView.from_arrow(_arrow()),
}


def shape(view):
    """What AC-16 compares: names, types and values (nullability is inferred per route)."""
    return [(f.name, f.type) for f in view.schema.fields], view.to_dict()


@pytest.mark.parametrize("route", list(ROUTES))
def test_RECORDS_constructors_agree(route):
    expected = shape(lq.RecordView.from_records(ROWS))
    assert expected == ([("id", lq.FieldType.Text), ("n", lq.FieldType.Int)], COLUMNS)
    assert shape(ROUTES[route]()) == expected


def test_RECORDS_to_list_and_to_dict_round_trip():
    rows = [
        {
            "flag": True,
            "n": 1,
            "x": 0.5,
            "s": "text",
            "b": b"\x00",
            "d": datetime.date(2026, 11, 1),
            "t": datetime.datetime(2026, 11, 1, 12, 30, tzinfo=datetime.timezone.utc),
        },
        {"flag": None, "n": None, "x": None, "s": None, "b": None, "d": None, "t": None},
    ]
    view = lq.RecordView.from_records(rows)
    assert [f.type for f in view.schema.fields] == [
        lq.FieldType.Bool,
        lq.FieldType.Int,
        lq.FieldType.Float,
        lq.FieldType.Text,
        lq.FieldType.Binary,
        lq.FieldType.Date,
        lq.FieldType.Timestamp,
    ]
    assert view.to_list() == rows
    again = lq.RecordView.from_records(view.to_list())
    assert again.schema == view.schema
    assert lq.RecordView.from_dict(view.to_dict()).to_list() == rows


def _declare(route, fields):
    if route == "from_records":
        return lq.RecordView.from_records([{"id": "a", "n": 1, "x": 2}, {"id": "b", "n": 2, "x": 3}], fields=fields)
    pd = pytest.importorskip("pandas")
    pytest.importorskip("pyarrow")
    frame = pd.DataFrame({"id": ["a", "b"], "n": [1, 2], "x": [2, 3]})
    return lq.RecordView.from_pandas(frame, fields=fields)


@pytest.mark.parametrize("route", ["from_records", "from_pandas"])
def test_RECORDS_fields_inferred_declared_or_given(route):
    inferred = _declare(route, None)
    assert [f.type for f in inferred.schema.fields] == [lq.FieldType.Text, lq.FieldType.Int, lq.FieldType.Int]

    declared = _declare(route, {"x": "float", "id": lq.FieldSchema("text", key="id", label="Identifier")})
    assert declared.schema.field("x").type == lq.FieldType.Float
    assert declared.schema.field("id").key == lq.KeyRole.Id and declared.schema.field("id").label == "Identifier"
    assert declared.to_dict()["x"] == [2.0, 3.0]

    schema = lq.RecordSchema(
        [lq.FieldSchema("text", name="id", key="id"), lq.FieldSchema("float", name="n"), lq.FieldSchema("text", name="x")]
    )
    given = _declare(route, schema)
    assert given.schema == schema
    assert given.to_dict() == {"id": ["a", "b"], "n": [1.0, 2.0], "x": ["2", "3"]}

    with pytest.raises(lq.ConversionError, match="field 'id', row 0"):
        _declare(route, {"id": "int"})


def test_RECORDS_inference_conflict_names_field_and_row():
    with pytest.raises(lq.ConversionError, match="field 'v', row 1"):
        lq.RecordView.from_records([{"v": "a"}, {"v": 1}])


def test_RECORDS_id_with_nulls_is_refused():
    with pytest.raises(lq.ConversionError, match="field 'id', row 1"):
        lq.RecordView.from_records([{"id": "a"}, {"id": None}], fields={"id": lq.FieldSchema("text", key="id")})


def test_RECORDS_int_out_of_range_names_field_and_row():
    with pytest.raises(lq.ConversionError, match="field 'n', row 1"):
        lq.RecordView.from_records([{"n": 1}, {"n": 2**64}])


def test_RECORDS_build_edge_cases():
    assert len(lq.RecordView.from_records([])) == 0
    empty = lq.RecordView.from_records([{}, {}])
    assert len(empty) == 0 and empty.num_columns == 0  # a table with no fields has no rows
    assert lq.RecordView.from_records([{"a": None}]).schema.field("a").type == lq.FieldType.Text
    assert lq.RecordView.from_records([{"a": 1}, {"a": 0.5}]).schema.field("a").type == lq.FieldType.Float
    vectors = lq.RecordView.from_records([{"v": [1.0, 2.0]}, {"v": [3, 4]}])
    assert vectors.schema.field("v").type == lq.FieldType.Vector and vectors.column("v").dim == 2
    with pytest.raises(lq.ConversionError, match="same length"):
        lq.RecordView.from_dict({"a": [1, 2], "b": [1]})
    with pytest.raises(lq.ConversionError):
        lq.RecordView.from_records([1, 2])
    with pytest.raises(lq.ConversionError):
        lq.RecordView.from_records([{"a": object()}])
    naive = lq.RecordView.from_records([{"t": datetime.datetime(2026, 1, 1)}])
    assert naive.value(0, "t") == datetime.datetime(2026, 1, 1, tzinfo=datetime.timezone.utc)
    missing = lq.RecordView.from_records([{"a": 1}], fields={"b": "text"})
    assert missing.to_dict() == {"a": [1], "b": [None]}
    with pytest.raises(lq.ConversionError, match="no such field"):
        lq.RecordView.from_records([{"a": 1}], fields=lq.RecordSchema([lq.FieldSchema("int", name="b")]))


def test_RECORDS_built_view_crosses_into_a_query():
    env = lq.Environment()
    built = lq.RecordView.from_records(ROWS, fields={"id": lq.FieldSchema("text", key="id")})

    @env.first_command
    def made():
        return built

    assert env.evaluate("made/ns-rec/to_json") is not None
    assert env.evaluate("made/ns-rec/row-1/select_columns-id") == "b"
    env.shutdown()
