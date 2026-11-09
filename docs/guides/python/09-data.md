---
title: Working with data
status: draft
sources: [reference/POLARS_COMMAND_LIBRARY.md, guides/RECORD_STREAM_GUIDE.md, guides/PYTHON_GUIDE.md, reference/PYTHON_INTEGRATION.md]
reviewed: 2026-11-09
---
# 9. Working with data

PURPOSE: The reader can load, filter and save tables with the command libraries, move them to and from pandas, Polars and pyarrow, read large tables in chunks, and work with images.

Liquers comes with command libraries for the data most programs handle: tables and images. The
commands are written in Rust, so they are fast and need no Python packages. You reach them from
a query, and you can mix them with commands of your own.

There are two libraries for tables. *Records* (`ns-rec`) are built into Liquers and give you an
`lq.RecordView`. *Polars* (`ns-pl`) uses the Polars engine and gives you a `polars.DataFrame`.
Start with records, which are always there; use Polars when you need its speed on large tables
or already work with it.

## 9.1 A first table

```python
import liquers as lq

env = lq.Environment()
env.store.set("data/sales.csv", b"region,month,amount\nEU,jan,120\nUS,jan,80\nEU,feb,95\n")

table = env.evaluate("-R/data/sales.csv/-/ns-rec/to_record")
assert len(table) == 3
assert table.schema.names == ["region", "month", "amount"]
assert table.column("amount").to_list() == [120, 80, 95]
assert table.to_list()[0] == {"region": "EU", "month": "jan", "amount": 120}

eu = env.evaluate("-R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/sort-amount")
assert eu.to_dict() == {"region": ["EU", "EU"], "month": ["feb", "jan"], "amount": [95, 120]}
env.shutdown()
```

`to_record` reads the bytes of the file as a table; the extension of the key tells it that they
are CSV. The result is a *record view*: a table with named, typed columns. `amount` was
recognized as a column of integers.

The commands you will use most, all in `ns-rec`:

| Command | Keeps or returns |
|---|---|
| `eq-column-value`, `ne`, `gt`, `gte`, `lt`, `lte` | the rows where the column compares so to the value |
| `contains-column-text`, `starts_with`, `ends_with` | the rows where the text column matches |
| `is_null-column`, `is_not_null-column` | the rows where the column is empty, or is not |
| `select_columns-a-b`, `drop_columns-a-b` | some of the columns |
| `sort-column`, `sort-column-f` | the rows in ascending order; `f` for descending |
| `head-n`, `tail-n`, `slice-offset-length` | some of the rows |
| `nrows`, `ncols`, `shape` | the size, as numbers |
| `to_json` | the rows as a list of dictionaries |

Appendix B has the complete list with all arguments, and
`lq.evaluate("ns-search/commands-rec")` returns it as a table.

## 9.2 Record views in Python

An `lq.RecordView` is a table you can read directly, without another package:

| To get | Use |
|---|---|
| the number of rows | `len(view)` |
| the columns and their types | `view.schema.names`, `view.schema.field(name).type` |
| one column | `view.column(name)`, then `.to_list()` |
| one cell | `view.value(row, column)` |
| all rows as dictionaries | `view.to_list()` |
| all columns as lists | `view.to_dict()` |
| a part | `view.select_columns([...])`, `view.slice(offset, length)`, `view.row(i)` |

Cells arrive as plain Python values: `None` for an empty cell, `bool`, `int`, `float`, `str`,
`bytes`, a `datetime.date` for a date, and a timezone-aware `datetime.datetime` for a timestamp.

To make a table in a command, build a view from rows:

```python
import liquers as lq

env = lq.Environment()

@env.first_command
def sales():
    return lq.RecordView.from_records(
        [{"region": "north", "amount": 120}, {"region": "south", "amount": 80.5}],
        fields={"amount": "float"},
    )

view = env.evaluate("sales")
assert view.schema.field("amount").type == lq.FieldType.Float
assert view.to_list() == [{"region": "north", "amount": 120.0}, {"region": "south", "amount": 80.5}]
assert env.evaluate("sales/ns-rec/nrows") == 2                   # a library command on your table
assert env.evaluate("sales/ns-rec/head-1").to_dict() == {"region": ["north"], "amount": [120.0]}
env.shutdown()
```

The type of each column is worked out from the values. A column must hold one type, except that
integers and floats together make a float column. `fields=` declares a type where you want to be
sure; a declared type converts the values and refuses one that would change (`"1"` becomes `1`
in an `int` column, `1.5` is refused there).

One thing may surprise you: a view with exactly one row and one column reaches Python as that
single value, which is why `nrows` returned the number 2 and not a table. To keep it a table,
use `env.evaluate_state(query).record_view()`.

Numeric columns can be handed to NumPy without a copy. A column supports the buffer protocol, so
`numpy.asarray(view.column("amount"))` shares the column's memory. The array is read-only.

## 9.3 pandas, Polars and pyarrow

A record view converts to and from the table types of the Python data ecosystem:

| From a view | To a view |
|---|---|
| `view.to_pandas()` | `lq.RecordView.from_pandas(df)` |
| `view.to_polars()` | `lq.RecordView.from_polars(df)` |
| `view.to_arrow()` | `lq.RecordView.from_arrow(table)` |

