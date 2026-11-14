---
title: Records
status: ready
sources: [reference/RECORD_STREAMS.md, guides/RECORD_STREAM_GUIDE.md, guides/SEARCH_GUIDE.md, reference/SEARCH.md, reference/AGENT_MEMORY_SERVICE.md, reference/VISION.md]
reviewed: 2026-11-14
---
# 13. Records

PURPOSE: The reader understands records: views, sources and streams, schemas and manifests, how a manifest relates to a recipe and records to Polars. The reader can use them from Rust, and sees search, the agent memory and the MCP crates as applications of records.

Chapter 9 used tables without saying much about what they are. This chapter does. It describes
*records*, the table type that is built into Liquers, first in the words shared by all three
guides (sections 13.1 to 13.5) and then from Rust (13.6). The last sections show what records
are used for inside Liquers itself: search, and the memory for AI agents.

A first look, to have something concrete in mind. The examples of this chapter share three
small files:

```rust
const JAN: &[u8] = b"id,region,amount\na1,EU,120\na2,US,80\n";
const FEB: &[u8] = b"id,region,amount\nb1,EU,95\n";
const MONTHLY: &[u8] = b"
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
";
```

```rust
use liquers_records::{FieldType, FieldValue};

let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
store.set(&parse_key("sales/jan.csv")?, JAN, &Metadata::new()).await?;
let env = library_environment(store)?;

let state = env.evaluate("-R/sales/jan.csv/-/ns-rec/to_record").await?.get().await?;
assert_eq!(state.type_identifier(), "RecordView"); // a table in memory
let table = state.value()?.as_record_view()?;
let fields: Vec<(&str, FieldType)> =
    table.schema().fields.iter().map(|field| (field.name.as_str(), field.data_type)).collect();
assert_eq!(fields, [("id", FieldType::Text), ("region", FieldType::Text), ("amount", FieldType::Int)]);
assert_eq!(table.value(0, 2)?, FieldValue::Int(120)); // one cell of one record
```

{{> ../_shared/records.md}}

## 13.6 Records in Rust

Section 9.2 showed how to read a record view and how to build one, and section 9.4 a first
record source. Here are the parts that belong to this chapter: schemas, manifests with keys,
and streams. Records need the feature `records` of `liquers-lib`, which is on by default. The
types are in the crate `liquers-records`, which depends on `liquers-core` only.

