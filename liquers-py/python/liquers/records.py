"""Record streams: columnar tables and their schemas, with conversions to and from pyarrow, pandas and polars.

Rust counterpart: the `liquers-records` crate. Every name here is also available directly from `liquers`
(`liquers.RecordView is liquers.records.RecordView`); `import liquers as lq` and `lq.RecordView` stays
the usual style.

# Main components
- `RecordView`: a table, built from lists, dicts or data frames.
- `RecordSource`, `RecordStream`, `RecordChunk`: tables read in chunks.
- `RecordColumn`, `RecordBuffer`: one column and its memory.
- `RecordSchema`, `FieldSchema`, `FieldType`, `KeyRole`, `ChunkOrigin`: schemas and their fields.

# Where it is used
- The `ns-rec` commands produce and consume record views.

# Related concepts
- A record view crosses to Rust without copying its columns.

# Examples
>>> from liquers.records import RecordView
>>> RecordView.from_records([{"a": 1}, {"a": 2}]).to_list()
[{'a': 1}, {'a': 2}]
"""

from liquers._backend import (
    RecordView,
    RecordSource,
    RecordStream,
    RecordChunk,
    RecordColumn,
    RecordBuffer,
    RecordSchema,
    FieldSchema,
    FieldType,
    KeyRole,
    ChunkOrigin,
)

__all__ = [
    "RecordView",
    "RecordSource",
    "RecordStream",
    "RecordChunk",
    "RecordColumn",
    "RecordBuffer",
    "RecordSchema",
    "FieldSchema",
    "FieldType",
    "KeyRole",
    "ChunkOrigin",
]
