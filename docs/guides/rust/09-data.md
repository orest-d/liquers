---
title: Working with data
status: ready
sources: [guides/RECORD_STREAM_GUIDE.md, reference/RECORD_STREAMS.md, reference/POLARS_COMMAND_LIBRARY.md, reference/IMAGE_COMMAND_LIBRARY.md]
reviewed: 2026-11-10
---
# 9. Working with data

PURPOSE: The reader can load, transform and save tables and images with the command libraries, and write commands that take and return them in Rust.

Most queries work on tables. Liquers has two table types. *Records* are its own: columnar,
typed, small in code size, and available on every platform including the browser. *Polars data
frames* are those of the Polars library: a full query engine, on native platforms only. The two
convert into each other, and the library has the same everyday commands for both. This chapter
covers both, then large tables, images, and saving.

## 9.1 A first table

```rust
use liquers_records::FieldValue;

let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
store
    .set(
        &parse_key("data/sales.csv")?,
        b"region,month,amount\nEU,jan,120\nUS,jan,80\nEU,feb,95\n",
        &Metadata::new(),
    )
    .await?;
let env = library_environment(store)?;

let state = env.evaluate("-R/data/sales.csv/-/ns-rec/to_record").await?.get().await?;
let table = state.value()?.as_record_view()?; // Arc<dyn RecordView>
assert_eq!(table.len(), 3);
let names: Vec<&str> = table.schema().fields.iter().map(|field| field.name.as_str()).collect();
assert_eq!(names, ["region", "month", "amount"]);
assert_eq!(table.value(0, 2)?, FieldValue::Int(120)); // row 0, column 2
assert_eq!(table.column(0)?.get(1)?, FieldValue::Text(Arc::from("US")));

let query = "-R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/sort-amount/eu.csv";
let eu = env.evaluate(query).await?.get().await?;
assert_eq!(
    String::from_utf8_lossy(&eu.as_bytes()?),
    "region,month,amount\nEU,feb,95\nEU,jan,120\n"
);
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
| `to_json` | the rows as a list of objects |

Appendix B has the complete list with all arguments.

## 9.2 Record views in Rust

A record view is an `Arc<dyn RecordView>`. The trait and everything around it are in the crate
`liquers-records`, which you add to your dependencies to name these types.

| To get | Use |
|---|---|
| the number of rows | `view.len()` |
| the columns and their types | `view.schema().fields`, each with a `name` and a `data_type` |
| one column | `view.column(i)?`, a `Column` |
| one cell | `view.value(row, column)?`, a `FieldValue` |
| a part, without copying | `view.select_columns(…)`, `view.slice(offset, length)`, `view.filter(&mask)`, `view.take(&rows)` |
| a copy that owns its data | `view.materialize()?`, a `RecordBatch` |

`FieldValue` is the type of one cell: `Null`, `Bool`, `Int`, `UInt`, `Float`, `Text`, `Bytes`,
`Date`, `Timestamp` and `Vector`. A column holds one `FieldType`.

Making and reading tables in commands looks like this:

```rust
use liquers_records::{
    CompareOp, FieldSchema, FieldType, FieldValue, RecordBatchMut, RecordSchema, RecordView,
    RecordViewMut,
};

// A command that makes a table.
fn sales() -> Result<Value, Error> {
    let schema = Arc::new(RecordSchema::new(vec![
        FieldSchema::new("region", FieldType::Text),
        FieldSchema::new("amount", FieldType::Float),
    ])?);
    let mut batch = RecordBatchMut::with_capacity(schema, 3);
    batch.append_row(&[FieldValue::Text(Arc::from("north")), FieldValue::Float(120.0)])?;
    batch.append_row(&[FieldValue::Text(Arc::from("south")), FieldValue::Float(80.5)])?;
    batch.append_row(&[FieldValue::Text(Arc::from("north")), FieldValue::Float(30.0)])?;
    let view: Arc<dyn RecordView> = Arc::new(batch.freeze()?);
    Ok(Value::from_record_view(view))
}

// A command that reads one: the sum of `amount` over the rows of one region.
fn total(state: &State<Value>, region: String) -> Result<Value, Error> {
    let table = state.value()?.as_record_view()?;
    let mask = table.column(0)?.compare(CompareOp::Eq, &FieldValue::Text(Arc::from(region)))?;
    let rows = table.filter(&mask)?; // a view: no data is copied
    let mut sum = 0.0;
    for row in 0..rows.len() {
        if let FieldValue::Float(amount) = rows.value(row, 1)? {
            sum += amount;
        }
    }
    Ok(Value::from(sum))
}

