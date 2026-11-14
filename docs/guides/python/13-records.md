---
title: Records
status: ready
sources: [reference/RECORD_STREAMS.md, guides/RECORD_STREAM_GUIDE.md, guides/SEARCH_GUIDE.md, reference/SEARCH.md, reference/AGENT_MEMORY_SERVICE.md, reference/PYTHON_INTEGRATION.md, reference/VISION.md]
reviewed: 2026-11-14
---
# 13. Records

PURPOSE: The reader understands records: views, sources and streams, schemas and manifests, how a manifest relates to a recipe and records to Polars. The reader can use them from Python, and sees search and the agent memory as applications of records.

Chapter 9 used tables without saying much about what they are. This chapter does. It describes
*records*, the table type that is built into Liquers, first in the words shared by all three
guides (sections 13.1 to 13.5) and then from Python (13.6). The last two sections show what
records are used for inside Liquers itself: search, and the memory for AI agents.

A first look, to have something concrete in mind:

```python
import liquers as lq

env = lq.Environment()
env.store.set("sales/jan.csv", b"id,region,amount\na1,EU,120\na2,US,80\n")

table = env.evaluate("-R/sales/jan.csv/-/ns-rec/to_record")
assert isinstance(table, lq.RecordView)                        # a table in memory
assert [(f.name, f.type) for f in table.schema.fields] == [
    ("id", lq.FieldType.Text), ("region", lq.FieldType.Text), ("amount", lq.FieldType.Int),
]
assert table.to_list()[0] == {"id": "a1", "region": "EU", "amount": 120}    # one record
env.shutdown()
```

{{> ../_shared/records.md}}

## 13.6 Records in Python

Section 9.2 showed how to read a record view and how to build one from rows, and section 9.3
how to move it to pandas, Polars and pyarrow. Here are the parts that belong to this chapter:
schemas, sources and manifests.

### Declaring a schema

`to_record` takes the schema as text, in YAML or JSON. A schema is too long and too structured
for a query, so this is a natural place for a pipeline (section 3.10), in which the argument is
simply a Python string:

```python
import liquers as lq

env = lq.Environment()
env.store.set("sales/jan.csv", b"id,region,amount\na1,EU,120\na2,US,80\n")

SCHEMA = """
fields:
  - {name: id, data_type: Text, key: Id}
  - {name: region, data_type: Text, nullable: false}
  - {name: amount, data_type: Float}
"""

table = env.evaluate([
    "-R/sales/jan.csv",
    {"action": "to_record", "ns": "rec", "arguments": {"schema": SCHEMA}},
])
assert table.schema.field("amount").type == lq.FieldType.Float     # declared, not inferred
assert table.schema.field("region").nullable is False
assert table.column("amount").to_list() == [120.0, 80.0]

# With an Id field, a row can be looked up by its identifier.
row = env.evaluate([
    "-R/sales/jan.csv",
    {"action": "to_record", "ns": "rec", "arguments": {"schema": SCHEMA}},
    "ns-rec/rec_id-a2",
])
assert row.to_list() == [{"id": "a2", "region": "US", "amount": 80.0}]
env.shutdown()
```

To stay with a query, and so with a cached result, store the schema under a key and pass it as
a link. A link is written `~X~` *query* `~E` in place of the argument:

```python
import liquers as lq

env = lq.Environment()
env.store.set("sales/jan.csv", b"id,region,amount\na1,EU,120\na2,US,80\n")
env.store.set("sales/schema.txt", b"fields:\n  - {name: id, data_type: Text, key: Id}\n"
              b"  - {name: region, data_type: Text}\n  - {name: amount, data_type: Float}\n")

editor = env.query_editor("-R/sales/jan.csv")
step = editor.action("to_record", ns="rec")
step.link("schema", "-R/sales/schema.txt")           # the argument is the result of this query
query = editor.query()
assert query.encode() == "-R/sales/jan.csv/-/ns-rec/to_record--true-~X~-R/sales/schema.txt~E"
assert env.evaluate(query).schema.field("amount").type == lq.FieldType.Float
env.shutdown()
```

The editor wrote the two arguments before `schema` with their defaults (an empty `format`,
which means "as the file says", and `header` true), because a query can only give arguments by
position. In a recipe, the same link is the entry `links: {schema:
-R/./schema.txt}`.

The schema is stored as a `.txt` file on purpose. The argument wants text, and a file named
`.yaml` or `.json` is read as a structure, which the argument refuses.

When you build a table in Python, `fields=` of `RecordView.from_records` and of the other
`from_` functions declares the types (section 9.2).

### Sources and manifests

A manifest in the store is a record source. Write the manifest with `env.store_set`, so that
Liquers notices the new file at once:

