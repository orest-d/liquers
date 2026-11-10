---
title: Commands
status: ready
sources: [guides/COMMAND_REGISTRATION_GUIDE.md, guides/COMMAND_DESIGN_GUIDE.md, reference/COMMAND_DECLARATION.md, reference/REGISTER_COMMAND_FSD.md, reference/PIPELINES.md, guides/PIPELINE_GUIDE.md, guides/TEMPLATE_GUIDE.md]
reviewed: 2026-11-10
---
# 5. Commands

PURPOSE: The reader can turn a Rust function into a Liquers command, declare its arguments, defaults and options, use the context, report errors, and knows the ways to define a command without writing a function.

A *command* is a function that a query can call. Writing commands is how you teach Liquers about
your data and your domain. This chapter covers the whole of it: the registration macro, the
input, the arguments, the context, the options, and the commands that need no function at all.

## 5.1 A function becomes a command

There are always two parts: an ordinary function, and its registration.

```rust
// 1. Ordinary functions. Each returns `Result<Value, Error>`.
fn hello() -> Result<Value, Error> {
    Ok(Value::from("world"))
}

fn greet(state: &State<Value>, greeting: String) -> Result<Value, Error> {
    let text = state.try_into_string()?;
    Ok(Value::from(format!("{greeting}, {text}!")))
}

// 2. Registration: the signature again, in the macro's own notation.
let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
let cr = &mut builder.command_registry;
register_command!(cr, fn hello() -> result)?;
register_command!(cr,
    fn greet(state, greeting: String = "Hello") -> result
    label: "Greet"
    doc: "Greets the input text"
)?;
let env = builder.build()?;

// 3. Use.
let state = env.evaluate("hello/greet-Hi").await?.get().await?;
assert_eq!(state.try_into_string()?, "Hi, world!");
```

The function knows nothing about queries. It takes Rust values and returns `Result<Value, Error>`,
and you can call and test it like any other function.

`register_command!` is a function-like macro with a small notation of its own. Its first argument
is the *command registry*, the table of commands of the environment under construction. The rest
describes the function: its name, how it wants its input, its arguments with their types and
defaults, and what it returns. The macro generates a wrapper that takes the arguments out of the
query, converts them, calls your function and wraps the result. It also records a description of
the command, its *command metadata*, which is what planning, the query editor, user interfaces
and the generated documentation read.

The general shape is:

```rust
// Sketch: the notation of register_command!.
register_command!(registry,
    [async] fn name(input, argument: Type = default, …, context) -> result
    option: value
    …
)?;
```

Why repeat the signature? A macro of this kind cannot see the function, only its name. In
exchange the declaration says things a Rust signature cannot: defaults, labels, which argument is
a link to another query.

The return is `-> result` for a function returning `Result<Value, Error>` and `-> value` for one
returning `Value`. Prefer `result`: nearly every command can fail.

Two things must be in scope where the macro is used: the type `Context` from `liquers-core`, and
a type named `CommandEnvironment` (section 2.4). The macro returns a `Result`, because
registering can fail, for instance when an option does not parse. On success it gives you the
new command metadata, which the examples ignore.

## 5.2 The input

The first parameter of a command is the result of the action before it. The word you write in
the macro decides in which form the function receives it:

| In the macro | The function receives | Use it when |
|---|---|---|
| `state` | `&State<Value>`, or `State<Value>` in an `async fn` | you want the value in its own type, or the metadata |
| `text` | `&str` | the command works on text |
| nothing | nothing | the command starts a query and makes its value from its arguments |

```rust
fn hello() -> Result<Value, Error> {
    Ok(Value::from("world"))
}

// `state`: the value with its metadata, by reference.
fn describe(state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(format!("{} from {}", state.type_identifier(), state.metadata.query()?.encode())))
}

// `text`: the value converted to text.
fn shout(text: &str) -> Result<Value, Error> {
    Ok(Value::from(text.to_uppercase()))
}

let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
let cr = &mut builder.command_registry;
register_command!(cr, fn hello() -> result)?;
register_command!(cr, fn describe(state) -> result)?;
register_command!(cr, fn shout(text) -> result)?;
let env = builder.build()?;

let text = |query: &'static str| {
    let env = env.clone();
    async move { env.evaluate(query).await?.get().await?.try_into_string() }
};
assert_eq!(text("hello/describe").await?, "Text from hello");
assert_eq!(text("hello/shout").await?, "WORLD");
```

