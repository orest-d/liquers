---
title: Queries
status: ready
sources: [reference/PROJECT_OVERVIEW.md, guides/QUERY_ESCAPING_GUIDE.md, reference/PIPELINES.md, guides/PIPELINE_GUIDE.md]
reviewed: 2026-11-14
---
# 3. Queries

PURPOSE: The reader can read and write a query, run one from Rust, build and edit one from values without escaping by hand, read and write a pipeline, and check one without running it.

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

## 3.9 Building and editing a query

Writing a query by hand is fine when you know its text. When part of it comes from a variable,
a user's input for example, do not paste the value into the text with `format!`. A value
containing `-`, `/` or a space would change the meaning of the query, and escaping it by hand is
easy to get wrong.

Use a *query editor* instead. It builds the query step by step, takes each argument as the Rust
value it is, and writes it into the query correctly:

```rust
use liquers_core::pipeline::QueryEditor;
use liquers_core::query::{ActionParameter, ActionRequest};

let env = library_environment(Arc::new(AsyncMemoryStore::new(&Key::new())))?;

let region = "North-West / 2"; // a value you do not control
let mut editor = QueryEditor::new(env.command_metadata_snapshot());
editor.resource("data/sales.csv", Default::default())?;
editor.action("to_record")?.ns("rec")?;
editor.action("eq")?.ns("rec")?.set("column", "region")?.set("value", region)?; // a text, escaped for you
editor.action("sort")?.ns("rec")?.set("column", "amount")?.set("ascending", false)?; // a boolean as a boolean
let mut head = editor.action("head")?;
head.ns("rec")?.set("n", 10)?; // a number as a number
assert!(head.set("no_such_argument", 1).is_err()); // refused, and nothing changed
assert_eq!(
    editor.query().encode(),
    "-R/data/sales.csv/-/ns-rec/to_record/eq-region-North~_West~.~/~.2/sort-amount-false/head-10"
);
```

Pass numbers and booleans as they are, not as text. You do not need to know how a query spells
`false` or a negative number; the editor does. `set` takes the name of an argument and any value
that converts to JSON, and puts it in the right position. `push` appends the next positional
argument instead, which is what you use for a command that takes any number of arguments.

The editor is created from the *command metadata* of the environment: the description of every
registered command, without the functions themselves. It uses the descriptions to place
arguments, so `set` with a name the command does not have is refused and leaves the editor
unchanged.

One thing the editor does not do when you add an action is check that the command exists.
`editor.editor().validate()` reports that, and so does planning the finished query
(section 3.11).

The same editor changes a query that already exists. Create it with `from_query` and edit the
steps by their position:

```rust
let query = parse_query("-R/data/sales.csv/-/ns-rec/to_record/sort-amount/head-10")?;
let mut editor = QueryEditor::from_query(&query, env.command_metadata_snapshot())?;
// The steps are counted from 0: the file, to_record, sort, head.
editor.edit_action(3)?.set("n", 3)?;
editor.edit_action(2)?.set("ascending", false)?;
assert_eq!(editor.query().encode(), "-R/data/sales.csv/-/ns-rec/to_record/sort-amount-false/head-3");

editor.edit_action(2)?.unset("ascending")?; // back to the default
let unset: Vec<&str> = editor.unset_arguments(2)?.iter().map(|argument| argument.name.as_str()).collect();
assert_eq!(unset, ["ascending", "nulls_last"]);

editor.remove(3)?; // drop the step
editor.set_filename(Some("sorted.csv"))?;
assert_eq!(editor.query().encode(), "-R/data/sales.csv/-/ns-rec/to_record/sort-amount/sorted.csv");
```

`unset_arguments` lists the arguments of a step that still have their defaults, with their types
and descriptions. It is what a form for editing a query is built from.

The older way to build a query is from its parts, with `ActionRequest` and `ActionParameter`.
It needs no environment, and it is still the right tool for a single action. But it knows
nothing about the command: every parameter is text that you place by position and convert
yourself. Encoding escapes it:

```rust
let action = ActionRequest::new("eq".to_owned()).with_parameters(vec![
    ActionParameter::new_string("region".to_owned()),
    ActionParameter::new_string(region.to_owned()),
]);
assert_eq!(action.encode(), "eq-region-North~_West~.~/~.2");
```

