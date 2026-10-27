---
id: LANGUAGE-GLOBAL-ENUM-IMPORT
kind: design
title: JavaScript and Python register the global enums their commands refer to
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [web, py, core/commands]
issues: [GLOBAL-ENUMS-NOT-IMPORTED-BY-LANGUAGE-INTEGRATIONS]
created: 2026-10-27
---
# JavaScript and Python register the global enums their commands refer to

Bulk design, 2026-10-27: Phases 1-4 written, not approved, nothing implemented.

## Phase 1: High-Level Design

### Purpose

A command declared from JavaScript or Python may name a global enum
(`argument_type: {GlobalEnum: colours}`), but neither integration can put the enum into the
registry, so the argument never plans. Both integrations should take global enums from a registry
document and from one dedicated call, through the core method `CommandMetadataRegistry::add_global_enum`
that `global-enum-registration` adds, and keep them across rebuilds like their commands.

### Problem Example

Verified at HEAD (`04667a80`):

- `liquers-web/src/command/import.rs` `ImportedRegistry` deserializes `global_enums` as
  `HashMap<String, serde_json::Value>`, and `read_registry` only warns "The command metadata
  defines global enums, which are not imported".
- `liquers-py/src/environment.rs` `ImportedMetadata` has only `commands`; `Declarations` retains
  no enum; there is no method for one.
- `liquers-core/src/command_metadata.rs` `CommandMetadataRegistry::global_enums` is a `pub` map
  with no adder (`// TODO: support global enums`). `EnumArgument` needs `name`, `values` (`alias`,
  `value`), `others_allowed` and `value_type`.

```javascript
liquers.registerCommands(`
global_enums:
  colours: { name: colours, values: [{alias: red, value: red}], others_allowed: false, value_type: string }
commands:
  - { name: paint, arguments: [{ name: colour, argument_type: { GlobalEnum: colours } }] }
`, { paint: (colour) => `painted ${colour}` });   // today: warns "...not imported..."
await liquers.evaluate("paint-red");             // today: rejects "Global enum colours not defined"
```

It should register `colours`, report no warning, and resolve to `"painted red"`. The same holds for
`env.import_metadata(...)` in Python. The query was checked with `liquers-validate --command paint`.

### Scope and Acceptance Criteria

- **AC-1** A JavaScript registry document brings its enums
  WHEN `registerCommands` gets a document whose `global_enums` a command refers to
  THEN each enum is registered, the "not imported" warning is gone, and `paint-red` evaluates
- **AC-2** A Python document brings its enums
  WHEN `import_metadata` gets a document with `global_enums`
  THEN each enum is registered at the next build, and a command referring to it evaluates
- **AC-3** One enum by itself
  WHEN `registerGlobalEnum(spec)` (JavaScript) or `Environment.add_global_enum(spec)` (Python) is
  called with an object or a YAML / JSON string in the `EnumArgument` form
  THEN the enum is registered, replacing one with the same name
- **AC-4** An invalid enum is refused at the call
  WHEN the enum has an empty name or a repeated alias
  THEN the call fails (`parameter_error` in JavaScript, `ConfigurationError` in Python) and
  nothing is retained
- **AC-5** Enums survive a rebuild
  WHEN commands are registered after the first evaluation, so the environment is rebuilt
  THEN every enum registered before still resolves
- **AC-6** Validation sees every registered enum
  WHEN a batch, or a later `validateCommands` / `validate()`, has a command referring to an enum
  registered earlier
  THEN no dangling-reference issue is reported; one that exists nowhere is the core design's error
  naming the command, the argument and the enum

Non-goals: computed alternatives from a language callable (`computed-enum-alternatives`); a
decorator keyword naming a global enum (`PY-COMMAND-DECORATOR-CANNOT-DECLARE-ARGUMENT-METADATA`);
enums on an explicit JavaScript `Environment` instance, which cannot register commands either.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. It adds a JavaScript function and a Python method (rule 4),
  spans two crates (rule 6), and readiness is `needs-decision` (rule 5).
- **Leading issue:** **Open design - the dedicated call.**
- **Explanation:** both integrations already parse the document and replay retained declarations
  into every new environment; an enum is one more retained declaration. Every check is the core
  design's, so the bindings add no validation rules of their own.
- **Open questions:**
  1. **Open design - the dedicated call.** Recommended: `liquers.registerGlobalEnum(spec)`
     (module level, like `registerCommands`) and `Environment.add_global_enum(spec)`, each taking
     the serialized `EnumArgument`. The alternative is documents only, but `registerCommands`
     requires `commands` so that a misspelled key is an error, and that should stay.
  2. **Proposed resolution - error classes.** JavaScript refuses with `parameter_error`, as
     `registerCommands` does for an invalid declaration; Python raises `ConfigurationError`, as
     `import_metadata` does for an invalid document. Both raise at the call, by trying the enum on
     a scratch `CommandMetadataRegistry`, because `EnumArgument::definition_issues` is crate-private.
  3. **Proposed resolution - map key differs from `name`.** Registered under `name`, with a
     warning in the report (web) or `validate()` (Python), as core `merge_str` does.

