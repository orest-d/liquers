---
id: PY-ORPHAN-MODULES
kind: design
title: liquers-py compiles every source file, and the pycall bridge is registered
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [py]
issues: [PY-MODULES-NOT-DECLARED-IN-LIB]
created: 2026-10-19
---
# liquers-py compiles every source file, and the pycall bridge is registered

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-19. Phases 1-4 are written but not
approved, and nothing is implemented.

## Phase 1: High-Level Design

### Purpose

Every file in `liquers-py/src/` should be compiled, or deleted. Three remain orphaned at HEAD
(`commands.rs`, `interpreter.rs`, `state.rs`; `value`/`context` were declared by
`foreign-value-type-registration`, `store`/`cache` deleted by `sync-store-removal`). The live
consequence is that `CommandMetadataRegistry::add_python_command`
(`liquers-py/src/command_metadata.rs`), which *is* compiled and exported, builds aliases to
`pycall`, a command defined only in the uncompiled `commands.rs` and registered nowhere.

### Problem Example

```python
reg = liquers_py.CommandMetadataRegistry()
reg.add_python_command("", "double", "mymod", "double", "pyobject", ["x"], False)
```

The resulting `double` is `CommandDefinition::Alias { command: "pycall", head_parameters:
["mymod", "double", "pyobject"] }`. In Rust, the Python `Environment`'s registry after
`set_cmr(reg)` answers `command_metadata_registry.alias_target(&double)` with
`Err(alias_target_not_registered)`, so planning any query that names `double` fails.
Should: `Environment::new()` registers `pycall` (metadata and executor), `set_cmr` keeps it, and
`alias_target(&double)` returns `pycall`'s metadata.

Verified at HEAD by reading: `commands.rs` reads the `pub(crate)` `arg.parameters.0`, uses a
`register_command!` syntax the macro rejects (liquers-py has no `liquers-macro` dependency), takes
`&mut CommandArguments` where the registry passes it by value, and has `todo!()`s, `unwrap()`s and a
`println!`. `interpreter.rs` is
imports plus commented-out code against the removed `PlanInterpreter` / `ArcEnvRef`. `state.rs` is
a 50-line `#[pyclass] State` using only current APIs. liquers-py was not built for this design.

### Scope and Acceptance Criteria

- **AC-1** No orphaned source file
  WHEN the files in `liquers-py/src/` are listed
  THEN each one except `lib.rs` is declared as a module in `lib.rs`, and `interpreter.rs` no longer
  exists
- **AC-2** The bridge is registered
  WHEN a Python `Environment` is constructed
  THEN its `command_registry` holds `pycall` (realm and namespace empty) with an executor, a state
  argument and the arguments `module`, `function`, `pass_state` (enum, default `pyobject`) and
  `argv` (`multiple`)
- **AC-3** A Python-registered command resolves
  WHEN `add_python_command` builds an alias and the registry is installed with `set_cmr`
  THEN `alias_target` on the environment's metadata registry returns `pycall`, and `pycall` is
  still registered after `set_cmr`
