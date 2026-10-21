---
id: GLOBAL-ENUM-REGISTRATION
kind: design
title: Global enums are registered, checked and versioned like commands
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/commands, core/validate]
issues: [COMMAND-METADATA-ENHANCEMENTS]
created: 2026-10-20
---
# Global enums are registered, checked and versioned like commands

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20: Phases 1-4 written and
reviewed without phase approval. Nothing is implemented. The source,
`COMMAND-METADATA-ENHANCEMENTS`, was an umbrella over four milestones and has been re-scoped (its
body records how): this design covers what remains of milestone 1. The other milestones are now
`COMMAND-ENUM-ALTERNATIVES-CANNOT-BE-COMPUTED`, `COMMAND-SPECIALIZATION-BY-INPUT-TYPE-MISSING` and
`COMMAND-INPUT-OUTPUT-TYPES-ARE-UNDECLARED`.

## Phase 1: High-Level Design

### Purpose

Inline enums work end to end. They are declared with `register_command!` `enum:`, expanded at plan
time and shown by UIs. A global enum (`ArgumentType::GlobalEnum(name)`, macro `enum_ref:`) is
resolved at plan time, but nothing registers one, nothing checks a reference, and changing one does
not invalidate results. This design gives global enums the same treatment commands get.

### Problem Example

Verified at HEAD (18732dd), `liquers-core/src/command_metadata.rs`:

```rust
// The only way in: the pub field, with no validation.
cr.command_metadata_registry.global_enums.insert(
    "img.method".into(),
    EnumArgument::new("img.method").with_alternative("nearest").with_alternative("nearest"));
register_command!(cr, fn shrink(state, method: String (enum_ref: "img.methd")) -> result)?; // typo
let envref = builder.build()?;            // succeeds today
envref.evaluate("shrink-nearest").await;  // Err only now: "Global enum img.methd not defined"
```

Today `CommandMetadataRegistry` (≈1311) carries `// TODO: support global enums`, has
`get_global_enum` but no adder, and `check()` never looks at enum references. The duplicate alias is
accepted. `calculate_metadata_version` hashes the command's JSON, which holds only the enum's
*name*, so editing the enum's alternatives leaves every dependent result cached. What should
happen: a registration method refuses the duplicate alias, `build()` fails naming `shrink`,
`method` and `img.methd`, and the metadata version of `shrink` follows the enum's content.

### Scope and Acceptance Criteria

- **AC-1** Register and use
  WHEN `add_global_enum(e)` is called and a command refers to `e.name` with `enum_ref`
  THEN `get_global_enum(&e.name)` returns `e`, and planning `shrink-nearest` expands the alias to
  its value as for an inline enum
- **AC-2** An invalid enum is refused
  WHEN `add_global_enum` gets an enum with an empty name or a repeated alias
  THEN it returns `Err`, and the registry is unchanged
- **AC-3** A dangling reference is reported
  WHEN `CommandMetadataRegistry::check` runs and an argument refers to an undefined global enum
  THEN the report has an `Error` naming the command, the argument and the enum, and
  `EnvironmentBuilder::build` fails
- **AC-4** Versions follow the enum
  WHEN a global enum's alternatives change and metadata versions are refreshed
  THEN every command referring to it gets a new `metadata_version`, and the versions of the other
  commands do not change
- **AC-5** Registry files
  WHEN a registry document with `global_enums` is merged (`validate::registry` `merge_str`)
  THEN each enum goes through `add_global_enum`, and an entry whose map key differs from its
  `name` is a warning in `check`
- **AC-6** Format unchanged
  WHEN a registry is serialized
  THEN `global_enums` is written as today, and `specs/command_registry.yaml` does not change

Non-goals: computed alternatives (`COMMAND-ENUM-ALTERNATIVES-CANNOT-BE-COMPUTED`); labels on
alternatives (`ENUM-ALTERNATIVES-HAVE-NO-LABEL`); importing global enums from JavaScript or Python
(`GLOBAL-ENUMS-NOT-IMPORTED-BY-LANGUAGE-INTEGRATIONS`); resolving global enums in the egui and
search display helpers, which show the enum's name and stay correct.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. It adds a `pub` method to `CommandMetadataRegistry` in
  `liquers-core` (rule 4), and the readiness is `needs-decision` (rule 5).
- **Leading issue:** **Proposed resolution - severity of a dangling reference:** `Error`
  (recommended), not `Warning`.
- **Explanation:** all pieces exist: the map, `resolve_global_enums`, the plan-time expansion, the
  `check` → `build` path and the version refresh in `try_to_ref`. The change adds one adder, two
  checks and one input to the version hash, all in `command_metadata.rs`, plus one call site in
  `validate/registry.rs`.