`text` converts with `try_into_string`, so such a command also accepts numbers and fails with a
conversion error on a table. A command that takes `state` decides for itself: it calls
`state.try_into_string()`, `state.value()?.as_record_view()` or whatever fits, and each of these
reports a conversion error if the input is of another kind.

A command with no input word ignores what came before it. It is what a query starts with.

## 5.3 Arguments and defaults

After the input come the arguments. Each has a name and a type, and optionally a default and a
description:

```rust
fn repeat(text: &str, count: usize, separator: String) -> Result<Value, Error> {
    Ok(Value::from(vec![text; count].join(&separator)))
}

fn first(text: &str, n: Option<usize>) -> Result<Value, Error> {
    Ok(Value::from(match n {
        Some(n) => text.chars().take(n).collect::<String>(),
        None => text.to_string(),
    }))
}

fn join(words: Vec<String>) -> Result<Value, Error> {
    Ok(Value::from(words.join(" ")))
}

fn scale(text: &str, unit: String) -> Result<Value, Error> {
    let number: i64 = text.parse().map_err(|e| Error::general_error(format!("{e}")))?;
    Ok(Value::from(number * if unit == "k" { 1000 } else { 1 }))
}

let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
let cr = &mut builder.command_registry;
register_command!(cr,
    fn repeat(text, count: usize = 2 (label: "Count", description: "How many copies"),
              separator: String = "") -> result
)?;
register_command!(cr, fn first(text, n: Option<usize>) -> result)?;
register_command!(cr, fn join(words: Vec<String> multiple) -> result)?;
register_command!(cr,
    fn scale(text, unit: String = "one" (enum: ["one", "k"])) -> result
)?;
let env = builder.build()?;

let text = |query: &'static str| {
    let env = env.clone();
    async move { env.evaluate(query).await?.get().await?.try_into_string() }
};
assert_eq!(text("join-ab/repeat").await?, "abab"); // both defaults
assert_eq!(text("join-ab/repeat-3").await?, "ababab");
assert_eq!(text("join-ab/repeat--~.").await?, "ab ab"); // count left empty: its default
assert_eq!(text("join-abcdef/first-2").await?, "ab");
assert_eq!(text("join-abcdef/first").await?, "abcdef"); // None
assert_eq!(text("join-a-b-c").await?, "a b c"); // three arguments
assert_eq!(text("join-a~_b").await?, "a-b"); // one argument with a dash in it
assert_eq!(text("join-3/scale-k").await?, "3000");
```

**Types.** The type decides how the text of the query is converted, and the conversion happens
when the query is planned:

| Type in the macro | From the query |
|---|---|
| `String` | the text, unescaped |
| `i8` … `i64`, `u8` … `u64`, `isize`, `usize` | an integer |
| `f32`, `f64` | a float |
| `bool` | `t`, `true`, `yes`, `y`, `1` or `f`, `false`, `no`, `n`, `0`, in any case |
| `Option<T>` for a number or `bool` | as above; left out or empty gives the default, or `None` |
| `Vec<T>` with `multiple` | every remaining argument of the action |
| `Value` | the text as a value, or the result of a link |
| `Key`, `Query` | a parsed key or query, or the result of a link |

**Defaults.** An argument with a default may be left out of the query. The default is written as
a literal: `"text"`, `42`, `3.14`, `true`. An argument without one is required, and a query that
leaves it out fails when it is planned. `Option<T>` has no meaning for text; for an optional text
use `String = ""`.

**`multiple`.** One argument can take all the remaining parameters of the action. It must be a
`Vec`, has no default, and must be the last argument that reads from the query.

**Descriptions.** The parentheses after an argument hold what a user interface shows for it: a
`label`, a `description`, a `gui` hint such as `IntegerSlider(0, 100, 1)` or `TextArea 80, 10`,
and `enum` for a fixed set of allowed values. None of them changes what the function receives,
except that a value outside an `enum` is refused.

**Checked early.** A wrong argument never reaches your function:

```rust
// Wrong arguments are refused when the query is planned, before any command runs.
let error = env.evaluate("join-ab/repeat-x").await.err().expect("not a number");
assert_eq!(error.error_type, ErrorType::ConversionError);
let error = env.evaluate("join-3/scale-M").await.err().expect("not a unit");
assert!(error.to_string().contains("Valid values: one, k"));
let error = env.evaluate("join-ab/repeat-1-x-y").await.err().expect("one too many");
assert_eq!(error.error_type, ErrorType::TooManyParameters);
```

