"""The pyarrow side of the record classes: Arrow tables in and out, and pandas through Arrow.

Rust writes and reads Arrow IPC (Feather v2) bytes; this module turns those bytes into a
``pyarrow.Table`` and back. On the way in it first normalizes the columns liquers' IPC reader
does not take, when no value is lost:

- ``large_string`` / ``string_view`` to ``string``, ``large_binary`` / ``binary_view`` to ``binary``;
- ``int8`` to ``int32`` to ``int64``, ``uint8`` to ``uint32`` to ``uint64``;
- ``float16`` and ``float32`` to ``float64``;
- dictionary-encoded columns decoded;
- ``date64`` to ``date32``;
- timestamps of any unit to microseconds, refused when that drops a value;
- a fixed-size list of ``float64`` (or ``float16``) to one of ``float32``, as liquers stores vectors.

Anything else (a nested list, a struct, a decimal, a duration, an overflow) is a
``ConversionError`` naming the column. pyarrow and pandas are imported inside each function,
through :func:`liquers.require`, so a missing package raises ``ConversionError`` naming the extra
that provides it. See ``specs/design/liquers-py-records/phase2-architecture.md`` §Arrow.
"""

from __future__ import annotations

import json
from typing import Any

from liquers._extras import require

CHUNK_ID_KEY = b"liquers.chunk_id"
ORIGINS_KEY = b"liquers.origins"


def _conversion_error(message: str) -> Exception:
    import liquers

    error: Exception = liquers.ConversionError(message)
    return error


def _require(extra: str) -> None:
    try:
        require(extra)
    except ModuleNotFoundError as error:
        raise _conversion_error(str(error)) from None


def _pyarrow() -> Any:
    _require("arrow")
    import pyarrow

    return pyarrow


def ipc_to_arrow(data: bytes, chunk_id: str | None, origins: str | None) -> Any:
    """The ``pyarrow.Table`` of an IPC file, with the batch's chunk id and origins (which
    ``read_all()`` drops with the batch message) copied into the schema metadata."""
    pa = _pyarrow()
    table = pa.ipc.open_file(pa.BufferReader(data)).read_all()
    metadata = dict(table.schema.metadata or {})
    if chunk_id is not None:
        metadata[CHUNK_ID_KEY] = chunk_id.encode()
    if origins is not None:
        metadata[ORIGINS_KEY] = origins.encode()
    return table.replace_schema_metadata(metadata)


def _normalize(pa: Any, name: str, column: Any) -> Any:
    """``column`` in a type liquers' IPC reader takes, or a ConversionError naming it."""
    import pyarrow.compute as pc

    t = column.type
    if pa.types.is_dictionary(t):
        column = column.dictionary_decode() if hasattr(column, "dictionary_decode") else pc.cast(column, t.value_type)
        return _normalize(pa, name, column)
    if pa.types.is_large_string(t) or getattr(pa.types, "is_string_view", lambda _: False)(t):
        return column.cast(pa.string())
    if pa.types.is_large_binary(t) or getattr(pa.types, "is_binary_view", lambda _: False)(t):
        return column.cast(pa.binary())
    if pa.types.is_integer(t) and t.bit_width < 64:
        return column.cast(pa.int64() if pa.types.is_signed_integer(t) else pa.uint64())
    if pa.types.is_floating(t) and t.bit_width < 64:
        return column.cast(pa.float64())
    if pa.types.is_date64(t):
        return column.cast(pa.date32())
    if pa.types.is_timestamp(t):
        if t.unit == "us":
            return column
        try:
            return column.cast(pa.timestamp("us", tz=t.tz), safe=True)
        except (pa.ArrowInvalid, ValueError) as error:
            raise _conversion_error(
                f"column {name!r}: {t} cannot be stored in microseconds without losing a value ({error})"
            ) from None
    if pa.types.is_fixed_size_list(t) and pa.types.is_floating(t.value_type):
        if t.value_type == pa.float32():
            return column
        return column.cast(pa.list_(pa.float32(), t.list_size))
    if (
        pa.types.is_boolean(t)
        or pa.types.is_integer(t)
        or pa.types.is_floating(t)
        or pa.types.is_string(t)
        or pa.types.is_binary(t)
        or pa.types.is_date32(t)
        or pa.types.is_null(t)
    ):
        return column
    raise _conversion_error(f"column {name!r}: Arrow type {t} has no record field type")


def arrow_to_ipc(table: Any) -> bytes:
    """An uncompressed IPC file of ``table`` (a ``pyarrow.Table`` or ``RecordBatch``), normalized."""
    pa = _pyarrow()
    if isinstance(table, pa.RecordBatch):
        table = pa.Table.from_batches([table])
    columns = []
    for name, column in zip(table.column_names, table.columns):
        normalized = _normalize(pa, name, column)
        if pa.types.is_null(normalized.type):
            normalized = normalized.cast(pa.string())
        columns.append(normalized)
    normalized = pa.Table.from_arrays(columns, names=table.column_names)
    metadata = table.schema.metadata
    if metadata:
        normalized = normalized.replace_schema_metadata(metadata)
    sink = pa.BufferOutputStream()
    with pa.ipc.new_file(sink, normalized.schema) as writer:
        writer.write_table(normalized)
    data: bytes = sink.getvalue().to_pybytes()
    return data


def pandas_to_arrow(df: Any) -> Any:
    """``df`` as a ``pyarrow.Table``, without its index."""
    _require("pandas")
    import pyarrow as pa

    return pa.Table.from_pandas(df, preserve_index=False)


def arrow_to_pandas(table: Any) -> Any:
    """A ``pandas.DataFrame`` of ``table``."""
    _require("pandas")
    return table.to_pandas()


def ipc_to_polars(data: bytes) -> Any:
    """A ``polars.DataFrame`` read from IPC bytes."""
    _require("polars")
    import io

    import polars

    return polars.read_ipc(io.BytesIO(data))


def polars_to_parquet(df: Any) -> bytes:
    """``df`` as Parquet bytes, the form DataFrames already cross in."""
    _require("polars")
    import io

    buffer = io.BytesIO()
    df.write_parquet(buffer)
    return buffer.getvalue()


