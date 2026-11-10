---
title: Installing and setting up
status: ready
sources: [reference/PROJECT_OVERVIEW.md, guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md]
reviewed: 2026-11-10
---
# 2. Installing and setting up

PURPOSE: The reader has a Rust project that depends on Liquers, knows which crate and which feature provides what, and has the two helper files the examples use.

Liquers is a set of crates in one repository. You depend on the ones you need. This chapter
lists them, sets up a project, and gives you the two small files that the examples of this guide
rely on.

## 2.1 The crates

| Crate | What it holds | Depend on it when |
|---|---|---|
| `liquers-core` | The query language, commands, stores, assets, recipes, the environment | always |
| `liquers-macro` | `register_command!` | you write commands |
| `liquers-lib` | The value type `Value` with tables, images and user interface values; the command library | you want the library, which is nearly always |
| `liquers-store` | Stores on a file system, on S3 and on other services, through OpenDAL | data lives outside memory |
| `liquers-records` | Record streams: columnar tables, table formats, sources | you work with records without `liquers-lib` |
| `liquers-template` | The template language | you render templates without `liquers-lib` |
| `liquers-markdown` | A Markdown parser that keeps addresses of sections and links | you navigate Markdown without `liquers-lib` |
| `liquers-axum` | The web API, as routes for an Axum server | you serve queries over HTTP |
| `liquers-mcp` | Commands as tools of the Model Context Protocol | you give commands to an AI agent |

`liquers-core` depends on none of the others and is kept small. `liquers-records`,
`liquers-template`, `liquers-markdown` and `liquers-mcp` depend on `liquers-core` only, so a
program that needs one of them does not have to compile the whole library. `liquers-lib` brings
them together behind features (section 2.3).

Three more crates are products built from these and are not dependencies of yours:
`liquers-py` is the Python package, `liquers-web` the browser module, and `liquers-agent` a
program that serves a memory to AI agents.

## 2.2 A project

Liquers is not on crates.io yet, so a project depends on the repository:

EDITOR: replace the git dependencies with version numbers when the crates are published.

```toml
[package]
name = "my-liquers-app"
version = "0.1.0"
edition = "2021"

[dependencies]
liquers-core  = { git = "https://github.com/orest-d/liquers.git" }
liquers-macro = { git = "https://github.com/orest-d/liquers.git" }
liquers-lib   = { git = "https://github.com/orest-d/liquers.git" }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
futures = "0.3"
serde_json = "1"
```

`tokio` runs the program: Liquers is asynchronous throughout, and an environment needs a Tokio
runtime from the moment it is built. `futures` and `serde_json` are used by the code that
`register_command!` generates, so a crate that registers commands needs both, even if its own
code never names them.

Add `rev = "…"` with a commit to each of the three git lines when a build has to be repeatable.
All crates have the same version, and the three lines should always point at the same commit.

**The toolchain.** Liquers needs Rust 1.95 or newer; `rustup update stable` brings an older
toolchain up to date. It also needs a C compiler, because two of its dependencies for
cryptography compile C and assembly code:

- Linux: `gcc` or `clang`, from the `build-essential` package on Debian and Ubuntu;
- macOS: the Xcode command line tools (`xcode-select --install`);
- Windows: the Visual Studio Build Tools with "Desktop development with C++". On Windows for
  arm64 also install LLVM, which provides `clang` (`winget install LLVM.LLVM`), and CMake
  (`winget install Kitware.CMake`).

Liquers does not need OpenSSL. The one exception is the test suite of `liquers-axum` itself,
which you only build when you work on that crate.

**Build size.** A debug build of the whole library with full debug information takes more than
20 GB of disk. If that matters, reduce the debug information in your `Cargo.toml`. These are the
settings the repository uses, and they bring the build to about 4 GB:

```toml
[profile.dev]
debug = "line-tables-only"

[profile.test]
debug = 0
```

## 2.3 Features

`liquers-lib` has its larger parts behind features. All but `webui` are on by default.