**Links.** The default of an argument can be a query instead of a literal:
`template: Value = query "-R/templates/default.md"`. The command then receives the *result* of
that query, and the result becomes a dependency of the command's own result. A caller can
override it with another link, written between `~X~` and `~E` (section 3.6). Section 5.5 uses a
link for a common need, the working directory.

**`injected`.** An argument marked `injected` is not read from the query. It is taken from the
payload of the evaluation, the value that a user interface passes along with a query
(chapter 11).

## 5.4 The context

A command that wants to say something about its work, or to ask for the result of another query,
declares the parameter `context`. It has no type in the macro; the function receives a
`Context<CommandEnvironment>`:

```rust
fn hello() -> Result<Value, Error> {
    Ok(Value::from("world"))
}

async fn report(state: State<Value>, context: Context<CommandEnvironment>) -> Result<Value, Error> {
    context.info("building the report")?;
    let greeting = context.get_dependency_state(&parse_query("hello")?).await?;
    if state.is_none() {
        context.warning("no input")?;
    }
    context.set_title("Report").await?;
    Ok(Value::from(format!("Hello, {}!", greeting.try_into_string()?)))
}

let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
let cr = &mut builder.command_registry;
register_command!(cr, fn hello() -> result)?;
register_command!(cr, async fn report(state, context) -> result)?;
let env = builder.build()?;

let state = env.evaluate("report").await?.get().await?;
assert_eq!(state.try_into_string()?, "Hello, world!");
assert_eq!(state.metadata.title(), "Report");
let record = state.metadata.metadata_record().expect("a metadata record");
assert!(record.log.iter().any(|entry| entry.message == "building the report"));
let dependencies = state.metadata.get_dependencies();
assert!(dependencies.iter().any(|dependency| dependency.key.to_string() == "hello"));
```

| Method | Does |
|---|---|
| `info(…)`, `warning(…)`, `debug(…)`, `error(…)` | add a line to the log in the metadata |
| `set_title(…)`, `set_description(…)` | name the result for people; `await` them |
| `get_dependency_state(&query)` | evaluate another query, wait for it, and record it as a dependency |
| `wait_for_dependency(&query)` | the same, without reading the value |
| `submit(&query)` | start another query without waiting and without a dependency |
| `evaluate(&query)` | get the `AssetRef` of another query |
| `is_cancelled()`, `check_cancelled()?` | see whether the caller gave up (section 5.7) |
| `get_envref()` | the environment itself, for everything else |

`get_dependency_state` is the important one. A dependency recorded this way makes the result of
your command out of date when the other query's result changes, exactly as if the two had been
written as one query. Evaluating through `context.get_envref()` instead gives the same value and
records nothing, so the cache would not know about the connection.

Put `context` last in both the function and the macro. It is not an argument of the query and may
stand anywhere, but last is the convention.

## 5.5 The working directory

A command often wants a file "next to" the one the query is about. A command cannot find out
where the query is working: the context does not carry a current directory. The directory comes
in as an argument instead, with a link as its default:

```rust
async fn read_sibling(
    _state: State<Value>,
    dir: Key,
    context: Context<CommandEnvironment>,
) -> Result<Value, Error> {
    let sibling = Query::from(dir.join("hello.txt")); // absolute, so it is accepted
    let state = context.get_dependency_state(&sibling).await?;
    Ok(Value::from(state.try_into_string()?))
}

let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
store.set(&parse_key("proj/a/hello.txt")?, b"hello from a", &Metadata::new()).await?;
let mut builder = library_builder(store)?;
let cr = &mut builder.command_registry;
register_command!(cr, async fn read_sibling(state, dir: Key = query "-R-key/.", context) -> result)?;
let env = builder.build()?;

let state = env.evaluate("-R-cwd/proj/a/-/read_sibling").await?.get().await?;
assert_eq!(state.try_into_string()?, "hello from a");
```

The link `-R-key/.` evaluates to the key of the query's working directory. A query sets that
directory with `-R-cwd/<folder>/-/`, and a recipe has the folder of its `recipes.yaml` as its
working directory without saying so (chapter 7). Because the directory is an ordinary argument,
a caller can pass another one, and the command can be tested without any directory at all.

