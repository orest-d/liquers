---
title: Queries
status: ready
sources: [reference/PROJECT_OVERVIEW.md, guides/QUERY_ESCAPING_GUIDE.md, reference/PIPELINES.md, guides/PIPELINE_GUIDE.md]
reviewed: 2026-11-10
---
# 3. Queries

PURPOSE: The reader can read and write a query, run one from Rust, build one from values without escaping by hand, and check one without running it.

The query is the one thing in Liquers that you cannot do without, and the one thing that is the
same in every language. This chapter first describes the query language itself, in the words
shared by all three guides. Sections 3.7 to 3.11 then show how to work with queries from Rust.

The examples of the first sections use small commands of your own, `hello`, `greet` and `repeat`,
like those of chapters 1 and 5, and a few commands of the library.

{{> ../_shared/queries.md}}

## 3.7 Running a query

There are three ways to run a query. They differ in what is kept:

```rust
fn hello() -> Result<Value, Error> {
    Ok(Value::from("world"))
}
fn greet(state: &State<Value>, greeting: String) -> Result<Value, Error> {
    Ok(Value::from(format!("{greeting}, {}!", state.try_into_string()?)))
}

let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
let mut builder = library_builder(store)?;
let cr = &mut builder.command_registry;
register_command!(cr, fn hello() -> result)?;
register_command!(cr, fn greet(state, greeting: String = "Hello") -> result)?;
let env = builder.build()?;

// 1. Ask for the asset, then wait for its state.
let asset = env.evaluate("hello/greet-Hi").await?;
let state = asset.get().await?;
assert_eq!(state.try_into_string()?, "Hi, world!");
assert_eq!(asset.status().await, Status::Ready);

// 2. A parsed query works wherever text does.
let query = parse_query("hello/greet")?;
let state = env.evaluate(&query).await?.get().await?;
assert_eq!(state.try_into_string()?, "Hello, world!");

// 3. One-off evaluation that keeps nothing.
let state = liquers_core::interpreter::evaluate(env.clone(), "hello/greet-Hey", None).await?;
assert_eq!(state.try_into_string()?, "Hey, world!");
```

`env.evaluate(query)` is the normal one. It goes through the *asset manager*, the part of the
environment that keeps results. It returns an `AssetRef`: a cheap, clonable reference to the
asset of this query. If the asset already exists, you get the same one, whether it is finished
or still being computed. `get()` waits for the value and returns the state. Two tasks that ask
for the same query at the same time therefore share one computation.

`evaluate` accepts whatever can be turned into a query: `&str`, `String`, `&Query`, `Query`. Text
is parsed first, so the call can fail with a parse error before any asset exists. It also fails
before anything runs when the query cannot be *planned*: when a command does not exist or its
arguments do not fit (section 3.11).

The free function `liquers_core::interpreter::evaluate` plans the query and runs the plan at
once, in the calling task. Nothing is looked up and nothing is kept, so calling it twice computes
twice. It is the right tool for a one-off evaluation in a test or a small program, and the wrong
one for a service. Its third argument is an optional *payload*: a value handed to the commands of
this one evaluation, which chapter 11 uses for user interfaces.

| Call | Returns | Kept |
|---|---|---|
| `env.evaluate(q).await?` | `AssetRef`, at once | yes |
| `env.evaluate(q).await?.get().await?` | `State`, when ready | yes |
| `interpreter::evaluate(env.clone(), q, None).await?` | `State` | no |

## 3.8 Looking inside a query

`parse_query` turns text into a `Query` without running anything, and `parse_key` does the same
for a key. Both types are plain data. You can take them apart, build them, compare them, use them
as keys of a map, and serialize them with `serde`:

```rust
let query = parse_query("-R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/eu.csv")?;
assert_eq!(query.encode(), "-R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/eu.csv");
assert_eq!(query.filename().map(|name| name.name), Some("eu.csv".to_string()));
assert_eq!(query.extension(), Some("csv".to_string()));
assert_eq!(query.segments.len(), 2);

let key = parse_key("data/sales.csv")?;
assert_eq!(key.len(), 2);
assert_eq!(key.extension(), Some("csv".to_string()));
assert_eq!(key.parent().encode(), "data");
assert_eq!(key.parent().join("eu.csv").encode(), "data/eu.csv");
assert_eq!(Query::from(key).encode(), "-R/data/sales.csv");

let error = parse_query("hello/greet-~").unwrap_err();
assert_eq!(error.error_type, ErrorType::ParseError);
assert_eq!(error.position.column, 13); // where in the text
```

A `Query` is a list of *segments*. The query above has two: the one that reads the file (a
`ResourceQuerySegment`) and the one that transforms it (a `TransformQuerySegment`, a list of
actions with an optional file name). `encode()` gives the text back in its canonical form, and a
parsed and encoded query is equal to its source for every query that was canonical to begin with.

A parse error carries the position of the problem in the text, which a user interface can use to
mark the place.

Parsing checks the syntax only. `parse_query("no_such_command")` succeeds, because the parser
does not know which commands exist.

## 3.9 Building a query from values

Writing a query by hand is fine when you know its text. When part of it comes from a variable,
a user's input for example, do not paste the value into the text with `format!`. A value
containing `-`, `/` or a space would change the meaning of the query, and escaping it by hand is
easy to get wrong.

Use a *query editor* instead. It builds the query step by step and escapes every value:

