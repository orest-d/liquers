# Phase 1: High-Level Design — commands declare the value types they accept and produce

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — `L`; adds a `pub` type and `pub` fields in `liquers-core`,
  new `register_command!` statements and changed metadata of 114 commands (rules 1, 3, 4, 6).
- **Leading issue:** **Open design - where the declaration lives** (question 1).
- **Explanation:** every change is additive and omitted from serialized metadata when undeclared,
  so undeclared commands are byte-for-byte unchanged. Phases 3-4 assume the recommended answers.
- **Open questions:**
  1. **Open design - where the declaration lives:** a new field `accepts: TypeSet` on
     `ArgumentInfo` (so the state argument and every argument share it) and `produces: TypeSet` on
     `CommandMetadata`. Recommended over a new `ArgumentType` variant: `ArgumentType` says how a
     query *parameter* is parsed (`int`, `string`, an enum), which is orthogonal to the value type
     a link or the state carries, and a new variant would break every exhaustive match and the
     Python/JS bindings that read it.
  2. **Open design - shape of a declaration:** an ordered list of type identifiers; empty means
     "any" (undeclared); the first is the *principal* type, which conversion targets and
     specialization reports. Serialized as a string for one, a list for several
     (`accepts: [polars.DataFrame, Text, Bytes]`). Recommended. Groups or purposes ("a table") are
     deferred: they are the conversion design's open question 6 and need a conversion to mean
     anything.
  3. **Open design - a mismatch is a plan warning, not an error:** when the previous step's
     declared `produces` and this command's declared `accepts` share no identifier, the plan gets a
     `Step::Warning` and `liquers-validate` reports `Warning`. Recommended: conversion
     (`automatic-value-conversion`) may still bridge it, and a refusal would turn every imperfect
     declaration into an outage. Nothing is enforced at run time by this design.
  4. **Proposed resolution - declarations are honest, not aspirational:** a command lists every
     type it actually handles, including the ones it converts itself (`ns-pl/head` accepts
     `polars.DataFrame`, `RecordView`, `Text`, `Bytes`). The consumers rely on this: a type outside
     the list is one the command does not handle.
  5. **Proposed resolution - no inference in v1:** the macro cannot see the state's Rust type (the
     state keyword has none) or the returned value's variant, and `TypeIdentifiedIn` has no impls
     (`TYPE-IDENTIFIED-IN-HAS-NO-IMPLEMENTATIONS`). Declarations are explicit: `accepts:` and
     `produces:` statements, `(accepts: …)` per argument.
  6. **Proposed resolution - identifiers are in the command's realm,** as
     `realm-type-registry` question 1 recommends; a test asserts every identifier declared in
     `liquers-lib` is registered.

## Purpose

Let validation, a UI's "what can I apply next" and an agent tell a well-typed pipeline from an
ill-typed one before running it, and give conversion and specialization something to key on
(VISION "Trust the result"; the agent workbench; priority 2, DataFrames and records).

## Problem Example

```
-R/data/sales.csv/-/ns-pl/head-5/ns-img/resize-100-100
```

Today `liquers-validate` reports `Ok` and evaluation fails inside `resize` with
`Can't convert 'polars.DataFrame' to Image`. Nothing in `CommandMetadata` says that `ns-pl/head`
produces a frame or that `ns-img/resize` needs an image. Should be: the plan carries a warning
"`img/resize` accepts Image, but `pl/head` produces polars.DataFrame", `liquers-validate` reports
`Warning`, and the exported registry shows `accepts: Image` on `resize`'s state argument.

## Scope and Acceptance Criteria

- **AC-1** Declarations are recorded and serialized
  WHEN a command is registered with `accepts: ["polars.DataFrame", "Text"]` and `produces: "polars.DataFrame"`
  THEN its state argument's `accepts` lists both in that order, `produces` is `polars.DataFrame`, and both survive a JSON and a YAML round trip
- **AC-2** Undeclared is unchanged
  WHEN a command declares neither
  THEN its serialized metadata and its `metadata_version` are identical to those before this change
