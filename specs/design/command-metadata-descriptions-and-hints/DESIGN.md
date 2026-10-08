---
id: COMMAND-METADATA-DESCRIPTIONS-AND-HINTS
kind: design
title: Command-level hints and per-argument descriptions
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/commands, macro, lib/ui]
issues: [COMMAND-METADATA-HAS-NO-COMMAND-LEVEL-HINTS, ARGUMENT-INFO-HAS-NO-DESCRIPTION]
merged: 2026-10-08
created: 2026-10-08
---
# Command-level hints and per-argument descriptions

Merged on 2026-10-08 by maintainer decision (backlog compaction, decision D1) from
[`command-metadata-command-hints`](../command-metadata-command-hints/) and
[`argument-info-description`](../argument-info-description/), now `superseded`. The two issues are
mirror images: an argument has `hints` but no prose, and a command has prose (`doc`) but no
`hints`. One design means one macro grammar change and one review of the serialized metadata.
Leading source: `COMMAND-METADATA-HAS-NO-COMMAND-LEVEL-HINTS`. Phases 1-4 carry both originals'
content; their reasoning stays readable in the superseded folders.

## Phase 1: High-Level Design

### Purpose

Give `CommandMetadata` a `hints` map like `ArgumentInfo`'s, and give `ArgumentInfo` a
`description` like `CommandMetadata`'s `doc`, both settable from `register_command!`.

### Problem Example

A no-argument command such as `export` has nowhere to carry a UI hint (`icon`, `toolbar`): the
parameter-level `hint key: "value"` statement is parsed and discarded
(`liquers-macro/src/registration.rs` ≈1057, `// TODO: Implement hints`), and `CommandMetadata`
(`liquers-core/src/command_metadata.rs` ≈941) has no `hints`. Conversely,
`register_command!(cr, fn wrap(state, width: i32 = 80 (label: "Width")) -> result)` can label
`width` but not explain it, so UIs have no tooltip and `specs/command_registry.yaml` shows only
name, type and default.

### Scope and Acceptance Criteria

- **AC-1** Command hints round-trip
  - WHEN a `CommandMetadata` gets `with_hint("toolbar", true)`
  - THEN JSON and YAML round trips keep the value
- **AC-2** Command hints from the macro
  - WHEN a command is registered with the command statement `hint icon: "download"`
  - THEN its metadata's `hints` contains `"icon": "download"`
- **AC-3** Duplicate command hint keys are refused
  - WHEN the same command hint key appears twice in one registration
  - THEN macro expansion fails with an error at the second key
- **AC-4** Argument description round-trips
  - WHEN an `ArgumentInfo` gets `with_description("Maximum line width in characters")`
  - THEN JSON and YAML round trips keep it, and an entry without `description` reads as empty
- **AC-5** Argument description from the macro
  - WHEN an argument is declared `width: i32 = 80 (label: "Width", description: "Maximum line width in characters")`
  - THEN `metadata.arguments[0].description` holds that text
- **AC-6** Nothing existing changes
  - WHEN no command uses either feature
  - THEN empty `hints` and empty `description` are omitted from serialization, so
    `registry_export`'s signatures and every command's `metadata_version` are unchanged and the
    committed registry needs no regeneration

Out of scope: writing descriptions or hints for existing commands; the parameter-level `hint` arm
(`MACRO-QUERY-VALIDATION-AND-HINTS`); arbitrary JSON literals in the macro; UI rendering.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds `pub` fields to `CommandMetadata` and `ArgumentInfo`
  and new `register_command!` syntax (rule 4), across `liquers-core`, `liquers-macro` and
  `liquers-py` (rule 6); also needs-decision (rule 5)
- **Leading issue:** **Open design question — the two macro spellings**, a public syntax for every
  command author.
- **Explanation:** The fields, serde behaviour and builders are mechanical and fixed by AC-6. Only
  the spellings remain, and a working design is specified around the recommended ones.
- **Open questions:**
  1. **Resolved — merge** (maintainer, 2026-10-08, D1).
  2. **Proposed resolution — command hint spelling:** a command statement `hint key: "value"`,
     beside `label:` / `doc:`, reusing the parameter statement's grammar; values are strings only.
  3. **Proposed resolution — argument description spelling:** `description: "…"` inside an
     argument's existing parenthesized option list (which already holds `label:`, `gui:`, `enum:`).
  4. **Proposed resolution — field name `description`**, matching `FieldSchema::description` and
     the command preset's `description:`; the alternative `doc` would mirror `CommandMetadata::doc`.