### Design Dependencies

- `global-enum-registration` (in review, `needs-decision`): **requires**. `add_global_enum` and the
  dangling-reference check come from it. This design assumes its questions 1-3 as recommended: a
  dangling reference is an `Error` at build, re-adding a name replaces, versions follow content.
- `language-late-registration-without-rebuild` (this run): **overlaps, T3 holds**. That design
  replaces the commands' use of `rebuild_with` / `rebuild_without` with `update_registry`, which
  Step 2 here also edits, and both edit Python's `configured_builder` path. Whichever lands second
  carries the enum replay into the other's replay function. Not merged: each is `M`, and they wait
  on different `needs-decision` core designs, so a merged design would be blocked by both.
- `registry-publication` (in review): **overlaps**. Its export inlines global enums, so an
  `importRealm` document holds none.
- `PY-COMMAND-DECORATOR-CANNOT-DECLARE-ARGUMENT-METADATA` (issue): **overlaps**. Once a decorator
  can declare `argument_type`, `add_global_enum` is how its enum gets in. Either order.

### Consolidated Findings

- Under the core design a dangling reference fails the build. `validate_batch` checks a registry
  holding only the batch, so it would refuse a command whose enum came in an earlier batch;
  it must be seeded (AC-6).
- Order matters for users: register the enum before, or in the same document as, the commands
  that use it. Python is lenient: its build runs at the next use.
- The issue's example writes alternatives as `{name: red}`; the field is `alias`, and
  `others_allowed` and `value_type` have no serde default. The examples here use the real form.

## Phase 2: Architecture

### Solution

**JavaScript** (`liquers-web`). `ImportedRegistry::global_enums` becomes
`HashMap<String, EnumArgument>`. `read_batch` returns the batch's enums beside its commands
(a `Batch { commands, global_enums }`, crate-private). A new thread-local
`REGISTERED_GLOBAL_ENUMS: RefCell<Vec<EnumArgument>>`, beside `REGISTERED_SPECS`, holds every
accepted enum, one per name. `validate_batch` builds its scratch registry from that list, then
the batch's enums, then the commands.
`register_commands_on` adds the enums before the commands, directly while the environment is
pending and through `rebuild_with` otherwise. `rebuild_with` and `rebuild_without` apply
`REGISTERED_GLOBAL_ENUMS` before replaying specs. `register_global_enum_on(spec)` follows the same
path for one enum. `rebuild_with` gains `enums: Vec<EnumArgument>`. Seeding from the list rather than an environment
keeps `validateCommands` pure and usable before `init()`.

**Python** (`liquers-py`). `ImportedMetadata` gains `#[serde(default)] global_enums:
HashMap<String, EnumArgument>`. `Declarations` gains `global_enums: Vec<EnumArgument>`.
`EnvCell::import_metadata` takes the enums too and, like `EnvCell::add_global_enum(e)`, checks each
on a scratch registry first, raising `ConfigurationError` and retaining nothing on failure; then it
replaces by name and calls `retire`. `configured_builder` calls `add_global_enum` for each before
`apply_imported`; only checked enums reach it, and an `Err` there anyway becomes an error-level
issue in its `IssueReport`, so `validate()` still returns a report and a build still raises
`ConfigurationError`.

Rejected: inserting into the `pub` map directly (skips the core checks); resolving enums into each
command at import (it freezes the enum, so a later replacement has no effect); a separate
enum-only document format.

### Changes

```rust
// liquers-web/src/lib.rs
#[wasm_bindgen(js_name = registerGlobalEnum)]
pub fn register_global_enum(spec: typescript::JsGlobalEnumSpec) -> Result<(), JsValue>;
// liquers-web/src/environment.rs
pub fn register_global_enum_on(spec: &JsValue) -> Result<(), Error>;   // ParameterError on refusal
```

```rust
// liquers-py/src/environment.rs, #[pymethods] impl Environment
fn add_global_enum(&self, py: Python<'_>, spec: &Bound<'_, PyAny>) -> PyResult<()>;  // dict or str
```

- `liquers-web/src/typescript.rs`: `JsGlobalEnumSpec` (`name`, `values: {alias, value}[]`,
  `others_allowed`, `value_type`, or a string).
- `liquers-web/src/command/import.rs` `ImportedRegistry`, `read_batch`, `read_registry` (warning
  removed); `environment.rs` as above.
- `liquers-py/src/environment.rs` `ImportedMetadata`, `Declarations`, `EnvCell::{import_metadata,
  add_global_enum, configured_builder}`, `Environment::add_global_enum`.