Each needs its package installed (the extras of chapter 2) and copies the data. Each `from_`
function accepts `fields=` like `from_records`.

```python
import liquers as lq
import pandas as pd

env = lq.Environment()
env.store.set("data/sales.csv", b"region,month,amount\nEU,jan,120\nUS,jan,80\nEU,feb,95\n")

@env.command
def total_by_region(table):
    df = table.to_pandas()
    totals = df.groupby("region", as_index=False)["amount"].sum()
    return lq.RecordView.from_pandas(totals)

totals = env.evaluate("-R/data/sales.csv/-/ns-rec/to_record/total_by_region")
assert totals.to_list() == [{"region": "EU", "amount": 215}, {"region": "US", "amount": 80}]
env.shutdown()
```

This is the usual shape of a Python command on tables: take a view, do the work in the package
you like, and return a view. Returning a view keeps the result usable by every other command and
storable in any table format. A `pyarrow.Table` returned from a command becomes a view by
itself.

A pandas integer column with a missing value is a float column in pandas, so it arrives as
`Float` unless `fields=` declares it `"int"`.

## 9.4 Polars data frames

The commands of `ns-pl` work on Polars data frames. A `polars.DataFrame` passes between your
commands and the library's in both directions:

```python
import liquers as lq
import polars as pl

env = lq.Environment()
env.store.set("data/sales.csv", b"region,month,amount\nEU,jan,120\nUS,jan,80\nEU,feb,95\n")

frame = env.evaluate("-R/data/sales.csv/-/ns-pl/from_csv/gt-amount-90")
assert isinstance(frame, pl.DataFrame) and frame.height == 2

@env.first_command
def numbers() -> pl.DataFrame:
    return pl.DataFrame({"a": [1, 2, 3]})

assert env.evaluate("numbers/ns-pl/head-2").height == 2      # your frame, a library command
env.shutdown()
```

`ns-pl` has the same filters as `ns-rec` (`eq`, `gt`, `contains`, …), `select_columns`, `sort`,
`head`, `tail`, and the aggregations `sum`, `mean`, `median`, `min`, `max`, `std`, `count` and
`describe`. `from_csv` reads a frame and `to_csv` writes one.

Receiving a data frame in Python needs the `polars` package. Without it, stay in `ns-rec`, or end
the query with a command that returns something plain, such as `ns-pl/nrows`.

## 9.5 Large tables in chunks

A table too large to hold at once is read as a *record source*: a sequence of chunks, each of
them a view. A source is described by a *manifest*, which lists the query that makes each chunk:

```python
import liquers as lq

env = lq.Environment()

@env.first_command
def part(i: int = 0):
    return lq.RecordView.from_records([{"id": f"r{i}", "n": float(i)}])

@env.first_command
def manifest():
    return {"manifest": "record-stream", "chunks": [{"query": f"part-{i}"} for i in range(3)]}

source = env.evaluate("manifest/ns-rec/to_record_source")
assert [c.query.encode() for c in source.chunks()] == ["part-0", "part-1", "part-2"]  # nothing runs

for chunk in source:                        # one chunk at a time
    print(chunk.to_list())

assert source.chunk(2).to_dict() == {"id": ["r2"], "n": [2.0]}     # only chunk 2 is computed
assert len(source.materialize()) == 3                              # everything, as one view
env.shutdown()
```

Iterating reads one chunk per step, so memory holds one chunk at a time. Each chunk is the
result of a query, and so each chunk is cached and tracked like any other result. In async code,
use `async for chunk in source` and `await source.chunk_async(i)`.

A manifest is more often a file than a command: a file named `<name>.manifest.yaml` in the
store, with the same content. Its chunks then get keys of their own and can be read like files.

## 9.6 Images

With pillow installed, a PIL image passes between Python and the image commands of `ns-img`:

```python
import liquers as lq
from PIL import Image

env = lq.Environment()

@env.first_command
def red() -> Image.Image:
    return Image.new("RGB", (40, 20), "red")

small = env.evaluate("red/ns-img/resize-20-10")
assert small.size == (20, 10)
assert env.evaluate("red/ns-img/dims") == "40x20"
png = env.evaluate("red/ns-img/gray/to_png")          # the bytes of a PNG file
assert png[:4] == b"\x89PNG"
env.shutdown()
```

`ns-img` covers the common operations: `resize`, `thumb`, `crop`, `rotate`, `fliph` and `flipv`,
`gray`, `blur`, `sharpen`, `brighten`, `contrast`, drawing commands, and conversion to PNG, JPEG,
WebP and other formats. An image stored under a key such as `pictures/cat.png` is read with
`ns-img/from_bytes`. Appendix B lists every command.

## 9.7 Saving results

The file name at the end of a query chooses the format in which a result is written. Together
with a recipe (chapter 7), that is all it takes to save a result:

```yaml
recipes:
  - query: -R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/eu.csv
  - query: -R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/eu.json
```

A record view can be written as CSV, as JSON and as an HTML table (`eu.html`), among others. To write a value from Python
instead, use `env.store_set(key, value)` (chapter 6).
