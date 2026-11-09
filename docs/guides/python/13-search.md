---
title: Search and agent memory
status: draft
sources: [guides/SEARCH_GUIDE.md, reference/AGENT_MEMORY_SERVICE.md, reference/VISION.md]
reviewed: 2026-11-09
---
# 13. Search and agent memory

PURPOSE: The reader can list and search stored documents and tables from Python, and knows what the agent memory and the MCP server offer.

A store fills up: notes, reports, tables, results. The search commands of `ns-search` find
things in it. They are ordinary commands that produce and filter tables, so a search is a query
like any other, and its result is cached and kept up to date in the same way.

## 13.1 Listing and searching a folder

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
assert [row["filename"] for row in listing.to_list()] == ["shopping.md", "trip.md"]

hits = env.evaluate("-R-key/notes/-/ns-search/catalog-t-t/search-milk")
assert [row["key"] for row in hits.to_list()] == ["notes/shopping.md"]
print(hits.to_list()[0]["excerpt"])          # the text around the hit
env.shutdown()
```

Two things are new in these queries.

`-R-key/notes` does not read anything. It passes the *key* `notes` to the next command, which is
how you hand a folder to a command. (`-R/notes` would try to read the folder as a value.)

`catalog` makes a table with one row for each entry of the folder: `key`, `filename`,
`extension`, `title`, `status` and more. It has two switches: whether to walk into subfolders (on by
default) and whether to read the text of each file (off by default). `catalog-t-t` turns both
on, which `search` needs in order to look inside the documents. `search-milk` then keeps the rows that match, best match first, and
adds three columns: `score`, `matched` (where the word was found) and `excerpt`.

Listing does not evaluate anything. A key that exists only as a recipe is listed with the status
`Recipe` and is not computed.

## 13.2 Searching by fields

Many documents begin with a block of fields, their *front matter*, as the notes above do.
`field_extract` turns those fields into columns, and a search expression can then refer to
them:

```python
import liquers as lq

env = lq.Environment()
env.store_set("notes/shopping.md", "---\ntitle: Shopping\nstatus: open\n---\nMilk and bread.\n")
env.store_set("notes/trip.md", "---\ntitle: Trip to Prague\nstatus: done\n---\nBook the train.\n")

search = lq.ActionRequest.from_arguments("search", ["status:open", "f", "f", "0"])
assert search.encode() == "search-status~ncolon~open-f-f-0"

open_notes = env.evaluate(f"-R-key/notes/-/ns-search/field_extract/{search.encode()}")
assert [row["title"] for row in open_notes.to_list()] == ["Shopping"]
env.shutdown()
```

The expression `status:open` contains a colon, which must be escaped in a query. The example
lets `lq.ActionRequest.from_arguments` do the escaping (chapter 3), which is the safe way to put
a search expression typed by a user into a query.

The arguments of `search` are, in order: the expression, whether to rank the results (`t` by
default), whether the case of letters matters (`f`), the largest number of results (50; 0 means
all), and then the names of the columns to search in. `search-…-f-f-0` therefore means: not
ranked, not case-sensitive, all matches.

## 13.3 Search expressions

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

## 13.4 Other things to search

The same `search` command works on any table:

| Query | Finds |
|---|---|
| `-R/data/sales.csv/-/ns-search/search-feb-f-f-0` | the rows of a stored table that contain `feb` |
| `-R-key/notes/-/ns-search/md_split/search-packing` | the *sections* of Markdown documents that match, with their headings and line numbers |
| `-R-key/src/-/ns-search/md_split-line/search-TODO-f-f-0` | the matching *lines* of text files, like `grep -n` |
| `ns-search/commands/search-resize` | the registered commands that have to do with resizing |

The last one is a quick way to find a command when you do not know its name. Each row has a
`query` column with the text that calls the command, such as `ns-img/resize`.

Some limits. Search reads the folder each time its result has to be computed again; there is no
index yet, so narrow the folder before you search the content of many files. It matches words,
not meanings. And like every result, a search result is refreshed when files change through
Liquers, not when another program edits them while Liquers runs.

## 13.5 The agent memory

The same building blocks, a store of documents and commands that list, search, read and write
them, make a *memory* for AI agents: a folder of Markdown documents with a schema that says
which collections exist and which fields their documents carry. The commands are in the
namespace `mem`:

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

TBD: A worked example of a memory used from Python (a schema, `ns-mem/new`, `ns-mem/search`), and how to start `liquers-agent`. Needs a decision on how the agent binary is distributed to Python users.
SOURCE: reference/AGENT_MEMORY_SERVICE.md