| Feature | Adds | Query namespace |
|---|---|---|
| `records` | tables as record views and record sources | `ns-rec`, `ns-search` |
| `records-ipc`, `records-parquet` | the Arrow IPC and Parquet formats for records | |
| `polars` | Polars data frames | `ns-pl` |
| `image-support` | image commands | `ns-img` |
| `templates` | templates | `ns-template` |
| `markdown` | navigation in Markdown documents | `ns-md` |
| `mem` | agent memory | `ns-mem` |
| `egui` | user interfaces on the desktop | `ns-lui` |
| `webui` | user interfaces in the browser | `ns-lui` |

A smaller build names what it wants:

```toml
liquers-lib = { git = "https://github.com/orest-d/liquers.git", default-features = false, features = ["records", "templates"] }
```

`polars` and `egui` bring by far the largest dependencies, so leave them out when you do not use
them. `polars` does not build for WebAssembly; `records` does, which is one reason why records
and not Polars are the table type that every Liquers platform has.

`liquers-store` has features of its own: `opendal` with one `services-…` feature for each kind of
storage. Chapter 6 lists them.

## 2.4 The library file

In Rust an environment has exactly the commands that you register. The examples of this guide
register the command library in one place, the file `src/library.rs`:

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
}

/// A builder that reads and writes `store` and has the library's commands registered.
/// Register your own commands on it, then call `build()`.
pub fn library_builder(
    store: Arc<dyn AsyncStore>,
) -> Result<EnvironmentBuilder<Value, (), LibKind>, Error> {
    let mut builder = default_environment_builder::<Value, ()>().with_async_store(store);
    register_library(&mut builder.command_registry)?;
    Ok(builder)
}

/// The same, built: ready to evaluate queries.
pub fn library_environment(
    store: Arc<dyn AsyncStore>,
) -> Result<EnvRef<CommandEnvironment>, Error> {
    library_builder(store)?.build()
}
```

Three things in it deserve a word now; chapter 8 explains the rest.

- `Value` is the type of every value that commands pass to each other. It is the one from
  `liquers-lib`, which can hold tables and images besides the basic kinds (chapter 4).
- `CommandEnvironment` is an alias for the type of the environment. The name matters:
  `register_command!` generates code that refers to a type called `CommandEnvironment`, so the
  alias must be in scope wherever you register a command.
- Each `register_…_commands!` macro registers one group of the library and exists only when its
  feature is on. Leave out the lines you do not need.
- `library_builder` gives a builder on which you register your own commands, and
  `library_environment` gives the finished environment. Almost every example calls one of them.

## 2.5 The prelude file

The second file, `src/prelude.rs`, imports the names that almost every example uses:

```rust
pub use std::sync::Arc;

pub use liquers_core::{
    context::{Context, EnvRef, Environment},
    error::{Error, ErrorType},
    metadata::{Metadata, Status},
    parse::{parse_key, parse_query},
    query::{Key, Query},
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::ValueInterface,
};
pub use liquers_lib::value::{ExtValueInterface, Value};
pub use liquers_macro::register_command;

pub use crate::library::{library_builder, library_environment, register_library, CommandEnvironment};
```

With both files in place, `src/main.rs` starts like this:

```rust
// Sketch: the top of main.rs.
mod library;
mod prelude;
use prelude::*;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let env = library_environment(Arc::new(AsyncMemoryStore::new(&Key::new())))?;
    let listing = env.evaluate("commands_doc").await?.get().await?.try_into_string()?;
    println!("{listing}");
    Ok(())
}
```

`cargo run` should print the list of the library's commands. The first build compiles all of
Liquers and takes several minutes.

## 2.6 Things to know before you start

**Progress messages.** While Liquers evaluates a query it writes diagnostic lines to the
standard error stream. They cannot be turned off yet. Standard output is never written to by the
library crates, so the output of your own program stays clean.

**One runtime.** `build()` starts the background work of the environment on the current Tokio
runtime, and fails outside one. Build the environment inside `#[tokio::main]`, or inside
`Runtime::block_on` if you manage the runtime yourself (chapter 8).

**The API reference.** This guide explains how the parts are used. Every public type and
function is described in the
[API reference](https://orest-d.github.io/liquers/overview.html#api-reference), one set of pages
for each crate.

**Version 0.1.** The interfaces described in this guide can still change between releases.