## 5.6 Names, namespaces and other options

Lines after the signature set options of the command. Each is a word, a colon and a value:

```rust
#[liquers_macro::command_version]
fn now() -> Result<Value, Error> {
    let since_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| Error::general_error(e.to_string()))?;
    Ok(Value::from(format!("{}", since_epoch.as_nanos())))
}

#[liquers_macro::command_version]
fn shout(text: &str) -> Result<Value, Error> {
    Ok(Value::from(text.to_uppercase()))
}

let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
let cr = &mut builder.command_registry;
register_command!(cr,
    fn now() -> result
    namespace: "clock"
    label: "Now"
    doc: "Nanoseconds since 1970, as text"
    volatile: true
)?;
register_command!(cr,
    fn shout(text) -> result
    namespace: "words"
    label: "Shout"
    doc: "The input text in capitals"
    expires: "in 5 min"
    version: auto
)?;
let env = builder.build()?;

let first = env.evaluate("ns-clock/now").await?.get().await?.try_into_string()?;
let second = env.evaluate("ns-clock/now").await?.get().await?.try_into_string()?;
assert_ne!(first, second); // volatile: computed on every request

// Volatility is contagious: whatever is made from a volatile value is volatile too.
let state = env.evaluate("ns-clock/now/ns-words/shout").await?.get().await?;
assert_eq!(state.status(), Status::Volatile);
```

| Option | Meaning |
|---|---|
| `namespace: "words"` | the command is called as `ns-words/shout`. Without it the command is in the root namespace and is found from every namespace |
| `label:`, `doc:` | a short name and a description, for user interfaces and the generated documentation |
| `volatile: true` | the result is different each time (the time, a random number), so it is never reused |
| `expires: "in 5 min"` | the result may be reused for a limited time. Also `"at 12:00"`, `"EOD"`, `"never"` |
| `cached: false` | the command is cheap and its output large: run it each time and keep only what is made from it |
| `version: auto` | the result depends on the code of the function. With `#[liquers_macro::command_version]` on the function, a change of its body makes stored results out of date |
| `filename: "report.html"` | the default file name of the result |
| `preset:`, `next:` | queries that a user interface offers together with the command |
| `realm:` | a third level above the namespace, for commands that run in another place than the rest |

Choose `volatile`, `expires` and `version` with care, because they decide when a result is
computed again. Volatility is contagious: a result made from a volatile value is itself volatile.
Chapter 7 explains how expiry and versions take part in keeping results fresh.

Give your commands a namespace as soon as there are more than a handful. The root namespace is
shared with the library and with everybody else's commands.

## 5.7 Async commands and cancelling

A command that waits, for the network, for a file or for another query, is an `async fn`,
registered with `async fn` in the macro:

```rust
async fn later(state: State<Value>, millis: u64) -> Result<Value, Error> {
    tokio::time::sleep(std::time::Duration::from_millis(millis)).await;
    Ok(Value::from(format!("{} later", state.try_into_string()?)))
}

fn word(text: String) -> Result<Value, Error> {
    Ok(Value::from(text))
}

let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
let cr = &mut builder.command_registry;
register_command!(cr, async fn later(state, millis: u64 = 10) -> result)?;
register_command!(cr, fn word(text: String) -> result)?;
let env = builder.build()?;

let state = env.evaluate("word-soon/later").await?.get().await?;
assert_eq!(state.try_into_string()?, "soon later");
```

There are two differences from a synchronous command: the function receives the state by value
(`State<Value>`, which is cheap to clone) and it may use `.await`. Use an async command whenever
the work is mostly waiting. A synchronous command occupies its worker thread until it returns.

The caller of a long evaluation can give up with `asset.cancel()`. An async command is stopped at
its next `.await` without doing anything for it. A synchronous command cannot be interrupted from
outside, so a long one should ask:

```rust
// Sketch: a synchronous command that can be cancelled.
fn process_rows(text: &str, context: Context<CommandEnvironment>) -> Result<Value, Error> {
    let mut output = String::new();
    for line in text.lines() {
        context.check_cancelled()?; // an error of the type Cancelled once somebody gave up
        output.push_str(&expensive(line));
    }
    Ok(Value::from(output))
}
```

Return the error of `check_cancelled` unchanged, with `?`. Wrapped in another error it would
count as a failure and not as a cancellation.