- **AC-4** pycall arguments convert without panicking
  WHEN the arguments after the three heads are value parameters, link parameters with a
  pre-materialised value, a `MultipleParameters` list, or `Injected("context")`
  THEN each becomes a Python object (a list's elements spliced in order); a `Placeholder`, a `None`
  parameter or another injected name is an `Error`, not a panic
- **AC-5** No panics or stdout in the ported code
  WHEN `commands.rs`, `state.rs` and `add_python_command` are read
  THEN they contain no `todo!()`, `unwrap()`, `expect(` or `println!`, and no `_ =>` arm over a
  Liquers enum
- **AC-6** The value classes reach Python
  WHEN `liquers_py` is imported
  THEN `Value`, `State`, `Environment` and `Context` are attributes of the module

Non-goals: evaluating a query from Python (the environment has no asset manager,
`PY-ENVIRONMENT-CANNOT-EVALUATE`), so `pycall` is not executed end to end here; `pycall_async` and
Python decorators (`design/python-wrapper/` milestone 1); a core API change.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — changes what the Python module exports and removes
  never-compiled `#[pyclass]` types (rule 4, Python bindings), with decisions open (rule 5).
- **Leading issue:** Proposed resolution - which parts of `commands.rs` survive.
- **Explanation:** a working port exists without touching `liquers-core` (`CommandArguments::len`,
  `get_parameter` and `get_value` are public), so Phases 3-4 are complete under the recommended
  answers below; they change the Python surface, so they stay visible.
- **Open questions:**
  1. **Proposed resolution - surviving code in `commands.rs`:** keep only the `pycall` bridge and a
     `pub fn register_bridge(&mut CommandRegistry<Environment>)`; delete the demo commands
     `hello`, `greet`, `pyprint` and the `CommandArguments` / `CommandRegistry` pyclasses. They have
     never compiled, so no user depends on them; the environment owns the registry. Alternative:
     port them too (needs a `liquers-macro` dependency or manual registration, and `pyprint`
     writes to stdout by design).
  2. **Proposed resolution - `interpreter.rs`:** delete it. Evaluation from Python belongs to
     `PY-ENVIRONMENT-CANNOT-EVALUATE`, which will be written against `EnvRef`, not this file.
  3. **Proposed resolution - `set_cmr` semantics:** replace the metadata registry, then
     re-register the bridge (so a registry built in Python without `pycall` still works).
     Alternative: merge the given commands into the existing registry, which changes what
     "set" means for Python callers.
  4. **Proposed resolution - module exports:** add `Value`, `State`, `Environment`, `Context` to
     `#[pymodule]` (AC-6). Today none is importable although `Value` is declared.

### Design Dependencies

- `python-wrapper` (complete) — **requires** (frozen, E4): the `CommandMetadataRegistry`-first
  bridge model (its Phase 2 §6) this design implements the first half of.
- `py-value-serializer` (in_review) — **overlaps**: same crate, different file; no order.
- `core-tracing-migration` (in_review) — **overlaps** (T3 on one line): its step 6 drops
  `println!("pycall")` and adds a subscriber call in `#[pymodule]`. Whichever lands second
  rebases; if this lands first, that step's `println!` half is already done.
- `PY-ENVIRONMENT-CANNOT-EVALUATE` (issue, filed by this design) — **required-by**: an
  evaluating Python environment is what will run `pycall` end to end.

### Consolidated Findings

- No core change is needed. The issue expected a public way to iterate parameters; `len()` and
  `get_parameter(i, name)` already give it, and `get_value(i, name)` returns a link's
  pre-materialised value.
- `register_command!` cannot be used as written; manual `register_command("pycall", pycall)`
  (already used for `pycall`) avoids a new `liquers-macro` dependency.
- `add_python_command` emits aliases whose arguments do not line up with `pycall`'s
  (`COMMAND_ALIASES.md` §2): N plain arguments against one `multiple` `argv`. Planning accepts a
  deserialized alias without the shape check, and `pycall` reads positionally from index 3, so it
  works; it is recorded here, not changed.
- `pycall` cannot be executed in a test: `Context::new` needs an `AssetRef`, which needs an asset
  manager the Python environment lacks. The argument conversion is therefore a separate function
  tested on its own (AC-4).

## Phase 2: Architecture

### Solution

1. **`lib.rs`** — `pub mod commands; pub mod state;`; delete `interpreter.rs`; add
   `m.add_class::<value::Value>()`, `state::State`, `context::Environment`, `context::Context`;
   replace the orphan comment.
2. **`commands.rs`** — rewrite around the bridge:

```rust
pub const PYCALL: &str = "pycall";
pub fn register_bridge(cr: &mut liquers_core::commands::CommandRegistry<Environment>)
    -> Result<(), liquers_core::error::Error>;
fn pycall(state: &State<Value>, arg: CommandArguments<Environment>, context: Context<Environment>)
    -> Result<Value, liquers_core::error::Error>;
fn arguments_to_py(py: Python, arg: &CommandArguments<Environment>, from: usize,
    context: &dyn Fn(Python) -> PyObject) -> Result<Vec<PyObject>, liquers_core::error::Error>;
```

`register_bridge` holds the existing manual registration (`with_state_argument`, `module`,
`function`, `pass_state` enum, `argv` `set_multiple`). `arguments_to_py` loops `from..arg.len()`
over `get_parameter(i, "argv")` with an exhaustive match: `MultipleParameters` → each element's
`value()` through `Value::try_from_json_value` and `value_to_pyobject`; `Injected("context")` →
the closure; other `Injected`, `Placeholder`, `None` → `Error::general_error`; every value and link
variant → `get_value(i, "argv")` then `value_to_pyobject`. `json2py` and `parameter2py` are
deleted. `pycall` uses `PyModule::import_bound`, logs through `context.info`, and maps `PyErr`
with `Error::general_error`. The closure keeps `arguments_to_py` testable without a `Context`.
3. **`context.rs`** — `Environment::new()` returns `PyResult<Self>` and calls `register_bridge`;
   `set_cmr` returns `PyResult<()>` and calls it after replacing the registry.
4. **`command_metadata.rs`** — split `add_python_command` into a GIL-free
   `fn python_command_metadata(namespace, name, module, function, pass_state, arg_names: &[String],
   multiple) -> CommandMetadata` (alias target `commands::PYCALL`) and the PyO3 wrapper, whose
   three `unwrap()`s become `?`.
5. **`state.rs`** — unchanged apart from unused imports.

Rejected: a core `CommandArguments::parameters()` (unnecessary, crosses crates); keeping the old
pyclasses (no caller); making `add_python_command` fail (the issue's stop-gap).

Known-issue preflight: `PY-ENVIRONMENT-CANNOT-EVALUATE` limits testing, does not block;
`PY-VALUE-ENUM-OVERSIZED` (`py-value-arc-payloads`) does not touch these files'
logic; nothing blocks. No Liquers command namespace is involved: `pycall` is registered only in the
Python environment, not in `liquers-lib`, so `specs/command_registry.yaml` does not change.

### Changes

`liquers-py/src/lib.rs`, `commands.rs` (rewrite), `context.rs` (`Environment::new`, `set_cmr`),
`command_metadata.rs` (`add_python_command` split), `state.rs` (imports), `interpreter.rs`
(deleted). Python surface: `Environment()` and `env.cmr = …` may now raise; `Value`, `State`,
`Environment`, `Context` importable. Documents: `liquers-py/README.md` (a short "Python commands"
note), `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` line 25 ("an experimental `pycall`") gains a
History row and `reviewed:` bump if its wording changes.

### Risks

| Risk | Note | Certainty |
|---|---|---|
| Files | five `liquers-py/src` files plus one deletion | high |
| Further drift in the orphans | compiler-guided, one module per step (step order below) | medium |
| PyO3 0.21 deprecations | use the `_bound` APIs | high |
| `Environment::new` signature (`PyResult`) | only commented-out Rust callers (`context.rs`) | high |
| Interpreter in tests | `pyorph04/05` call `pyo3::prepare_freethreaded_python()` in a `--no-default-features` build (existing tests are GIL-free); if that fails, they move to pytest under maturin | medium |
| Python tests | `liquers-py/tests/*.py` do not use these classes; unaffected | high |

Recovery: one commit per step; reverting step 3 re-orphans `commands.rs` without breaking the build.

## Phase 3: Examples and Tests

### Examples

The Problem Example is `pyorph03`. Python, after the change:

```python
env = liquers_py.Environment()           # registers pycall
env.cmr = reg                            # pycall re-registered; `double` resolves
```

### Tests

Rust unit tests (`cargo test -p liquers-py --lib --no-default-features --features async_store`):

- `lib.rs` `pyorph01_every_source_file_is_a_module` — reads `CARGO_MANIFEST_DIR/src`, checks each
  stem against `include_str!("lib.rs")`'s `mod` lines, and that `interpreter.rs` is absent — AC-1
- `commands.rs` `pyorph02_bridge_is_registered` — `Environment::new()`; `find_command("", "",
  "pycall")` with the four arguments, `argv.multiple`, the enum default — AC-2
- `command_metadata.rs` `pyorph03_python_command_alias_resolves` — `python_command_metadata(...)`
  into a fresh `CommandMetadataRegistry`, installed via the Rust half of `set_cmr`; `alias_target`
  is `Ok(Some(pycall))`, and `pycall` is present afterwards — AC-3
- `commands.rs` `pyorph04_arguments_convert_to_python` — interpreter initialized; a
  `CommandArguments` built from `ResolvedParameterValues` with heads, a value, a link plus
  `set_value`, a `MultipleParameters` and `Injected("context")` (closure returns a sentinel);
  objects in order — AC-4
- `commands.rs` `pyorph05_unsupported_parameters_are_errors` — `Placeholder`, `None`,
  `Injected("session")` each give `Err` — AC-4
- AC-5: review item in step 6 — `grep -nE "todo!|unwrap\(|expect\(|println!" liquers-py/src/commands.rs
  liquers-py/src/state.rs` and the `add_python_command` body — AC-5
- `liquers-py/tests/test_module_exports.py` `test_value_classes_are_exported` (pytest, maturin) —
  AC-6, plus `test_environment_registers_pycall` (AC-2, AC-3 from Python)

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Decisions 1-4 confirmed or changed by the maintainer; update this design — `python3 .claude/skills/liquers-project/scripts/validate_phase.py py-orphan-modules 4`.
- [ ] 2. `lib.rs` — `pub mod state;`, delete `interpreter.rs`, add `pyorph01` (fails: `commands`) — `cargo test -p liquers-py --lib --no-default-features --features async_store pyorph01`. Rollback: revert.
- [ ] 3. `commands.rs` rewrite + `pub mod commands;` — `register_bridge`, `pycall`, `arguments_to_py`; add `pyorph04`, `pyorph05` — `cargo test … pyorph0`. Rollback: revert (re-orphans the file).
- [ ] 4. `context.rs` `Environment::new`, `set_cmr` — call `register_bridge`; add `pyorph02` — `cargo test … pyorph02`.
- [ ] 5. `command_metadata.rs` — `python_command_metadata` split, `unwrap()` → `?`; add `pyorph03` — `cargo test … pyorph03`.
- [ ] 6. `lib.rs` `#[pymodule]` — four `add_class` calls; AC-5 grep — `cargo check -p liquers-py --lib` (default features).
- [ ] 7. `liquers-py/tests/test_module_exports.py`; `liquers-py/README.md` note — `maturin develop && pytest liquers-py/tests` (if maturin is unavailable, say so in the PR).
- [ ] 8. Records — close `PY-MODULES-NOT-DECLARED-IN-LIB` (resolution note); guide line 25 if reworded (History row, `reviewed:`) — `python3 scripts/docs_index.py && python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-py --lib --no-default-features --features async_store`; `cargo check -p
liquers-py --lib` (prefix `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1` on CPython 3.13+); pytest under
maturin; diff review: only `liquers-py/`, the issue, the guide and the index; no `println!`, no
`liquers-core` change, `specs/command_registry.yaml` untouched.
