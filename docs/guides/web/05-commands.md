---
title: Commands
status: ready
sources: [guides/JAVASCRIPT_GUIDE.md, guides/COMMAND_REGISTRATION_GUIDE.md, reference/COMMAND_DECLARATION.md, reference/PIPELINES.md, guides/PIPELINE_GUIDE.md]
reviewed: 2026-11-10
---
# 5. Commands

PURPOSE: The reader can turn a JavaScript in the browser function into a Liquers command, choose its namespace and defaults, and call it from a query.

A *command* is a function that a query can call. Writing commands is how you teach Liquers about
your data and your page. This chapter covers the whole of it: the declaration, the input, the
arguments, the context, the options, and the commands that need no function at all.

## 5.1 A function becomes a command

A command is a JavaScript function registered with a *declaration*: an object with the command's
`name` and its implementation, `run`. That is all a declaration needs:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "hello", run: () => "hello" });
liquers.registerCommand({
  name: "repeat",
  state: "text",
  arguments: [{ name: "count", type: "int", default: 2 }],
  run: (text, count) => text.repeat(count),
});

assert.strictEqual(await liquers.evaluate("hello/repeat-3"), "hellohellohello");
assert.strictEqual(await liquers.evaluate("hello/repeat"), "hellohello");            // the default
assert.strictEqual(await liquers.evaluate("hello/repeat/to_text"), "hellohello");    // a Rust command after it
```

`hello` declares only what it must, so it has the defaults: no input (it starts a query), no
arguments, the root namespace, its name as its label. `repeat` adds the two things that differ:
it takes the text before it, and it has one integer argument.

In Rust a command is registered with a macro, and in Python with a decorator that reads the
function's signature. JavaScript functions carry no types, so here the declaration says what the
signature cannot. It is the same description in all three: Liquers keeps it as the command's
*metadata*, which is what `commands_doc` prints, what a query editor uses, and what a query is
checked against before it runs.

## 5.2 The input

`state` says whether the command takes the value before it in the query, and in what form:

| `state` | `run` receives first |
|---|---|
| omitted, or `"none"` | nothing: the command starts a query |
| `"value"` | the input value, converted to JavaScript (chapter 4) |
| `"text"` | the input as a string |
| `"state"` | `{ value, metadata, status, log }`: the value with its metadata (section 4.3) |

The arguments follow it, in the order they are declared.

Choose `"text"` when the command works on text: the input is converted for you, so the command
also accepts a number or bytes that hold text. Choose `"value"` for structured data, for bytes,
and for opaque objects (section 4.2). A handle arrives as a handle: after `ns-rec/to_record`, a
command with `state: "value"` receives a `RecordBatch` (chapter 9).

## 5.3 Arguments and defaults

An argument is given in the query after the command's name, separated by `-`: `repeat-3`. A
query is text, so every argument arrives as text unless the declaration gives it a `type`:

| `type` | `run` receives |
|---|---|
| `"int"`, `"float"` | a `number`; text that is not a number is a `conversion_error` |
| `"bool"` | a `boolean` |
| `"string"` | a string |
| `"any"`, or omitted | the text of the query, or the value of a link |

With a `default`, the argument may be left out of the query. A default is a string, a number, a
boolean or `null`. An argument without a default must be given: leaving it out is an
`argument_missing` error.

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({
  name: "scale",
  arguments: [
    { name: "value", type: "int" },
    { name: "factor", type: "float", default: 0.5 },
    { name: "round", type: "bool", default: false },
  ],
  run: (value, factor, round) => (round ? Math.round(value * factor) : value * factor),
});

assert.strictEqual(await liquers.evaluate("scale-3"), 1.5);
assert.strictEqual(await liquers.evaluate("scale-3-2.5-true"), 8);
await assert.rejects(liquers.evaluate("scale-three"), (e) => e.errorType === "conversion_error");
await assert.rejects(liquers.evaluate("scale"), (e) => e.errorType === "argument_missing");
await assert.rejects(liquers.evaluate("scale-1-2-true-4"), (e) => e.errorType === "too_many_parameters");
```

Leave `arguments` out and the names are read from `run`'s parameters. They then have the type
`any` and no default:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "join", run: (left, right) => `${left}+${right}` });

