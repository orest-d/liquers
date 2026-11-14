EDITOR: shared fragment (the record model: views, sources, schemas, manifests, and how records relate to Polars; the same in every guide), included by chapter 13 of all three guides. Keep it language-neutral: queries and YAML only, no host-language code. Written from reference/RECORD_STREAMS.md and guides/RECORD_STREAM_GUIDE.md.

## 13.1 What records are

A *record* is one row of a table: a set of named values, each of a declared type. Records are
the table type that is built into Liquers. They need no other package, they exist on every
platform, and the commands that work on them are in the namespace `rec`.

Much of Liquers is built from records. A CSV file read with `ns-rec/to_record` is a table of
records. So is the listing of a folder, the list of registered commands, and the result of a
search. Because all of these are the same kind of value, one set of commands filters, sorts and
saves them all.

A table of records comes in two forms, and the difference between them is the main thing to
understand in this chapter:

| Form | Is | Value type | Use it for |
|---|---|---|---|
| *Record view* | A finite table that is in memory. Any row and any column can be read at once | `RecordView` | Tables that fit in memory, which is most of them |
| *Record source* | A table that is read in *chunks*, one after another, any number of times. It may be larger than memory, and it may have no known end | `RecordSource` | Large tables, tables assembled from many files, tables that grow |

One pass over a source is a *record stream*: the chunks arrive one at a time, and each chunk is
a record view. A stream is not a value that a query returns. It is what your code gets when it
walks a source.

So there are three scales. A *record* is the unit you search for and read. A *view* is the unit
that is in memory. A *chunk* is the unit that is computed, stored and refreshed: each chunk of a
source is the result of one query, and so it is one asset (chapter 7). When one input file of a
large table changes, only the chunk made from it is computed again.

## 13.2 Schemas

Every record view has a *schema*: the list of its fields. A field has a name, a type, and a few
optional properties.

| Property | Meaning |
|---|---|
| `name` | The name of the column |
| `data_type` | `Bool`, `Int`, `UInt`, `Float`, `Text`, `Binary`, `Date`, `Timestamp` or `Vector` (a fixed number of floats, for embeddings) |
| `nullable` | Whether a cell may be empty. `true` unless you say otherwise |
| `label`, `description` | Text for people. The label is used as a column heading |
| `key` | `Id` marks the one field that identifies a row. A schema has at most one |
| `role` | How the field can be searched: as full text, as an exact keyword, as a number |

A schema comes from one of two places.

**Inferred.** When a table is read from a format that does not carry types, such as CSV, each
column gets the first type that fits all its cells: `Bool`, then `Int`, then `Float`, then
`Date`, then `Timestamp`, otherwise `Text`. A column with an empty cell is nullable. Inference
never chooses an `Id` field and never assigns a role.

**Declared.** You write the schema and the data is read against it. Nothing is guessed: a cell
that does not fit its field is an error, and so is a column the schema does not know.

```yaml
fields:
  - {name: id, data_type: Text, key: Id}
  - {name: region, data_type: Text, nullable: false}
  - {name: amount, data_type: Float}
  - {name: note, data_type: Text}
```

`ns-rec/to_record` takes a declared schema as its argument `schema`, and a manifest declares one
for all its chunks (section 13.4). The command `ns-rec/schema` returns the schema of any table as
JSON text, in the form that these two accept. A convenient way to write a schema is therefore to
read the table once without one, print its schema, and correct it.

Declare a schema when the types matter, when the table has an identifier, or when you want an
error instead of a silently changed type. An `Id` field is what `ns-rec/rec_id-<id>` looks rows
up by. Without one, a row is identified by its position: `ns-rec/rowid-<chunk>-<row>`.

Whether a schema survives being stored depends on the format. Arrow IPC (`.feather`) and Parquet
files written by Liquers keep the whole schema. CSV and JSON keep the names and the values; the
types are inferred again when the file is read.

## 13.3 Views, sources and streams

Most record commands accept both forms, and the form of the result follows the form of the
input.

| Command | On a view | On a source |
|---|---|---|
| `eq`, `gt`, `contains`, `is_in`, `filter` and the other filters | the matching rows | a source that filters each chunk as it is read |
| `select_columns`, `drop_columns` | fewer columns | a source that does so chunk by chunk |
| `head-n` | the first rows | a source that stops reading after `n` rows |
| `nrows` | the number of rows | the number of rows, counted chunk by chunk |
| `sort`, `tail` | the result | the source is read completely first |
| `materialize` | the view itself | all chunks joined into one view |

The commands in the upper rows are *lazy* on a source: they describe what to do and read
nothing. The work happens when something walks the result, and only one chunk is in memory at a
time. `head-10` on a source of a thousand chunks reads the first chunk and stops.

`materialize` is the one step from a source to a view. It reads every chunk and joins them, so
it is the step at which a table that does not fit into memory fails. It refuses a result of more
than a million rows unless you allow more: `materialize-5000000`.

Two rules keep the forms apart. A command that needs a whole table, such as `ns-rec/to_record`
or any command of `ns-pl`, refuses a source and tells you to materialize it first. And a view
becomes a source at no cost, with `ns-rec/to_record_source`, when a command wants one.

Put the lazy steps first and `materialize` last:

```text
-R/sales/monthly.manifest.yaml/-/ns-rec/eq-region-EU/select_columns-amount/materialize
```

