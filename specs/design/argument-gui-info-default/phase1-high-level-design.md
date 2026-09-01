# Phase 1: Consistent Default GUI Metadata

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** **Proposed resolution - canonical default:** Use `TextField(40)` for an
  otherwise unspecified argument in the macro, core constructor, declaration, and serde paths.
- **Explanation:** A working compatible solution is fully specified, but the default is observable
  UI metadata and changes metadata versions for macro-registered commands.
- **Open questions:** **Proposed resolution - canonical default:** Approve `TextField(40)` rather
  than a new type-aware policy; type-aware defaults are broader and should be a separate design.

## Problem and Evidence

The macro emits `TextField(20)`, `ArgumentInfo::any_argument` emits `TextField(40)`, and omitted
serde data uses `ArgumentGUIInfo::None`. Equal command declarations therefore produce different UI
hints and `metadata_version` values depending on registration path.

## Expected Behaviour and Acceptance Criteria

An unspecified argument has `TextField(40)` through all four paths. Explicit `gui` metadata is
unchanged. A parity test compares complete argument metadata and command metadata versions without
excluding `gui_info`; serde omission and round-trip tests establish the persisted default.

## Scope, Users, and Non-Goals

This affects command authors, JavaScript declarations, registry consumers, and argument UIs. It
changes the default only; it does not introduce type-aware widgets, migrate stored registries, or
alter explicit hints. Existing serialized documents remain readable.

## Design Dependencies

- `overlaps` `state-argument-serde-default`: both settle omitted command-metadata defaults, but
  neither orders the other and each retains its own source and tests.
- `overlaps` `command-declaration`: its parity test exposed the divergence and currently documents
  the `TextField(40)` declaration choice.

## Documentation Assessment

Update `specs/reference/COMMAND_DECLARATION.md` only if it enumerates omitted argument defaults;
otherwise the source docs and regression tests are sufficient. Record the metadata-version impact
in the issue resolution when implemented.

## Consolidated Findings

`ArgumentInfo::any_argument` and the declaration path already agree on 40, so changing the macro
and serde default minimizes cross-language disruption. Macro command metadata versions will change
once and dependent assets may expire; explicit GUI hints and wire shape remain compatible. Validate
core serde, macro expansion, declaration parity, registry freshness, and the build matrix.
