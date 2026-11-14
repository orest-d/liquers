---
title: Introduction
status: ready
sources: [reference/VISION.md, reference/PROJECT_OVERVIEW.md, guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md]
reviewed: 2026-11-14
---
# 1. Introduction

PURPOSE: The reader has run a first query from Rust, knows what Liquers offers, and knows how the query connects its parts: commands, the store, assets and caching, recipes, templates, the web API and the user interface.

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

Rust is the language Liquers is written in. Everything in this chapter is the toolkit itself, not
a binding to it, and the crates you depend on are the ones the Python package and the browser
module are made of.

## 1.1 A first query

In a project set up as chapter 2 describes, this is a complete `src/main.rs`:

```rust
use liquers_core::{context::{Context, Environment}, error::Error, state::State};
use liquers_lib::{environment::{default_environment_builder, DefaultEnvironment}, value::Value};
use liquers_macro::register_command;

type CommandEnvironment = DefaultEnvironment<Value>;

fn hello() -> Result<Value, Error> {
    Ok(Value::from("world"))
```

Two ordinary Rust functions became *commands*: `hello`, which makes a value from nothing, and
`greet`, which takes a value and makes another one. `register_command!` registers each under its
name; what stands in the macro is the function's signature again, in a small notation that also
holds the default of an argument. The text `hello/greet-Hi` is a query. Read it from left to
right, like a path: run `hello`, pass its result to `greet`, and give `greet` the argument `Hi`.

The commands are registered on a *builder*, and `build()` turns the builder into an
*environment*: the object that holds the commands, the store and the cache, and in which queries
are evaluated. `env.evaluate(query)` asks the environment for the result of a query, and `get()`
waits for it. What comes back is a *state*: the value together with a description of it.
`try_into_string()` reads the value as text.

That is the whole mechanism. A query is a chain of commands, and a command is a function. What
makes it worth having is everything that follows from writing the chain down as text.

## 1.2 One text, many uses

A function call disappears once it returns. A query does not, because it is a piece of data. The
same text is, at the same time:

- **a recipe**: it says completely how the value is made, so anybody with the same commands and
  the same data can make it again;
- **a name**: the value can be asked for by its query, from Rust, from Python, from a web page or
  from a command line;
- **a link**: a query contains only characters that are allowed in a URL, so it can be the path
  of one;
- **a cache key**: if the value of this query was already computed, it does not have to be
  computed again;
- **a record of origin**: a result that carries its query tells you how it came to be.

Each tool of the toolkit uses one or more of these. The rest of this chapter walks through them.
Every step is a few lines long.

## 1.3 The command library

You do not have to write every command yourself. The crate `liquers-lib` comes with a library of
commands for tables, images, text, search and templates. In Rust nothing is registered behind
your back: an environment has exactly the commands you put into it. Each group of library
commands is registered with one macro, and it is convenient to collect those lines in a function
of your own. This is the file `src/library.rs` that the rest of this guide uses:

```rust
//! An environment with the Liquers command library.
use std::sync::Arc;

use liquers_core::{
    commands::CommandRegistry,
    context::{Context, EnvRef},
    environment_builder::EnvironmentBuilder,
    error::Error,
    store::AsyncStore,
};
use liquers_lib::{
    environment::{default_environment_builder, DefaultEnvironment, LibKind},
    value::Value,
};

/// The environment type. `register_command!` looks for a type of this name.
pub type CommandEnvironment = DefaultEnvironment<Value>;

/// Registers the command library. Leave out the groups you do not need.
pub fn register_library(cr: &mut CommandRegistry<CommandEnvironment>) -> Result<(), Error> {
    liquers_lib::register_core_commands!(cr)?; // to_text, commands_doc, ...
    liquers_lib::register_records_commands!(cr)?; // ns-rec
    liquers_lib::register_polars_commands!(cr)?; // ns-pl
    liquers_lib::register_image_commands!(cr)?; // ns-img
    liquers_lib::register_search_commands!(cr)?; // ns-search
    liquers_lib::register_template_commands!(cr)?; // ns-template
    Ok(())
```

`library_builder` returns the builder, so that you can add your own commands before building.
`library_environment` builds it at once. The argument is the store, which section 1.4 explains.
With the library registered, one of its commands lists all the others:

```rust
let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
let env = library_environment(store)?;

let listing = env.evaluate("commands_doc").await?.get().await?.try_into_string()?;
assert!(listing.starts_with("# Commands"));
```

From here on the examples are the body of an `async fn`, as the preface explains, and the names
they use without a `use` line (`Arc`, `AsyncMemoryStore`, `Key`, `Value`, `Error`, …) come from
the file `src/prelude.rs` of section 2.4.