let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
let cr = &mut builder.command_registry;
register_command!(cr, fn sales() -> result)?;
register_command!(cr, fn total(state, region: String) -> result)?;
let env = builder.build()?;

let text = |query: &'static str| {
    let env = env.clone();
    async move { env.evaluate(query).await?.get().await?.try_into_string() }
};
assert_eq!(text("sales/total-north").await?, "150");
assert_eq!(text("sales/ns-rec/nrows").await?, "3"); // a library command on your table
assert_eq!(text("sales/ns-rec/gt-amount-100/total-north").await?, "120");
```

The pieces:

- **A schema** names and types the columns. `RecordSchema::new` checks it and refuses, for
  example, two columns of the same name. A field can carry more: a label for display, a role for
  search (chapter 13), and one field may be declared the identifier of a row.
- **`RecordBatchMut`** builds a table row by row. `append_row` is all or nothing: a row with a
  value of the wrong type is refused and leaves the batch as it was. `freeze` turns the builder
  into an immutable `RecordBatch`, which is a `RecordView`.
- **Views are lazy.** `filter`, `slice`, `select_columns` and `take` return a new view over the
  same data. Nothing is copied until you call `materialize`. Do that when you will read the
  result many times, because a view does not remember what it computed.
- **A mask** is a `Bitmap` with one bit for each row. `Column::compare` makes one, and `and`,
  `or` and `not` combine them.

What records deliberately do not have is grouping, joining and aggregating. That is the work of
a query engine, and for it there is Polars.

## 9.3 Polars data frames

The commands of `ns-pl` work on Polars data frames. A value holds one as
`Arc<polars::frame::DataFrame>`:

```rust
use polars::prelude::*;

fn numbers() -> Result<Value, Error> {
    let frame = df!("a" => [1, 2, 3]).map_err(|e| Error::from_error(ErrorType::General, e))?;
    Ok(Value::from_polars_dataframe(frame))
}

let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
store.set(&parse_key("data/sales.csv")?, SALES, &Metadata::new()).await?;
let mut builder = library_builder(store)?;
let cr = &mut builder.command_registry;
register_command!(cr, fn numbers() -> result)?;
let env = builder.build()?;

let state = env.evaluate("-R/data/sales.csv/-/ns-pl/from_csv/gt-amount-90").await?.get().await?;
assert_eq!(state.type_identifier(), "polars.DataFrame");
let frame = state.value()?.as_polars_dataframe()?; // Arc<DataFrame>
assert_eq!(frame.height(), 2);

let head = env.evaluate("numbers/ns-pl/head-2").await?.get().await?;
assert_eq!(head.value()?.as_polars_dataframe()?.height(), 2);

// The two table types convert at the namespace border.
let count = env.evaluate("numbers/ns-rec/nrows").await?.get().await?;
assert_eq!(count.try_into_string()?, "3");
```

Your crate needs `polars` as a dependency, in the version `liquers-lib` uses, to name its types.

`ns-pl` has the same filters as `ns-rec` (`eq`, `gt`, `contains`, …), `select_columns`, `sort`,
`head` and `tail`, with the same arguments, so that a query reads the same in either namespace.
It adds the aggregations `sum`, `mean`, `median`, `min`, `max`, `std`, `count` and `describe`.
`from_csv` reads a frame and `to_csv` writes one.

The two table types convert at the border between the namespaces: an `ns-pl` command accepts a
record view, and an `ns-rec` command accepts a data frame. The conversion copies each column. In
Rust the same is `liquers_lib::records::polars::record_batch_to_dataframe` and
`dataframe_to_record_batch`.

Which one to use? Return records from a command when you can: they work everywhere, including a
browser build, and every table format can write them. Use Polars inside a command for the heavy
lifting, and in queries for aggregation.

## 9.4 Large tables in chunks

A table too large to hold at once is a *record source*: something that can be asked, repeatedly,
for a stream of chunks, each of them a view. A source is usually described by a *manifest*, a
small file that lists the query which makes each chunk:

```rust
    use liquers_records::{FieldSchema, FieldType, FieldValue, RecordBatchMut, RecordSchema, RecordView, RecordViewMut};

    fn part(i: i64) -> Result<Value, Error> {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("n", FieldType::Int)])?);
        let mut batch = RecordBatchMut::with_capacity(schema, 2);
        batch.append_row(&[FieldValue::Int(10 * i)])?;
        batch.append_row(&[FieldValue::Int(10 * i + 1)])?;
        let view: Arc<dyn RecordView> = Arc::new(batch.freeze()?);
        Ok(Value::from_record_view(view))
    }

    let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
    let manifest = b"
