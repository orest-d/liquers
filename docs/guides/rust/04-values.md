---
title: Values, state and metadata
status: ready
sources: [reference/VALUE_TYPE_SYSTEM.md, guides/TYPE_SYSTEM_GUIDE.md, reference/PIPELINES.md]
reviewed: 2026-11-10
---
# 4. Values, state and metadata

PURPOSE: The reader knows what a command receives and returns in Rust, what metadata travels with a value, how a value becomes bytes, and what adding a value type involves.

A query passes a value from command to command. This chapter explains the three layers a value
comes in: the *value* itself, the *state* that pairs it with metadata, and the *asset* that adds
a life cycle. The asset has its own chapter (7). The first two are here.

## 4.1 The value

Every value that commands exchange has one Rust type. In this guide it is
`liquers_lib::value::Value`, and its content is one of a fixed set of kinds:

```rust
// Building values.
let text = Value::from("hello");
let number = Value::from(42);
let ratio = Value::from(0.5);
let flag = Value::from(true);
let bytes = Value::from_bytes(vec![1, 2, 3]);
let nothing = Value::none();
let record = Value::try_from_json_value(&serde_json::json!({"name": "Ada", "scores": [1, 2.5]}))?;

// Every value knows its type.
assert_eq!(text.identifier(), "Text");
assert_eq!(number.identifier(), "I32");
assert_eq!(ratio.identifier(), "F64");
assert_eq!(flag.identifier(), "Bool");
assert_eq!(bytes.identifier(), "Bytes");
assert_eq!(record.identifier(), "Object");
assert!(nothing.is_none());

// Reading them back. A conversion that loses nothing succeeds; the rest are errors.
assert_eq!(number.try_into_i64()?, 42);
assert_eq!(number.try_into_string()?, "42");
assert!(Value::from("42").try_into_i64().is_err()); // text is not parsed for you
assert_eq!(record.try_into_json_value()?["name"], "Ada");
```

The kinds fall into two groups, and `Value` is literally an enum of the two:

```rust
// Sketch: how Value is defined in liquers-lib.
pub type Value = CombinedValue<SimpleValue, ExtValue>;

pub enum CombinedValue<Base, Ext> {
    Base(Base),
    Extended(Ext),
}
```

**The base kinds** (`SimpleValue`) are what `liquers-core` needs for itself:

| Identifier | Holds | Made with |
|---|---|---|
| (none) | nothing | `Value::none()` |
| `Bool` | `bool` | `Value::from(true)` |
| `I32`, `I64` | integers | `Value::from(42)`, `Value::from(42_i64)` |
| `F64` | a float | `Value::from(0.5)` |
| `Text` | `String` | `Value::from("text")` |
| `Bytes` | `Vec<u8>` | `Value::from_bytes(vec)` |
| `Array`, `Object` | what JSON can say | `Value::try_from_json_value(&json)` |
| `Query`, `Key`, `Recipe`, `Pipeline` | parts of Liquers itself, as values | `Value::from(query)` … |
| `Metadata`, `AssetInfo`, `CommandMetadata` | descriptions that commands return | |

**The extended kinds** (`ExtValue`) are the rich types of `liquers-lib`:

| Identifier | Holds | Feature | Read with |
|---|---|---|---|
| `Image` | `Arc<image::DynamicImage>` | always | `as_image()` |
| `polars.DataFrame` | `Arc<polars::frame::DataFrame>` | `polars` | `as_polars_dataframe()` |
| `RecordView` | `Arc<dyn RecordView>`, a finite table | `records` | `as_record_view()` |
| `RecordSource` | `Arc<dyn RecordSource>`, a table read in chunks | `records` | `as_record_source()` |
| `UIElement`, widgets | parts of a user interface | `egui` for widgets | `as_ui_element()` |
| foreign objects | an object of another language: Python, JavaScript | always | |

The `try_into_…` methods of the base kinds come from the trait `ValueInterface` of
`liquers-core`, and the `as_…` and `from_…` methods of the extended kinds from
`ExtValueInterface` of `liquers-lib`. Both are in the prelude. An extended value works the same
way as a base one:

```rust
use image::DynamicImage;

let picture = Value::from_image(Arc::new(DynamicImage::new_rgb8(4, 3)));
assert_eq!(picture.identifier(), "Image");
assert!(picture.is_extended());
assert_eq!(picture.as_image()?.width(), 4);
assert_eq!(picture.default_extension(), "png");

// Asking a value for something it is not is a conversion error, never a panic.
let error = Value::from("not a picture").as_image().err().expect("text is not an image");
assert_eq!(error.error_type, ErrorType::ConversionError);
assert!(picture.try_into_string().is_err());
```