assert.strictEqual(await liquers.evaluate("join-a-b"), "a+b");
assert.strictEqual(liquers.describeCommand("join").argumentsInferred, true);
```

Reading the parameters works only for plain names. A function with a default value, a rest
parameter or destructuring is refused with an error that says so, and so is a bound function. A
minifier renames parameters, which changes how the arguments are labelled, though not how they
work. Declare `arguments` whenever a type, a default or a name matters.

An argument can also be the value of another query, a *link* (section 3.6). The command receives
the value, not the text:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "hello", run: () => "world" });
liquers.registerCommand({ name: "limit", run: () => 3 });
liquers.registerCommand({
  name: "cut",
  state: "text",
  arguments: [{ name: "length", type: "any" }],
  run: (text, length) => text.slice(0, Number(length)),
});

assert.strictEqual(await liquers.evaluate("hello/cut-2"), "wo");
assert.strictEqual(await liquers.evaluate("hello/cut-~X~limit~E"), "wor");   // the value of `limit`
```

## 5.4 The context

Name a parameter, or a declared argument, `context` and the command receives its execution
context there. It is not an argument of the query. The context writes to the log of the result
and evaluates other queries on the command's behalf:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({
  name: "price",
  arguments: [{ name: "item", type: "string" }],
  run: (item) => ({ apple: 2, pear: 3 })[item],
});
liquers.registerCommand({
  name: "total",
  async: true,
  run: async (context) => {
    context.info("adding two prices");
    const apple = await context.evaluate("price-apple");
    const pear = await context.evaluate("price-pear");
    return apple + pear;
  },
});

assert.strictEqual(await liquers.evaluate("total"), 5);

const state = await (await liquers.getAsset("total")).get();
assert(state.log().some((entry) => entry.message === "adding two prices"));
const dependencies = state.metadata().dependencies.map((dependency) => dependency.key);
assert(dependencies.includes("price-apple") && dependencies.includes("price-pear"));
```

| Member | What it does |
|---|---|
| `context.info(message)`, `context.warning(message)` | add an entry to the log of the result |
| `context.error(message)` | logs an error, and makes the evaluation fail once `run` returns |
| `context.evaluate(query)` | evaluates another query and resolves with its value |
| `context.render`, `context.expand`, `context.validate` | render a template (chapter 10) |
| `context.element` | the user-interface element that ran the command, or `null` (chapter 11) |
| `context.isCancelled()` | always `false` for now (section 5.7) |

Use `context.evaluate` rather than `liquers.evaluate` inside a command. Both give the value, but
only the first records the other query as a *dependency* of the result, as the last lines of the
example show. The record is what tells you, and Liquers, what a value was made from.

Use the context only during the call. Keeping it in a variable and using it after `run` has
returned throws. A command that really has an argument called `context` declares
`conventions: { context: false }`.

## 5.5 The working directory

A command often wants a file "next to" the one the query is about. A command cannot find out
where the query is working: the context does not carry a current directory. The directory comes
in as an argument instead, whose default is a link:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({
  name: "read_sibling",
  async: true,
  arguments: [{ name: "dir", default: { pipeline: ["-R-key/."] } }, { name: "context" }],
  run: (dir, context) => context.evaluate(`-R/${dir.encode()}/hello.txt`),
});

const env = liquers.Environment.global();
await env.configureStore({ stores: [{ type: "memory", prefix: "proj" }] });
await env.store().set("proj/a/hello.txt", new TextEncoder().encode("hello from a"), {});

assert.strictEqual(await liquers.evaluate("-R-cwd/proj/a/-/read_sibling"), "hello from a");
```

The link `-R-key/.` evaluates to the key of the query's working directory, and the command
receives it as a `Key` object. A query sets that directory with `-R-cwd/<folder>/-/`, and a
recipe has the folder of its `recipes.yaml` as its working directory without saying so
(chapter 7). Because the directory is an ordinary argument, a caller can pass another one.

The default is written as `{ pipeline: ["-R-key/."] }` because a declaration cannot yet spell a
link default as plain query text.

## 5.6 Names, namespaces and other options

Further fields of the declaration describe the command:

| Field | Meaning |
|---|---|
| `namespace` | the group the command is in; a query selects it with `ns-<namespace>` before the command |
| `label`, `doc` | a display name and a description, shown by `commands_doc` and by editors |
| `volatile` | `true` for a result that must never be reused (chapter 7) |
| `cached` | `false` for a cheap command with a large result: it is run again instead of being kept |
| `async` | `true` when `run` returns a `Promise` (section 5.7) |

