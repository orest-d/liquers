# Liquers Development Guide

## Project Structure

```
liquers-core/     # Core abstractions (Query, Key, Store, Assets, Commands)
liquers-macro/    # register_command! function-like proc-macro
liquers-store/    # Storage backends (OpenDAL integration, config)
liquers-lib/      # Command library, Rich value types (Polars DataFrames, egui UI, images)
liquers-axum/     # HTTP REST API server
liquers-py/       # Python bindings (PyO3)
specs/            # Specifications and design documents
```

**Dependency flow**: `liquers-core` ← `liquers-macro` ← `liquers-store` ← `liquers-lib` ← `liquers-axum`

**Key specs**: See `specs/PROJECT_OVERVIEW.md` for architecture, `specs/REGISTER_COMMAND_FSD.md` for macro details, `specs/ASSETS.md` for asset lifecycle.

**Known issues** are tracked in `specs/ISSUES.md`.

## Architecture Rules

### Where Code Goes
- Query language, parsing, plans: `liquers-core/src/query.rs`, `liquers-core/src/parse.rs`, `liquers-core/src/plan.rs`
- Storage traits and implementations: `liquers-core/src/store.rs`, `liquers-store/src/`
- Command execution framework: `liquers-core/src/commands.rs`, `liquers-core/src/command_metadata.rs`
- Asset lifecycle: `liquers-core/src/assets.rs`
- New value types (DataFrames, images): `liquers-lib/src/value/`
- New storage backends: `liquers-store/src/`
- New commands: `liquers-lib/src/commands.rs`
- Polars DataFrame operations: `liquers-lib/src/polars/` (see `specs/POLARS_COMMAND_LIBRARY.md`)

### Key Types
- `Query`, `Key`, `ActionRequest` - query DSL (`liquers-core/src/query.rs`)
- `Value` (layer 1) → `State<V>` (layer 2) → `Asset` (layer 3) - value encapsulation
- `Environment` - global services (store, assets, commands)
- `Context` - per-command execution context
- `Error` with `ErrorType` - all errors use `liquers_core::error::Error`

## Code Conventions

### Match Statements

Match statements of enums should be explicit; avoid the default match arm (`_ =>`).
This ensures future changes (new `Status` variants, `Step` types, channel messages) trigger compile errors.

### Error Handling
```rust
use liquers_core::error::{Error, ErrorType};

// DO: Use typed error constructors
Error::key_not_found(&key)
Error::general_error("message".to_string())
Error::from_error(ErrorType::General, external_error)

// DON'T: Use Error::new directly
// Error::new(ErrorType::ParseError, "...")  // Avoid this
```

### Diagnostic Output

Library code must never write to **stdout**. Use `eprintln!`, never `println!`:

```rust
// DO: diagnostics go to stderr
eprintln!("Recipe already has CWD set to {:?}", recipe.cwd);

// DON'T: this corrupts machine-readable stdout
// println!("Recipe already has CWD set to {:?}", recipe.cwd);
```

Stdout is reserved for a binary's *intended* output — CLI tools serialize JSON/YAML there, and a
stray `println!` anywhere in the libraries they link makes that output unparseable. The rule is
blanket (it applies inside `#[cfg(test)]` modules too) so nobody has to reason about whether a
given line sits in a code path some future binary will call. Only `[[bin]]` targets, examples and
doc-comment examples print to stdout.

### Async Patterns
- Default to async (`AsyncStore`, `AsyncStoreRouter`)
- Use `#[async_trait]` for async trait methods
- Tokio runtime with `sync`, `rt`, `macros`, `time` features
- Sync wrappers (`AsyncStoreWrapper`) only for Python compatibility

### Naming
- Traits: `ValueInterface`, `ExtValueInterface`, `AsyncStore`, `CommandExecutor`
- Async variants: prefix with `Async` (e.g., `AsyncStoreRouter`)
- Builders: `StoreRouterBuilder`, `PlanBuilder`
- Test modules: `#[cfg(test)] mod tests { ... }` at end of file

