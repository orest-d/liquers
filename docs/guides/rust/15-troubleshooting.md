---
title: Troubleshooting
status: ready
sources: [reference/PROJECT_OVERVIEW.md, guides/COMMAND_REGISTRATION_GUIDE.md, guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md]
reviewed: 2026-11-14
---
# 15. Troubleshooting

PURPOSE: The reader can read a Liquers error, find its cause, and knows the frequent mistakes and the limits of the Rust crates.

{{> ../_shared/errors.md}}

## 15.3 Errors in Rust

All of Liquers uses one error type, `liquers_core::error::Error`. Its fields are public:

| Field | Holds |
|---|---|
| `error_type` | an `ErrorType`, the types of section 15.1 |
| `message` | the message |
| `position` | where in the query text, when known |
| `query`, `key` | what the error is about, when known |
| `command_key` | the command that failed, when one did |

Where an error comes out depends on the stage:

```rust
fn fail() -> Result<Value, Error> {
    Err(Error::general_error("the disk is on fire".to_string()))
}

let mut builder = library_builder(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
let cr = &mut builder.command_registry;
register_command!(cr, fn fail() -> result)?;
let env = builder.build()?;

// 1. The text is not a query.
let error = env.evaluate("fail/-x-/").await.err().expect("a parse error");
assert_eq!(error.error_type, ErrorType::ParseError);

// 2. The query cannot be planned.
let error = env.evaluate("no_such_command").await.err().expect("a planning error");
assert_eq!(error.error_type, ErrorType::ActionNotRegistered);

// 3. A command fails: the asset ends in the status Error and keeps the report.
let asset = env.evaluate("fail").await?;
let state = asset.get().await?;
assert_eq!(state.status(), Status::Error);
let error = state.value().err().expect("the command's error");
assert_eq!(error.error_type, ErrorType::General);
assert_eq!(error.message, "the disk is on fire");
assert!(error.to_string().contains("fail")); // the text names the command
assert!(state.metadata.is_error()?);

// 4. Data is missing.
let state = env.evaluate("-R/data/missing.csv").await?.get().await?;
let error = state.value().err().expect("no such file");
assert_eq!(error.error_type, ErrorType::AssetNotFound);
```

Stages 1 and 2 are errors of `env.evaluate(…).await`. Stages 3 and 4 happen inside the asset: the
evaluation was started, so `evaluate` and `get` succeed, the state has the status `Error`, and
the error comes out when you read the value. Code that chains everything with `?` sees an `Err`
in each case.

One thing differs from the table of section 15.1. A command written in Rust reports the type it
chose itself, `General` in the example, and Liquers adds the command to the error. `ExecutionError`
is used for a command of another language whose failure has no Liquers type, such as a Python
exception.

Match on `ErrorType` without a catch-all arm when the cases matter to you, so that a new type in
a later version is a compile error and not a silently ignored case.

## 15.4 Checking a query before running it

`PlanBuilder` checks a query in code and `liquers-validate` on the command line; section 3.11
shows both. When a query is valid and still does the wrong thing, look at the plan. Its steps
say which key is read, which command each action resolved to, and what every argument became
after conversion and defaults.

## 15.5 Frequent mistakes

| What you see | The usual cause |
|---|---|
| ``cannot find type `CommandEnvironment` in this scope`` | `register_command!` needs a type alias of that name where it is used (section 2.4) |
| ``cannot find type `Context` in this scope`` | the macro needs `liquers_core::context::Context` imported |
| an unresolved `futures` or `serde_json` in code you did not write | the macro's code uses both; add them to your dependencies (section 2.2) |
| `expected Result<Value, Error>` at the macro | the function returns another type; convert to `Value` inside the function |
| a panic "there is no reactor running" | the environment was built outside a Tokio runtime (section 8.3) |
| `ActionNotRegistered` for a library command | its group was not registered (section 2.4), its feature is off, or the query lacks `ns-…` |
| `AssetNotFound` with a key that ends in a command's name | the `/-/` between the key and the actions is missing (section 3.4) |
| recipes are ignored | the environment was built from the bare `EnvironmentBuilder` of `liquers-core` (section 8.1) |
| a result does not change after its input file changed | the file was written with `store.set` or by another program while the environment was running (section 6.5) |
| a command that reads a clock or the network returns an old value | it is not declared `volatile` and has no `expires` (section 5.6) |
| `ConversionError` on a CSV file | a stored CSV file is bytes until `ns-rec/to_record` or `ns-pl/from_csv` reads it (section 4.4) |
| a query argument arrives cut at a `-` | the value was pasted into the query unescaped (section 3.9) |
| a stored result cannot be written: unknown type identifier | a value type without a `TypeInfo` (section 4.5) |
| a build fails with ``failed to find tool "clang"`` on Windows for arm64 | install LLVM (section 2.2) |
| the disk fills up during a build | reduce the debug information (section 2.2) |

## 15.6 Known limitations

Liquers is at version 0.1. These are the limits most likely to matter to a Rust program.

- **The interfaces are not stable.** Names and signatures can change between releases, most of
  all in the user interface framework (chapter 11) and in the MCP crate.
- **Not on crates.io.** The crates are used from the repository (section 2.2).
- **No limit on the cache.** The asset manager keeps what its strategies tell it to keep, with
  no bound on memory. Choose the strategies of section 7.10 for a long-running service.
- **Diagnostics go to standard error** and cannot be turned off or redirected to a logging
  framework yet.
- **One process.** Evaluation happens in the process that asks. Several processes can share a
  store, and nothing coordinates their writes.
- **Changes behind Liquers' back** are noticed when a file is next read, not when they happen.
- **No authentication in the server.** Put the server behind a proxy that authenticates
  (section 12.2).
- **Search has no index.** It reads what it searches each time (section 13.7).
- **Records have no grouping and joining.** Use Polars for those (section 9.3), which is not
  available in a WebAssembly build.
- **Cancelling is cooperative.** A synchronous command that never checks runs to its end
  (section 5.7).

The same in Python: [15. Troubleshooting](../python/15-troubleshooting.md).
