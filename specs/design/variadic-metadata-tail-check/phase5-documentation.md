---
id: VARIADIC-METADATA-TAIL-CHECK-PHASE5
kind: design-phase
title: Documentation and completion summary for runtime metadata validation
---

# Phase 5: Documentation - Runtime Metadata Validation

## Completion Preconditions

Implementation is complete: core metadata checks, builder preflight, error reporting, native and
wasm emission, regression coverage, and a runnable example are present. The core library suite
passes and the wasm core target compiles. The remaining approval will mark this design complete.

## Implementation Summary

`liquers-core::issue_report` introduces composable `IssueReport`, `Issue`, and `IssueSeverity`.
`Issue::Generic` supports debug, info, warning, and error diagnostics independently of command
metadata; `Issue::CommandRegistry` carries command identity. Reports retain every issue, render
in deterministic order, emit to stderr or the wasm browser console, and turn error diagnostics
into a short bounded `Error` summary.

`CommandMetadata::check()` and `CommandMetadataRegistry::check()` now return `IssueReport`.
They reject an empty argument name, `multiple` combined with `injected`, and every ordinary
argument after the first ordinary variadic argument. Injected followers remain valid.
`CommandDeclaration` delegates to that canonical validation. `EnvironmentBuilder` retains a
non-optional report, exposes `validate()` and `validation_report()`, and rejects errors before it
constructs a configured store or assembles an environment. The runnable core example demonstrates
the preflight and compact build error.

## Documentation Delivered

- `specs/reference/COMMAND_DECLARATION.md` now defines the query-consuming variadic-tail rule,
  injected exception, and mutually exclusive flags.
- `specs/guides/COMMAND_REGISTRATION_GUIDE.md` now explains validation of hand-built/imported
  metadata and preflight report access.
- `specs/guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md` now documents builder-owned validation,
  target-specific full report emission, and the consuming-builder limitation.
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` now requires ENVIRON integrations to expose the
  preflight report to their host language before consuming builder initialization.
- `VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS` is closed. The generated command registry was
  reviewed: no command metadata declaration changed, so regeneration is unnecessary.

## Issues Filed

`ISSUE-REPORT-PLAN-AND-METADATA-LOGGING` remains draft follow-up scope for applying the generic
report to plan validation and metadata logging. It is deliberately not implemented here.

## Important Learning

The builder must validate before store construction, rather than relying on `try_to_ref`, so
deserialized metadata fails at the supported construction boundary and GUI/custom loggers can
inspect the full report first. Full report emission and returned errors serve different roles:
the latter is deliberately concise and safe for normal error surfaces.

## Conformance and Remaining Work

The implementation follows all approved phases. `Issue` matches are exhaustive, uses target-gated
wasm dependencies, adds no new core error type, and keeps diagnostics off stdout. Existing native
and wasm compilation warnings outside the changed code remain; no new warnings were introduced.

## Validation

- `cargo check -p liquers-core`
- `cargo test -p liquers-core --lib` — 793 passed
- `cargo run -p liquers-core --example issue_report_validation_demo`
- `cargo check -p liquers-core --target wasm32-unknown-unknown`
- `git diff --check`