```python
import time
import liquers as lq

env = lq.Environment()
env.store.set("sales/jan.raw.csv", b"id,region,amount\na1,EU,120\na2,US,80\n")
env.store.set("sales/feb.raw.csv", b"id,region,amount\nb1,EU,95\n")
env.store_set("sales/monthly.manifest.yaml", """
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
""")

# Record commands read the manifest as one table.
assert env.evaluate("-R/sales/monthly.manifest.yaml/-/ns-rec/nrows") == 3
eu = env.evaluate("-R/sales/monthly.manifest.yaml/-/ns-rec/eq-region-EU/materialize")
assert eu.to_dict() == {"id": ["a1", "b1"], "region": ["EU", "EU"], "amount": [120, 95]}
found = env.evaluate("-R/sales/monthly.manifest.yaml/-/ns-rec/rec_id-b1")
assert found.to_list() == [{"id": "b1", "region": "EU", "amount": 95}]

# The source itself, in Python.
source = env.evaluate("-R/sales/monthly.manifest.yaml/-/ns-rec/to_record_source")
assert isinstance(source, lq.RecordSource) and source.bounded
assert [chunk.query.encode() for chunk in source.chunks()] == ["-R/sales/jan.csv", "-R/sales/feb.csv"]
assert [len(view) for view in source] == [2, 1]              # a stream: one chunk at a time
assert source.chunk(1).to_list() == [{"id": "b1", "region": "EU", "amount": 95}]

# Each chunk has a key and is stored under it, like the result of a recipe.
assert env.evaluate("-R/sales/feb.csv").to_list() == [{"id": "b1", "region": "EU", "amount": 95}]
time.sleep(0.5)                                               # written in the background
assert env.store.get("sales/feb.csv") == b"id,region,amount\nb1,EU,95\n"
assert env.recipes.recipe("sales/feb.csv").query == "-R/sales/feb.raw.csv/-/ns-rec/to_record/feb.csv"
env.shutdown()
```

| To | Use |
|---|---|
| list the chunks without reading any | `source.chunks()`; each has `index`, `kind` (`"key"` or `"query"`) and `query` |
| walk the source | `for view in source:`, or `async for view in source:` |
| read one chunk | `source.chunk(i)`, `await source.chunk_async(i)` |
| get everything as one view | `source.materialize()`, `await source.materialize_async()` |
| know the schema and whether there is an end | `source.schema` (`None` when none is declared), `source.bounded` |

The last lines of the example show the kinship of manifests and recipes from Python:
`env.recipes` answers for the key of a chunk as it answers for a key from a `recipes.yaml`.