This example needs the crate `image` in your `Cargo.toml`, in the version `liquers-lib` uses.

Three rules hold for all values.

**A value is cheap to clone.** Large payloads sit behind an `Arc`, so cloning a value that holds
a table copies a pointer. Commands receive values by reference and return new ones; nothing is
modified in place.

**Conversions do not guess.** `try_into_string` on a number gives its text, because nothing is
lost. `try_into_i64` on a text is an error, even if the text is `"42"`. A command that wants a
number from a text parses it and says so. A conversion that is not possible is an error of the
type `ConversionError`, never a panic.

**The identifier is the type's name everywhere.** `identifier()` returns the name by which the
type is known in metadata, in the store and in the other languages. Liquers' own concepts have a
bare name (`Image`, `RecordView`); a type that belongs to another library carries that library's
name as a prefix (`polars.DataFrame`).

## 4.2 The state

A command does not receive a bare value. It receives a `State<Value>`: the value together with
its *metadata*, the record of what the value is and how it was made.

```rust
fn hello() -> Result<Value, Error> {
    Ok(Value::from("world"))
}

let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
let cr = &mut builder.command_registry;
register_command!(cr, fn hello() -> result)?;
let env = builder.build()?;

let state = env.evaluate("hello/greeting.txt").await?.get().await?;

// The value.
let value: Arc<Value> = state.value()?;
assert_eq!(value.try_into_string()?, "world");
assert_eq!(state.try_into_string()?, "world"); // a shortcut for the same

// The metadata.
assert_eq!(state.status(), Status::Ready);
assert_eq!(state.type_identifier(), "Text");
assert_eq!(state.metadata.query()?.encode(), "hello/greeting.txt");
assert_eq!(state.metadata.filename(), Some("greeting.txt".to_string()));
assert_eq!(state.get_data_format(), "txt");
assert_eq!(state.metadata.get_media_type(), "text/plain");
assert!(!state.metadata.get_dependencies().is_empty()); // what it was made from

// A state is two reference-counted pointers: cloning it copies neither value nor metadata.
let copy = state.clone();
assert!(Arc::ptr_eq(state.data_unchecked(), copy.data_unchecked()));
```

A state is immutable. It consists of two `Arc`s, one for the value and one for the metadata, so
it can be cloned and sent between tasks freely, and many readers can hold the same result. A
command that wants to change something returns a new value; Liquers builds the new state.

`state.value()` returns `Result<Arc<Value>, Error>`, not the value directly. The reason is that
a state can describe a *failed* evaluation: its status is then `Status::Error`, it has no value,
and `value()` returns the error that the command reported. So the `?` after `value()` or
`try_into_string()` is where the failure of a query surfaces in your code (section 5.8).

## 4.3 Metadata

The metadata answers the questions people ask about a result months later.

| Question | `state.metadata.…` |
|---|---|
| How was it made? | `query()`, and the plan stored in the record |
| What is it? | `type_identifier()`, `get_data_format()`, `get_media_type()`, `filename()` |
| Is it good? | `status()`, `is_error()`, `message()` |
| What was it made from? | `get_dependencies()`: every file, recipe and command version it used |
| What happened on the way? | the log in `metadata_record()`: what commands reported with `context.info(…)` |
| What should a user interface call it? | `title()`, `description()` |

Status, type and the rest are filled in by Liquers. A command adds to the log, the title and the
description through its context (section 5.5).

`Metadata` is an enum with two forms. `Metadata::MetadataRecord` is the structured form that
Liquers writes, and `metadata_record()` returns it. `Metadata::LegacyMetadata` is arbitrary JSON,
kept so that metadata written by other tools can still be read. The methods in the table work on
both. The metadata serializes to JSON, and it is stored next to the data in the store, in the
same shape for every platform.

The `Status` of a state is one of the stages of the asset's life cycle: `Ready` for a finished
value, `Error` for a failed one, `Volatile` for a value that must not be reused (section 5.7),
`Source` for a file that was put into the store and not computed, and a few more that describe
work in progress. Chapter 7 has the full list.

## 4.4 Values as files

A value leaves the program as bytes: when it is written to the store, served over HTTP or saved.
Which bytes is decided by the *data format*, a short name such as `json`, `csv` or `png`. Every
type has a default format and a list of formats it can be written in. A file name at the end of
the query chooses among them by its extension (section 3.5).

In the other direction, a file read from the store becomes a value according to its extension:

```rust
let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
store.set(&parse_key("data/note.txt")?, b"hello", &Metadata::new()).await?;
store.set(&parse_key("data/config.json")?, b"{\"retries\": 3}", &Metadata::new()).await?;
store.set(&parse_key("data/sales.csv")?, SALES, &Metadata::new()).await?;
let env = library_environment(store)?;

let note = env.evaluate("-R/data/note.txt").await?.get().await?;
assert_eq!(note.type_identifier(), "Text");
let config = env.evaluate("-R/data/config.json").await?.get().await?;
assert_eq!(config.type_identifier(), "Object");
assert_eq!(config.value()?.try_into_json_value()?["retries"], 3);
let sales = env.evaluate("-R/data/sales.csv").await?.get().await?;
assert_eq!(sales.type_identifier(), "Bytes"); // not parsed until a command asks
assert_eq!(sales.get_data_format(), "csv");
let table = env.evaluate("-R/data/sales.csv/-/ns-rec/to_record").await?.get().await?;
assert_eq!(table.type_identifier(), "RecordView");
```

Text files arrive as `Text`, JSON and YAML files as the data they describe, and everything else
as `Bytes` with the data format remembered. A CSV file stays bytes until a command reads it as a
table. This is deliberate: reading a file costs nothing beyond the bytes, and the query decides
what the bytes are to become, as records with `ns-rec/to_record` or as a Polars data frame with
`ns-pl/from_csv`.

To write a value yourself, ask the state. `as_bytes()` uses the format the metadata names, and
`as_bytes_with_data_format("json")` the one you name. An unsupported format is an error of the
type `SerializationError` that lists the supported ones.

What each type can be written in is described by the *type registry* of the environment:

```rust
let info = env.get_type_registry().get("RecordView").expect("a registered type");
assert_eq!(info.default_extension, "csv");
assert!(info.supported_data_formats.iter().any(|format| format == "json"));
```

The registry is how the parts of Liquers that are not written in Rust know the types. A value's
identifier is stored in its metadata, and the write path refuses an identifier that the registry
does not contain, so that nothing is stored that could not be read back.

Not every value has a byte form. A widget or an object of another language has none. Such a
value lives in memory as long as its asset does, and writing it is refused.

## 4.5 Adding a value type

`Value` has a fixed set of kinds because every command, every store and every language binding
must be able to rely on it. When your data is a table, an image, text or JSON, use those kinds;
that covers most commands ever written. A struct of your own usually travels well as JSON:
`Value::try_from_json_value(&serde_json::to_value(&my_struct)?)` on the way out, and
`serde_json::from_value(value.try_into_json_value()?)` on the way in.

A new kind is justified when a value is large or special enough that converting it at every step
would be wasteful or lossy: a geometry, a model, a document tree. Adding one is a change to
`liquers-lib` itself, in four steps:

1. **Add a variant** to `ExtValue`, with the payload behind an `Arc` so that cloning stays
   cheap. Put it behind a feature if it brings a dependency.
2. **Choose its identifier**: bare if Liquers owns the concept, `provider.Name` if it exposes
   another library's type. One identifier for each variant and one variant for each identifier.
3. **Add the conversions**: the `from_…` and `as_…` methods in `ExtValueInterface`, and the
   variant's arm in each of `identifier`, `type_name`, `default_extension`, `default_filename`,
   `default_media_type`, and in the functions that write and read bytes.
4. **Describe it** with a `TypeInfo` in `ExtValue::type_descriptions()`, naming the formats it
   can be written in. Without this step the type works in memory and cannot be stored.

```rust
// Sketch: step 4 for a type called Sketch.
TypeInfo::new("Sketch")
    .with_type_name("sketch")
    .with_defaults("svg", "svg", "image/svg+xml", "sketch.svg")
    .with_data_formats(["svg", "png"]),
```

The compiler helps with step 3. Liquers' own code never uses a catch-all arm (`_ =>`) when it
matches on a value, so a new variant produces a compile error at every place that has to decide
what to do with it. Follow the same rule in your code that matches on `Status`, `ExtValue` or
`ErrorType`.

There is a second, lighter way, which needs no change to Liquers: the *foreign* variant. It holds
any object that implements the trait `ForeignValue`, and it is how Python objects and JavaScript
values travel through queries. A foreign value is opaque to the command library and is not
stored, but your own commands can downcast it and use it. Finally, `Value` is only one choice of
value type. The environment is generic over it, and a program with unusual needs can combine
`SimpleValue` with an extension enum of its own through `CombinedValue`. The command library is
written for `liquers_lib::value::Value`, so this route gives up the library.
