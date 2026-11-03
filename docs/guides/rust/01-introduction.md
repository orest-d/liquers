---
title: Introduction
status: outline
sources: [reference/PROJECT_OVERVIEW.md]
reviewed:
---
# 1. Introduction

PURPOSE: The reader has run a first query from Rust and knows the words query, command, store and asset.

```rust
use liquers_core::{
    context::{Environment, SimpleEnvironment},
    error::Error,
    interpreter::evaluate,
    state::State,
    value::Value,
};
use liquers_macro::*;

type CommandEnvironment = SimpleEnvironment<Value>;
let mut env = SimpleEnvironment::<Value>::new();

fn world(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("world"))
}
async fn greet(state: State<Value>, greet: String) -> Result<Value, Error> {
    let what = state.try_into_string()?;
    Ok(Value::from(format!("{greet}, {what}!")))
}
let cr = &mut env.command_registry;
register_command!(cr, fn world(state) -> result)?;
register_command!(cr, async fn greet(state, greet: String = "Hello") -> result)?;

let state = evaluate(env.to_ref(), "world/greet", None).await?;
assert_eq!(state.try_into_string()?, "Hello, world!");
```

EDITOR: copied from `liquers-core/tests/async_hellow_world.rs` (test_async_hello_world), which runs it; keep the two identical.

TBD: 1.1 What Liquers is: queries name computations; commands do the work; stores hold data; assets cache results.
SOURCE: reference/PROJECT_OVERVIEW.md
TBD: 1.2 Hello, world: the example below, explained line by line: registering two commands and evaluating `world/greet`.
TBD: 1.3 What happens when a query runs: parse, plan, execute, cache; one paragraph and a diagram.
SOURCE: reference/PROJECT_OVERVIEW.md
TBD: 1.4 Next steps: which chapter to read for which task.