Prefer the editor whenever you have an environment.

API reference: [`QueryEditor`](https://orest-d.github.io/liquers/api/rust/liquers_core/pipeline/struct.QueryEditor.html),
[`ActionEditor`](https://orest-d.github.io/liquers/api/rust/liquers_core/pipeline/struct.ActionEditor.html), the object that `action` and
`edit_action` return, and [`ActionRequest`](https://orest-d.github.io/liquers/api/rust/liquers_core/query/struct.ActionRequest.html).

## 3.10 Pipelines: queries as data

{{> ../_shared/pipelines.md}}

### Running a pipeline from Rust

`Pipeline::from_json` reads a pipeline from JSON, and `serde` reads one from YAML.
`evaluate_pipeline` runs it the way `interpreter::evaluate` runs a query:

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

The result is a `State`, the value with its metadata, and not an asset reference: there is no
asset behind a pipeline, as the next section explains.

A pipeline turns into a query, and a query into a pipeline:

```rust
let cmr = env.command_metadata_snapshot();
let query = pipeline.to_query_with(&cmr)?;
assert_eq!(query.encode(), "-R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/nrows");
let back = Pipeline::try_from(&query)?;
assert_eq!(back.to_query_with(&cmr)?.encode(), query.encode());
```

`to_query_with` needs the command metadata, because it has to know the order of a command's
arguments to write named ones positionally. The other direction is also a way to look inside a
query, as data and with every step taken apart:

```rust
// The steps of a query, as data: this is how to look inside one.
let steps = Pipeline::try_from(&parse_query("-R/data/sales.csv/-/ns-rec/to_record/head-2")?)?.to_json();
assert_eq!(steps["elements"][0]["resource"], "data/sales.csv");
assert_eq!(steps["elements"][2]["action"], "head");
assert_eq!(steps["elements"][2]["parameters"][0]["value"], "2");
```

Here is a pipeline that no query can replace. Its first step is a value written in place, and
one of its arguments is a list:

```rust
// Rows written in place: a value that no query can hold.
let structured = Pipeline::from_json(&json!([
    {"value": [{"region": "EU", "amount": 120}, {"region": "US", "amount": 80}]},
    {"action": "to_record", "ns": "rec"},
    {"action": "is_in", "ns": "rec", "arguments": {"column": "region", "values": ["EU", "ASIA"]}},
    {"action": "nrows", "ns": "rec"}
]))?;
let state = evaluate_pipeline(env.clone(), &structured, None).await?;
assert_eq!(state.try_into_string()?, "1");
let error = structured.to_query_with(&cmr).unwrap_err();
assert_eq!(error.error_type, ErrorType::ParameterError);
assert!(error.to_string().contains("not representable as a query"));
```

### Editing a pipeline

A `PipelineEditor` is the query editor of section 3.9 without the limit to what a query can
hold. It has the same methods, and its values may be lists and maps:

```rust
use liquers_core::pipeline::PipelineEditor;

let mut editor = PipelineEditor::from_pipeline(structured, cmr.clone())?;
// The steps are counted from 0: the value, to_record, is_in, nrows.
editor.edit_action(2)?.set("values", json!(["US"]))?;
editor.remove(3)?;
editor.action("select_columns")?.ns("rec")?.push("amount")?;
editor.action("to_json")?.ns("rec")?;
let state = evaluate_pipeline(env.clone(), editor.pipeline(), None).await?;
assert_eq!(state.value()?.try_into_json_value()?, json!([{"amount": 80}]));
```

`PipelineEditor::new` starts an empty pipeline. A `QueryEditor` is in fact a pipeline editor
that refuses every edit after which the pipeline would no longer be a query.

API reference: [`Pipeline`](https://orest-d.github.io/liquers/api/rust/liquers_core/pipeline/struct.Pipeline.html),
[`PipelineEditor`](https://orest-d.github.io/liquers/api/rust/liquers_core/pipeline/struct.PipelineEditor.html) and
[`evaluate_pipeline`](https://orest-d.github.io/liquers/api/rust/liquers_core/interpreter/fn.evaluate_pipeline.html).

{{> ../_shared/pipeline-or-query.md}}

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
