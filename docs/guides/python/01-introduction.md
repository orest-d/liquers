---
title: Introduction
status: draft
sources: [reference/VISION.md, reference/PROJECT_OVERVIEW.md, guides/PYTHON_GUIDE.md]
reviewed: 2026-11-09
---
# 1. Introduction

PURPOSE: The reader has run a first query from Python, knows what Liquers offers, and knows how the query connects its parts: commands, the store, assets and caching, recipes, templates, the web API and the user interface.

Liquers is a universal toolkit for working with data. Think of a Swiss Army knife: many tools,
each useful on its own, folded into one handle so that they are always at hand and fit together.
Liquers has a tool for turning your functions into reusable building blocks, a tool for storing
data, a tool for caching results and keeping them fresh, a tool for describing which files a
folder should contain, a tool for writing reports, a tool for serving all of this over the web,
and more.

The handle that holds these tools together is the *query*: a short piece of text that says how
to make a value. Every tool in Liquers either makes queries possible or does something useful
with them. So the plan of this chapter is simple. You run a query first. Then you meet the tools
one at a time, each as a new thing the same query can do.

## 1.1 A first query

After you install the package (chapter 2), this runs as it is:

```python
import liquers as lq

@lq.first_command
def hello() -> str:
    return "world"

@lq.command
def greet(text: str, greeting: str = "Hello") -> str:
    return f"{greeting}, {text}!"

print(lq.evaluate("hello/greet"))       # Hello, world!
print(lq.evaluate("hello/greet-Hi"))    # Hi, world!
```

Two ordinary Python functions became *commands*: `hello`, which makes a value from nothing, and
`greet`, which takes a value and makes another one. The text `hello/greet-Hi` is a query. Read it
from left to right, like a path: run `hello`, pass its result to `greet`, and give `greet` the
argument `Hi`. `lq.evaluate` runs the query and returns its value.

That is the whole mechanism. A query is a chain of commands, and a command is a function. What
makes it worth having is everything that follows from writing the chain down as text.

## 1.2 One text, many uses

A function call disappears once it returns. A query does not, because it is a piece of data. The
same text is, at the same time:

- **a recipe**: it says completely how the value is made, so anybody with the same commands and
  the same data can make it again;
- **a name**: the value can be asked for by its query, from Python, from a web page or from a
  command line;
- **a link**: a query contains only characters that are allowed in a URL, so it can be the path
  of one;
- **a cache key**: if the value of this query was already computed, it does not have to be
  computed again;
- **a record of origin**: a result that carries its query tells you how it came to be.

Each tool of the toolkit uses one or more of these. The rest of this chapter walks through them.
Every step is a few lines long, and every example is a complete program.

## 1.3 The command library

You do not have to write every command yourself. Liquers comes with a library of commands for
tables, images, text, search and templates. One of them lists all the others:

```python
import liquers as lq

listing = lq.evaluate("commands_doc")       # Markdown: every command you can use
assert listing.startswith("# Commands")
```

EDITOR: this block is also the first example of specs/guides/PYTHON_GUIDE.md §3; keep the two identical.

Your commands and the library's commands mix freely in one query. The library is written in
Rust, so its commands are fast, and from the point of view of a query it makes no difference in
which language a command was written. Chapter 5 is about writing commands, and appendix B lists
the library.

## 1.4 The store

Data has to live somewhere. In Liquers it lives in the *store*, which you can picture as a folder
tree: every file has a *key*, such as `data/sales.csv`. The store can be a directory on your
disk, a bucket in the cloud or, as in this example, just memory.

A query can start from a file. `-R/` followed by a key reads it, and `/-/` separates the key from
the commands that follow:

```python
import liquers as lq

env = lq.Environment()
env.store.set("data/sales.csv", b"region,month,amount\nEU,jan,120\nUS,jan,80\nEU,feb,95\n")

table = env.evaluate("-R/data/sales.csv/-/ns-rec/to_record/eq-region-EU")
print(table.to_list())
# [{'region': 'EU', 'month': 'jan', 'amount': 120}, {'region': 'EU', 'month': 'feb', 'amount': 95}]

print(env.evaluate("-R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/nrows"))   # 2
env.shutdown()
```

The query reads the file, turns it into a table with `to_record`, and keeps the rows where
`region` equals `EU`. The three commands come from the library; `ns-rec` selects the group of
commands they belong to. No Python function was written for this example.

It also introduced the *environment*. An environment holds a store and a set of commands, and
queries are evaluated in it. `lq.evaluate` and `@lq.command` use a default environment that the
package creates for you. Making your own, as here, gives you control over where the data lives
(chapters 6 and 8).

## 1.5 Assets and caching

The result of a query is kept. Ask for the same query twice, and it is computed once:

```python
import liquers as lq

env = lq.Environment()
runs = []

@env.first_command
def slow_answer() -> int:
    runs.append(1)          # stands in for a long computation
    return 42

assert env.evaluate("slow_answer") == 42
assert env.evaluate("slow_answer") == 42
assert len(runs) == 1       # the second call reused the first result
env.shutdown()
```

You did not name a cache or invent a key. The query is the key.

What Liquers keeps is called an *asset*: the value together with its status (is it being
computed, ready, failed?) and its metadata (which query made it, what it was made from, what the
commands logged). Because an asset knows what it was made from, Liquers can tell when it is out
of date. If `data/sales.csv` is replaced, the results made from it are computed again the next
time they are asked for, and nothing else is. Chapter 7 explains assets.

