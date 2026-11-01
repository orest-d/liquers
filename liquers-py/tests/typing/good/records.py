"""Record views, columns, buffers and sources."""

from typing import Any, assert_type

import liquers as lq

view = lq.RecordView.from_records([{"id": "a", "n": 1}], fields={"n": "float"})
assert_type(view, lq.RecordView)
assert_type(lq.RecordView.from_dict({"id": ["a"]}, fields={"id": lq.FieldType.Text}), lq.RecordView)
schema = lq.RecordSchema([lq.FieldSchema(lq.FieldType.Text, name="id", key=lq.KeyRole.Id)])
assert_type(lq.RecordView.from_records([{"id": "a"}], fields=schema), lq.RecordView)
assert_type(view.schema, lq.RecordSchema)
assert_type(view.schema.field("n").type, lq.FieldType)
assert_type(view.schema.names, list[str])
assert_type(len(view), int)
assert_type(view.to_list(), list[dict[str, Any]])
assert_type(view.to_dict(), dict[str, list[Any]])
assert_type(view.item(), Any)
assert_type(view.select_columns("id", 1).slice(0, 1), lq.RecordView)
assert_type(view.chunk_id, lq.Key | lq.Query | None)
assert_type(view.origins, list[lq.ChunkOrigin])
column = view.column("n")
assert_type(column, lq.RecordColumn)
assert_type(column[0], Any)
assert_type(column.data, lq.RecordBuffer | None)
assert_type(memoryview(column).nbytes, int)
assert_type(view.to_arrow(), Any)
assert_type(view.to_pandas(), Any)
assert_type(view.to_polars(), Any)
assert_type(lq.RecordView.from_arrow(object()), lq.RecordView)

env = lq.Environment()
source = env.evaluate("manifest/ns-rec/to_record_source")
if isinstance(source, lq.RecordSource):
    assert_type(source.chunks(), list[lq.RecordChunk])
    assert_type(source.bounded, bool)
    assert_type(source.manifest, dict[str, Any] | None)
    for chunk in source:
        assert_type(chunk, lq.RecordView)
    assert_type(source.materialize(100), lq.RecordView)

    async def walk(source: lq.RecordSource) -> int:
        rows = 0
        async for chunk in source:
            assert_type(chunk, lq.RecordView)
            rows += len(chunk)
        return rows
