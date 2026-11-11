---
title: Testing
status: ready
sources: [guides/UNITTEST_GUIDE.md, guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md]
reviewed: 2026-11-10
---
# 14. Testing

PURPOSE: The reader can test commands, queries, recipes and a server built on Liquers.

Code built on Liquers is easy to test, for two reasons. A command is an ordinary function. And
a query is a complete description of a computation that can be run against a small store in
memory.

## 14.1 Three levels

| Test | Checks | Needs |
|---|---|---|
| the function | the logic of a command | nothing from Liquers |
| a query | registration, argument conversion, working together with other commands | an environment with a memory store |
| the whole program | configuration, recipes, the server | a configuration document, a port |

Write most tests at the first level, for speed and for failures that point at your code. Write a
few at the second for each command, because a wrong declaration in `register_command!` is found
only there. The third level needs a handful of tests in all.

## 14.2 A test module

```rust
use crate::prelude::*;

// The command under test.
fn shout(text: &str) -> Result<Value, Error> {
    if text.is_empty() {
        return Err(Error::general_error("nothing to shout".to_string()));
    }
    Ok(Value::from(text.to_uppercase()))
```

The parts worth copying:

- **`register` is a function of its own.** The program and the tests register the commands
  through the same function, so the tests exercise the declarations that the program uses.
- **Every test builds its own environment.** An environment with a memory store is cheap to
  build, and tests that share nothing cannot disturb each other. Results
  are cached per environment, so a shared environment would also hide a second evaluation.
- **Files are put into the store directly,** with `store.set`, before the environment is built.
- **`#[tokio::test]`** provides the runtime that the environment needs. The test returns
  `Result<(), Error>`, so that `?` works and a failure prints the error.

The tests print diagnostic lines of the library to the standard error stream. `cargo test` hides
them for passing tests.

## 14.3 Useful assertions

| To check | Assert on |
|---|---|
| the value | `state.try_into_string()?`, `state.value()?.as_record_view()?`, … |
| the type | `state.type_identifier()` |
| the bytes a client would get | `state.as_bytes()?`, with a file name at the end of the query |
| that a query is refused | `env.evaluate(q).await.err()` and its `error_type` |
| that a command failed | `state.status() == Status::Error`, then `state.value().err()` |
| what a command logged | `state.metadata.metadata_record()`, the `log` |
| what a result depends on | `state.metadata.get_dependencies()` |
| that a result was reused | a counter in the command (section 1.5), or `asset.id()` of two evaluations |
| that a result is recomputed after a change | `asset.status().await == Status::Expired` after `set_state` (section 7.6) |

Compare tables as CSV text when they are small, as chapter 1 does: a failing assertion then
shows both tables.

Two things make tests of asynchronous behaviour reliable. `build()` returns a ready environment,
so no test needs to sleep before its first query. And `asset.get().await` waits for the end of
the evaluation, so no test needs to poll. A test that cancels a *synchronous* command needs a
runtime with more than one thread, `#[tokio::test(flavor = "multi_thread")]`, because the
command occupies the only thread otherwise.

## 14.4 Testing queries without running them

Queries live in recipes, templates, configuration and documentation, and they go stale when a
command changes. Planning finds that without running anything (section 3.11). A test that plans
every query your project ships is cheap and catches a renamed command or a removed argument:

```rust
// Sketch: plan every query of a list.
let cmr = env.command_metadata_snapshot();
for query in SHIPPED_QUERIES {
    PlanBuilder::new(parse_query(query)?, &cmr)
        .build()
        .map_err(|error| Error::general_error(format!("{query}: {error}")))?;
}
```

For the templates you ship, evaluate `ns-template/validate` on each (section 10.4).

## 14.5 Testing a server

Bind the server to port 0, which picks a free port, and call it with an HTTP client. The example
of section 12.2 is such a test: it reads a configuration, binds, sends requests and shuts the
server down. Each test gets its own server and its own environment, so they run in parallel.

## 14.6 Testing a store

A store of your own is tested against the conformance suite (section 6.6). It checks the rules
that every store must follow, including the awkward ones: keys that are prefixes of each other,
metadata without data, removing the last file of a folder.

The same in Python: [14. Testing](../python/14-testing.md).