An argument can carry a `label` too.

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "hello", run: () => "hello" });
liquers.registerCommand({
  name: "shout",
  namespace: "demo",
  label: "Shout",
  doc: "Returns the text in capital letters.",
  state: "text",
  run: (text) => text.toUpperCase(),
});

assert.strictEqual(await liquers.evaluate("hello/ns-demo/shout"), "HELLO");
await assert.rejects(liquers.evaluate("hello/shout"), (e) => e.errorType === "action_not_registered");
assert((await liquers.evaluate("commands_doc")).includes("Returns the text in capital letters."));
```

Put the commands of an application into a namespace of their own. The root namespace is shared
with the commands of the library, and a name you register there replaces a library command of
the same name (section 5.10).

Rules for names: use lower-case letters, digits and underscores. A command whose name has a dash
or a dot can never be called, because a dash separates arguments and a dot marks a file name in a
query.
The names `ns`, `q` and `v` are query instructions: `ns` is refused, and a command named `q` or
`v` registers but never runs. The namespace `web` is reserved for the module's own commands.

`liquers.describeCommand(name)` returns the metadata of a command of the root namespace, or
`null`. It returns Liquers' own record, so its field names are those of the metadata, such as
`argument_type`, not those of the declaration.

## 5.7 Asynchronous commands

A command may return a `Promise`, and Liquers waits for it. Say so with `async: true`:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({
  name: "later",
  async: true,
  run: async () => {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return "done";
  },
});
liquers.registerCommand({ name: "wrong", async: false, run: () => Promise.resolve(1) });

assert.strictEqual(await liquers.evaluate("later"), "done");
await assert.rejects(liquers.evaluate("wrong"), (e) => e.errorType === "execution_error");
```

With `async: false`, a returned `Promise` is an error instead of a value, which catches a
forgotten `await`.

This is where the browser differs most from a native program. Native Liquers runs commands on a
pool of threads and can stop one that is no longer wanted. A page has one thread:

- **A synchronous command blocks the page** for as long as it runs. Nothing repaints, no click is
  handled. Keep synchronous commands short.
- **An asynchronous command gives the thread back** whenever it awaits: a `fetch`, a timer, a
  message from a worker. Do slow work that way. A command that calls `fetch` and awaits the
  answer costs the page nothing while it waits.
- **Cancelling does nothing yet.** `asset.cancel()` exists and `context.isCancelled()` always
  answers `false`. A command that may run long should have its own way to stop, such as an
  `AbortController` that your page controls.
- **A mounted Liquers user interface** does not repaint while a query it started is being
  evaluated (chapter 11).

## 5.8 Errors from a command

An exception a command throws fails the query. The caller receives a `LiquersError` of the type
`execution_error`, whose message starts with the exception's class and message and continues
with its stack. A command that throws a `LiquersError` keeps its type:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "fails", run: () => { throw new TypeError("bad input"); } });
liquers.registerCommand({
  name: "lookup",
  arguments: [{ name: "id", type: "string" }],
  run: (id) => { throw new liquers.LiquersError("key_not_found", `no record ${id}`); },
});
liquers.registerCommand({
  name: "careful",
  run: (context) => { context.error("the input was empty"); return null; },
});

await assert.rejects(liquers.evaluate("fails"), (e) =>
  e.errorType === "execution_error" && e.message.startsWith("TypeError: bad input"));
await assert.rejects(liquers.evaluate("lookup-a"), (e) =>
  e.errorType === "key_not_found" && e.message === "no record a");
await assert.rejects(liquers.evaluate("careful"), (e) => e.message === "the input was empty");
```

Throw a `LiquersError` when the caller can do something about the kind of error: "not found" is
handled differently from "the command is broken". Section 15.3 lists the types.

A failed evaluation is not kept. Asking for the same query again runs the command again, so an
error that came from the network can go away on the next try.

## 5.9 Commands without a function

Two kinds of command are defined by data instead of a function.

**A pipeline command** is a declaration with `pipeline` in place of `run`. Its arguments fill
the pipeline's `{ placeholder }` gaps, and a call runs as the long query would:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommands([
  {
    name: "loud_greet",
    pipeline: [{ action: "greet", parameters: [{ placeholder: "who" }] }, { action: "shout" }],
    arguments: [{ name: "who", default: "world" }],
  },
  { name: "greet", arguments: [{ name: "who", type: "string" }], run: (who) => `hi ${who}` },
  { name: "shout", state: "text", run: (text) => text.toUpperCase() },
]);

assert.strictEqual(await liquers.evaluate("loud_greet-Ada"), "HI ADA");
assert.strictEqual(await liquers.evaluate("loud_greet"), "HI WORLD");
```