- **Open questions:**
  1. **Proposed resolution - severity of a dangling reference.** Recommended `Error`: such an
     argument can never plan, so building should fail as it does for a starved variadic argument.
     Consequence: a library that registers a command with `enum_ref` must register the enum before
     `build`. A stub registry file used with `liquers-validate` must carry the enum too.
  2. **Proposed resolution - re-adding a name.** Recommended: replace, as `add_command` does for a
     key. Refusing would make re-registration (Python and web replay their declarations) fail.
  3. **Proposed resolution - version input.** Recommended: hash the command *with its global enums
     resolved* (AC-4). A command with no global enum hashes exactly as today, so no stored result is
     invalidated by the upgrade itself.

### Design Dependencies

- `macro-argument-hints-and-query-literals` — `overlaps` (weak): both are command-metadata work in
  this cluster with no shared change site. Either order.
- `GLOBAL-ENUMS-NOT-IMPORTED-BY-LANGUAGE-INTEGRATIONS` (issue, no design) — **required-by**: the
  integrations should call `add_global_enum`, so this lands first.
- `register-command-enum` (draft, 2026-03-02, sourceless) — its scope, the `enum:` / `enum_ref:`
  DSL and the run-time alias expansion, is implemented at HEAD (`registration.rs`
  `EnumParameterSpec`, `plan.rs` `ParameterValue::from_string` `Enum` arm,
  `REGISTER_COMMAND_FSD.md` §Parameter Metadata). It is not this design and is recommended for
  closing.

### Consolidated Findings

- `calculate_metadata_version` is an associated function with no registry. It becomes a method
  (`&self`) so that it can resolve. `add_command` computes the version before it mutably borrows
  the entry, so the borrow order is unchanged.
- `try_to_ref` already calls `refresh_metadata_versions` before startup snapshots the versions, so
  an enum added after its commands still versions them correctly.
- `liquers-mcp/src/server.rs` calls `resolve_global_enums(cmr)?` when it lists tools. With AC-3, a
  built environment can no longer hold a dangling reference, so that `?` cannot fail on one.

## Phase 2: Architecture

### Solution

`liquers-core/src/command_metadata.rs`:

```rust
impl EnumArgument {
    /// Problems with this enum's own definition: an empty name, an empty or repeated alias.
    pub(crate) fn definition_issues(&self) -> Vec<String>;
}
impl CommandMetadataRegistry {
    /// Registers (or replaces) a global enum under `enum_argument.name`, after
    /// `definition_issues` is empty; then refreshes the metadata versions of the commands that
    /// refer to it.
    pub fn add_global_enum(&mut self, enum_argument: EnumArgument) -> Result<&mut Self, Error>;
}
```

`add_global_enum` returns `Error::general_error(format!("Global enum '{name}': {issues}"))` on a
non-empty `definition_issues`. It changes nothing in that case.

`CommandMetadataRegistry::check` additionally:

- for every command and every argument whose `argument_type` is `GlobalEnum(name)` with no entry,
  adds `Issue::command_registry(Error, realm, namespace, command, format!("Argument '{arg}' refers
  to global enum '{name}', which is not defined"))`;
- for every `(key, e)` in `global_enums`, adds each of `e.definition_issues()` as an `Error` and,
  when `key != e.name`, a `Warning` (`Issue::generic`).

`calculate_metadata_version(command)` becomes `fn calculate_metadata_version(&self, command:
&CommandMetadata) -> Version`. It hashes `command.resolve_global_enums(self)` when that succeeds
and `command` itself otherwise. `add_command`, `update_command_metadata_version` and
`refresh_metadata_versions` call it on `self`. In `refresh_metadata_versions`, compute all versions
first and assign them after, which avoids borrowing `self` while iterating `&mut self.commands`.

`liquers-core/src/validate/registry.rs` `merge_str`: replace `global_enums.insert` with
`add_global_enum(global_enum)?`. A key ≠ name entry is first inserted as `e.name` and warned by
`check`.

Rejected: a separate `GlobalEnumRegistry` type (no second consumer); refusing re-registration
(question 2); resolving `GlobalEnum` into `Enum` at `add_command` time (it would freeze the enum
into the command and defeat replacement).

### Changes

- `liquers-core/src/command_metadata.rs`: `EnumArgument::definition_issues`,
  `CommandMetadataRegistry::{add_global_enum, check, calculate_metadata_version, add_command,
  update_command_metadata_version, refresh_metadata_versions}`; the `// TODO: support global
  enums` comment is removed.