API reference: [`RecordView`](https://orest-d.github.io/liquers/api/rust/liquers_records/batch/trait.RecordView.html),
[`RecordSource`](https://orest-d.github.io/liquers/api/rust/liquers_records/value/trait.RecordSource.html),
[`RecordSchema`](https://orest-d.github.io/liquers/api/rust/liquers_records/schema/struct.RecordSchema.html) and
[`ManifestSpec`](https://orest-d.github.io/liquers/api/rust/liquers_records/manifest/struct.ManifestSpec.html), the manifest as a type; the
whole crate is [`liquers_records`](https://orest-d.github.io/liquers/api/rust/liquers_records/index.html).

### Declaring a schema

`to_record` takes the schema as text, in YAML or JSON. A schema is too long and too structured
for a query, so this is a natural place for a pipeline (section 3.10), in which the argument is
simply a string:

```rust
use liquers_core::interpreter::evaluate_pipeline;
use liquers_core::pipeline::{Pipeline, QueryEditor};
use liquers_records::{FieldType, FieldValue, KeyRole};
use serde_json::json;

const SCHEMA: &str = "
fields:
  - {name: id, data_type: Text, key: Id}
  - {name: region, data_type: Text, nullable: false}
  - {name: amount, data_type: Float}
";
let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
store.set(&parse_key("sales/jan.csv")?, JAN, &Metadata::new()).await?;
store.set(&parse_key("sales/schema.txt")?, SCHEMA.as_bytes(), &Metadata::new()).await?;
let env = library_environment(store)?;

// The schema as an argument of a pipeline step.
let pipeline = Pipeline::from_json(&json!([
    "-R/sales/jan.csv",
    {"action": "to_record", "ns": "rec", "arguments": {"schema": SCHEMA}},
]))?;
let table = evaluate_pipeline(env.clone(), &pipeline, None).await?.value()?.as_record_view()?;
let amount = &table.schema().fields[2];
assert_eq!(amount.data_type, FieldType::Float); // declared, not inferred
assert_eq!(table.schema().fields[0].key, KeyRole::Id);
assert_eq!(table.value(0, 2)?, FieldValue::Float(120.0));
```

To stay with a query, and so with a cached result, store the schema under a key and pass it as
a link. A link is written `~X~` *query* `~E` in place of the argument, and the query editor
writes it for you:

```rust
// The schema as a link: the argument is the result of another query.
let mut editor = QueryEditor::new(env.command_metadata_snapshot());
editor.resource("sales/jan.csv", Default::default())?;
editor.action("to_record")?.ns("rec")?.link("schema", "-R/sales/schema.txt")?;
editor.action("rec_id")?.ns("rec")?.set("id", "a2")?; // look a row up by its Id
let query = editor.query();
assert_eq!(
    query.encode(),
    "-R/sales/jan.csv/-/ns-rec/to_record--true-~X~-R/sales/schema.txt~E/rec_id-a2"
);
let row = env.evaluate(&query).await?.get().await?.value()?.as_record_view()?;
assert_eq!(row.len(), 1);
assert_eq!(row.value(0, 1)?, FieldValue::Text("US".into()));
```

The editor wrote the two arguments before `schema` with their defaults (an empty `format`,
which means "as the file says", and `header` true), because a query can only give arguments by
position. In a recipe, the same link is the entry `links: {schema: -R/./schema.txt}`.

The schema is stored as a `.txt` file on purpose. The argument wants text, and a file named
`.yaml` or `.json` is read as a structure, which the argument refuses.

When you build a table in Rust, you make the `RecordSchema` yourself and nothing is inferred
(section 9.2).

### Manifests

A manifest in the store is a record source, and its chunks are assets with keys:

```rust
use liquers_core::assets::AssetManager;

let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
store.set(&parse_key("sales/jan.raw.csv")?, JAN, &Metadata::new()).await?;
store.set(&parse_key("sales/feb.raw.csv")?, FEB, &Metadata::new()).await?;
store.set(&parse_key("sales/monthly.manifest.yaml")?, MONTHLY, &Metadata::new()).await?;
let env = library_environment(store.clone())?;

// Record commands read the manifest as one table.
let count = env.evaluate("-R/sales/monthly.manifest.yaml/-/ns-rec/nrows").await?.get().await?;
assert_eq!(count.try_into_string()?, "3");

// A lazy filter gives a source; materialize makes the table.
let lazy = env.evaluate("-R/sales/monthly.manifest.yaml/-/ns-rec/eq-region-EU").await?.get().await?;
assert_eq!(lazy.type_identifier(), "RecordSource");
let eu = env.evaluate("-R/sales/monthly.manifest.yaml/-/ns-rec/eq-region-EU/materialize").await?.get().await?;
assert_eq!(eu.value()?.as_record_view()?.len(), 2);

// Each chunk has a key, a recipe, and after its first use a stored file.
let assets = env.get_asset_manager();
let recipe = assets.recipe_opt(&parse_key("sales/feb.csv")?).await?.expect("the chunk's recipe");
assert_eq!(recipe.query, "-R/sales/feb.raw.csv/-/ns-rec/to_record/feb.csv");
let feb = env.evaluate("-R/sales/feb.csv").await?.get().await?;
assert_eq!(feb.value()?.as_record_view()?.len(), 1);
assert!(store.contains(&parse_key("sales/feb.csv")?).await?);
```

The last lines show the kinship of manifests and recipes from Rust: the asset manager answers
`recipe_opt` for the key of a chunk as it answers for a key from a `recipes.yaml`. Both come
from recipe providers (section 7.9), and the provider for manifests is part of every
environment that has the record commands.

### Walking a source

In Rust a source is an `Arc<dyn RecordSource>`, from `value.as_record_source()`. Its `stream`
method returns one pass over it, an asynchronous stream of views:

```rust
use futures::StreamExt;
use liquers_lib::records::EnvResolver;

let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
store.set(&parse_key("sales/jan.raw.csv")?, JAN, &Metadata::new()).await?;
store.set(&parse_key("sales/feb.raw.csv")?, FEB, &Metadata::new()).await?;
store.set(&parse_key("sales/monthly.manifest.yaml")?, MONTHLY, &Metadata::new()).await?;
let env = library_environment(store)?;

let state = env.evaluate("-R/sales/monthly.manifest.yaml/-/ns-rec/to_record_source").await?.get().await?;
let source = state.value()?.as_record_source()?;
assert!(source.schema().is_some()); // the manifest declares one

// One pass: a stream of views, one chunk at a time.
let resolver = Arc::new(EnvResolver::new(env.clone()));
let mut stream = source.clone().stream(resolver.clone()).await?;
let mut sizes = Vec::new();
while let Some(view) = stream.next().await {
    sizes.push(view?.len());
}
assert_eq!(sizes, [2, 1]);

// Everything as one table, refused beyond 1000 rows.
let all = source.materialize(resolver, 1000).await?;
assert_eq!(all.len, 3);
```

A stream needs a *resolver*, the object that evaluates the queries of the chunks. Outside a
command that is an `EnvResolver`, made from the environment. Inside a command, use a
`ContextResolver` made from the context, so that every chunk becomes a dependency of the
command's result and the result is computed again when a chunk changes.

Memory deserves a thought when you walk a large source. With the default settings the
environment keeps every chunk it has computed, so that a second pass does not compute them
again. Section 7.10 shows the setting that releases each chunk as the stream moves on.

### Crossing to Polars

With the feature `polars`, the query of section 13.5 returns a Polars data frame:

```rust
// Records cut the table down, Polars computes on what is left.
let query = "-R/sales/monthly.manifest.yaml/-/ns-rec/eq-region-EU/materialize/ns-pl/select_columns-amount/sum/total.csv";
let total = env.evaluate(query).await?.get().await?;
assert_eq!(total.type_identifier(), "polars.DataFrame");
let (bytes, _metadata) = env.evaluate(query).await?.get_binary().await?;
assert_eq!(String::from_utf8_lossy(&bytes), "amount\n215\n");
```

`select_columns` is called in `ns-pl` here, after the crossing. The record command of the same
name always keeps the `Id` field, and `sum` would then meet a column of text.

## 13.7 Search: records put to work

A store fills up: notes, reports, tables, results. The search commands of `ns-search` find
things in it, and they are built entirely from records. One command turns what you want to
search into a table, one row for each thing. Another keeps the rows that match. So a search is
a query like any other, and its result is cached and kept up to date in the same way.

```rust
use liquers_core::assets::AssetManager;
use liquers_core::metadata::MetadataRecord;
use liquers_core::pipeline::QueryEditor;
use liquers_records::FieldValue;

// A Markdown document as a state: the text, and the format it is written in.
fn document(text: &str) -> State<Value> {
    let mut record = MetadataRecord::new();
    record.data_format = Some("md".to_string());
    State::from_value_and_metadata(Value::from(text), Arc::new(Metadata::MetadataRecord(record)))
}

let env = library_environment(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
let assets = env.get_asset_manager();
let shopping = "---\ntitle: Shopping\nstatus: open\n---\n# Shopping\n\nRemember the milk and the bread.\n";
let trip = "---\ntitle: Trip to Prague\nstatus: done\n---\n# Trip\n\nBook the train to Prague.\n";
assets.set_state(&parse_key("notes/shopping.md")?, document(shopping)).await?;
assets.set_state(&parse_key("notes/trip.md")?, document(trip)).await?;

// The values of one column of a table, as text.
let column = |query: String, name: &'static str| {
    let env = env.clone();
    async move {
        let table = env.evaluate(query).await?.get().await?.value()?.as_record_view()?;
        let index = table.schema().fields.iter().position(|field| field.name == name);
        let index = index.ok_or_else(|| Error::general_error(format!("no column {name}")))?;
        let mut cells = Vec::new();
        for row in 0..table.len() {
            match table.value(row, index)? {
                FieldValue::Text(text) => cells.push(text.to_string()),
                other => cells.push(format!("{other:?}")),
            }
        }
        Ok::<_, Error>(cells)
    }
};

// A folder as a table: one row for each file.
let files = column("-R-key/notes/-/ns-search/catalog".to_string(), "filename").await?;
assert_eq!(files, ["shopping.md", "trip.md"]);

// Full-text search in the files.
let hits = column("-R-key/notes/-/ns-search/catalog-t-t/search-milk".to_string(), "key").await?;
assert_eq!(hits, ["notes/shopping.md"]);
```

The example first stores two Markdown documents as values. `document` gives each state the data
format `md`, so that the stored file is known to be text.

Two things are new in the queries.

`-R-key/notes` does not read anything. It passes the *key* `notes` to the next command, which is
how you hand a folder to a command. (`-R/notes` would try to read the folder as a value.)

`catalog` makes a record view with one row for each entry of the folder: `key`, `filename`,
`extension`, `title`, `status`, `media_type` and more. It has two switches: whether to walk into
subfolders (on by default) and whether to read the text of each file (off by default).
`catalog-t-t` turns both on, which `search` needs in order to look inside the documents.
`search-milk` then keeps the rows that match, best match first, and adds three columns: `score`,
`matched` (where the word was found) and `excerpt`.

Listing does not evaluate anything. A key that exists only as a recipe is listed with the status
`Recipe` and is not computed. The text of a file is read only when the file is known to be
text.

The closure `column` in the example is ordinary record-view code (section 9.2). In a program you
would more likely end the query with a file name, such as `hits.json`, and hand the bytes on.

### Tables to search

Every command in the left column makes a table, and `search` works on all of them:

| Query | The rows are | So a search finds |
|---|---|---|
| `-R-key/notes/-/ns-search/catalog` | the entries of a folder | files, by name, title and status |
| `-R-key/notes/-/ns-search/catalog-t-t` | the same, with the text of each file | files, by content |
| `-R-key/notes/-/ns-search/field_extract` | the documents, with a column for each field of their front matter | documents, by field |
| `-R-key/notes/-/ns-search/md_split` | the *sections* of Markdown documents, with headings and line numbers | sections |
| `-R-key/src/-/ns-search/md_split-line` | the *lines* of text files | lines, like `grep -n` |
| `ns-search/commands` | the registered commands | a command whose name you do not know |
| `-R/data/sales.csv`, or a manifest | the rows of your own table | rows |

The last row is the point of this section. `search` has no notion of files or documents. It
searches records, and whatever you can turn into records, you can search: the manifest of
section 13.6 as well as a folder of notes. On a record source, `search` with ranking turned off
is lazy like the other filters; `ns-rec/filter-<expression>` is that form under a shorter name.

Each row of `ns-search/commands` has a `query` column with the text that calls the command,
such as `ns-img/resize`.

### Searching by fields

Many documents begin with a block of fields, their *front matter*, as the notes above do.
`field_extract` turns those fields into columns, and a search expression can then refer to
them. The expression is the kind of value that should not be pasted into a query, so the
example builds the query with a query editor (section 3.9):

```rust
// Search by a field of the front matter. The editor escapes the colon of the expression.
let start = parse_query("-R-key/notes/-/ns-search/field_extract")?;
let mut editor = QueryEditor::from_query(&start, env.command_metadata_snapshot())?;
editor
    .action("search")?
    .ns("search")?
    .set("query", "status:open")? // a text
    .set("rank", false)? // a boolean
    .set("limit", 0)?; // a number; 0 means all matches
let query = editor.query().encode();
assert_eq!(query, "-R-key/notes/-/ns-search/field_extract/search-status~ncolon~open-false-false-0");
assert_eq!(column(query, "title").await?, ["Shopping"]);
```

The arguments of `search` are, in order: `query`, the expression; `rank`, whether to order the
results by relevance (on by default); `case_sensitive` (off); `limit`, the largest number of
results (50; 0 means all); and then the names of the columns to search in. The editor filled in
`case_sensitive`, which the example did not set, with its default, because a query gives
arguments by position.

### Search expressions

| Write | It matches when |
|---|---|
| `word` | the word occurs in one of the searched columns |
| `a b` | both occur |
| `a \| b` | either occurs |
| `-a` | `a` does not occur |
| `"a phrase"` | the words occur in this order |
| `/re/` | the regular expression matches |
| `field:value` | the column equals the value; `*` and `?` are wildcards, `field:*` means "has a value" |
| `field:>5`, `>=`, `<`, `<=` | the column compares so, for numbers and dates |
| `( … )` | grouping |

A malformed expression does not fail. Its words are searched for, and a warning with the
position of the mistake is written to the log of the result. When a search returns something
unexpected, read the log in the metadata of its state.

Which columns a bare word is looked for in is decided by the schema (section 13.2): the fields
whose role says they hold text to search. When no field has such a role, `search` looks in the
columns `filename`, `title`, `description`, `doc` and `content`, whichever exist, and when none
of those exists either, in every text column. You can also name the columns yourself, as the
last arguments of `search`.

**Your own tables.** A table that your command returns is searched best when its schema says
how (section 9.2). Give `FieldRole::text()` to the columns a person would search by words, such
as names, titles and descriptions, and `FieldRole::keyword()` to codes and statuses.

Some limits. Search reads what it searches each time its result has to be computed again; there
is no index yet, so narrow the folder before you search the content of many files. It matches
words, not meanings. And like every result, a search result is refreshed when files change
through Liquers, not when another program edits them while Liquers runs.

For Markdown there is also the namespace `md` (feature `markdown`), which reads one document by
its structure: its outline, one section by its heading, its links.

## 13.8 The agent memory

The second application is a *memory* for AI agents: a folder of Markdown documents with a
schema that says which collections exist and which fields their documents carry. It is the
search of section 13.7 with rules added, and it speaks in records too: listing, searching and
checking a memory each return a record view, one row for a document or a finding, which the
record commands can filter and sort further. The commands are in the namespace `mem` (feature
`mem`):

| Command | Does |
|---|---|
| `ns-mem/overview`, `ns-mem/describe` | describe the memory and its collections |
| `ns-mem/list`, `ns-mem/search`, `ns-mem/similar` | find documents |
| `ns-mem/read` | reads a document, or a summary of it |
| `ns-mem/new`, `ns-mem/write`, `ns-mem/edit` | create and change documents, with a log of every change |
| `ns-mem/check` | checks the memory against its schema |

Their natural user is an agent, and for agents Liquers has a program, `liquers-agent`. It serves
these commands as tools over the Model Context Protocol (MCP), so that an MCP client such as a
coding assistant can use a memory directly. It is configured with a file, and the same commands
are available from its command line.

Why offer queries to an agent at all? An agent normally works either with a fixed list of tools,
which is safe but rigid, or by running code it writes, which is flexible but hard to check.
Queries sit in between. An agent can combine commands freely, and yet each query can be checked
before it runs, is cached, and leaves a record of what it read and produced.

## 13.9 Your commands as tools

The crate behind `liquers-agent` is `liquers-mcp`, and it is not specific to memories. A program
declares *tool sets*. Each tool is one query whose last action is the command the tool runs, and
a set may add a general query tool that is limited by a list of allowed commands. The crate
works out the arguments of every tool from the command metadata, evaluates a call as a query,
and returns the value in the format that the query's file name selects. The descriptions that
the agent sees are the `doc:` and the argument descriptions of your commands (sections 5.3
and 5.6), so a command that is documented well for people is documented for agents too.

`liquers-mcp` depends on `liquers-core` only. A program that serves its own commands to agents
does not need the command library.

EDITOR: still to be written: a worked example of a tool set served with `liquers-mcp` (`ToolSetDecl`, `McpServer`, `serve_stdio`).
SOURCE: reference/AGENT_MEMORY_SERVICE.md
