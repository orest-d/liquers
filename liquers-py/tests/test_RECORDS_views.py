"""RECORDS: record views reaching Python, and the schema classes (LANGUAGE-INTEGRATION_GUIDE.md §RECORDS).

Design: specs/design/liquers-py-records/phase3-examples.md, "test_RECORDS_views.py".
"""

import pytest

import liquers as lq


# --- The schema classes -----------------------------------------------------------------------


def test_RECORDS_schema_classes_construct():
    field = lq.FieldSchema("text", key="id")
    assert field.type == lq.FieldType.Text
    assert field.key == lq.KeyRole.Id
    assert field.name is None and field.label is None
    assert field.nullable is True and field.description == ""

    named = lq.FieldSchema(lq.FieldType.Float, name="unit_price", description="Price per unit", nullable=False)
    assert named.label == "unit price"
    assert named.key == lq.KeyRole.Data
    assert repr(named) == "FieldSchema('float', name='unit_price', nullable=False)"

    schema = lq.RecordSchema(
        [lq.FieldSchema("text", name="id", key=lq.KeyRole.Id), named, lq.FieldSchema("uint", name="src", key="source")],
        type_identifier="Sales",
    )
    assert schema.names == ["id", "unit_price", "src"]
    assert len(schema) == 3
    assert schema.type_identifier == "Sales"
    assert schema.field("unit_price").label == "unit price"
    assert schema.field("src").key == lq.KeyRole.Source
    assert [f.type for f in schema.fields] == [lq.FieldType.Text, lq.FieldType.Float, lq.FieldType.UInt]
    with pytest.raises(KeyError):
        schema.field("missing")

    assert str(lq.FieldType.Timestamp) == "timestamp"
    assert str(lq.KeyRole.Data) == "data"
    with pytest.raises(lq.ConversionError):
        lq.FieldSchema("decimal")
    with pytest.raises(lq.ConversionError):
        lq.FieldSchema("int", key="primary")


def test_RECORDS_schema_to_dict_matches_liquers_schema():
    schema = lq.RecordSchema(
        [lq.FieldSchema("text", name="id", key="id", label="Identifier"), lq.FieldSchema("int", name="n")]
    )
    data = schema.to_dict()
    assert [f["name"] for f in data["fields"]] == ["id", "n"]
    first, second = data["fields"]
    assert first["data_type"] == "Text" and first["key"] == "Id" and first["label"] == "Identifier"
    assert second["data_type"] == "Int" and second["key"] == "None" and second["nullable"] is True
    assert schema == lq.RecordSchema(
        [lq.FieldSchema("text", name="id", key="id", label="Identifier"), lq.FieldSchema("int", name="n")]
    )


def test_RECORDS_schema_refuses_duplicate_id():
    with pytest.raises(lq.LiquersError):
        lq.RecordSchema([lq.FieldSchema("text", name="a", key="id"), lq.FieldSchema("text", name="b", key="id")])
    with pytest.raises(lq.LiquersError):
        lq.RecordSchema([lq.FieldSchema("text", name="a"), lq.FieldSchema("int", name="a")])
    with pytest.raises(lq.LiquersError):
        lq.RecordSchema([lq.FieldSchema("text")])  # a field in a RecordSchema needs a name


# --- Views ---------------------------------------------------------------------------------------

import datetime
from pathlib import Path

FIXTURE = Path(__file__).parent / "data" / "records_all_types.feather"
TABLE_CSV = b"id,amount\na,1\nb,2\nc,\n"


@pytest.fixture
def renv():
    """An environment with the record fixtures: ``table`` (CSV bytes) and ``ipc_bytes`` (the
    committed all-types feather file, written by liquers-records' ``ipc_writes_the_python_fixture``)."""
    e = lq.Environment()

    @e.first_command
    def table() -> bytes:
        return TABLE_CSV

    @e.first_command
    def ipc_bytes() -> bytes:
        return FIXTURE.read_bytes()

    yield e
    e.shutdown()


def fixture_view(renv):
    return renv.evaluate("ipc_bytes/ns-rec/to_record-feather")