### Serialization
- Use `serde` with `Serialize, Deserialize` derives
- First-class support for JSON, YAML, and TOML
- Environment variables: `${VAR_NAME}` syntax in configs

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_functionality() { ... }

    #[tokio::test]
    async fn test_async_functionality() { ... }
}
```

- Unit tests in same file, integration tests in `tests/`
- Use `parse_key()`, `parse_query()` helpers for test setup
- Memory stores for testing: `MemoryStore::new(&Key::new())`, wrapped via `AsyncStoreWrapper`
- See `liquers-core/tests/async_hellow_world.rs` for full flow: Environment with memory store, RecipeProvider, command registration, query evaluation

## Building and testing

Rust debug builds of this workspace are large — large enough that a full build does not fit in a
cloud dev environment (Claude Code on the web caps sessions at **30 GB of disk**, and `df` reports
the allowance, not the machine). The settings below are already applied; this section records what
they are worth so nobody removes them by accident.

### Default test command

```bash
cargo test -p liquers-lib --lib --tests     # the normal loop: unit + integration, no examples
```

`liquers-lib` is where most work lands and it transitively builds `liquers-core`, `liquers-macro`
and `liquers-store`. Run the browser tests **separately, after `cargo clean`** — they build a
different target and their own crate:

```bash
cargo clean
cd liquers-lib/examples-web/ui_spec_demo && trunk build && npx playwright test
```

Avoid `cargo test --workspace` in a constrained environment: it also builds the examples and every
crate's test binaries at once, which is what exhausts the allowance.

### Applied measures, in order of effect

Measured on a cold `cargo test -p liquers-lib --lib --tests` (`cargo clean` before each,
`CARGO_INCREMENTAL=0`, all 14 suites passing in every configuration):

| Configuration | `target/` | Wall time |
|---|---|---|
| Full debug info + vendored OpenSSL (former default) | 22.5 GB | 6m17s |
| …system OpenSSL | 22.1 GB | 4m48s |
| **…+ thin debug info (current settings)** | **4.2 GB** | **3m03s** |
| …but `[profile.test]` inheriting line tables | 9.4 GB | 3m30s |
| …`--release` instead | 2.5 GB | 12m05s |

1. **Thin debug info** — `[profile.dev] debug = "line-tables-only"` and `[profile.test] debug = 0`
   in the workspace `Cargo.toml`. **81% smaller and 36% faster**, and it costs less than it sounds:
   a failing assertion still reports its own `file:line`, because that comes from `#[track_caller]`
   rather than debug info. What is lost is line numbers in `RUST_BACKTRACE` frames and full
   debugger support — recover either temporarily with `RUSTFLAGS="-Cdebuginfo=2" cargo test …`.