```rust
use liquers_core::pipeline::QueryEditor;
use liquers_core::query::{ActionParameter, ActionRequest};

let env = library_environment(Arc::new(AsyncMemoryStore::new(&Key::new())))?;

let region = "North-West / 2"; // a value you do not control
let mut editor = QueryEditor::new(env.command_metadata_snapshot());
editor.resource("data/sales.csv", Default::default())?;
editor.action("to_record")?.ns("rec")?;
let mut filter = editor.action("eq")?;
filter.ns("rec")?.push("region")?.push(region)?;
assert!(filter.set("no_such_argument", 1).is_err()); // refused, and nothing changed
assert_eq!(
    editor.query().encode(),
    "-R/data/sales.csv/-/ns-rec/to_record/eq-region-North~_West~.~/~.2"
);
```

The editor is created from the *command metadata* of the environment: the description of every
registered command, without the functions themselves. It uses the descriptions to place
arguments, so `set` with a name the command does not have is refused and leaves the editor
unchanged. `push` appends the next positional argument.

One thing the editor does not do when you add an action is check that the command exists.
`editor.editor().validate()` reports that, and so does planning the finished query
(section 3.11).

For a single action, without an environment, build an `ActionRequest` from its parts. Encoding
escapes the parameters:

```rust
let action = ActionRequest::new("eq".to_owned()).with_parameters(vec![
    ActionParameter::new_string("region".to_owned()),
    ActionParameter::new_string(region.to_owned()),
]);
assert_eq!(action.encode(), "eq-region-North~_West~.~/~.2");
```

## 3.10 Pipelines: queries as data

Some arguments cannot be written into a query at all: a nested structure, a list, a long text.
For these there is a second form of a query, the *pipeline*. A pipeline is the same chain of
steps written as data: a list in which each step is either query text or a map that names an
action and its arguments. It has a JSON and a YAML form, so it is also the form in which another
program can send you a computation.

```rust
use liquers_core::interpreter::evaluate_pipeline;
use liquers_core::pipeline::Pipeline;
use serde_json::json;

let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
store.set(&parse_key("data/sales.csv")?, SALES, &Metadata::new()).await?;
let env = library_environment(store)?;

let pipeline = Pipeline::from_json(&json!([
    "-R/data/sales.csv/-/ns-rec/to_record",
    {"action": "eq", "ns": "rec", "parameters": ["region", "EU"]},
    {"action": "nrows", "ns": "rec"}
]))?;
let state = evaluate_pipeline(env.clone(), &pipeline, None).await?;
assert_eq!(state.try_into_string()?, "2");
```

`evaluate_pipeline` runs a pipeline the way `interpreter::evaluate` runs a query. Every query can
be turned into a pipeline, and a pipeline can be turned back into a query when all its arguments
can be written as text:

```rust
let cmr = env.command_metadata_snapshot();
let query = pipeline.to_query_with(&cmr)?;
assert_eq!(query.encode(), "-R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/nrows");
let back = Pipeline::try_from(&query)?;
assert_eq!(back.to_query_with(&cmr)?.encode(), query.encode());
```

`to_query_with` needs the command metadata, because it has to know the order of a command's
arguments to write named ones positionally.

Prefer a query when one can say what you mean: it is shorter, it is a link, it is cached as an
asset, and it can be used in a recipe and in a URL. Use a pipeline when an argument is
structured data. Section 5.9 shows a third use: a pipeline registered as a command.

## 3.11 Checking a query without running it

Between parsing and running stands the *plan*. Planning resolves every action to a command,
converts every argument to the type the command declares, fills in the defaults, and produces a
list of steps. Everything that can be wrong with a query short of the data is found here, and no
command runs:

```rust
use liquers_core::plan::PlanBuilder;

let env = library_environment(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
let cmr = env.command_metadata_snapshot();

let plan = PlanBuilder::new(parse_query("-R/data/sales.csv/-/ns-rec/to_record/nrows")?, &cmr).build()?;
assert_eq!(plan.len(), 3); // read the file, to_record, nrows

let error = PlanBuilder::new(parse_query("ns-rec/nrows-1-2-3")?, &cmr).build().unwrap_err();
assert_eq!(error.error_type, ErrorType::TooManyParameters);
let error = PlanBuilder::new(parse_query("no_such_command")?, &cmr).build().unwrap_err();
assert_eq!(error.error_type, ErrorType::ActionNotRegistered);
assert!(error.to_string().contains("no_such_command"));
```

`env.evaluate` plans before it creates the asset, so these are the errors it returns directly. A
plan is data too: it serializes to JSON, and it is stored in the metadata of the result as the
record of what was done.

The same check is available on the command line, for the queries you put in documentation,
recipes and tests. The tool `liquers-validate` is part of `liquers-core`:

```bash
cargo run -p liquers-core --features cli --bin liquers-validate -- -- '-R/data/sales.csv/-/ns-rec/to_record/nrows'
```

It prints how the query was understood: the segments, the resolved commands and the arguments
after conversion. The second `--` is needed because a query usually starts with a dash. Run
inside the Liquers repository, the tool checks against a list of the library's commands that is
kept there; for your own commands, export that list from your environment or name the commands
with `--command`.

A query that passes this check is well formed. That is not the same as correct: of the two
queries in section 3.4, both pass. Compare what the tool prints with what you meant.