def test_RECORDS_view_reaches_python(renv):
    view = renv.evaluate("table/ns-rec/to_record-csv")
    assert isinstance(view, lq.RecordView) and len(view) == 3
    assert view.schema.names == ["id", "amount"]
    assert view.num_columns == 2
    assert view.to_list() == [{"id": "a", "amount": 1}, {"id": "b", "amount": 2}, {"id": "c", "amount": None}]

    state = renv.evaluate_state("table/ns-rec/to_record-csv")
    assert isinstance(state.value, lq.RecordView)
    assert state.metadata.type_identifier() == "RecordView"
    assert len(state.record_view()) == 3
    assert renv.evaluate_state("table").record_view() is None


def test_RECORDS01_view_round_trips_with_identity(renv):
    seen = {}

    @renv.first_command
    def produce():
        return fixture_view(renv)

    @renv.command
    def inspect(value):
        seen["chunk_id"] = value.chunk_id
        seen["origins"] = value.origins
        seen["id_key"] = value.schema.field("id").key
        return len(value)

    original = fixture_view(renv)
    assert renv.evaluate("produce/inspect") == 4
    assert seen["chunk_id"] == original.chunk_id
    assert isinstance(seen["chunk_id"], lq.Key) and seen["chunk_id"].encode() == "fx/part-0.csv"
    assert seen["origins"] == original.origins and len(seen["origins"]) == 2
    assert seen["origins"][0].asset.encode() == "-R/fx/source-a.csv"
    assert seen["origins"][1].to_dict()["chunk"] == "-R/fx/source-b.csv"
    assert seen["id_key"] == lq.KeyRole.Id


def test_RECORDS10_single_cell_is_a_scalar(renv):
    query = "table/ns-rec/to_record-csv/select_columns-amount/row-1"
    assert renv.evaluate(query) == 2
    state = renv.evaluate_state(query)
    assert state.value == 2
    view = state.record_view()
    assert isinstance(view, lq.RecordView) and len(view) == 1
    assert view.item() == 2


def test_RECORDS_item_on_a_larger_view_names_its_shape(renv):
    view = renv.evaluate("table/ns-rec/to_record-csv/slice-0-2")
    with pytest.raises(lq.ConversionError, match="2 rows x 2 fields"):
        view.item()


def test_RECORDS_narrowing_matches_commands(renv):
    view = renv.evaluate("table/ns-rec/to_record-csv")
    assert view.slice(0, 2).to_list() == renv.evaluate_state("table/ns-rec/to_record-csv/slice-0-2").record_view().to_list()
    assert (
        view.select_columns("id", "amount").to_list()
        == renv.evaluate_state("table/ns-rec/to_record-csv/select_columns-id-amount").record_view().to_list()
    )
    assert view.slice(1, 100).to_dict() == {"id": ["b", "c"], "amount": [2, None]}
    narrowed = view.select_columns("amount").row(1)
    assert isinstance(narrowed, lq.RecordView)  # narrowing never collapses to a scalar
    assert narrowed.item() == renv.evaluate("table/ns-rec/to_record-csv/select_columns-amount/row-1")
    assert view.cell(1, "amount").item() == 2
    assert view.value(1, "amount") == 2 and view.value(-1, 0) == "c"
    with pytest.raises(lq.LiquersError):
        view.select_columns("missing")
    with pytest.raises(lq.LiquersError):
        view.row(3)


def test_RECORDS_fixture_cells_reach_python_typed(renv):
    view = fixture_view(renv)
    rows = view.to_list()
    assert [row["id"] for row in rows] == ["a", "b", "c", "d"]
    first = rows[0]
    assert first["flag"] is True and first["count"] == 1 and first["score"] == 0.5
    assert first["name"] == "alpha" and first["blob"] == b"\x00\x01"
    assert first["day"] == datetime.date(2023, 12, 9)
    assert first["at"] == datetime.datetime(2023, 11, 14, 22, 13, 20, 1, tzinfo=datetime.timezone.utc)
    assert first["vec"] == [1.0, 2.0]
    assert all(value is None for key, value in rows[2].items() if key != "id")
    assert rows[3]["count"] == 2**63 - 1 and rows[3]["name"] == "été"
    assert rows[3]["day"] == datetime.date(1969, 12, 31)
    assert view.schema.field("score").label == "Score"
    assert view.schema.field("src").key == lq.KeyRole.Source