### Design Dependencies

- `MACRO-QUERY-VALIDATION-AND-HINTS` (issue) — `overlaps`: completes the parameter-level hint arm.
- `command-declaration` (complete) — its declaration JSON must carry both new fields.
- `REGISTRY-IMPL-VERSION-DRIFT-UNDETECTED` — `overlaps`: registry export test semantics.

## Phase 2: Architecture

### Solution

`liquers-core/src/command_metadata.rs`:

```rust
// CommandMetadata
#[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
pub hints: serde_json::Map<String, serde_json::Value>,
pub fn with_hint(&mut self, key: &str, value: serde_json::Value) -> &mut Self

// ArgumentInfo
#[serde(default, skip_serializing_if = "String::is_empty")]
pub description: String,
pub fn with_description(&mut self, description: &str) -> &mut Self
```

Initialize both in every constructor and struct literal (`CommandMetadata::new`, `from_key`,
`ArgumentInfo::any_argument` and siblings, literals in `command_declaration.rs` and tests).

`liquers-macro/src/registration.rs`: a `CommandSignatureStatement::Hint(String, String)` parsed as
`hint <ident>: "<str>"`, collected with duplicate-key rejection (`syn::Error` at the second key's
span) and emitted as `.with_hint(key, Value::String(..))`; a `description` arm in the argument
option parser emitting `arg.description = ...` beside the label assignment. Update the grammar
comment in the same file.

`liquers-py/src/command_metadata.rs`: add read-only getters where the wrapper exposes per-field
getters.

Rejected: prose in `hints["description"]` (untyped); a command-level `arg_doc:` map (duplicates the
argument list); command facts on an arbitrary argument (wrong owner); silently accepting duplicate
keys.

### Changes

No commands added; existing namespaces unaffected. Documents: `REGISTER_COMMAND_FSD.md` (statement
and argument option tables), `COMMAND_DECLARATION.md` (field lists),
`COMMAND_REGISTRATION_GUIDE.md` (one example each), `CLAUDE.md` DSL lines; History rows and
`reviewed:` bumps.

### Risks

Struct literals across the workspace need the new fields (`cargo check --workspace --exclude
liquers-web` lists them). A wrong serde attribute would change every `metadata_version` and expire
every dependent on the next start; AC-6's test catches it, and the fix is the attribute, never a
registry regeneration. Certainty: high once the spellings are approved.

## Phase 3: Examples and Tests

### Examples

The Problem Example's `wrap` registration (AC-5) and an `export` command with `hint icon:
"download"` (AC-2).

### Tests

- `command_hints_round_trip` (`liquers-core/src/command_metadata.rs`) — AC-1
- `argument_description_round_trips` (same) — AC-4
- `empty_hints_and_description_are_omitted` (same) — AC-6
- `register_command_sets_command_hint` (`liquers-lib/tests/`, beside the existing macro tests) — AC-2
- `register_command_rejects_duplicate_command_hint` (parser unit test in `liquers-macro`) — AC-3
- `register_command_sets_argument_description` (`liquers-lib/tests/`) — AC-5
- `cargo test -p liquers-lib --test registry_export` passes without regenerating — AC-6

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `command_metadata.rs` — both fields, builders, constructors; fix literals — `cargo test -p liquers-core --lib command_metadata`
- [ ] 2. `registration.rs` — command `hint` statement with duplicate rejection — `cargo test -p liquers-macro`
- [ ] 3. `registration.rs` — argument `description` option — `cargo test -p liquers-lib --lib --tests register_command`
- [ ] 4. `liquers-py` getters if applicable — `cargo check -p liquers-py --lib`
- [ ] 5. Registry check — `cargo test -p liquers-lib --test registry_export` (no regeneration)
- [ ] 6. Documents above; both issues' resolutions; `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-macro`, `cargo test -p
liquers-lib --lib --tests`, `bash scripts/check-build-matrix.sh`. Rollback: revert; the fields are
additive.
