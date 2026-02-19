---
name: liquers-unittest
description: Create comprehensive unit tests and integration tests for the Liquers Rust project. Use this skill when writing tests for any liquers crate (liquers-core, liquers-macro, liquers-store, liquers-lib, liquers-axum), including tests for commands, stores, query parsing, asset lifecycle, state management, error handling, and the register_command! macro. Triggers on requests like "write tests for", "add unit tests", "test this module", "create test coverage", or any test-writing task in the liquers codebase.
---

# Liquers Unit Test Creator

Write unit tests and integration tests for the Liquers Rust project following established codebase conventions.

## Test Writing Workflow

1. **Read the target code** to understand what needs testing
2. **Determine test type**: unit (inline) vs integration (tests/ directory)
3. **Select the right environment** based on what's being tested
4. **Write tests** following project conventions (see references)
5. **Run tests** with `cargo test -p <crate-name>` to verify

## Mandatory Conventions

- **No `unwrap()` / `expect()` in library code** — only in tests
- **No `_ =>` default match arms** — match all enum variants explicitly
- **Typed error constructors only**: `Error::key_not_found()`, `Error::general_error()`, `Error::conversion_error()` — never `Error::new()`
- **Async-first**: `#[tokio::test]` for async, `#[test]` for sync
- **Test return type**: `-> Result<(), Box<dyn std::error::Error>>` for tests using `?`
- **Test module**: `#[cfg(test)] mod tests { use super::*; }` at end of file
- **`type CommandEnvironment`**: required alias before any `register_command!` calls

## Test Type Decision

**Unit test (inline)**: private functions, single type methods, no environment needed

**Integration test (tests/)**: end-to-end evaluation, command registration + execution, cross-crate features

## Environment Selection

| Scenario | Environment |
|----------|------------|
| Basic commands | `SimpleEnvironment<Value>` |
| Payload injection | `SimpleEnvironmentWithPayload<Value, P>` |
| Full features (polars/image) | `DefaultEnvironment<Value>` |
| Full features + payload | `DefaultEnvironment<Value, P>` |

## Reference Files

- **[references/test-patterns.md](references/test-patterns.md)**: Complete test templates for every scenario — sync/async commands, stores, plans, errors, payloads, metadata verification. **Read this first when writing tests.**
- **[references/testable-components.md](references/testable-components.md)**: What to test for each component, edge cases, error conditions, and coverage guidance. **Read this when deciding what tests to write.**

## Common Imports

```rust
use liquers_core::{
    parse::{parse_key, parse_query},
    command_metadata::{CommandKey, CommandMetadata, ArgumentInfo},
    commands::{CommandArguments, CommandRegistry, ResolvedParameterValues},
    context::{SimpleEnvironment, Environment, EnvRef},
    error::{Error, ErrorType},
    state::State,
    store::{MemoryStore, Store},
    value::Value,
    metadata::{Metadata, MetadataRecord},
    query::{Key, Query, TryToQuery},
};
use liquers_macro::register_command;
use std::sync::Arc;
```

## Running Tests

```bash
cargo test -p liquers-core              # All tests in a crate
cargo test -p liquers-core test_name    # Specific test
cargo test -p liquers-core -- --nocapture  # With stdout
```