2. **System OpenSSL** — `liquers-lib`'s `openssl` dependency no longer uses `features =
   ["vendored"]`, so it links the system library instead of compiling OpenSSL from source. Small on
   disk, but ~90 s off every cold build. Requires OpenSSL development headers (`libssl-dev`);
   re-add `vendored` if you build where they are absent.
3. **`CARGO_INCREMENTAL=0`** for one-shot/CI-style runs. Incremental caches reached ~2 GB here and
   buy nothing when the build is not repeated.

### Not recommended

- **`--release` for the routine test loop.** Smallest on disk, but 4× slower to build, and it
  changes what the tests exercise: `debug_assertions` and integer-overflow checks are off, so
  `debug_assert!` never runs. Use it deliberately (a performance check), not as the default.
- **Raising the ceiling.** There is no setting for it; cloud sessions are fixed at ~16 GB RAM and
  30 GB disk. For genuinely bigger workloads the documented route is Remote Control (Claude Code
  against your own hardware).

The table's figures are **clean-build** sizes. Changing a profile setting does not invalidate
artifacts built under the previous one, so a `target/` that has seen several configurations grows
well past them — run `cargo clean` after editing a profile if the size matters.

### When a build fails with "No space left on device"

Deletes still succeed while writes fail, so recovery is local: `cargo clean` (or delete
`target/debug/incremental` and `target/debug/examples` first, which is usually enough). Check with
`df -h /` — "Avail" near zero with a low "Used" means the allowance is spent, not that the machine
is broken.

## Constraints

### Do NOT
- Use `unwrap()` or `expect()` in library code (only in tests)
- Use `println!` in library code (use `eprintln!` — stdout belongs to the binary's output)
- Create new error types outside `liquers_core::error`
- Use `Error::new` directly
- Use blocking I/O in async contexts
- Add sync Store implementations (async only, sync via wrapper)
- Modify Query/Key encoding without updating `specs/PROJECT_OVERVIEW.md`

### Performance-Sensitive Areas
- Query parsing (`liquers-core/src/parse.rs`) - used on every request
- Key encoding/decoding (`liquers-core/src/query.rs`) - frequent operations
- Asset lookups in `AssetManager` - use `scc` concurrent map

## Modifying Existing Code

### Before Changing APIs
1. Check if type is used in `liquers-py` (Python bindings break easily)
2. Check `register_command!` macro usage in `liquers-lib`
3. Update `specs/PROJECT_OVERVIEW.md` if core concepts change

### Refactoring Guidelines
- Prefer extending traits over modifying them
- Add new methods with default implementations when possible
- Keep `liquers-core` minimal; rich features go in `liquers-lib`

## Common Tasks

### Adding a Command

The `register_command!` macro is a **function-like macro** (not an attribute macro) with a custom DSL.
The actual function must be defined SEPARATELY, then registered via the macro.

**See `specs/COMMAND_REGISTRATION_GUIDE.md` for comprehensive guidelines** covering:
- Using the `register_command!` macro (recommended)
- Manual registration (fine-grained control)
- Generic Environment commands (library reusability)
- Best practices and examples

For macro syntax details, see `specs/REGISTER_COMMAND_FSD.md`.

```rust
use liquers_macro::register_command;
use liquers_core::{state::State, error::Error, context::Context};

// 1. Define the function separately
fn greet(state: &State<Value>, greeting: String) -> Result<Value, Error> {
    let input = state.try_into_string()?;
    Ok(Value::from(format!("{}, {}!", greeting, input)))
}

// 2. Register using the macro DSL
let cr = env.get_mut_command_registry();
register_command!(cr, fn greet(state, greeting: String) -> result)?;

// Async command with default value
register_command!(cr, async fn async_greet(state, greeting: String = "Hello") -> result)?;

// With metadata
register_command!(cr,
    fn to_text(state, context) -> result
    label: "To text"
    doc: "Convert input state to string"
    filename: "text.txt"
)?;
```

**DSL Syntax Reference**:
- State parameter (first): `state`, `value`, `text`, or omit entirely
- `context` - special parameter for execution context
- Parameters: `name: Type`, optionally `injected`, optionally `= default_value`
- Default value types: string `"foo"`, bool `true`, int `42`, float `3.14`, query `query "path/to/query"`
- Return: `-> result` (returns `Result<V, Error>`) or `-> value` (returns `V`)
- Metadata: `label:`, `doc:`, `namespace:`, `realm:`, `preset:`, `next:`, `filename:`, `volatile:`

See examples in `liquers-lib/src/commands.rs` and `liquers-core/tests/async_hellow_world.rs`.

### Adding a Store Backend
1. Implement `AsyncStore` trait in `liquers-store/src/`
2. Add config support in `liquers-store/src/config.rs` and `liquers-store/src/store_builder.rs`
3. Update `OPENDAL_STORE_TYPES` in `liquers-store/src/config.rs` if OpenDAL-based
4. See `specs/STORE_CONFIG_FSD.md` for configuration format

### Adding a Value Type
1. Extend `ExtValue` enum in `liquers-lib/src/value/mod.rs`
2. Implement conversions in `ExtValueInterface` trait
3. Add serialization support
