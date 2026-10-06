# Phase 2: Solution and Architecture

## Core

`liquers-core/src/command_metadata.rs`:

```rust
pub struct ArgumentInfo {
    pub name: String,
    #[serde(default)]
    pub label: String,
    /// Prose explaining the argument: what it means, units, effect. Empty when absent.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    // … unchanged fields
}

impl ArgumentInfo {
    pub fn with_description(&mut self, description: &str) -> &mut Self
}
```

Every constructor of `ArgumentInfo` (`any_argument`, `string_argument`, …, and struct literals in
`command_declaration.rs` and tests) gets `description: String::new()`. Search
`ArgumentInfo {` for literals.

## Macro

`liquers-macro/src/registration.rs`: the argument option parser (the `(label: …, gui: …)` list)
gets a `"description" => description = Some(lit.value())` arm. Code generation emits
`arg.description = #description.to_string();` next to the label assignment. Update the macro's
copy of the grammar documentation in the same file.

## Python

`liquers-py/src/command_metadata.rs`: if an `ArgumentInfo` wrapper exposes getters, add
`#[getter] fn description(&self) -> String`.

## Alternatives

- Put prose in `hints["description"]`. Rejected: untyped and unconventional, as the issue
  explains.
- A separate command-level `arg_doc:` statement mapping names to text. Rejected: it duplicates the
  argument list.

## Known-issue preflight

| Issue | Relation | Blocks? |
|---|---|---|
| `COMMAND-METADATA-HAS-NO-COMMAND-LEVEL-HINTS` | Mirror field; same files | No; merge recommended |
| `COMMAND-CACHE-FLAG-IS-DECLARED-BUT-NEVER-READ` | Same struct family; registry regeneration | No |
| `REGISTRY-IMPL-VERSION-DRIFT-UNDETECTED` | Registry export test semantics | No |

## Relevant commands

No new commands. Existing namespaces are unaffected until authors add descriptions.

## Documentation architecture

| Document | Change |
|---|---|
| `specs/reference/REGISTER_COMMAND_FSD.md` | Argument options table: `description: "..."` row; History; `reviewed:` |
| `specs/reference/COMMAND_DECLARATION.md` | Field list, if present |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | One example |
| `CLAUDE.md` | DSL line mentions `(label:, description:, gui:)` |

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `command_metadata.rs`, `command_declaration.rs` (literals), `liquers-macro/src/registration.rs`, `liquers-py/src/command_metadata.rs`, the docs above |
| Existing tests | Struct-literal tests need the new field. Registry export unchanged. |
| New validation | Phase 3 |
| Compatibility | Additive serde field (omitted when empty). Old YAML deserializes. |
| Data | `metadata_version` stable for unchanged commands |
| Recovery | Revert |
| Certainty | High |