This reads each chunk, keeps the rows of the EU and one column of them, and only then builds a
table, which holds a fraction of the data.

## 13.4 Manifests

A record source has to be described somewhere: which chunks it has and how each is made. That
description is a *manifest*, a YAML file whose name ends in `.manifest.yaml`:

```yaml
manifest: record-stream
title: Sales by month
chunks:
  - query: -R/sales/jan.raw.csv/-/ns-rec/to_record/jan.csv
  - query: -R/sales/feb.raw.csv/-/ns-rec/to_record/feb.csv
uniform_schema:
  fields:
    - {name: id, data_type: Text, key: Id}
    - {name: region, data_type: Text, nullable: false}
    - {name: amount, data_type: Int}
    - {name: note, data_type: Text}
```

Stored as `sales/monthly.manifest.yaml`, this file is a table of two chunks. Any record command
reads it: `-R/sales/monthly.manifest.yaml/-/ns-rec/nrows` counts the rows of both.

| Field | Meaning |
|---|---|
| `manifest` | Always `record-stream`. It is how Liquers recognizes a manifest |
| `title`, `description` | Text for people |
| `chunks` | The chunks, in order. Each is a recipe: a `query`, and optionally `title`, `arguments` and `links` |
| `template` | A rule that generates chunks; see below |
| `uniform_schema` | The schema that every chunk must have. A chunk that does not match is an error that names the chunk |
| `extension` | The format in which generated chunks are stored. `csv` unless you say otherwise |
| `stored`, `cached`, `volatile`, `expires` | As in a recipe (section 7.4), applied to every chunk |

**Chunks with keys.** A chunk whose query ends in a file name gets a key: the folder of the
manifest plus that name. The first chunk above is `sales/jan.csv`. It is computed when it is
first needed, stored under its key, and from then on read from the store. It is also an asset
like any other, so `-R/sales/jan.csv` reads that one chunk, from anywhere. A chunk without a
file name has no key; it is identified by its query and kept as a query result is.

**Templates.** When the chunks follow a pattern, a manifest can generate them instead of listing
them:

```yaml
manifest: record-stream
template:
  query: -R/db/orders/-/ns-shop/page
  step: 1000
  batch_size: 1000
```

Chunk number `i` is the template's query with two more arguments, the offset `step × i` and the
`batch_size`: `…/page-0-1000`, `…/page-1000-1000`, and so on. The source ends at the first chunk
that has fewer rows than `batch_size`, so the command must answer a short or empty table past
the end, not an error. In a manifest stored as `orders.manifest.yaml`, the generated chunks get
the keys `orders_0000.csv`, `orders_0001.csv`, and so on. A source with a template has no known
length until it has been walked.

### Manifests and recipes

A manifest and a `recipes.yaml` (section 7.4) do the same kind of thing. Both say how results
are made without making them, both give the results keys in a folder, and both leave it to
Liquers to compute each result when it is first asked for, to store it, and to compute it again
when its inputs change. A chunk of a manifest is in fact written as a recipe, with the same
fields.

They differ in what they describe:

| | `recipes.yaml` | A manifest |
|---|---|---|
| Describes | Separate results that happen to share a folder | The parts of one table |
| A result is | Any value: a table, a text, an image | A table, with the same schema as the other chunks |
| Results are | Listed one by one | Listed, or generated by a template without a known end |
| Read together | No. Each key is read on its own | Yes. The manifest is a record source |
| The file | `recipes.yaml`, one in a folder | `<name>.manifest.yaml`, any number in a folder |
| `stored`, `cached`, `expires` | Set for each recipe | Set once, for all chunks |

Use a recipe for a result that stands on its own. Use a manifest when the results are pieces of
one table, and you want to filter, count or search across them. One name cannot be defined by
both: a chunk with the name of a recipe in the same folder is reported as an error.

## 13.5 Records and Polars

Native Liquers has a second library for tables, built on the Polars engine, with its commands in
`ns-pl` and its own value type, the Polars data frame. The two are meant to be used together.

| | Records (`ns-rec`) | Polars (`ns-pl`) |
|---|---|---|
| Available | Everywhere, the browser included | In Rust and Python; not in the browser |
| Tables larger than memory | Yes, as record sources | No. A data frame is in memory |
| Works lazily | Yes, on sources | No. Each command computes its result |
| Statistics (`sum`, `mean`, `median`, `min`, `max`, `std`, `describe`) | No | Yes |
| Speed on large tables in memory | Good | Better |
| Identifier and search roles in the schema | Yes | No |
| Stored as | CSV, TSV, JSON, NDJSON, Markdown, HTML, Arrow IPC, Parquet | CSV, Parquet, Arrow IPC |

The commands that both libraries have carry the same names and take the same arguments: `eq`,
`gt`, `contains`, `is_in`, `sort`, `head`, `select_columns` and the rest. And a query can cross
from one library to the other without a conversion step, because every `ns-pl` command accepts a
record view and every `ns-rec` command accepts a data frame:

```text
-R/sales/monthly.manifest.yaml/-/ns-rec/eq-region-EU/materialize/ns-pl/select_columns-amount/sum
```

This is the usual division of labour. Records bring a large table in chunk by chunk and cut it
down; `materialize` makes one table of what is left; Polars computes on it. Two things to know
about the crossing: a record source must be materialized first, and a data frame has no place
for an identifier or for search roles, so those are lost on the way to Polars.