manifest: record-stream
chunks:
  - query: part-0
  - query: part-1
  - query: part-2
";
    store.set(&parse_key("data/parts.manifest.yaml")?, manifest, &Metadata::new()).await?;
    let mut builder = library_builder(store)?;
    let cr = &mut builder.command_registry;
    register_command!(cr, fn part(i: i64 = 0) -> result)?;
    let env = builder.build()?;

    // The source: nothing is computed yet.
    let source = env.evaluate("-R/data/parts.manifest.yaml/-/ns-rec/to_record_source").await?.get().await?;
    assert_eq!(source.type_identifier(), "RecordSource");

    // Narrowed chunk by chunk, then gathered into one view.
    let query = "-R/data/parts.manifest.yaml/-/ns-rec/to_record_source/gt-n-10/materialize";
    let rows = env.evaluate(query).await?.get().await?.value()?.as_record_view()?;
    assert_eq!(rows.len(), 3); // 11, 20, 21
```

A file whose name ends in `.manifest.yaml` is recognized as a manifest. `to_record_source` turns
it into a source without running anything. The filters, `select_columns`, `drop_columns` and
`head` then work lazily: each chunk is narrowed as it streams past. `materialize` gathers what is
left into one view. `sort` and `tail` need the whole table, so narrow a stream first. `ns-pl`
refuses a source, because Polars commands are eager; `materialize` first.

Each chunk is the result of a query, so each chunk is cached and tracked like any other result.
That has a consequence for memory: with the default settings the environment keeps every chunk
it has computed. For a table that really does not fit into memory, set the query cache strategy
to `None` (section 7.10), and each chunk is released when the stream moves on.

A manifest can also give its chunks keys of their own, by a naming pattern, so that
`data/daily_0010.csv` can be read like a file. That is what the `manifest` recipe provider of
section 7.9 is for.

In Rust a source is an `Arc<dyn RecordSource>`, from `value.as_record_source()`. Its `stream`
method returns an asynchronous stream of views. It needs a *resolver*, the object that
evaluates the chunk queries: inside a command, a `ContextResolver` made from the context, so
that every chunk becomes a dependency of the command's result.

## 9.5 Images

A value holds an image as `Arc<image::DynamicImage>`, the type of the `image` crate:

```rust
use image::{DynamicImage, Rgb, RgbImage};

fn red() -> Result<Value, Error> {
    let picture = RgbImage::from_pixel(40, 20, Rgb([255, 0, 0]));
    Ok(Value::from_image(Arc::new(DynamicImage::ImageRgb8(picture))))
}

let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
let cr = &mut builder.command_registry;
register_command!(cr, fn red() -> result)?;
let env = builder.build()?;

let small = env.evaluate("red/ns-img/resize-20-10").await?.get().await?;
assert_eq!(small.value()?.as_image()?.width(), 20);
let dims = env.evaluate("red/ns-img/dims").await?.get().await?;
assert_eq!(dims.try_into_string()?, "40x20");
let png = env.evaluate("red/ns-img/gray/picture.png").await?.get().await?;
assert_eq!(&png.as_bytes()?[..4], b"\x89PNG"); // the file name chose the format
```

`ns-img` covers the common operations: `resize`, `thumb`, `crop`, `rotate`, `fliph` and `flipv`,
`gray`, `blur`, `sharpen`, `brighten`, `contrast`, drawing commands, and conversion to PNG, JPEG,
WebP and other formats. An image stored under a key such as `pictures/cat.png` arrives as bytes,
like every file that is not text or JSON, and `ns-img/from_bytes` reads it. Appendix B lists
every command.

## 9.6 Saving results

The file name at the end of a query chooses the format in which a result is written. Together
with a recipe (chapter 7), that is all it takes to save a result:

```yaml
recipes:
  - query: -R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/eu.csv
  - query: -R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/eu.json
```

A record view can be written as CSV, as JSON, as an HTML or Markdown table and, with the
features `records-ipc` and `records-parquet`, as Arrow IPC and Parquet. In code,
`liquers_records::write_table` writes a view in a `TableFormat` and `read_table` reads one. To
store a value from code, use the asset manager's `set_state` (section 7.6).

Formats differ in what survives a round trip. CSV keeps no types, so a column of text that looks
like numbers comes back as numbers. JSON cannot hold every float. When a table must come back
exactly as it was written, use Arrow IPC or Parquet.