Your commands and the library's commands mix freely in one query, and from the point of view of
a query it makes no difference where a command was written: in your crate, in the library, or, in
a Python process or a browser page, in another language. Chapter 5 is about writing commands, and
appendix B lists the library.

## 1.4 The store

Data has to live somewhere. In Liquers it lives in the *store*, which you can picture as a folder
tree: every file has a *key*, such as `data/sales.csv`. The store can be a directory on your
disk, a bucket in the cloud or, as in this example, just memory.

A query can start from a file. `-R/` followed by a key reads it, and `/-/` separates the key from
the commands that follow:

```rust
let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
store
    .set(
        &parse_key("data/sales.csv")?,
        b"region,month,amount\nEU,jan,120\nUS,jan,80\nEU,feb,95\n",
        &Metadata::new(),
    )
    .await?;
let env = library_environment(store)?;

let query = "-R/data/sales.csv/-/ns-rec/to_record/eq-region-EU";
let table = env.evaluate(query).await?.get().await?;
let csv = String::from_utf8_lossy(&table.as_bytes_with_data_format("csv")?).to_string();
assert_eq!(csv, "region,month,amount\nEU,jan,120\nEU,feb,95\n");

let count = env.evaluate(format!("{query}/nrows")).await?.get().await?;
assert_eq!(count.try_into_string()?, "2");
```

The query reads the file, turns it into a table with `to_record`, and keeps the rows where
`region` equals `EU`. Such a table is called a *record view*, or *records* for short. Records are
Liquers' own table type: columnar, typed, laid out like Apache Arrow, and readable in chunks when
a table is too large for memory (chapter 9). The three commands come from the library; `ns-rec`
selects the group of commands they belong to. No Rust function was written for this example.

`as_bytes_with_data_format("csv")` writes the value in a format of your choice. A state can do
that for every format its type supports, which is how the same table becomes CSV for one caller
and JSON for another.

## 1.5 Assets and caching

The result of a query is kept. Ask for the same query twice, and it is computed once:

```rust
use std::sync::atomic::{AtomicUsize, Ordering};

static RUNS: AtomicUsize = AtomicUsize::new(0);

fn slow_answer() -> Result<Value, Error> {
    RUNS.fetch_add(1, Ordering::SeqCst); // stands in for a long computation
    Ok(Value::from(42))
}

let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
let mut builder = library_builder(store)?;
let cr = &mut builder.command_registry;
register_command!(cr, fn slow_answer() -> result)?;
let env = builder.build()?;

let first = env.evaluate("slow_answer").await?.get().await?;
let second = env.evaluate("slow_answer").await?.get().await?;
assert_eq!(first.try_into_string()?, "42");
assert_eq!(second.try_into_string()?, "42");
assert_eq!(RUNS.load(Ordering::SeqCst), 1); // the second call reused the first result
```

You did not name a cache or invent a key. The query is the key.

What Liquers keeps is called an *asset*: the value together with its status (is it being
computed, ready, failed?) and its metadata (which query made it, what it was made from, what the
commands logged). `env.evaluate` returns a reference to the asset, an `AssetRef`, and that is why
there are two steps: the first finds or creates the asset, the second waits until it has a value.
Between the two you can watch its progress or cancel it.

Because an asset knows what it was made from, Liquers can tell when it is out of date. If
`data/sales.csv` is replaced, the results made from it are computed again the next time they are
asked for, and nothing else is. Chapter 7 explains assets.

## 1.6 Recipes

Some results deserve a name of their own. A *recipe* gives a query's result a key in the store,
so that it looks like a file. Recipes are written in a file called `recipes.yaml`, placed in the
folder where the results should appear:

```rust
    let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
    let sales = b"region,month,amount\nEU,jan,120\nUS,jan,80\nEU,feb,95\n";
    let recipes = b"
recipes:
  - query: -R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/eu.csv
    title: Sales in the EU
";
    store.set(&parse_key("data/sales.csv")?, sales, &Metadata::new()).await?;
    store.set(&parse_key("data/recipes.yaml")?, recipes, &Metadata::new()).await?;
    let env = library_environment(store)?;

    let eu = env.evaluate("-R/data/eu.csv").await?.get().await?;
    let csv = String::from_utf8_lossy(&eu.as_bytes_with_data_format("csv")?).to_string();
    assert_eq!(csv, "region,month,amount\nEU,jan,120\nEU,feb,95\n"); // the two EU rows
```

Nobody wrote a file `data/eu.csv`. It is made on demand, from the query in the recipe, the first
time somebody reads it. A reader of `data/eu.csv` does not need to know the query, and you can
change the query without telling the readers. This is what `make` does for programs, applied to
data: you describe what a folder should contain, and Liquers produces the files when they are
needed and keeps them up to date.

## 1.7 Templates

A report is text with results in it. A Liquers *template* is text with queries in it:

{{=<% %>=}}
```rust
let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
let sales = b"region,month,amount\nEU,jan,120\nUS,jan,80\nEU,feb,95\n";
let template = b"We have {{$ -R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/nrows}} EU sales.";
store.set(&parse_key("data/sales.csv")?, sales, &Metadata::new()).await?;
store.set(&parse_key("templates/summary.txt")?, template, &Metadata::new()).await?;
let env = library_environment(store)?;

let text = env.evaluate("-R/templates/summary.txt/-/ns-template/expand").await?.get().await?;
assert_eq!(text.try_into_string()?, "We have 2 EU sales.");
```

The tag `{{$ …}}` is replaced by the value of its query. Rendering is itself a command,
`ns-template/expand`, applied to the template that the query reads from the store. So the
rendered text is the value of a query like any other: it depends on the template and on the
query inside it, and it is cached and refreshed with them (chapter 10).
<%={{ }}=%>

## 1.8 The web, the user interface and more

The tools so far all run inside your program. Because a query is text that fits into a URL, it
also travels well beyond it:

- **A web API.** The crate `liquers-axum` turns an environment into an HTTP server in a few
  lines. Every query becomes an address: a request for
  `/liquer/q/-R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/eu.csv` returns the EU rows as
  CSV. Nobody writes an endpoint for it. Whatever can be said as a query is already served, with
  the store and the assets available over HTTP as well (chapter 12).
- **User interfaces.** In a Liquers user interface, what you look at is the value of a query, so
  a view can be bookmarked, shared and restored. The same description of a user interface runs
  in a desktop application, with egui, and in a browser (chapter 11).
- **The browser.** The crates compile to WebAssembly. A page can run queries with no server at
  all, with commands written in JavaScript next to the ones written in Rust. That is the subject
  of *Liquers for the Web*.
- **Python.** The package `liquers-py` wraps the same crates, so a query means the same thing in
  a notebook as in your Rust service. See *Liquers for Python*.
- **Search.** Commands that list, read and search the documents and tables in a store.
  They are built on records, the table type of Liquers (chapter 13).
- **Tools for AI agents.** A query is a tool call that can be checked before it runs, is cached,
  and leaves a record of what it did. Liquers serves commands as tools over the Model Context
  Protocol (chapter 13).

These are not separate products that you would have to connect. They are the same toolkit, and a
query means the same thing in all of them. The query you try in a test is the query a server
serves and a web page shows.

## 1.9 How the parts fit together

| Tool | What it does with a query | Crate | Chapter |
|---|---|---|---|
| Commands | Provide the steps a query is made of | `liquers-core`, `liquers-macro` | [5](05-commands.md) |
| The store | Holds the files a query starts from, and the results | `liquers-core`, `liquers-store` | [6](06-stores.md) |
| Assets | Evaluate a query, track its status, cache its value, keep it fresh | `liquers-core` | [7](07-assets.md) |
| Recipes | Give the result of a query a key, so it reads like a file | `liquers-core` | [7](07-assets.md) |
| The environment | Holds the store and the commands that queries run in | `liquers-core` | [8](08-environment.md) |
| Records | Tables with named, typed columns: read from CSV and other formats, filtered and sorted by library commands, read in chunks when they are large | `liquers-records` | [9](09-data.md) |
| Other command libraries | Polars data frames and images: more steps you do not have to write | `liquers-lib` | [9](09-data.md) |
| Templates | Put the values of queries into text | `liquers-template` | [10](10-templates.md) |
| User interfaces | Show the values of queries, on the desktop and in the browser | `liquers-lib` | [11](11-ui.md) |
| The web API | Serves every query at a URL | `liquers-axum` | [12](12-serving.md) |
| Records | Tables in memory and in chunks, schemas, manifests; search and the agent memory built on them | `liquers-lib`, `liquers-mcp`, `liquers-agent` | [13](13-records.md) |

You can use as many of these as you need. A program that only wants cached, named results uses
commands and assets and ignores the rest. The tools are there when the program grows into a
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

Read chapters 2 and 3 next: setting up a project, and the query language. After that the
chapters can be read in any order.

| If you want to | Read |
|---|---|
| Write your own commands | [5. Commands](05-commands.md) |
| Keep data on disk or in the cloud, or write a store | [6. Stores](06-stores.md) |
| Understand caching and write recipes | [7. Recipes, assets and caching](07-assets.md) |
| Configure an application from a file | [8. The environment](08-environment.md) |
| Work with tables, Polars or images | [9. Working with data](09-data.md) |
| Produce reports | [10. Templates](10-templates.md) |
| Run a server | [12. Serving and embedding](12-serving.md) |
| Find out why something failed | [15. Troubleshooting](15-troubleshooting.md) |