API reference: [`RecordView`](https://orest-d.github.io/liquers/api/python/liquers.html#RecordView) and [`RecordSource`](https://orest-d.github.io/liquers/api/python/liquers.html#RecordSource).

A template makes the chunks from a command of yours. The command receives the offset and the
number of rows, and answers a short table at the end:

```python
import liquers as lq

env = lq.Environment()
ROWS = [{"n": i} for i in range(5)]

@env.first_command
def page(offset: int = 0, size: int = 2):
    return lq.RecordView.from_records(ROWS[offset:offset + size], fields={"n": "int"})

env.store_set("numbers/all.manifest.yaml", """
manifest: record-stream
template:
  query: page
  step: 2
  batch_size: 2
""")

assert env.evaluate("-R/numbers/all.manifest.yaml/-/ns-rec/nrows") == 5     # chunks of 2, 2 and 1
source = env.evaluate("-R/numbers/all.manifest.yaml/-/ns-rec/to_record_source")
assert source.bounded is False                                # no known end before the walk
assert env.evaluate("-R/numbers/all_0001.csv").to_list() == [{"n": 2}, {"n": 3}]
env.shutdown()
```

### Crossing to Polars

With the `polars` package installed, the query of section 13.5 returns a `polars.DataFrame`:

```python
import liquers as lq
import polars as pl

env = lq.Environment()
env.store.set("sales/jan.raw.csv", b"id,region,amount\na1,EU,120\na2,US,80\n")
env.store.set("sales/feb.raw.csv", b"id,region,amount\nb1,EU,95\n")
env.store_set("sales/monthly.manifest.yaml", """
manifest: record-stream
chunks:
  - query: -R/sales/jan.raw.csv/-/ns-rec/to_record/jan.csv
  - query: -R/sales/feb.raw.csv/-/ns-rec/to_record/feb.csv
""")

total = env.evaluate(
    "-R/sales/monthly.manifest.yaml/-/ns-rec/eq-region-EU/materialize/ns-pl/select_columns-amount/sum")
assert isinstance(total, pl.DataFrame)
assert total.to_dicts() == [{"amount": 215}]

frame = env.evaluate("-R/sales/jan.raw.csv/-/ns-pl/from_csv")            # a data frame …
view = env.evaluate("-R/sales/jan.raw.csv/-/ns-pl/from_csv/ns-rec/eq-region-EU")   # … into ns-rec
assert isinstance(frame, pl.DataFrame) and isinstance(view, lq.RecordView)
env.shutdown()
```

Memory deserves a thought when you walk a large source. With the default settings the
environment keeps every chunk it has computed, so that a second pass does not compute them
again. Section 9.5 shows the setting that releases each chunk as you move on.

## 13.7 Search: records put to work

A store fills up: notes, reports, tables, results. The search commands of `ns-search` find
things in it, and they are built entirely from records. One command turns what you want to
search into a table, one row for each thing. Another keeps the rows that match. So a search is
a query like any other, and its result is cached and kept up to date in the same way.

```python
import liquers as lq

env = lq.Environment()
env.store_set("notes/shopping.md", """---
title: Shopping
status: open
---
# Shopping

Remember the milk and the bread.
""")
env.store_set("notes/trip.md", """---
title: Trip to Prague
status: done
---
# Trip

Book the train to Prague.

## Packing

Bread for the journey.
""")

listing = env.evaluate("-R-key/notes/-/ns-search/catalog")
assert isinstance(listing, lq.RecordView)                     # the folder as a table
assert [row["filename"] for row in listing.to_list()] == ["shopping.md", "trip.md"]

hits = env.evaluate("-R-key/notes/-/ns-search/catalog-t-t/search-milk")
assert [row["key"] for row in hits.to_list()] == ["notes/shopping.md"]
print(hits.to_list()[0]["excerpt"])          # the text around the hit
env.shutdown()
```

Two things are new in these queries.

`-R-key/notes` does not read anything. It passes the *key* `notes` to the next command, which is
how you hand a folder to a command. (`-R/notes` would try to read the folder as a value.)

`catalog` makes a record view with one row for each entry of the folder: `key`, `filename`,
`extension`, `title`, `status` and more. It has two switches: whether to walk into subfolders (on by
default) and whether to read the text of each file (off by default). `catalog-t-t` turns both
on, which `search` needs in order to look inside the documents. `search-milk` then keeps the rows that match, best match first, and
adds three columns: `score`, `matched` (where the word was found) and `excerpt`.

Listing does not evaluate anything. A key that exists only as a recipe is listed with the status
`Recipe` and is not computed.

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

### Searching by fields

Many documents begin with a block of fields, their *front matter*, as the notes above do.
`field_extract` turns those fields into columns, and a search expression can then refer to
them. The expression is the kind of value that should not be pasted into a query, so the
example builds the query with a query editor:

```python
import liquers as lq

env = lq.Environment()
env.store_set("notes/shopping.md", "---\ntitle: Shopping\nstatus: open\n---\nMilk and bread.\n")
env.store_set("notes/trip.md", "---\ntitle: Trip to Prague\nstatus: done\n---\nBook the train.\n")

editor = env.query_editor("-R-key/notes/-/ns-search/field_extract")
search = editor.action("search", ns="search")
search.set("query", "status:open")           # a text; the editor escapes the colon
search.set("rank", False)                    # a boolean
search.set("limit", 0)                       # a number; 0 means all matches

query = editor.query()
assert query.encode() == "-R-key/notes/-/ns-search/field_extract/search-status~ncolon~open-false-false-0"
open_notes = env.evaluate(query)
assert [row["title"] for row in open_notes.to_list()] == ["Shopping"]
env.shutdown()
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
unexpected, look at `evaluate_state(query).metadata.log()`.

Which columns a bare word is looked for in is decided by the schema (section 13.2): the fields
whose role says they hold text to search. When no field has such a role, `search` looks in the
columns `filename`, `title`, `description`, `doc` and `content`, whichever exist, and when none
of those exists either, in every text column. You can also name the columns yourself, as the
last arguments of `search`.

Some limits. Search reads what it searches each time its result has to be computed again; there
is no index yet, so narrow the folder before you search the content of many files. It matches
words, not meanings. And like every result, a search result is refreshed when files change
through Liquers, not when another program edits them while Liquers runs.

## 13.8 The agent memory

The second application is a *memory* for AI agents: a folder of Markdown documents with a
schema that says which collections exist and which fields their documents carry. It is the
search of section 13.7 with rules added, and it speaks in records too: listing, searching and
checking a memory each return a record view, one row for a document or a finding, which the
record commands can filter and sort further. The commands are in the namespace `mem`:

| Command | Does |
|---|---|
| `ns-mem/overview`, `ns-mem/describe` | describe the memory and its collections |
| `ns-mem/list`, `ns-mem/search`, `ns-mem/similar` | find documents |
| `ns-mem/read` | reads a document, or a summary of it |
| `ns-mem/new`, `ns-mem/write`, `ns-mem/edit` | create and change documents, with a log of every change |
| `ns-mem/check` | checks the memory against its schema |

They can be evaluated from Python like any other command. Their natural user, however, is an
agent, and for agents Liquers has a separate program, `liquers-agent`. It serves these commands
as tools over the Model Context Protocol (MCP), so that an MCP client such as a coding assistant
can use a memory directly. The program is configured with a file and needs no Python.

Why offer queries to an agent at all? An agent normally works either with a fixed list of tools,
which is safe but rigid, or by running code it writes, which is flexible but hard to check.
Queries sit in between. An agent can combine commands freely, and yet each query can be checked
before it runs, is cached, and leaves a record of what it read and produced.

EDITOR: still to be written: a worked example of a memory used from Python (a schema, `ns-mem/new`, `ns-mem/search`), and how to start `liquers-agent`. Needs a decision on how the agent binary is distributed to Python users.
SOURCE: reference/AGENT_MEMORY_SERVICE.md