- Sync only. No command; `specs/command_registry.yaml` unchanged.
- Documents (History row, `reviewed:` bump): `guides/JAVASCRIPT_GUIDE.md` (commands section),
  `guides/PYTHON_GUIDE.md` §5, `reference/PYTHON_INTEGRATION.md` §3 methods table,
  `reference/COMMAND_DECLARATION.md` global-enum note; `liquers-web/README.md` API list. Proposed
  `affects_docs`: the four under `specs/`.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A batch that warned today is refused once the core check exists (its enum was never there) | That command could never plan; the report names it | high |
| A retained enum outlives the commands that used it | Harmless: an unused enum resolves nothing | high |
| Stub check fails on the new declaration | `check-stubs.sh --build` in Step 3 | high |
| The core adder's error text changes | Tests match the error type, not the text | medium |

Recovery: revert the two crates. No existing test asserts the "not imported" warning.

## Phase 3: Examples and Tests

### Examples

```python
env.import_metadata("""
global_enums:
  colours: {name: colours, values: [{alias: red, value: red}], others_allowed: false, value_type: string}
commands:
  - {name: paint, arguments: [{name: colour, argument_type: {GlobalEnum: colours}}]}
""")
env.evaluate("paint-red")         # "painted red", with a Python command `paint` registered
```

### Tests

JavaScript, `liquers-web/tests/environment_ENVIRON.rs`
(`cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`):

- `environ_registry_global_enums_are_imported`: the Problem Example; no warning; `paint-red`
  gives `"painted red"` (AC-1)
- `environ_register_global_enum_replaces_by_name`: object, YAML and JSON string forms; a second
  call changes the value `paint-red` resolves to (AC-3)
- `environ08_invalid_global_enum_is_refused`: a repeated alias → `parameter_error`; a later
  rebuild does not bring it back (AC-4)
- `environ_global_enums_survive_a_rebuild`: register, evaluate, register another command,
  evaluate `paint-red` again (AC-5)
- `environ08_later_batch_sees_earlier_enum`: the enum in one call, the command in the next;
  `validateCommands`, also before `init()`, reports nothing, and an unknown enum is an error
  naming it (AC-6)

`ENVIRON08` (an invalid import's report) covers only AC-4 and AC-6; the others are unnumbered, as
`web_register_after_sharing_rebuilds` is.

Unit, `liquers-web/src/command/import.rs` `tests`: `registry_global_enums_are_read_typed` (Step 1).

Python, `liquers-py/tests/test_ENVIRON_environment.py` (`bash scripts/test-liquers-py.sh`):

- `test_ENVIRON_imported_global_enums_resolve` (AC-2)
- `test_ENVIRON_add_global_enum_from_dict_yaml_and_json` (AC-3)
- `test_ENVIRON08_invalid_global_enum_raises_configuration_error`: through `add_global_enum` and
  through `import_metadata`; afterwards `validate()` returns a report without it and `paint-red`
  still fails only as unknown enum, so nothing was retained (AC-4)
- `test_ENVIRON_global_enums_survive_a_rebuild`: evaluate, register a command, evaluate (AC-5)
- `test_ENVIRON08_validate_sees_registered_enums` (AC-6)

## Phase 4: Implementation Plan

### Steps

Prerequisite: `global-enum-registration` implemented.

- [ ] 1. `liquers-web/src/command/import.rs` typed `global_enums`, `read_batch` returns them; `environment.rs` `validate_batch` seeding from an empty `REGISTERED_GLOBAL_ENUMS` — `registry_global_enums_are_read_typed` — `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles --lib import`
- [ ] 2. `environment.rs` `REGISTERED_GLOBAL_ENUMS`, `register_commands_on`, `rebuild_with` (new `enums` parameter), `rebuild_without`, `register_global_enum_on` — `environ_registry_global_enums_are_imported`, `environ_global_enums_survive_a_rebuild` — `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles environ`
- [ ] 3. `liquers-web/src/lib.rs` `registerGlobalEnum`, `typescript.rs` `JsGlobalEnumSpec` — the remaining web tests (`environ_register_global_enum_replaces_by_name`, `environ08_invalid_global_enum_is_refused`, `environ08_later_batch_sees_earlier_enum`) — same command, and `./liquers-web/scripts/check-stubs.sh --build` and with `--records`
- [ ] 4. `liquers-py/src/environment.rs` `ImportedMetadata`, `Declarations`, `EnvCell`, `Environment::add_global_enum` — the Python tests — `bash scripts/test-liquers-py.sh`
- [ ] 5. Documents of Phase 2 with History rows — `python3 scripts/docs_index.py --check`
- [ ] 6. `specs/issues/GLOBAL-ENUMS-NOT-IMPORTED-BY-LANGUAGE-INTEGRATIONS.md` closed with a resolution note; this design `complete` — `python3 scripts/docs_index.py` then `--check`

Rollback: steps 1-3 (web) and step 4 (Python) revert independently.

### Validation

Both bindings' loops as in `CLAUDE.md` (web with and without `records`); `cargo fmt --check -p
liquers-web -p liquers-py`. Diff: two crates and the documents; no core change.