A pipeline command gives a name to a combination of commands that users would otherwise have to
repeat. It has no function, so `run`, `state` and `async` are refused beside `pipeline`. The
batch is checked as a whole before anything is registered: a step that names an unknown command,
or a placeholder without an argument, is reported.

**A template command** is a declaration with `template` in place of `run`: the link of a
template in the store. A call renders the template with the input as its value (section 10.5).

## 5.10 Registering many, replacing and removing

`registerCommands(input, impls)` registers a batch, all or nothing. `input` is an array of
declarations, as in section 5.9, or a JSON or YAML document of command metadata; `impls` then
gives each command's function by name:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
const yaml = `
commands:
  - name: greet
    arguments:
      - { name: who, argument_type: string, default: world }
`;
const report = liquers.registerCommands(yaml, { greet: (who) => `Hello, ${who}!` });

assert.strictEqual(report.hasErrors, false);
assert.strictEqual(await liquers.evaluate("greet"), "Hello, world!");
assert.strictEqual(await liquers.evaluate("greet-Ada"), "Hello, Ada!");
```

The document form lets a page and a server share one description of their commands. If anything
in a batch is wrong, nothing is registered and a `LiquersError` of the type `parameter_error` is
thrown; its `report` lists every problem, not only the first. `validateCommands(input, impls)`
returns the same report without registering anything, and works before `liquers.init()`:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
const report = liquers.validateCommands([
  { name: "ok", run: () => 1 },
  { name: "missing_run" },
  { name: "odd", state: "sideways", run: (x) => x },
]);

assert.strictEqual(report.errorCount, 2);
assert.deepStrictEqual(report.issues.map((issue) => issue.command.name), ["missing_run", "odd"]);
```

**Register before the first evaluation.** A command registered later still works, but the
environment is rebuilt for it: the commands and the store configuration are kept, the results
computed so far are forgotten, and a `memory` store starts empty again (section 8.3). Register a
batch in one call, which rebuilds once.

Registering a name that already exists replaces the command, with a `console.warn` that names
what was replaced. A library command can be overridden this way. `unregisterCommand(name)`
removes your command and returns whether there was one:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();
await liquers.init();
liquers.registerCommand({ name: "hello", run: () => "hello" });
assert.strictEqual(await liquers.evaluate("hello/to_text"), "hello");

liquers.registerCommand({ name: "to_text", state: "text", run: (t) => `<${t}>` });   // warns
assert.strictEqual(await liquers.evaluate("hello/to_text"), "<hello>");

assert.strictEqual(liquers.unregisterCommand("to_text"), true);
assert.strictEqual(await liquers.evaluate("hello/to_text"), "hello");   // the library command is back
```

`unregisterCommand` takes a name only, and removes your commands of that name in every
namespace. Removing a command releases its function.

## 5.11 Designing a good command

A command is used in queries you will never see: in a link, in a template, in a recipe somebody
else writes. A few habits make that work.

- **Do one thing.** Small commands combine; a command with a `mode` argument is usually three
  commands. The query is where steps are put together.
- **Take the main input from the state**, and only parameters from the arguments. Then the
  command can follow any other command.
- **Declare the arguments**, with types and defaults. A user who writes the bare name should get
  something sensible.
- **Keep arguments short and textual.** A query is a name and a URL. A command that needs a
  structure should read it from the store, or be called in a pipeline.
- **Be deterministic, or say that you are not.** A command that reads a clock, the network, the
  page or a random source is `volatile`.
- **Reach other data through the context**, so that the dependency is recorded.
- **Return plain data** whenever it fits, so that the library's commands can continue where yours
  stops. Keep opaque objects for what really cannot be data.
- **Do not touch the page from a command.** A command that writes into the document cannot be
  cached, tested under Node or reused on a server. Return a value, and let the page, a template
  or a user-interface element show it.
- **Write the `doc`.** It is what the next person reads in the list of commands.