- `liquers-core/src/validate/registry.rs`: `merge_str`.
- Documents: `guides/COMMAND_REGISTRATION_GUIDE.md` §Enum Parameters (how to register the enum an
  `enum_ref` names), `reference/REGISTER_COMMAND_FSD.md` (the `enum_ref` row: the enum must be
  registered before `build`), `reference/COMMAND_DECLARATION.md` §global enum note (≈425). Each
  gets a History row and a `reviewed:` bump.

Sync only. No serialized field is added or renamed.

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | two core files, three documents |
| Existing tests | `command_declaration.rs` test with `{"GlobalEnum":"colours"}` (≈1567) builds no environment and is unaffected. No in-repo command uses `enum_ref` |
| Compatibility | an environment with a dangling `enum_ref` now fails `build` (question 1). Metadata versions change only for commands with a global enum (question 3) |
| Performance | `check` and version refresh walk arguments once more. They run at build, not per query |
| Recovery | revert. The version change cannot corrupt stored data. At worst it recomputes results |
| Certainty | high; every reader of `global_enums` was found (`plan.rs`, `recipes.rs`, MCP, web import, egui, search) |

## Phase 3: Examples and Tests

### Examples

```rust
let cmr = &mut builder.command_registry.command_metadata_registry;
cmr.add_global_enum(EnumArgument::new("img.method")
    .with_alternative("nearest").with_string_value("hq", "lanczos3"))?;
register_command!(cr, fn shrink(state, method: String (enum_ref: "img.method")) -> result)?;
envref.evaluate("shrink-hq").await?;   // method == "lanczos3"
```

The query `ns-img/resize-100-100` (validated) shows today's inline-enum expansion: `default
method="lanczos3"`.

### Tests

Unit, `liquers-core/src/command_metadata.rs` `tests`:

- `add_global_enum_registers_by_name` (AC-1).
- `add_global_enum_refuses_invalid_definition` (AC-2): an empty name and a repeated alias, both
  `Err`, `global_enums` unchanged.
- `check_reports_undefined_global_enum_reference` (AC-3): message contains the command, argument
  and enum names. A defined reference reports nothing.
- `metadata_version_follows_global_enum_content` (AC-4): the version changes after the enum is
  replaced and `refresh_metadata_versions` runs. A command without a global enum keeps a version
  equal to the hash of its JSON (today's formula).
- `check_warns_when_global_enum_key_differs_from_name` (AC-5).

Unit, `liquers-core/src/plan.rs` `tests`: `global_enum_alias_expands_like_inline_enum` (AC-1),
`hq` → `ParameterValue("lanczos3")`.

Unit, `liquers-core/src/validate/registry.rs` `tests`: `merged_global_enums_are_validated` (AC-5),
a document with a repeated alias fails `merge_str`.

Integration, `liquers-core/tests/environment_builder.rs`:
`build_refuses_undefined_global_enum_reference` (AC-3).

Format: `cargo test -p liquers-lib --test registry_export` stays green (AC-6).

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib global_enum`, `--test
environment_builder`, and the `registry_export` test.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Record the answers to questions 1-3 in Phase 1 — review only
- [ ] 2. `command_metadata.rs` `EnumArgument::definition_issues`, `add_global_enum` — the AC-1 /
  AC-2 unit tests — `cargo test -p liquers-core --lib add_global_enum`
- [ ] 3. `command_metadata.rs` `check` — the AC-3 / AC-5 unit tests —
  `cargo test -p liquers-core --lib check_`
- [ ] 4. `command_metadata.rs` version functions — `metadata_version_follows_global_enum_content` —
  `cargo test -p liquers-core --lib metadata_version`
- [ ] 5. `plan.rs` test `global_enum_alias_expands_like_inline_enum` —
  `cargo test -p liquers-core --lib global_enum_alias`
- [ ] 6. `validate/registry.rs` `merge_str` and its test — `cargo test -p liquers-core --lib merged_global_enums`
- [ ] 7. `tests/environment_builder.rs` `build_refuses_undefined_global_enum_reference` —
  `cargo test -p liquers-core --test environment_builder`
- [ ] 8. Documents (COMMAND_REGISTRATION_GUIDE, REGISTER_COMMAND_FSD, COMMAND_DECLARATION) with
  History rows; close `COMMAND-METADATA-ENHANCEMENTS` (its remaining scope is this design); this
  design to `complete` — `python3 scripts/docs_index.py` then `--check`

### Validation

`cargo fmt --check -p liquers-core`; `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib
--tests`; `cargo test -p liquers-lib --lib --tests` (every library registration must still build:
none uses `enum_ref`); `cargo test -p liquers-mcp`. `specs/command_registry.yaml` unchanged. Diff
review: only `liquers-core` and the three documents.