- **AC-3** A known mismatch is a plan warning
  WHEN the problem example is planned
  THEN the plan has a warning naming both commands and both type lists, and `liquers-validate` reports `Warning`, not `Error`
- **AC-4** Unknown input type, no warning
  WHEN `-R/data/photo.png/-/ns-img/resize-100-100` is planned
  THEN no type warning is added, because a resource's type is not known at plan time
- **AC-5** Overlap or an alias, no warning
  WHEN a command producing `polars.DataFrame` is followed by one accepting it, directly or through an alias
  THEN no type warning is added, and an alias reports its target's `produces`
- **AC-6** Arguments declare too
  WHEN an argument is declared `(accepts: "RecordView")`
  THEN its `ArgumentInfo::accepts` holds `RecordView`, and the declaration path accepts the same key
- **AC-7** Declared identifiers exist
  WHEN `check_declared_types` runs over `liquers-lib`'s registered commands and its type registry
  THEN it reports no issue, and an unknown identifier in a test registry is reported as an error naming the command
- **AC-8** Commands can be listed by input type
  WHEN `commands_accepting("Image")` is asked
  THEN it returns the commands whose state declares `Image`, and none that are undeclared

Non-goals: converting anything (`automatic-value-conversion`); choosing between commands
(`command-specialization-by-input-type`); purposes or groups; inference from Rust types; plan-time
checks of link arguments; run-time enforcement.

## Core Interactions

Commands (metadata fields, macro statements, declaration format); Plan (type tracking, warning);
Value types (identifiers, `TypeRegistry` check); validation (`liquers-validate` shows the warning).

## Crate Placement

`TypeSet` in `liquers-core/src/type_system.rs`; fields and queries in
`liquers-core/src/command_metadata.rs`; tracking in `liquers-core/src/plan.rs`; DSL in
`liquers-macro`; the declarations themselves on `ns-img`, `ns-pl`, `ns-rec` in `liquers-lib`.

## Documentation Intent

- Reference: extend `REGISTER_COMMAND_FSD.md` (statements), `COMMAND_DECLARATION.md` (keys),
  `VALUE_TYPE_SYSTEM.md` (§Declared command types).
- Guide: extend `COMMAND_REGISTRATION_GUIDE.md` with "Declaring what a command accepts and produces".
- Other documents: `specs/command_registry.yaml` regenerated.

## Design Dependencies

- `automatic-value-conversion` — **required-by**: converts to the principal declared type.
- `command-specialization-by-input-type` — **required-by**: selects a variant by `accepts`, at plan
  time through this design's type tracking.
- `value-conversion-registry` — **overlaps**: its open question on purposes decides whether a
  `TypeSet` may later name a purpose; no shared code.
- `realm-type-registry` — **overlaps**: realm of an identifier (question 6).
- `global-enum-registration` — **overlaps**: also edits `ArgumentInfo`/`ArgumentType` and the
  registry export; independent fields, either order.
- `value-type-definition-macro` — **overlaps**: generating `TypeIdentifiedIn` impls would enable
  inference later (question 5).

## Consolidated Findings

- The macro DSL's state keyword carries no Rust type and the function's return is always `V`, so
  inference is impossible at HEAD; explicit declaration is the only route (question 5).
- `ns-pl/head` is an alias of `ns-pl/slice`; `register_alias` copies `state_argument`, and must also
  copy `produces`, or every alias reads as undeclared output.
- `try_to_polars_dataframe` accepts a frame, a `RecordView` (via the bridge), `Text` and `Bytes`;
  `try_to_image` accepts only `Image`. Declarations follow the helpers, not the ideal.
- `PlanBuilder` processes a query front to back before predecessor cutting, so one field tracks the
  flowing type without seeing `Evaluate` boundaries.
- Declared metadata changes `metadata_version` of the 114 declared commands once; their cached
  results become stale once (Phase 2 risk).
- Order: this design first; the other two in either order after it.

## References

- `specs/issues/COMMAND-INPUT-OUTPUT-TYPES-ARE-UNDECLARED.md`; `specs/reference/VALUE_TYPE_SYSTEM.md`;
  `specs/design/value-type-system/type-conversion-draft.md`.