## 5.8 Errors from a command

A command reports a failure by returning `Err`. All of Liquers uses one error type,
`liquers_core::error::Error`, which has an `ErrorType`, a message and, when known, the query and
the position the error belongs to. Make one with a constructor:

| Constructor | For |
|---|---|
| `Error::general_error(message)` | anything without a more specific type |
| `Error::conversion_error(from, to)` | a value of the wrong kind |
| `Error::key_not_found(&key)` | a missing file |
| `Error::from_error(ErrorType::General, error)` | wrapping the error of another library |

Do not panic in a command, and do not use `unwrap`. A panic takes a worker of the environment
with it; an error becomes part of the result.

That last point shapes how errors reach the caller:

```rust
fn checked(text: &str) -> Result<Value, Error> {
    let number: i64 = text
        .parse()
        .map_err(|e| Error::general_error(format!("'{text}' is not a number: {e}")))?;
    Ok(Value::from(number))
}

fn word(text: String) -> Result<Value, Error> {
    Ok(Value::from(text))
}

let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
let cr = &mut builder.command_registry;
register_command!(cr, fn checked(text) -> result)?;
register_command!(cr, fn word(text: String) -> result)?;
let env = builder.build()?;

let asset = env.evaluate("word-abc/checked").await?; // planned and started: no error yet
let state = asset.get().await?; // finished: still no error
assert_eq!(state.status(), Status::Error);
let error = state.value().err().expect("the command's error");
assert_eq!(error.error_type, ErrorType::General);
assert!(error.message.starts_with("'abc' is not a number"));
```

The query was valid and was planned, so `evaluate` succeeded. The asset ran, the command failed,
and the asset ended with the status `Error`. `get()` still returns a state: the state of a failed
asset, whose metadata holds the error, the log up to that point and the query. The error itself
comes out of `value()`, `try_into_string()` and every other attempt to read the value. In a chain
written with `?` you do not notice the difference. When you want to show a failure to a user,
keep the state: its metadata is the report.

Results that failed are remembered like any others, so asking again does not run the command
again. Chapter 7 says how to make an asset try again.

## 5.9 Commands without a function

Three kinds of commands are defined by data instead of code. They are registered on the same
registry, appear in the same documentation and are called the same way.

```rust
use liquers_core::command_metadata::{ArgumentInfo, ArgumentType, CommandKey, CommandParameterValue};
use liquers_core::pipeline::Pipeline;
use serde_json::json;

let store = Arc::new(AsyncMemoryStore::new(&Key::new()));
store.set(&parse_key("data/sales.csv")?, SALES, &Metadata::new()).await?;
store.set(&parse_key("templates/count.txt")?, b"{{.}} rows", &Metadata::new()).await?;
let mut builder = library_builder(store)?;
let cr = &mut builder.command_registry;

// An alias: `sales/first_rows` is `rec/head` with a default of one row.
cr.register_alias(
    CommandKey::new("", "sales", "first_rows"),
    CommandKey::new("", "rec", "head"),
    vec![],
    vec![ArgumentInfo::integer_argument("n", false).with_default(1)],
)?
.with_label("First rows");

// A pipeline command: several steps under one name, with an argument.
cr.register_pipeline(
    CommandKey::new("", "sales", "count_region"),
    Pipeline::from_json(&json!([
        {"action": "to_record", "ns": "rec"},
        {"action": "eq", "ns": "rec", "parameters": ["region", {"placeholder": "region"}]},
        {"action": "nrows", "ns": "rec"}
    ]))?,
    vec![ArgumentInfo::any_argument("region").with_type(ArgumentType::String)],
)?;

// A template command: an alias of `template/render` with the template fixed.
cr.register_alias(
    CommandKey::new("", "sales", "count_text"),
    CommandKey::new("", "template", "render"),
    vec![CommandParameterValue::Query(parse_query("-R/templates/count.txt")?)],
    vec![
        ArgumentInfo::any_argument("data").with_default(""),
        ArgumentInfo::string_argument("errors").with_default("strict"),
        ArgumentInfo::any_argument("cwd").with_default_link(parse_query("-R-key/.")?),
    ],
)?;
let env = builder.build()?;

let count = env.evaluate("-R/data/sales.csv/-/ns-sales/count_region-EU").await?.get().await?;
assert_eq!(count.try_into_string()?, "2");
let text = env
    .evaluate("-R/data/sales.csv/-/ns-sales/count_region-EU/count_text")
    .await?
    .get()
    .await?;
assert_eq!(text.try_into_string()?, "2 rows");
let rows = env
    .evaluate("-R/data/sales.csv/-/ns-rec/to_record/ns-sales/first_rows/ns-rec/nrows")
    .await?
    .get()
    .await?;
assert_eq!(rows.try_into_string()?, "1");
```

