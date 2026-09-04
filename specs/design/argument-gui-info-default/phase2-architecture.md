# Phase 2: Solution and Architecture

## Chosen Solution

Add `command_metadata::DEFAULT_GUI`, a reusable core constant holding
`ArgumentGUIInfo::TextField(40)`. Apply a clone of it to `ArgumentInfo.gui_info` deserialization
and make the macro's implicit `gui_info` expression clone the same value. Keep explicit macro
`gui:` statements authoritative. `command_declaration.rs` and
`liquers-web/src/command/spec.rs` already construct arguments through the 40-column core path and
need verification, not behavioural edits.

## Files and Symbols

- `liquers-core/src/command_metadata.rs`: `ArgumentGUIInfo`, `ArgumentInfo::any_argument`,
  `ArgumentInfo::argument`, and the `ArgumentInfo.gui_info` serde attribute.
- `liquers-macro/src/registration.rs`: `CommandParameter` default GUI expression and expansion
  assertions that currently contain `TextField(20)`.
- `liquers-core/src/command_declaration.rs`: `def04_argument_gui_info_defaults_to_text_field_40`
  and parity coverage.
- `specs/command_registry.yaml`: regenerate only through the registry exporter if implementation
  changes committed macro metadata.

## Compatibility and Rejected Alternatives

The serialized field and enum stay unchanged. Macro-generated commands receive new metadata
versions; JavaScript/core-constructor commands do not. Reject `TextField(20)` because it would
re-version the web path, and reject type-aware inference because it changes more UI behaviour and
requires a separate compatibility policy.

## Rust Feasibility

The constant is a small enum value with no borrowing, async, trait, or error changes. Serde and
each consumer clone the one value, preventing a fourth default from emerging. No panic is needed.

## Risk Assessment

| Concern | Assessment and control |
|---|---|
| Files and crates | Core metadata plus macro expansion; web/declaration are verification callers. |
| Existing tests | Token snapshots containing 20 must intentionally change; explicit GUI tests stay fixed. |
| New validation | Serde omission, constructor parity, macro/declaration parity, metadata-version equality. |
| Compatibility/data | Wire shape compatible; macro metadata versions change and assets may expire once. |
| Concurrency/performance/security | None; constant construction only. |
| Recovery | Revert helper/default use and regenerated registry together. |
| Certainty | High technically; product choice remains visible in Phase 1. |