## 1.6 Recipes

Some results deserve a name of their own. A *recipe* gives a query's result a key in the store,
so that it looks like a file. Recipes are written in a file called `recipes.yaml`, placed in the
folder where the results should appear:

```python
import liquers as lq

env = lq.Environment()
env.store.set("data/sales.csv", b"region,month,amount\nEU,jan,120\nUS,jan,80\nEU,feb,95\n")
env.store.set("data/recipes.yaml", b"""
recipes:
  - query: -R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/eu.csv
    title: Sales in the EU
""")

print(env.evaluate("-R/data/eu.csv").to_list())     # the two EU rows
env.shutdown()
```

Nobody wrote a file `data/eu.csv`. It is made on demand, from the query in the recipe, the first
time somebody reads it. A reader of `data/eu.csv` does not need to know the query, and you can
change the query without telling the readers. This is what `make` does for programs, applied to
data: you describe what a folder should contain, and Liquers produces the files when they are
needed and keeps them up to date.

## 1.7 Templates

A report is text with results in it. A Liquers *template* is text with queries in it:

{{=<% %>=}}
```python
import liquers as lq

env = lq.Environment()
env.store.set("data/sales.csv", b"region,month,amount\nEU,jan,120\nUS,jan,80\nEU,feb,95\n")

text = env.expand("We have {{$ -R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/nrows}} EU sales.")
print(text)                 # We have 2 EU sales.
env.shutdown()
```

The tag `{{$ …}}` is replaced by the value of its query. The rendered text depends on that query,
so it is cached and refreshed like any other result (chapter 10).
<%={{ }}=%>

## 1.8 The web, the user interface and more

The tools so far all run inside your Python program. Because a query is text that fits into a
URL, it also travels well beyond it:

- **A web API.** A Liquers server makes every query an address: a request for
  `/q/-R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/eu.csv` returns the EU rows as CSV. Nobody
  writes an endpoint for it. Whatever can be said as a query is already served, with the store
  and the assets available over HTTP as well (chapter 12).
- **User interfaces.** In a Liquers user interface, what you look at is the value of a query, so
  a view can be bookmarked, shared and restored. The same queries run in a desktop application
  and, compiled to WebAssembly, in a browser tab with no server at all (chapter 11).
- **Search.** Commands that list, read and search the documents and tables in a store
  (chapter 13).
- **Tools for AI agents.** A query is a tool call that can be checked before it runs, is cached,
  and leaves a record of what it did. Liquers serves commands as tools over the Model Context
  Protocol (chapter 13).

Not all of these are driven from Python. The server is a Rust program and the browser runs
JavaScript, and each has its own guide: *Liquers for Rust* and *Liquers for the Web*. What matters
here is that they are not separate products that you would have to connect. They are the same
toolkit, and a query means the same thing in all of them. The query you try in a notebook is the
query a server serves and a web page shows.

## 1.9 How the parts fit together

| Tool | What it does with a query | Chapter |
|---|---|---|
| Commands | Provide the steps a query is made of | [5](05-commands.md) |
| The store | Holds the files a query starts from, and the results | [6](06-stores.md) |
| Assets | Evaluate a query, track its status, cache its value, keep it fresh | [7](07-assets.md) |
| Recipes | Give the result of a query a key, so it reads like a file | [7](07-assets.md) |
| The environment | Holds the store and the commands that queries run in | [8](08-environment.md) |
| Command libraries | Tables, data frames, images: steps you do not have to write | [9](09-data.md) |
| Templates | Put the values of queries into text | [10](10-templates.md) |
| The web API | Serves every query at a URL | [12](12-serving.md) |
| Search and agent memory | Find documents and rows; give agents queries as tools | [13](13-search.md) |

You can use as many of these as you need. A script that only wants cached, named results uses
commands and assets and ignores the rest. The tools are there when the script grows into a
service.

## 1.10 Where Liquers fits, and where it does not

Liquers is at its best when many small or medium-sized results are derived from shared data and
used by several people or programs: reports, extracts, charts, dashboards, internal data
services. It fits when it matters that a result is current and that you can say where it came
from. It suits teams in which some people write commands and others combine them, and it lets a
prototype grow into a service without being rewritten, because the queries stay the same.

It is the wrong tool for some jobs, by design:

- distributed processing of very large data: Liquers evaluates inside one process, and is not a
  cluster engine;
- transactional applications: its model is derived data that can be recomputed, not concurrent
  updates of records;
- code where every microsecond counts: an evaluation is planned and recorded, which costs a
  little each time;
- endless streams of events: tables can be read in chunks, but Liquers is not a streaming
  engine.

Liquers is at version 0.1. It works and it is tested, but its interfaces can still change
between releases.

## 1.11 Where to go next

Read chapters 2 and 3 next: installing the package, and the query language. After that the
chapters can be read in any order.

| If you want to | Read |
|---|---|
| Write your own commands | [5. Commands](05-commands.md) |
| Keep data on disk or in the cloud | [6. Stores](06-stores.md) |
| Understand caching and write recipes | [7. Recipes, assets and caching](07-assets.md) |
| Work with tables, pandas or Polars | [9. Working with data](09-data.md) |
| Produce reports | [10. Templates](10-templates.md) |
| Find out why something failed | [15. Troubleshooting](15-troubleshooting.md) |