**An alias** is another name for an existing command, possibly with some arguments fixed and
others given new defaults. `sales/first_rows` is `rec/head` with a default of one row. An alias
costs nothing at run time, because planning replaces it with its target.

**A pipeline command** is a pipeline (section 3.10) under a name. Its steps can contain
*placeholders*, which become the arguments of the new command. `count_region` takes one argument,
`region`, and puts it into the `eq` step. This is the way to give a name to a chain of steps
that people keep writing.

**A template command** is an alias of the library's `template/render` with the template fixed.
The result of `count_text` is the text of the template with the input put in. Since the template
is read from the store, editing it changes the command, and results made with the old template
are computed again. Chapter 10 covers templates.

Reach for these before writing a function whenever the new command only combines what exists.

## 5.10 Registering without the macro

The macro is a convenience. Underneath, a command is a closure that receives the state, the
arguments and the context, and you can register one directly:

```rust
use liquers_core::command_metadata::CommandKey;

let prefix = String::from(">> "); // captured by the closure
let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
let cr = &mut builder.command_registry;
cr.register_command(CommandKey::new_name("answer"), |_state, _arguments, _context| {
    Ok(Value::from(42))
})?
.with_label("The answer");
cr.register_command(CommandKey::new_name("quote"), move |state, _arguments, _context| {
    Ok(Value::from(format!("{prefix}{}", state.try_into_string()?)))
})?;
let env = builder.build()?;

let state = env.evaluate("answer/quote").await?.get().await?;
assert_eq!(state.try_into_string()?, ">> 42");
```

Do this when a command has to capture something, as `quote` captures `prefix`: a connection, a
configuration, a function of another language. The Python and JavaScript bindings register every
command this way. You then read the arguments yourself from the second parameter, and describe
them yourself on the metadata that `register_command` returns, so prefer the macro whenever the
command is a plain function.

**Commands for more than one environment.** The examples fix the environment type with
`type CommandEnvironment = DefaultEnvironment<Value>`. A library of commands that should work in
any environment, with any value type, is written generically instead: its functions are generic
over the environment `E` and use `E::Value` where the examples use `Value`.

```rust
// Sketch: a command that works with every environment and every value type.
pub fn shout<E: Environment>(state: &State<E::Value>) -> Result<E::Value, Error> {
    Ok(E::Value::from_string(state.try_into_string()?.to_uppercase()))
}
```

Such a function builds its result with the methods of `ValueInterface`, such as `from_string`,
and asks for more with a bound on `E::Value` only when it needs more. It is registered like any
other command, in a place where the environment type is known. The commands of `liquers-lib` are
written this way, which is why the same commands serve a desktop program, the Python package and
the browser module.

## 5.11 Designing a good command

A command is used by people who did not write it, in queries you will never see. A few habits
make that work.

- **Do one thing.** Small commands combine; a command with a `mode` argument is usually three
  commands. The query is where steps are put together.
- **Take the main input from the state**, and only parameters from the arguments. Then the
  command can follow any other command.
- **Order arguments by how often they are given**, and give all but the first one or two a
  default. A user who writes `head` should get something sensible.
- **Keep arguments short and textual.** A query is a name and a URL. A command that needs a
  structure should read it from the store, or be called in a pipeline.
- **Be deterministic, or say that you are not.** A command that reads a clock, the network or a
  random source is `volatile` or has an `expires`.
- **Reach other data through the context**, with `get_dependency_state`, so that the dependency
  is recorded. A command that opens a file behind Liquers' back produces results that are never
  refreshed.
- **Return a value of a standard kind** whenever one fits, so that the library's commands can
  continue where yours stops.
- **Name commands for what they make**, in lower case with underscores: `monthly_totals`, not
  `process`. Names never contain a dot or a dash.
- **Write the `doc:`.** It is what the next person reads in the list of commands, and what an AI
  agent reads when it decides whether to call your command.
